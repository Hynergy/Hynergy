mod store;
mod traversal;
#[cfg(any(test, debug_assertions))]
mod validate;

use hynergy_ids::define_non_zero_id;
use hynergy_model::device::definition::DeviceId;
use hynergy_model::network::{Network, WireId};
use smallvec::SmallVec;
use store::{DenseId, DenseIdStore};
use traversal::{island_components, wire_components};

pub(crate) use traversal::TraversalScratch;

define_non_zero_id!(NetId, IslandId);

impl DenseId for NetId {
    #[inline]
    fn from_slot(slot: usize) -> Self {
        assert!(slot as u32 <= hynergy_ids::MAX_PACKED_ID);

        let raw = slot
            .checked_add(1)
            .and_then(|value| u32::try_from(value).ok())
            .expect("31-bit NetId space exhausted");

        NetId::try_from(raw).expect("NetId values are one-based")
    }

    #[inline]
    fn slot(self) -> usize {
        self.index()
    }
}

impl DenseId for IslandId {
    #[inline]
    fn from_slot(slot: usize) -> Self {
        let raw = slot
            .checked_add(1)
            .and_then(|value| u32::try_from(value).ok())
            .expect("IslandId space exhausted");
        IslandId::try_from(raw).expect("IslandId values are one-based")
    }

    #[inline]
    fn slot(self) -> usize {
        self.index()
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct DerivedTopology {
    nets: DenseIdStore<NetId, Net>,
    wire_net_map: Vec<Option<NetId>>,

    islands: DenseIdStore<IslandId, IslandTopology>,
    net_island_map: Vec<Option<IslandId>>,
    device_island_map: Vec<Option<IslandId>>,
}

impl DerivedTopology {
    #[allow(dead_code)]
    pub(crate) fn from_network(network: &Network) -> Self {
        let mut topology = Self::default();

        for (index, slot) in network.wires().iter().enumerate() {
            if slot.is_some() {
                topology.add_wire(wire_id(index));
            }
        }

        for (index, slot) in network.wires().iter().enumerate() {
            if slot.is_none() {
                continue;
            }

            let wire = wire_id(index);
            for connection in network
                .wire_connections(wire)
                .expect("live Network wire must be readable")
            {
                let Some(other) = connection.as_wire() else {
                    continue;
                };

                if wire.index() < other.index() {
                    topology.connect_wires(network, wire, other);
                }
            }
        }

        for (index, slot) in network.devices().iter().enumerate() {
            if slot.is_some() {
                topology.add_device(device_id(index));
            }
        }

        for (index, slot) in network.devices().iter().enumerate() {
            let Some(device_slot) = slot else {
                continue;
            };
            let device = device_id(index);

            for connection in device_slot.terminals() {
                let Some(connection) = *connection else {
                    continue;
                };

                if let Some(wire) = connection.as_wire() {
                    topology.attach_terminal(network, wire, device);
                    continue;
                }

                if let Some((other_device, _)) = connection.as_terminal()
                    && device.index() < other_device.index()
                {
                    topology.connect_devices(network, device, other_device);
                }
            }
        }

        #[cfg(any(test, debug_assertions))]
        topology.assert_consistent(network);

        topology
    }

    #[inline]
    pub(crate) fn add_wire(&mut self, wire: WireId) {
        ensure_slot(&mut self.wire_net_map, wire.index());
        debug_assert!(self.wire_net_map[wire.index()].is_none());

        let net_id = self.nets.insert(Net { wires: vec![wire] });

        debug_assert_eq!(net_id.index(), self.net_island_map.len());
        self.net_island_map.push(None);

        self.wire_net_map[wire.index()] = Some(net_id);
    }

    #[inline]
    pub(crate) fn add_device(&mut self, device: DeviceId) {
        ensure_slot(&mut self.device_island_map, device.index());
        debug_assert!(self.device_island_map[device.index()].is_none());

        let island_id = self.islands.insert(IslandTopology {
            devices: vec![device],
            revision: 0,
        });
        self.device_island_map[device.index()] = Some(island_id);
    }

    pub(crate) fn connect_wires(&mut self, network: &Network, wire_a: WireId, wire_b: WireId) {
        debug_assert_ne!(wire_a, wire_b);

        let net_a = self.wire_net(wire_a);
        let net_b = self.wire_net(wire_b);

        if net_a == net_b {
            return;
        }

        let island_a = self.net_island(net_a);
        let island_b = self.net_island(net_b);

        let (dst_net_id, src_net_id) = match (island_a, island_b) {
            (Some(_), None) => (net_a, net_b),
            (None, Some(_)) => (net_b, net_a),

            _ => {
                let a_len = self
                    .nets
                    .get(net_a)
                    .expect("wire must reference a live net")
                    .wires
                    .len();

                let b_len = self
                    .nets
                    .get(net_b)
                    .expect("wire must reference a live net")
                    .wires
                    .len();

                if a_len >= b_len {
                    (net_a, net_b)
                } else {
                    (net_b, net_a)
                }
            }
        };

        let dst_island = self.net_island(dst_net_id);
        let src_island = self.net_island(src_net_id);

        let mut bump_final_island = false;

        let final_island = match (dst_island, src_island) {
            (None, None) => None,
            (Some(island), None) | (None, Some(island)) => Some(island),
            (Some(a), Some(b)) if a == b => {
                bump_final_island = true;
                Some(a)
            }
            (Some(a), Some(b)) => Some(self.merge_islands(network, a, b)),
        };

        let src_net = self
            .nets
            .remove(src_net_id)
            .expect("source net must remain live until merge");

        self.set_net_island(src_net_id, None);
        self.set_net_island(dst_net_id, final_island);

        for &wire in &src_net.wires {
            self.wire_net_map[wire.index()] = Some(dst_net_id);
        }

        self.nets
            .get_mut(dst_net_id)
            .expect("destination net must survive source removal")
            .wires
            .extend(src_net.wires);

        if bump_final_island {
            self.islands
                .get_mut(final_island.expect("same-island net merge must have an island"))
                .expect("merged net must reference a live island")
                .bump_revision();
        }
    }

    pub(crate) fn attach_terminal(&mut self, network: &Network, wire: WireId, device: DeviceId) {
        let net_id = self.wire_net(wire);
        let device_island = self.device_island(device);

        match self.net_island(net_id) {
            None => {
                self.net_island_map[net_id.index()] = Some(device_island);
                self.islands
                    .get_mut(device_island)
                    .expect("device must reference a live island")
                    .bump_revision();
            }

            Some(island) if island == device_island => {
                self.islands
                    .get_mut(island)
                    .expect("device/net island must be live")
                    .bump_revision();
            }

            Some(island) => {
                self.merge_islands(network, island, device_island);
            }
        }
    }

    pub(crate) fn detach_terminal(
        &mut self,
        network: &Network,
        scratch: &mut TraversalScratch,
        wire: WireId,
        device: DeviceId,
    ) {
        let net_id = self.wire_net(wire);
        let island_id = self.device_island(device);

        debug_assert_eq!(
            self.net_island(net_id),
            Some(island_id),
            "detached terminal's net and device must have belonged to the same island"
        );

        self.repair_island(network, scratch, island_id, std::slice::from_ref(&net_id));
    }

    pub(crate) fn disconnect_wires(
        &mut self,
        network: &Network,
        scratch: &mut TraversalScratch,
        wire_a: WireId,
        wire_b: WireId,
    ) {
        let net_a = self.wire_net(wire_a);
        let net_b = self.wire_net(wire_b);
        debug_assert_eq!(
            net_a, net_b,
            "a model wire edge must have belonged to one derived net before removal"
        );
        if net_a != net_b {
            return;
        }

        let island_id = self.net_island(net_a);
        let affected_nets = self.repair_net(network, scratch, net_a);

        if affected_nets.len() > 1
            && let Some(island_id) = island_id
            && self.islands.get(island_id).is_some()
        {
            self.repair_island(network, scratch, island_id, &affected_nets);
        }
    }

    pub(crate) fn remove_wire(
        &mut self,
        network: &Network,
        scratch: &mut TraversalScratch,
        wire: WireId,
    ) {
        let net_id = self
            .wire_net_map
            .get_mut(wire.index())
            .and_then(Option::take)
            .expect("removed model wire must still have a derived net before repair");
        let island_id = self.net_island(net_id);
        let affected_nets = self.repair_net(network, scratch, net_id);

        if let Some(island_id) = island_id
            && self.islands.get(island_id).is_some()
        {
            self.repair_island(network, scratch, island_id, &affected_nets);
        }
    }

    pub(crate) fn remove_device(
        &mut self,
        network: &Network,

        scratch: &mut TraversalScratch,
        device: DeviceId,
        affected_nets: &[NetId],
    ) {
        let island_id = self
            .device_island_map
            .get_mut(device.index())
            .and_then(Option::take)
            .expect("removed model device must still have a derived island before repair");

        let island = self
            .islands
            .get_mut(island_id)
            .expect("removed device must reference a live island");

        let position = island
            .devices
            .iter()
            .position(|&member| member == device)
            .expect("device island must contain the removed device");

        island.devices.swap_remove(position);

        self.repair_island(network, scratch, island_id, affected_nets);
    }

    pub(crate) fn device_nets(&self, network: &Network, device: DeviceId) -> Vec<NetId> {
        let Some(device) = network
            .devices()
            .get(device.index())
            .and_then(Option::as_ref)
        else {
            return Vec::new();
        };

        device
            .terminals()
            .iter()
            .flatten()
            .filter_map(|connection| connection.as_wire())
            .filter_map(|wire| self.wire_net_map.get(wire.index()).copied().flatten())
            .collect()
    }

    fn repair_net(
        &mut self,
        network: &Network,
        scratch: &mut TraversalScratch,
        net_id: NetId,
    ) -> SmallVec<[NetId; 2]> {
        let old_island = self.net_island(net_id);

        let mut components = wire_components(self, network, net_id, scratch);

        if components.is_empty() {
            self.nets
                .remove(net_id)
                .expect("empty repaired net must still be live");

            self.net_island_map[net_id.index()] = None;

            return SmallVec::new();
        }

        let keep_index = components
            .iter()
            .enumerate()
            .max_by_key(|(_, component)| component.len())
            .map(|(index, _)| index)
            .expect("non-empty component list must have a largest member");

        let kept = components.swap_remove(keep_index);

        for &wire in &kept {
            self.wire_net_map[wire.index()] = Some(net_id);
        }

        self.nets
            .get_mut(net_id)
            .expect("old NetId must survive a non-empty repartition")
            .wires = kept;

        let mut resulting_nets = SmallVec::with_capacity(components.len() + 1);
        resulting_nets.push(net_id);

        for component in components {
            let new_id = self.nets.insert(Net { wires: component });

            debug_assert_eq!(new_id.index(), self.net_island_map.len());
            self.net_island_map.push(old_island);

            let wires = &self
                .nets
                .get(new_id)
                .expect("newly inserted net must be live")
                .wires;

            for &wire in wires {
                self.wire_net_map[wire.index()] = Some(new_id);
            }

            resulting_nets.push(new_id);
        }

        resulting_nets
    }

    fn repair_island(
        &mut self,
        network: &Network,
        scratch: &mut TraversalScratch,
        island_id: IslandId,
        affected_nets: &[NetId],
    ) {
        let old_revision = match self.islands.get(island_id) {
            Some(island) => island.revision,
            None => return,
        };

        let mut components = island_components(self, network, scratch, island_id, affected_nets);

        let mut index = 0;
        while index < components.len() {
            if !components[index].devices.is_empty() {
                index += 1;
                continue;
            }

            let component = components.swap_remove(index);

            for net_id in component.nets {
                debug_assert_eq!(
                    self.net_island(net_id),
                    Some(island_id),
                    "device-free component must still belong to the island being repaired"
                );

                self.net_island_map[net_id.index()] = None;
            }
        }

        if components.is_empty() {
            self.islands
                .remove(island_id)
                .expect("empty repaired island must still be live");

            return;
        }

        let keep_index = components
            .iter()
            .enumerate()
            .max_by_key(|(_, component)| component.rewrite_cost())
            .map(|(index, _)| index)
            .expect("non-empty island component list must have a largest member");

        let kept = components.swap_remove(keep_index);

        let next_revision = old_revision
            .checked_add(1)
            .expect("island revision exhausted u64 range");

        {
            let island = self
                .islands
                .get_mut(island_id)
                .expect("old IslandId must survive a non-empty repartition");

            island.devices = kept.devices;
            island.revision = next_revision;
        }

        for component in components {
            let nets = component.nets;
            let devices = component.devices;

            let new_island_id = self.islands.insert(IslandTopology {
                devices,
                revision: 0,
            });

            for net_id in nets {
                debug_assert_eq!(
                    self.net_island(net_id),
                    Some(island_id),
                    "split net must still reference the old island before reassignment"
                );

                self.net_island_map[net_id.index()] = Some(new_island_id);
            }

            let (islands, device_island_map) = (&self.islands, &mut self.device_island_map);

            let devices = &islands
                .get(new_island_id)
                .expect("newly inserted island must be live")
                .devices;

            for &device_id in devices {
                debug_assert_eq!(
                    device_island_map[device_id.index()],
                    Some(island_id),
                    "split device must still reference the old island before reassignment"
                );

                device_island_map[device_id.index()] = Some(new_island_id);
            }
        }
    }

    fn merge_islands(
        &mut self,
        network: &Network,
        island_a: IslandId,
        island_b: IslandId,
    ) -> IslandId {
        if island_a == island_b {
            return island_a;
        }

        let a_cost = self.islands.get(island_a).unwrap().devices.len();
        let b_cost = self.islands.get(island_b).unwrap().devices.len();

        let (dst_id, src_id) = if a_cost >= b_cost {
            (island_a, island_b)
        } else {
            (island_b, island_a)
        };

        let src = self
            .islands
            .remove(src_id)
            .expect("source island must remain live until merge");

        for &device_id in &src.devices {
            self.device_island_map[device_id.index()] = Some(dst_id);

            for connection in network.devices()[device_id.index()]
                .as_ref()
                .expect("stored device id must be live")
                .terminals()
            {
                let connection = match connection {
                    Some(c) => c,
                    None => continue,
                };

                if let Some(wire_id) = connection.as_wire() {
                    let net_id = self.wire_net(wire_id);

                    if self.net_island(net_id) == Some(src_id) {
                        self.net_island_map[net_id.index()] = Some(dst_id);
                    }
                }
            }
        }

        let dst = self
            .islands
            .get_mut(dst_id)
            .expect("destination island must survive source removal");
        dst.devices.extend(src.devices);
        dst.bump_revision();

        dst_id
    }

    #[allow(unused)]
    #[deprecated]
    #[inline]
    fn merge_cost(&self, network: &Network, island_id: IslandId) -> usize {
        let island = self
            .islands
            .get(island_id)
            .expect("merge cost requires a live island");

        island.devices.len()
            + island
                .devices
                .iter()
                .map(|&device_id| {
                    network.devices()[device_id.index()]
                        .as_ref()
                        .expect("island device must be live")
                        .terminals()
                        .iter()
                        .filter(|connection| {
                            connection.is_some_and(|connection| connection.as_wire().is_some())
                        })
                        .count()
                })
                .sum::<usize>()
    }

    fn connect_devices(&mut self, network: &Network, device_a: DeviceId, device_b: DeviceId) {
        let island_a = self.device_island(device_a);
        let island_b = self.device_island(device_b);
        if island_a != island_b {
            self.merge_islands(network, island_a, island_b);
        }
    }

    #[inline]
    fn wire_net(&self, wire: WireId) -> NetId {
        self.wire_net_map[wire.index()].expect("live wire must have a derived NetId")
    }

    #[inline]
    fn device_island(&self, device: DeviceId) -> IslandId {
        self.device_island_map[device.index()].expect("live device must have a derived IslandId")
    }

    #[inline]
    fn net_island(&self, net: NetId) -> Option<IslandId> {
        self.net_island_map[net.index()]
    }

    #[inline]
    fn set_net_island(&mut self, net: NetId, island: Option<IslandId>) {
        self.net_island_map[net.index()] = island;
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn net(&self, id: NetId) -> Option<&Net> {
        self.nets.get(id)
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn island(&self, id: IslandId) -> Option<&IslandTopology> {
        self.islands.get(id)
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn nets(&self) -> impl ExactSizeIterator<Item = (NetId, &Net)> {
        self.nets.iter()
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn islands(&self) -> impl ExactSizeIterator<Item = (IslandId, &IslandTopology)> {
        self.islands.iter()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IslandTopology {
    devices: Vec<DeviceId>,
    revision: u64,
}

impl IslandTopology {
    #[inline]
    fn bump_revision(&mut self) {
        self.revision += 1;
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn devices(&self) -> &[DeviceId] {
        &self.devices
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Net {
    wires: Vec<WireId>,
}

impl Net {
    #[allow(dead_code)]
    #[inline]
    pub(crate) fn wires(&self) -> &[WireId] {
        &self.wires
    }
}

fn ensure_slot<T>(map: &mut Vec<Option<T>>, index: usize) {
    if map.len() <= index {
        map.resize_with(index + 1, || None);
    }
}

fn wire_id(index: usize) -> WireId {
    WireId::try_from(u32::try_from(index + 1).expect("wire index must fit WireId"))
        .expect("wire IDs are one-based")
}

fn device_id(index: usize) -> DeviceId {
    DeviceId::try_from(u32::try_from(index + 1).expect("device index must fit DeviceId"))
        .expect("device IDs are one-based")
}

#[cfg(test)]
mod tests {
    use super::{DerivedTopology, IslandId, NetId, TraversalScratch};
    use hynergy_model::device::definition::{DeviceId, PrimitiveElementKind, TerminalId};
    use hynergy_model::device::registry::DefinitionRegistry;
    use hynergy_model::network::{Network, WireId};

    fn wire(raw: u32) -> WireId {
        WireId::try_from(raw).unwrap()
    }

    fn device(raw: u32) -> DeviceId {
        DeviceId::try_from(raw).unwrap()
    }

    fn add_wire(network: &mut Network, topology: &mut DerivedTopology, id: WireId) {
        network.add_wire(id).unwrap();
        topology.add_wire(id);
        topology.assert_consistent(network);
    }

    fn add_device(
        network: &mut Network,
        topology: &mut DerivedTopology,
        definitions: &DefinitionRegistry,
        id: DeviceId,
        kind: PrimitiveElementKind,
    ) {
        network.add_device(definitions, id, kind.into()).unwrap();

        topology.add_device(id);
        topology.assert_consistent(network);
    }

    fn connect(network: &mut Network, topology: &mut DerivedTopology, a: WireId, b: WireId) {
        network.connect_wires(a, b).unwrap();
        topology.connect_wires(network, a, b);
        topology.assert_consistent(network);
    }

    fn disconnect(
        network: &mut Network,
        topology: &mut DerivedTopology,
        scratch: &mut TraversalScratch,
        a: WireId,
        b: WireId,
    ) {
        network.disconnect_wires(a, b).unwrap();
        topology.disconnect_wires(network, scratch, a, b);
        topology.assert_consistent(network);
    }

    fn attach(
        network: &mut Network,
        topology: &mut DerivedTopology,
        wire: WireId,
        device: DeviceId,
        terminal: u32,
    ) {
        network
            .attach_terminal(wire, device, TerminalId::new(terminal))
            .unwrap();

        topology.attach_terminal(network, wire, device);
        topology.assert_consistent(network);
    }

    fn detach(
        network: &mut Network,
        topology: &mut DerivedTopology,
        scratch: &mut TraversalScratch,
        wire: WireId,
        device: DeviceId,
        terminal: u32,
    ) {
        network
            .detach_terminal(wire, device, TerminalId::new(terminal))
            .unwrap();

        topology.detach_terminal(network, scratch, wire, device);
        topology.assert_consistent(network);
    }

    fn remove_wire(
        network: &mut Network,
        topology: &mut DerivedTopology,
        scratch: &mut TraversalScratch,
        wire: WireId,
    ) {
        network.remove_wire(wire).unwrap();
        topology.remove_wire(network, scratch, wire);
        topology.assert_consistent(network);
    }

    fn remove_device(
        network: &mut Network,
        topology: &mut DerivedTopology,
        scratch: &mut TraversalScratch,
        device: DeviceId,
    ) -> Vec<NetId> {
        let affected_nets = topology.device_nets(network, device);

        network.remove_device(device).unwrap();

        topology.remove_device(network, scratch, device, &affected_nets);

        topology.assert_consistent(network);

        affected_nets
    }

    #[test]
    fn dense_relocation_does_not_change_unrelated_net_id() {
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();

        let a = wire(1);
        let b = wire(2);
        let c = wire(3);

        add_wire(&mut network, &mut topology, a);
        add_wire(&mut network, &mut topology, b);
        add_wire(&mut network, &mut topology, c);

        let c_id = topology.wire_net(c);

        connect(&mut network, &mut topology, a, b);

        assert_eq!(topology.wire_net(c), c_id);
        assert!(topology.net(c_id).is_some());
    }

    #[test]
    fn connecting_islandless_wires_merges_only_their_nets() {
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();

        let a = wire(1);
        let b = wire(2);

        add_wire(&mut network, &mut topology, a);
        add_wire(&mut network, &mut topology, b);

        connect(&mut network, &mut topology, a, b);

        assert_eq!(topology.wire_net(a), topology.wire_net(b));
        assert_eq!(topology.nets.len(), 1);
        assert_eq!(topology.islands.len(), 0);
    }

    #[test]
    fn attaching_terminal_adds_net_to_devices_island() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();

        let w = wire(1);
        let d = device(1);

        add_wire(&mut network, &mut topology, w);

        add_device(
            &mut network,
            &mut topology,
            &definitions,
            d,
            PrimitiveElementKind::Admittance,
        );

        attach(&mut network, &mut topology, w, d, 0);

        let net = topology.wire_net(w);
        let island = topology.device_island(d);

        assert_eq!(topology.net_island(net), Some(island));
    }

    #[test]
    fn connecting_two_device_backed_nets_merges_nets_and_islands() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();

        let a = wire(1);
        let b = wire(2);
        let da = device(1);
        let db = device(2);

        add_wire(&mut network, &mut topology, a);
        add_wire(&mut network, &mut topology, b);

        add_device(
            &mut network,
            &mut topology,
            &definitions,
            da,
            PrimitiveElementKind::Admittance,
        );

        add_device(
            &mut network,
            &mut topology,
            &definitions,
            db,
            PrimitiveElementKind::Admittance,
        );

        attach(&mut network, &mut topology, a, da, 0);
        attach(&mut network, &mut topology, b, db, 0);

        connect(&mut network, &mut topology, a, b);

        assert_eq!(topology.wire_net(a), topology.wire_net(b));
        assert_eq!(topology.device_island(da), topology.device_island(db));
        assert_eq!(topology.nets.len(), 1);
        assert_eq!(topology.islands.len(), 1);
    }

    #[test]
    fn redundant_wire_edge_deletion_does_not_split_net() {
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();
        let mut scratch = TraversalScratch::default();

        let a = wire(1);
        let b = wire(2);
        let c = wire(3);

        for id in [a, b, c] {
            add_wire(&mut network, &mut topology, id);
        }

        connect(&mut network, &mut topology, a, b);
        connect(&mut network, &mut topology, b, c);
        connect(&mut network, &mut topology, a, c);

        let old = topology.wire_net(a);

        disconnect(&mut network, &mut topology, &mut scratch, a, c);

        assert_eq!(topology.nets.len(), 1);
        assert_eq!(topology.wire_net(a), old);
        assert_eq!(topology.wire_net(b), old);
        assert_eq!(topology.wire_net(c), old);
    }

    #[test]
    fn bridge_deletion_splits_net_and_keeps_old_id_on_larger_side() {
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();
        let mut scratch = TraversalScratch::default();

        let a = wire(1);
        let b = wire(2);
        let c = wire(3);

        for id in [a, b, c] {
            add_wire(&mut network, &mut topology, id);
        }

        connect(&mut network, &mut topology, a, b);
        connect(&mut network, &mut topology, b, c);

        let old = topology.wire_net(a);

        disconnect(&mut network, &mut topology, &mut scratch, b, c);

        assert_eq!(topology.nets.len(), 2);
        assert_eq!(topology.wire_net(a), old);
        assert_eq!(topology.wire_net(b), old);
        assert_ne!(topology.wire_net(c), old);
    }

    #[test]
    fn removing_articulation_wire_can_create_more_than_two_nets() {
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();
        let mut scratch = TraversalScratch::default();

        let center = wire(1);
        let branches = [wire(2), wire(3), wire(4)];

        add_wire(&mut network, &mut topology, center);

        for branch in branches {
            add_wire(&mut network, &mut topology, branch);
            connect(&mut network, &mut topology, center, branch);
        }

        remove_wire(&mut network, &mut topology, &mut scratch, center);

        assert_eq!(topology.nets.len(), 3);

        assert_ne!(
            topology.wire_net(branches[0]),
            topology.wire_net(branches[1])
        );

        assert_ne!(
            topology.wire_net(branches[1]),
            topology.wire_net(branches[2])
        );

        assert_ne!(
            topology.wire_net(branches[0]),
            topology.wire_net(branches[2])
        );
    }

    #[test]
    fn net_can_split_while_island_stays_connected_through_device() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();
        let mut scratch = TraversalScratch::default();

        let a = wire(1);
        let b = wire(2);
        let bridge = device(1);

        add_wire(&mut network, &mut topology, a);
        add_wire(&mut network, &mut topology, b);

        connect(&mut network, &mut topology, a, b);

        add_device(
            &mut network,
            &mut topology,
            &definitions,
            bridge,
            PrimitiveElementKind::Admittance,
        );

        attach(&mut network, &mut topology, a, bridge, 0);
        attach(&mut network, &mut topology, b, bridge, 1);

        let island = topology.device_island(bridge);

        disconnect(&mut network, &mut topology, &mut scratch, a, b);

        assert_ne!(topology.wire_net(a), topology.wire_net(b));

        assert_eq!(topology.net_island(topology.wire_net(a)), Some(island));

        assert_eq!(topology.net_island(topology.wire_net(b)), Some(island));

        assert_eq!(topology.islands.len(), 1);
    }

    #[test]
    fn wire_disconnect_can_split_both_net_and_island() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();
        let mut scratch = TraversalScratch::default();

        let a = wire(1);
        let b = wire(2);
        let da = device(1);
        let db = device(2);

        add_wire(&mut network, &mut topology, a);
        add_wire(&mut network, &mut topology, b);

        add_device(
            &mut network,
            &mut topology,
            &definitions,
            da,
            PrimitiveElementKind::Admittance,
        );

        add_device(
            &mut network,
            &mut topology,
            &definitions,
            db,
            PrimitiveElementKind::Admittance,
        );

        attach(&mut network, &mut topology, a, da, 0);
        attach(&mut network, &mut topology, b, db, 0);

        connect(&mut network, &mut topology, a, b);

        let old_island = topology.device_island(da);

        assert_eq!(old_island, topology.device_island(db));

        disconnect(&mut network, &mut topology, &mut scratch, a, b);

        assert_ne!(topology.wire_net(a), topology.wire_net(b));

        assert_ne!(topology.device_island(da), topology.device_island(db));

        assert!(
            topology.device_island(da) == old_island || topology.device_island(db) == old_island
        );

        assert_eq!(topology.islands.len(), 2);
    }

    #[test]
    fn detaching_bridge_terminal_repartitions_only_the_affected_island() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();
        let mut scratch = TraversalScratch::default();

        let a = wire(1);
        let b = wire(2);

        let left = device(1);
        let right = device(2);
        let bridge = device(3);

        add_wire(&mut network, &mut topology, a);
        add_wire(&mut network, &mut topology, b);

        for id in [left, right, bridge] {
            add_device(
                &mut network,
                &mut topology,
                &definitions,
                id,
                PrimitiveElementKind::Admittance,
            );
        }

        attach(&mut network, &mut topology, a, left, 0);
        attach(&mut network, &mut topology, b, right, 0);
        attach(&mut network, &mut topology, a, bridge, 0);
        attach(&mut network, &mut topology, b, bridge, 1);

        assert_eq!(topology.islands.len(), 1);

        detach(&mut network, &mut topology, &mut scratch, b, bridge, 1);

        assert_eq!(topology.islands.len(), 2);

        assert_eq!(topology.device_island(left), topology.device_island(bridge));

        assert_ne!(topology.device_island(left), topology.device_island(right));
    }

    #[test]
    fn removing_multi_terminal_bridge_device_can_create_many_islands() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();
        let mut scratch = TraversalScratch::default();

        let wires = [wire(1), wire(2), wire(3)];
        let leaves = [device(1), device(2), device(3)];
        let bridge = device(4);

        for id in wires {
            add_wire(&mut network, &mut topology, id);
        }

        for id in leaves {
            add_device(
                &mut network,
                &mut topology,
                &definitions,
                id,
                PrimitiveElementKind::Admittance,
            );
        }

        add_device(
            &mut network,
            &mut topology,
            &definitions,
            bridge,
            PrimitiveElementKind::ControlledThroughSource,
        );

        for (index, (&wire_id, &leaf)) in wires.iter().zip(leaves.iter()).enumerate() {
            attach(&mut network, &mut topology, wire_id, leaf, 0);

            attach(
                &mut network,
                &mut topology,
                wire_id,
                bridge,
                u32::try_from(index).unwrap(),
            );
        }

        assert_eq!(topology.islands.len(), 1);

        let old_island = topology.device_island(bridge);

        let affected_nets = remove_device(&mut network, &mut topology, &mut scratch, bridge);

        assert_eq!(affected_nets.len(), 3);

        for wire in wires {
            assert!(
                affected_nets.contains(&topology.wire_net(wire)),
                "every net formerly attached to the bridge must be captured"
            );
        }

        assert!(
            topology.island(old_island).is_some(),
            "the old IslandId should survive on one resulting component"
        );

        assert_eq!(topology.islands.len(), 3);

        assert_ne!(
            topology.device_island(leaves[0]),
            topology.device_island(leaves[1])
        );

        assert_ne!(
            topology.device_island(leaves[1]),
            topology.device_island(leaves[2])
        );

        assert_ne!(
            topology.device_island(leaves[0]),
            topology.device_island(leaves[2])
        );
    }

    #[test]
    fn removing_only_device_makes_all_incident_nets_islandless() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();
        let mut scratch = TraversalScratch::default();

        let wires = [wire(1), wire(2), wire(3)];
        let bridge = device(1);

        for wire in wires {
            add_wire(&mut network, &mut topology, wire);
        }

        add_device(
            &mut network,
            &mut topology,
            &definitions,
            bridge,
            PrimitiveElementKind::ControlledThroughSource,
        );

        for (terminal, wire) in wires.into_iter().enumerate() {
            attach(
                &mut network,
                &mut topology,
                wire,
                bridge,
                u32::try_from(terminal).unwrap(),
            );
        }

        let nets = wires.map(|wire| topology.wire_net(wire));
        let old_island = topology.device_island(bridge);

        assert_eq!(topology.islands.len(), 1);

        let affected_nets = remove_device(&mut network, &mut topology, &mut scratch, bridge);

        assert_eq!(affected_nets.len(), 3);
        assert_eq!(topology.islands.len(), 0);
        assert!(topology.island(old_island).is_none());

        for net_id in nets {
            assert!(topology.net(net_id).is_some());
            assert_eq!(topology.net_island(net_id), None);
        }
    }

    #[test]
    fn detaching_last_terminal_makes_net_islandless() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();
        let mut scratch = TraversalScratch::default();

        let w = wire(1);
        let d = device(1);

        add_wire(&mut network, &mut topology, w);

        add_device(
            &mut network,
            &mut topology,
            &definitions,
            d,
            PrimitiveElementKind::Admittance,
        );

        attach(&mut network, &mut topology, w, d, 0);

        let net_id = topology.wire_net(w);
        let island_id = topology.device_island(d);

        assert_eq!(topology.net_island(net_id), Some(island_id));

        detach(&mut network, &mut topology, &mut scratch, w, d, 0);

        assert_eq!(topology.device_island(d), island_id);

        assert_eq!(topology.islands.len(), 1);

        assert!(topology.net(net_id).is_some());
        assert_eq!(topology.net_island(net_id), None);
    }

    #[test]
    fn removing_last_wire_retires_net_and_clears_island_membership() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();
        let mut scratch = TraversalScratch::default();

        let w = wire(1);
        let d = device(1);

        add_wire(&mut network, &mut topology, w);

        add_device(
            &mut network,
            &mut topology,
            &definitions,
            d,
            PrimitiveElementKind::Admittance,
        );

        attach(&mut network, &mut topology, w, d, 0);

        let net_id = topology.wire_net(w);
        let island_id = topology.device_island(d);

        remove_wire(&mut network, &mut topology, &mut scratch, w);

        assert!(topology.net(net_id).is_none());

        assert_eq!(topology.net_island_map[net_id.index()], None);

        assert_eq!(topology.device_island(d), island_id);

        assert_eq!(topology.islands.len(), 1);
    }

    #[test]
    fn merging_two_nets_inside_same_island_bumps_revision() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();

        let a = wire(1);
        let b = wire(2);
        let d = device(1);

        add_wire(&mut network, &mut topology, a);
        add_wire(&mut network, &mut topology, b);

        add_device(
            &mut network,
            &mut topology,
            &definitions,
            d,
            PrimitiveElementKind::Admittance,
        );

        attach(&mut network, &mut topology, a, d, 0);
        attach(&mut network, &mut topology, b, d, 1);

        assert_ne!(topology.wire_net(a), topology.wire_net(b));

        let island_id = topology.device_island(d);

        let old_revision = topology.island(island_id).unwrap().revision();

        connect(&mut network, &mut topology, a, b);

        assert_eq!(topology.wire_net(a), topology.wire_net(b));

        assert_eq!(topology.net_island(topology.wire_net(a)), Some(island_id));

        assert_eq!(
            topology.island(island_id).unwrap().revision(),
            old_revision + 1,
            "merging electrical nets changes the island IR topology"
        );
    }

    #[test]
    fn island_split_keeps_old_id_on_component_with_larger_rewrite_cost() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();
        let mut scratch = TraversalScratch::default();

        let a = wire(1);
        let b = wire(2);

        let left_a = device(1);
        let left_b = device(2);
        let right = device(3);

        add_wire(&mut network, &mut topology, a);
        add_wire(&mut network, &mut topology, b);

        connect(&mut network, &mut topology, a, b);

        for device in [left_a, left_b, right] {
            add_device(
                &mut network,
                &mut topology,
                &definitions,
                device,
                PrimitiveElementKind::Admittance,
            );
        }

        attach(&mut network, &mut topology, a, left_a, 0);
        attach(&mut network, &mut topology, a, left_b, 0);
        attach(&mut network, &mut topology, b, right, 0);

        let old_island = topology.device_island(left_a);

        assert_eq!(topology.device_island(left_b), old_island);

        assert_eq!(topology.device_island(right), old_island);

        disconnect(&mut network, &mut topology, &mut scratch, a, b);

        assert_eq!(topology.device_island(left_a), old_island);

        assert_eq!(topology.device_island(left_b), old_island);

        assert_ne!(topology.device_island(right), old_island);
    }

    #[test]
    fn topology_can_rebuild_multi_net_island_from_existing_network() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let wires = [wire(1), wire(2), wire(3)];
        let leaves = [device(1), device(2), device(3)];
        let bridge = device(4);

        for wire in wires {
            network.add_wire(wire).unwrap();
        }

        for device in leaves {
            network
                .add_device(
                    &definitions,
                    device,
                    PrimitiveElementKind::Admittance.into(),
                )
                .unwrap();
        }

        network
            .add_device(
                &definitions,
                bridge,
                PrimitiveElementKind::ControlledThroughSource.into(),
            )
            .unwrap();

        for (terminal, (&wire, &leaf)) in wires.iter().zip(leaves.iter()).enumerate() {
            network
                .attach_terminal(wire, leaf, TerminalId::new(0))
                .unwrap();

            network
                .attach_terminal(
                    wire,
                    bridge,
                    TerminalId::new(u32::try_from(terminal).unwrap()),
                )
                .unwrap();
        }

        let topology = DerivedTopology::from_network(&network);

        topology.assert_consistent(&network);

        assert_eq!(topology.nets.len(), 3);
        assert_eq!(topology.islands.len(), 1);

        let island = topology.device_island(bridge);

        for leaf in leaves {
            assert_eq!(topology.device_island(leaf), island);
        }

        for wire in wires {
            assert_eq!(topology.net_island(topology.wire_net(wire)), Some(island));
        }
    }

    #[test]
    fn stable_component_ids_are_one_based() {
        assert_eq!(NetId::try_from(1).unwrap().get(), 1);

        assert_eq!(IslandId::try_from(1).unwrap().get(), 1);
    }
}
