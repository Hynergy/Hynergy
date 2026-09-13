mod store;
mod traversal;
#[cfg(any(test, debug_assertions))]
mod validate;

use hynergy_ids::define_non_zero_id;
use hynergy_model::device::definition::DeviceId;
use hynergy_model::network::{Network, WireId};
use store::{DenseId, DenseIdStore};
use traversal::{island_components, wire_components};

define_non_zero_id!(NetId, IslandId);

impl DenseId for NetId {
    #[inline]
    fn from_slot(slot: usize) -> Self {
        let raw = slot
            .checked_add(1)
            .and_then(|value| u32::try_from(value).ok())
            .expect("NetId space exhausted");
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
                    topology.connect_wires(wire, other);
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
                    topology.attach_terminal(wire, device);
                    continue;
                }

                if let Some((other_device, _)) = connection.as_terminal()
                    && device.index() < other_device.index()
                {
                    topology.connect_devices(device, other_device);
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

        let net_id = self.nets.insert(Net {
            island: None,
            wires: vec![wire],
        });
        self.wire_net_map[wire.index()] = Some(net_id);
    }

    #[inline]
    pub(crate) fn add_device(&mut self, device: DeviceId) {
        ensure_slot(&mut self.device_island_map, device.index());
        debug_assert!(self.device_island_map[device.index()].is_none());

        let island_id = self.islands.insert(IslandTopology {
            nets: Vec::new(),
            devices: vec![device],
            revision: 0,
        });
        self.device_island_map[device.index()] = Some(island_id);
    }

    pub(crate) fn connect_wires(&mut self, wire_a: WireId, wire_b: WireId) {
        debug_assert_ne!(wire_a, wire_b);

        let net_a = self.wire_net(wire_a);
        let net_b = self.wire_net(wire_b);
        if net_a == net_b {
            return;
        }

        let (dst_net_id, src_net_id) = {
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
        };

        let dst_island = self
            .nets
            .get(dst_net_id)
            .expect("destination net must be live")
            .island;
        let src_island = self
            .nets
            .get(src_net_id)
            .expect("source net must be live")
            .island;

        let final_island = match (dst_island, src_island) {
            (None, None) => None,
            (Some(island), None) | (None, Some(island)) => Some(island),
            (Some(a), Some(b)) if a == b => Some(a),
            (Some(a), Some(b)) => Some(self.merge_islands(a, b)),
        };

        let src_net = self
            .nets
            .remove(src_net_id)
            .expect("source net must remain live until merge");

        for &wire in &src_net.wires {
            self.wire_net_map[wire.index()] = Some(dst_net_id);
        }

        {
            let dst_net = self
                .nets
                .get_mut(dst_net_id)
                .expect("destination net must survive source removal");
            dst_net.wires.extend(src_net.wires);
            dst_net.island = final_island;
        }

        if let Some(island_id) = final_island {
            let island = self
                .islands
                .get_mut(island_id)
                .expect("merged net must reference a live island");
            island
                .nets
                .retain(|&net_id| net_id != dst_net_id && net_id != src_net_id);
            island.nets.push(dst_net_id);
            island.bump_revision();
        }
    }

    pub(crate) fn attach_terminal(&mut self, wire: WireId, device: DeviceId) {
        let net_id = self.wire_net(wire);
        let device_island = self.device_island(device);
        let net_island = self
            .nets
            .get(net_id)
            .expect("wire must reference a live net")
            .island;

        match net_island {
            None => {
                self.nets
                    .get_mut(net_id)
                    .expect("wire must reference a live net")
                    .island = Some(device_island);

                let island = self
                    .islands
                    .get_mut(device_island)
                    .expect("device must reference a live island");
                if !island.nets.contains(&net_id) {
                    island.nets.push(net_id);
                }
                island.bump_revision();
            }
            Some(island_id) if island_id == device_island => {
                self.islands
                    .get_mut(island_id)
                    .expect("device/net island must be live")
                    .bump_revision();
            }
            Some(island_id) => {
                self.merge_islands(island_id, device_island);
            }
        }
    }

    pub(crate) fn detach_terminal(&mut self, network: &Network, wire: WireId, device: DeviceId) {
        let net_id = self.wire_net(wire);
        let island_id = self.device_island(device);
        debug_assert_eq!(
            self.nets
                .get(net_id)
                .expect("wire must reference a live net")
                .island,
            Some(island_id)
        );

        self.repair_island(network, island_id);
    }

    pub(crate) fn disconnect_wires(&mut self, network: &Network, wire_a: WireId, wire_b: WireId) {
        let net_a = self.wire_net(wire_a);
        let net_b = self.wire_net(wire_b);
        debug_assert_eq!(
            net_a, net_b,
            "a model wire edge must have belonged to one derived net before removal"
        );
        if net_a != net_b {
            return;
        }

        let island_id = self
            .nets
            .get(net_a)
            .expect("wire must reference a live net")
            .island;
        let component_count = self.repair_net(network, net_a);

        if component_count > 1
            && let Some(island_id) = island_id
            && self.islands.get(island_id).is_some()
        {
            self.repair_island(network, island_id);
        }
    }

    pub(crate) fn remove_wire(&mut self, network: &Network, wire: WireId) {
        let net_id = self
            .wire_net_map
            .get_mut(wire.index())
            .and_then(Option::take)
            .expect("removed model wire must still have a derived net before repair");
        let island_id = self
            .nets
            .get(net_id)
            .expect("removed wire must reference a live net")
            .island;

        self.repair_net(network, net_id);

        if let Some(island_id) = island_id
            && self.islands.get(island_id).is_some()
        {
            self.repair_island(network, island_id);
        }
    }

    pub(crate) fn remove_device(&mut self, network: &Network, device: DeviceId) {
        let island_id = self
            .device_island_map
            .get_mut(device.index())
            .and_then(Option::take)
            .expect("removed model device must still have a derived island before repair");

        if let Some(island) = self.islands.get_mut(island_id) {
            island.devices.retain(|&member| member != device);
        }

        self.repair_island(network, island_id);
    }

    fn repair_net(&mut self, network: &Network, net_id: NetId) -> usize {
        let (old_island, candidates) = {
            let net = self
                .nets
                .get(net_id)
                .expect("net repair requires a live net");
            (net.island, net.wires.clone())
        };
        let mut components = wire_components(network, &candidates);
        let component_count = components.len();

        if components.is_empty() {
            self.nets
                .remove(net_id)
                .expect("empty repaired net must still be live");
            if let Some(island_id) = old_island
                && let Some(island) = self.islands.get_mut(island_id)
            {
                island.nets.retain(|&member| member != net_id);
                island.bump_revision();
            }
            return 0;
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
            .expect("old net ID must survive non-empty repartition")
            .wires = kept;

        let mut created = Vec::with_capacity(components.len());
        for component in components {
            let new_id = self.nets.insert(Net {
                island: old_island,
                wires: component,
            });

            let wires = &self
                .nets
                .get(new_id)
                .expect("newly inserted net must be live")
                .wires;
            for &wire in wires {
                self.wire_net_map[wire.index()] = Some(new_id);
            }
            created.push(new_id);
        }

        if let Some(island_id) = old_island
            && !created.is_empty()
        {
            let island = self
                .islands
                .get_mut(island_id)
                .expect("net island must remain live until island repair");
            island.nets.extend(created);
            island.bump_revision();
        }

        component_count
    }

    fn repair_island(&mut self, network: &Network, island_id: IslandId) {
        if self.islands.get(island_id).is_none() {
            return;
        }

        let old_revision = self
            .islands
            .get(island_id)
            .expect("island must be live")
            .revision;
        let mut components = island_components(self, network, island_id);

        for component in components
            .iter()
            .filter(|component| component.devices.is_empty())
        {
            for &net_id in &component.nets {
                self.nets
                    .get_mut(net_id)
                    .expect("island component net must be live")
                    .island = None;
            }
        }
        components.retain(|component| !component.devices.is_empty());

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

        for &net_id in &kept.nets {
            self.nets
                .get_mut(net_id)
                .expect("kept island net must be live")
                .island = Some(island_id);
        }
        for &device_id in &kept.devices {
            self.device_island_map[device_id.index()] = Some(island_id);
        }

        {
            let island = self
                .islands
                .get_mut(island_id)
                .expect("old island ID must survive non-empty repartition");
            island.nets = kept.nets;
            island.devices = kept.devices;
            island.revision = next_revision;
        }

        for component in components {
            let nets = component.nets;
            let devices = component.devices;
            let new_island_id = self.islands.insert(IslandTopology {
                nets: nets.clone(),
                devices: devices.clone(),
                revision: 0,
            });

            for net_id in nets {
                self.nets
                    .get_mut(net_id)
                    .expect("new island net must be live")
                    .island = Some(new_island_id);
            }
            for device_id in devices {
                self.device_island_map[device_id.index()] = Some(new_island_id);
            }
        }
    }

    fn merge_islands(&mut self, island_a: IslandId, island_b: IslandId) -> IslandId {
        if island_a == island_b {
            return island_a;
        }

        let (dst_id, src_id) = {
            let a = self
                .islands
                .get(island_a)
                .expect("first island must be live");
            let b = self
                .islands
                .get(island_b)
                .expect("second island must be live");
            let a_cost = a.nets.len() + a.devices.len();
            let b_cost = b.nets.len() + b.devices.len();

            if a_cost >= b_cost {
                (island_a, island_b)
            } else {
                (island_b, island_a)
            }
        };

        let src = self
            .islands
            .remove(src_id)
            .expect("source island must remain live until merge");

        for &net_id in &src.nets {
            self.nets
                .get_mut(net_id)
                .expect("source island net must be live")
                .island = Some(dst_id);
        }
        for &device_id in &src.devices {
            self.device_island_map[device_id.index()] = Some(dst_id);
        }

        let dst = self
            .islands
            .get_mut(dst_id)
            .expect("destination island must survive source removal");
        dst.nets.extend(src.nets);
        dst.devices.extend(src.devices);
        dst.bump_revision();
        dst_id
    }

    fn connect_devices(&mut self, device_a: DeviceId, device_b: DeviceId) {
        let island_a = self.device_island(device_a);
        let island_b = self.device_island(device_b);
        if island_a != island_b {
            self.merge_islands(island_a, island_b);
        }
    }

    #[inline]
    fn wire_net(&self, wire: WireId) -> NetId {
        self.wire_net_map
            .get(wire.index())
            .copied()
            .flatten()
            .expect("wire must have a derived NetId")
    }

    #[inline]
    fn device_island(&self, device: DeviceId) -> IslandId {
        self.device_island_map
            .get(device.index())
            .copied()
            .flatten()
            .expect("device must have a derived IslandId")
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
    nets: Vec<NetId>,
    devices: Vec<DeviceId>,
    revision: u64,
}

impl IslandTopology {
    #[inline]
    fn bump_revision(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("island revision exhausted u64 range");
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn nets(&self) -> &[NetId] {
        &self.nets
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
    island: Option<IslandId>,
    wires: Vec<WireId>,
}

impl Net {
    #[allow(dead_code)]
    #[inline]
    pub(crate) fn island(&self) -> Option<IslandId> {
        self.island
    }

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
    use super::{DerivedTopology, IslandId, NetId};
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
        topology.connect_wires(a, b);
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
        topology.attach_terminal(wire, device);
        topology.assert_consistent(network);
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
        assert_eq!(topology.net(net).unwrap().island(), Some(island));
        assert_eq!(topology.island(island).unwrap().nets(), &[net]);
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

        network.disconnect_wires(a, c).unwrap();
        topology.disconnect_wires(&network, a, c);
        topology.assert_consistent(&network);

        assert_eq!(topology.nets.len(), 1);
        assert_eq!(topology.wire_net(a), old);
        assert_eq!(topology.wire_net(b), old);
        assert_eq!(topology.wire_net(c), old);
    }

    #[test]
    fn bridge_deletion_splits_net_and_keeps_old_id_on_larger_side() {
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();
        let a = wire(1);
        let b = wire(2);
        let c = wire(3);

        for id in [a, b, c] {
            add_wire(&mut network, &mut topology, id);
        }
        connect(&mut network, &mut topology, a, b);
        connect(&mut network, &mut topology, b, c);
        let old = topology.wire_net(a);

        network.disconnect_wires(b, c).unwrap();
        topology.disconnect_wires(&network, b, c);
        topology.assert_consistent(&network);

        assert_eq!(topology.nets.len(), 2);
        assert_eq!(topology.wire_net(a), old);
        assert_eq!(topology.wire_net(b), old);
        assert_ne!(topology.wire_net(c), old);
    }

    #[test]
    fn removing_articulation_wire_can_create_more_than_two_nets() {
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();
        let center = wire(1);
        let branches = [wire(2), wire(3), wire(4)];

        add_wire(&mut network, &mut topology, center);
        for branch in branches {
            add_wire(&mut network, &mut topology, branch);
            connect(&mut network, &mut topology, center, branch);
        }

        network.remove_wire(center).unwrap();
        topology.remove_wire(&network, center);
        topology.assert_consistent(&network);

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

        network.disconnect_wires(a, b).unwrap();
        topology.disconnect_wires(&network, a, b);
        topology.assert_consistent(&network);

        assert_ne!(topology.wire_net(a), topology.wire_net(b));
        assert_eq!(
            topology.net(topology.wire_net(a)).unwrap().island(),
            Some(island)
        );
        assert_eq!(
            topology.net(topology.wire_net(b)).unwrap().island(),
            Some(island)
        );
        assert_eq!(topology.islands.len(), 1);
    }

    #[test]
    fn wire_disconnect_can_split_both_net_and_island() {
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
        let old_island = topology.device_island(da);
        assert_eq!(old_island, topology.device_island(db));

        network.disconnect_wires(a, b).unwrap();
        topology.disconnect_wires(&network, a, b);
        topology.assert_consistent(&network);

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

        network
            .detach_terminal(b, bridge, TerminalId::new(1))
            .unwrap();
        topology.detach_terminal(&network, b, bridge);
        topology.assert_consistent(&network);

        assert_eq!(topology.islands.len(), 2);
        assert_eq!(topology.device_island(left), topology.device_island(bridge));
        assert_ne!(topology.device_island(left), topology.device_island(right));
    }

    #[test]
    fn removing_multi_terminal_bridge_device_can_create_many_islands() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();
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

        network.remove_device(bridge).unwrap();
        topology.remove_device(&network, bridge);
        topology.assert_consistent(&network);

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
    fn topology_can_be_rebuilt_from_an_existing_network() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let a = wire(1);
        let b = wire(2);
        let d = device(1);

        network.add_wire(a).unwrap();
        network.add_wire(b).unwrap();
        network.connect_wires(a, b).unwrap();
        network
            .add_device(&definitions, d, PrimitiveElementKind::Admittance.into())
            .unwrap();
        network.attach_terminal(a, d, TerminalId::new(0)).unwrap();

        let topology = DerivedTopology::from_network(&network);
        topology.assert_consistent(&network);

        assert_eq!(topology.nets.len(), 1);
        assert_eq!(topology.islands.len(), 1);
    }

    #[test]
    fn stable_component_ids_are_one_based() {
        assert_eq!(NetId::try_from(1).unwrap().get(), 1);
        assert_eq!(IslandId::try_from(1).unwrap().get(), 1);
    }
}
