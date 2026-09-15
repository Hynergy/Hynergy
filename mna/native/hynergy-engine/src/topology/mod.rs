mod store;
mod traversal;
#[cfg(any(test, debug_assertions))]
mod validate;

use hynergy_ids::define_non_zero_id;
use hynergy_model::device::definition::{
    DeviceDefinition, DeviceId, DevicePartitionId, TerminalId,
};
use hynergy_model::device::registry::DefinitionRegistry;
use hynergy_model::network::{Network, WireId};
use smallvec::{SmallVec, smallvec};
use store::{DenseId, DenseIdStore};
use traversal::wire_components;

use crate::topology::traversal::island_components;
pub(crate) use traversal::TraversalScratch;

define_non_zero_id!(NetId, IslandId);

impl DenseId for NetId {
    #[inline]
    fn from_slot(slot: usize) -> Self {
        assert!(
            slot < hynergy_ids::MAX_PACKED_ID as usize,
            "31-bit NetId space exhausted"
        );

        let raw = u32::try_from(slot + 1).expect("31-bit NetId space exhausted");

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct DeviceComponent {
    device: DeviceId,
    partition: DevicePartitionId,
}

impl DeviceComponent {
    #[inline]
    pub(crate) const fn new(device: DeviceId, partition: DevicePartitionId) -> Self {
        Self { device, partition }
    }

    #[inline]
    pub(crate) const fn device(self) -> DeviceId {
        self.device
    }

    #[inline]
    pub(crate) const fn partition(self) -> DevicePartitionId {
        self.partition
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DeviceComponentSpan {
    start_plus_one: std::num::NonZeroUsize,
    len: usize,
}

impl DeviceComponentSpan {
    #[inline]
    fn new(start: usize, len: usize) -> Self {
        let start_plus_one = start
            .checked_add(1)
            .and_then(std::num::NonZeroUsize::new)
            .expect("device component index space exhausted");

        Self {
            start_plus_one,
            len,
        }
    }

    #[inline]
    fn start(self) -> usize {
        self.start_plus_one.get() - 1
    }

    #[inline]
    fn len(self) -> usize {
        self.len
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct DerivedTopology {
    nets: DenseIdStore<NetId, Net>,
    wire_net_map: Vec<Option<NetId>>,

    islands: DenseIdStore<IslandId, IslandTopology>,
    net_island_map: Vec<Option<IslandId>>,

    device_component_spans: Vec<Option<DeviceComponentSpan>>,
    component_island_map: Vec<Option<IslandId>>,

    invalidation: WorldInvalidation,
}

impl PartialEq for DerivedTopology {
    fn eq(&self, other: &Self) -> bool {
        self.nets == other.nets
            && self.wire_net_map == other.wire_net_map
            && self.islands == other.islands
            && self.net_island_map == other.net_island_map
            && self.device_component_spans == other.device_component_spans
            && self.component_island_map == other.component_island_map
    }
}

impl DerivedTopology {
    #[allow(dead_code)]
    pub(crate) fn from_network(network: &Network, definitions: &DefinitionRegistry) -> Self {
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
                    topology.connect_wires(definitions, network, wire, other);
                }
            }
        }

        for (index, slot) in network.devices().iter().enumerate() {
            if slot.is_none() {
                continue;
            }

            let device = device_id(index);

            let definition_id = network
                .device_definition_id(device)
                .expect("live Network device must have a definition");

            let definition = definitions
                .get(definition_id)
                .expect("live Network device definition must remain registered");

            topology.add_device(device, definition);
        }

        for (index, slot) in network.devices().iter().enumerate() {
            let Some(device_slot) = slot else {
                continue;
            };
            let device = device_id(index);

            for (terminal_index, connection) in device_slot.terminals().iter().enumerate() {
                let Some(connection) = *connection else {
                    continue;
                };

                let terminal = TerminalId::new(
                    u32::try_from(terminal_index).expect("terminal index must fit TerminalId"),
                );

                if let Some(wire) = connection.as_wire() {
                    topology.attach_terminal(definitions, network, wire, device, terminal);
                    continue;
                }

                if let Some((other_device, other_terminal)) = connection.as_terminal() {
                    let this_key = (device.index(), terminal.index());
                    let other_key = (other_device.index(), other_terminal.index());

                    if this_key < other_key {
                        let component_a =
                            terminal_component(definitions, network, device, terminal);

                        let component_b =
                            terminal_component(definitions, network, other_device, other_terminal);

                        topology.connect_components(definitions, network, component_a, component_b);
                    }
                }
            }
        }

        topology.invalidation.clear();

        let live_islands: Vec<_> = topology
            .islands
            .iter()
            .map(|(island_id, _)| island_id)
            .collect();

        for island_id in live_islands {
            topology.invalidation.mark_topology_dirty(island_id);
        }

        #[cfg(any(test, debug_assertions))]
        topology.assert_consistent(definitions, network);

        topology
    }

    #[inline]
    pub(crate) fn add_wire(&mut self, wire: WireId) {
        ensure_slot(&mut self.wire_net_map, wire.index());
        debug_assert!(self.wire_net_map[wire.index()].is_none());

        let net_id = self.nets.insert(Net {
            wires: smallvec![wire],
            terminal_components: SmallVec::new(),
        });

        debug_assert_eq!(net_id.index(), self.net_island_map.len());
        self.net_island_map.push(None);

        self.wire_net_map[wire.index()] = Some(net_id);
    }

    pub(crate) fn add_device(&mut self, device: DeviceId, definition: &DeviceDefinition) {
        ensure_slot(&mut self.device_component_spans, device.index());

        debug_assert!(self.device_component_spans[device.index()].is_none());

        let start = self.component_island_map.len();
        let partition_count = definition.partition_count();

        self.device_component_spans[device.index()] =
            Some(DeviceComponentSpan::new(start, partition_count));

        for partition_index in 0..partition_count {
            let partition = DevicePartitionId::new(
                u16::try_from(partition_index)
                    .expect("device partition index fits DevicePartitionId"),
            );

            let component = DeviceComponent::new(device, partition);

            let island_id = self.islands.insert(IslandTopology {
                components: smallvec![component],
                revision: 0,
            });

            self.component_island_map.push(Some(island_id));

            self.invalidation.mark_topology_dirty(island_id);

            debug_assert_eq!(
                self.component_island(DeviceComponent::new(device, partition)),
                island_id,
            );
        }
    }

    pub(crate) fn connect_wires(
        &mut self,
        definitions: &DefinitionRegistry,
        network: &Network,
        wire_a: WireId,
        wire_b: WireId,
    ) {
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
            (Some(a), Some(b)) => Some(self.merge_islands(definitions, network, a, b)),
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

        let dst_net = self
            .nets
            .get_mut(dst_net_id)
            .expect("destination net must survive source removal");

        dst_net.wires.extend(src_net.wires);
        dst_net
            .terminal_components
            .extend(src_net.terminal_components);

        if bump_final_island {
            let island = final_island.expect("same-island net merge must have an island");

            self.islands
                .get_mut(island)
                .expect("merged net must reference a live island")
                .bump_revision();

            self.invalidation.mark_topology_dirty(island);
        }
    }

    pub(crate) fn attach_terminal(
        &mut self,
        definitions: &DefinitionRegistry,
        network: &Network,
        wire: WireId,
        device: DeviceId,
        terminal: TerminalId,
    ) {
        let net_id = self.wire_net(wire);

        let component = terminal_component(definitions, network, device, terminal);

        let component_island = self.component_island(component);

        self.nets
            .get_mut(net_id)
            .expect("attached wire must reference a live net")
            .terminal_components
            .push(component);

        match self.net_island(net_id) {
            None => {
                self.net_island_map[net_id.index()] = Some(component_island);

                self.islands
                    .get_mut(component_island)
                    .expect("component must reference a live island")
                    .bump_revision();

                self.invalidation.mark_topology_dirty(component_island);
            }

            Some(island) if island == component_island => {
                self.islands
                    .get_mut(island)
                    .expect("component/net island must be live")
                    .bump_revision();

                self.invalidation.mark_topology_dirty(island);
            }

            Some(island) => {
                self.merge_islands(definitions, network, island, component_island);
            }
        }
    }

    pub(crate) fn detach_terminal(
        &mut self,
        definitions: &DefinitionRegistry,
        network: &Network,
        scratch: &mut TraversalScratch,
        wire: WireId,
        device: DeviceId,
        terminal: TerminalId,
    ) {
        let net_id = self.wire_net(wire);

        let device_component = terminal_component(definitions, network, device, terminal);

        let island_id = self.component_island(device_component);

        debug_assert_eq!(
            self.net_island(net_id),
            Some(island_id),
            "detached terminal's net and component must have belonged to the same island",
        );

        {
            let net = self
                .nets
                .get_mut(net_id)
                .expect("detached wire must reference a live net");

            let position = net
                .terminal_components
                .iter()
                .position(|&member| member == device_component)
                .expect("net must contain an incidence for the detached component");

            net.terminal_components.swap_remove(position);
        }

        self.repair_island(
            definitions,
            network,
            scratch,
            island_id,
            std::slice::from_ref(&net_id),
        );
    }

    pub(crate) fn disconnect_wires(
        &mut self,
        definitions: &DefinitionRegistry,
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
        let affected_nets = self.repair_net(definitions, network, scratch, net_a);

        if affected_nets.len() > 1
            && let Some(island_id) = island_id
            && self.islands.get(island_id).is_some()
        {
            self.repair_island(definitions, network, scratch, island_id, &affected_nets);
        }
    }

    pub(crate) fn remove_wire(
        &mut self,
        definitions: &DefinitionRegistry,
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
        let affected_nets = self.repair_net(definitions, network, scratch, net_id);

        if let Some(island_id) = island_id
            && self.islands.get(island_id).is_some()
        {
            self.repair_island(definitions, network, scratch, island_id, &affected_nets);
        }
    }

    pub(crate) fn remove_device(
        &mut self,
        definitions: &DefinitionRegistry,
        network: &Network,
        scratch: &mut TraversalScratch,
        device: DeviceId,
        affected_nets: &[NetId],
    ) {
        for &net_id in affected_nets {
            let net = self
                .nets
                .get_mut(net_id)
                .expect("removed device's attached net must remain live");

            let position = net
                .terminal_components
                .iter()
                .position(|member| member.device() == device)
                .expect("attached net must contain removed device component incidence");

            net.terminal_components.swap_remove(position);
        }

        let span = self
            .device_component_spans
            .get_mut(device.index())
            .and_then(Option::take)
            .expect("removed model device must still have a component span before repair");

        let end = span
            .start()
            .checked_add(span.len())
            .expect("device component index overflow");

        let mut affected_islands = SmallVec::<[IslandId; 4]>::new();

        for component_index in span.start()..end {
            let island_id = self.component_island_map[component_index]
                .take()
                .expect("removed component must still have a derived IslandId");

            if !affected_islands.contains(&island_id) {
                affected_islands.push(island_id);
            }
        }

        for &island_id in &affected_islands {
            let island = self
                .islands
                .get_mut(island_id)
                .expect("removed component must reference a live island");

            island
                .components
                .retain(|component| component.device() != device);
        }

        for island_id in affected_islands {
            let mut island_nets = SmallVec::<[NetId; 4]>::new();

            for &net_id in affected_nets {
                if self.net_island(net_id) == Some(island_id) && !island_nets.contains(&net_id) {
                    island_nets.push(net_id);
                }
            }

            self.repair_island(definitions, network, scratch, island_id, &island_nets);
        }
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
        definitions: &DefinitionRegistry,
        network: &Network,
        scratch: &mut TraversalScratch,
        net_id: NetId,
    ) -> SmallVec<[NetId; 4]> {
        let old_island = self.net_island(net_id);

        let mut components = wire_components(self, definitions, network, net_id, scratch);

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
            .max_by_key(|(_, component)| component.wires.len())
            .map(|(index, _)| index)
            .expect("non-empty component list must have a largest member");

        let kept = components.swap_remove(keep_index);

        for &wire in &kept.wires {
            self.wire_net_map[wire.index()] = Some(net_id);
        }

        {
            let net = self
                .nets
                .get_mut(net_id)
                .expect("old NetId must survive a non-empty repartition");

            net.wires = kept.wires;
            net.terminal_components = kept.terminal_components;
        }

        let mut resulting_nets = SmallVec::with_capacity(components.len() + 1);
        resulting_nets.push(net_id);

        for component in components {
            let new_id = self.nets.insert(Net {
                wires: component.wires,
                terminal_components: component.terminal_components,
            });

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
        definitions: &DefinitionRegistry,
        network: &Network,
        scratch: &mut TraversalScratch,
        island_id: IslandId,
        affected_nets: &[NetId],
    ) {
        let old_revision = match self.islands.get(island_id) {
            Some(island) => island.revision,
            None => return,
        };

        let mut pieces = island_components(
            self,
            definitions,
            network,
            scratch,
            island_id,
            affected_nets,
        );

        let mut index = 0;

        while index < pieces.len() {
            if !pieces[index].components.is_empty() {
                index += 1;
                continue;
            }

            let piece = pieces.swap_remove(index);

            for net_id in piece.nets {
                debug_assert_eq!(
                    self.net_island(net_id),
                    Some(island_id),
                    "component-free net must still belong to the island being repaired",
                );

                self.net_island_map[net_id.index()] = None;
            }
        }

        if pieces.is_empty() {
            self.islands
                .remove(island_id)
                .expect("empty repaired island must still be live");

            self.invalidation.mark_retired(island_id);
            return;
        }

        let keep_index = pieces
            .iter()
            .enumerate()
            .max_by_key(|(_, piece)| piece.rewrite_cost())
            .map(|(index, _)| index)
            .expect("non-empty island component list must have a largest member");

        let kept = pieces.swap_remove(keep_index);

        let next_revision = old_revision
            .checked_add(1)
            .expect("island revision exhausted u64 range");

        {
            let island = self
                .islands
                .get_mut(island_id)
                .expect("old IslandId must survive a non-empty repartition");

            island.components = kept.components;
            island.revision = next_revision;
        }

        self.invalidation.mark_topology_dirty(island_id);

        for piece in pieces {
            let component_indices: Vec<_> = piece
                .components
                .iter()
                .map(|&component| self.component_index(component))
                .collect();

            let new_island_id = self.islands.insert(IslandTopology {
                components: piece.components,
                revision: 0,
            });

            for net_id in piece.nets {
                debug_assert_eq!(self.net_island(net_id), Some(island_id),);

                self.net_island_map[net_id.index()] = Some(new_island_id);
            }

            for component_index in component_indices {
                debug_assert_eq!(
                    self.component_island_map[component_index],
                    Some(island_id),
                    "split component must still reference the old island",
                );

                self.component_island_map[component_index] = Some(new_island_id);
            }

            self.invalidation.mark_topology_dirty(new_island_id);
        }
    }

    fn merge_islands(
        &mut self,
        definitions: &DefinitionRegistry,
        network: &Network,
        island_a: IslandId,
        island_b: IslandId,
    ) -> IslandId {
        if island_a == island_b {
            return island_a;
        }

        let a_cost = self
            .islands
            .get(island_a)
            .expect("merge source island must be live")
            .components
            .len();

        let b_cost = self
            .islands
            .get(island_b)
            .expect("merge source island must be live")
            .components
            .len();

        let (dst_id, src_id) = if a_cost >= b_cost {
            (island_a, island_b)
        } else {
            (island_b, island_a)
        };

        let src = self
            .islands
            .remove(src_id)
            .expect("source island must remain live until merge");

        for &component in &src.components {
            let component_index = self.component_index(component);

            debug_assert_eq!(
                self.component_island_map[component_index],
                Some(src_id),
                "moved component must still reference the source island",
            );

            self.component_island_map[component_index] = Some(dst_id);

            let device = component.device();

            let definition_id = network
                .device_definition_id(device)
                .expect("island component device must have a definition");

            let definition = definitions
                .get(definition_id)
                .expect("island component definition must remain registered");

            let device_slot = network.devices()[device.index()]
                .as_ref()
                .expect("island component device must be live");

            for (terminal_index, connection) in device_slot.terminals().iter().enumerate() {
                if definition.terminal_partitions()[terminal_index] != component.partition() {
                    continue;
                }

                let Some(connection) = *connection else {
                    continue;
                };

                let Some(wire) = connection.as_wire() else {
                    continue;
                };

                let net_id = self.wire_net(wire);

                if self.net_island(net_id) == Some(src_id) {
                    self.net_island_map[net_id.index()] = Some(dst_id);
                }
            }
        }

        let dst = self
            .islands
            .get_mut(dst_id)
            .expect("destination island must survive source removal");

        dst.components.extend(src.components);
        dst.bump_revision();

        self.invalidation.mark_retired(src_id);
        self.invalidation.mark_topology_dirty(dst_id);

        dst_id
    }

    fn connect_components(
        &mut self,
        definitions: &DefinitionRegistry,
        network: &Network,
        component_a: DeviceComponent,
        component_b: DeviceComponent,
    ) {
        let island_a = self.component_island(component_a);
        let island_b = self.component_island(component_b);

        if island_a != island_b {
            self.merge_islands(definitions, network, island_a, island_b);
        }
    }

    #[inline]
    fn wire_net(&self, wire: WireId) -> NetId {
        self.wire_net_map[wire.index()].expect("live wire must have a derived NetId")
    }

    #[inline]
    fn component_index(&self, component: DeviceComponent) -> usize {
        let span = self
            .device_component_spans
            .get(component.device().index())
            .copied()
            .flatten()
            .expect("live device must have a component span");

        let partition_index = component.partition().index();

        assert!(
            partition_index < span.len(),
            "component partition is out of range",
        );

        span.start()
            .checked_add(partition_index)
            .expect("device component index overflow")
    }

    #[inline]
    pub(crate) fn component_island(&self, component: DeviceComponent) -> IslandId {
        self.component_island_map[self.component_index(component)]
            .expect("live component must have a derived IslandId")
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

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn invalidation(&self) -> &WorldInvalidation {
        &self.invalidation
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn clear_invalidation(&mut self) {
        self.invalidation.clear();
    }

    #[inline]
    pub(crate) fn mark_device_numerical_dirty(&mut self, device: DeviceId) {
        let span = self.device_component_spans[device.index()]
            .expect("live device must have a component span");

        let end = span
            .start()
            .checked_add(span.len())
            .expect("device component index overflow");

        for component_index in span.start()..end {
            let Some(island_id) = self.component_island_map[component_index] else {
                continue;
            };

            self.invalidation.mark_numerical_dirty(island_id);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IslandTopology {
    components: SmallVec<[DeviceComponent; 2]>,
    revision: u64,
}

impl IslandTopology {
    #[inline]
    fn bump_revision(&mut self) {
        self.revision += 1;
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn components(&self) -> &[DeviceComponent] {
        &self.components
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Net {
    wires: SmallVec<[WireId; 4]>,
    terminal_components: SmallVec<[DeviceComponent; 2]>,
}

impl Net {
    #[allow(dead_code)]
    #[inline]
    pub(crate) fn wires(&self) -> &[WireId] {
        &self.wires
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn terminal_components(&self) -> &[DeviceComponent] {
        &self.terminal_components
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

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct WorldInvalidation {
    topology_dirty: SmallVec<[IslandId; 4]>,
    numerical_dirty: SmallVec<[IslandId; 4]>,
    retired: SmallVec<[IslandId; 4]>,
}

impl WorldInvalidation {
    #[inline]
    fn mark_topology_dirty(&mut self, island: IslandId) {
        if self.retired.contains(&island) {
            return;
        }

        self.numerical_dirty.retain(|dirty| *dirty != island);

        if !self.topology_dirty.contains(&island) {
            self.topology_dirty.push(island);
        }
    }

    #[inline]
    fn mark_numerical_dirty(&mut self, island: IslandId) {
        if self.retired.contains(&island) || self.topology_dirty.contains(&island) {
            return;
        }

        if !self.numerical_dirty.contains(&island) {
            self.numerical_dirty.push(island);
        }
    }

    #[inline]
    fn mark_retired(&mut self, island: IslandId) {
        self.topology_dirty.retain(|dirty| *dirty != island);
        self.numerical_dirty.retain(|dirty| *dirty != island);

        if !self.retired.contains(&island) {
            self.retired.push(island);
        }
    }

    #[inline]
    fn clear(&mut self) {
        self.topology_dirty.clear();
        self.numerical_dirty.clear();
        self.retired.clear();
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn topology_dirty_islands(&self) -> &[IslandId] {
        &self.topology_dirty
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn numerical_dirty_islands(&self) -> &[IslandId] {
        &self.numerical_dirty
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn retired_islands(&self) -> &[IslandId] {
        &self.retired
    }
}

#[inline]
fn terminal_component(
    definitions: &DefinitionRegistry,
    network: &Network,
    device: DeviceId,
    terminal: TerminalId,
) -> DeviceComponent {
    let definition_id = network
        .device_definition_id(device)
        .expect("topology device must exist in Network");

    let definition = definitions
        .get(definition_id)
        .expect("topology device definition must remain registered");

    let partition = *definition
        .terminal_partitions()
        .get(terminal.index())
        .expect("topology terminal must exist in its definition");

    DeviceComponent::new(device, partition)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hynergy_model::device::definition::{DefinitionId, DevicePartitionId};
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
    }

    fn add_device(
        network: &mut Network,
        topology: &mut DerivedTopology,
        definitions: &DefinitionRegistry,
        id: DeviceId,
        kind: PrimitiveElementKind,
    ) {
        let definition_id = kind.into();

        network.add_device(definitions, id, definition_id).unwrap();

        let definition = definitions.get(definition_id).unwrap();

        topology.add_device(id, definition);
    }

    fn connect(
        definitions: &DefinitionRegistry,
        network: &mut Network,
        topology: &mut DerivedTopology,
        a: WireId,
        b: WireId,
    ) {
        network.connect_wires(a, b).unwrap();
        topology.connect_wires(definitions, network, a, b);
    }

    fn disconnect(
        definitions: &DefinitionRegistry,
        network: &mut Network,
        topology: &mut DerivedTopology,
        scratch: &mut TraversalScratch,
        a: WireId,
        b: WireId,
    ) {
        network.disconnect_wires(a, b).unwrap();
        topology.disconnect_wires(definitions, network, scratch, a, b);
    }

    fn attach(
        definitions: &DefinitionRegistry,
        network: &mut Network,
        topology: &mut DerivedTopology,
        wire: WireId,
        device: DeviceId,
        terminal: u32,
    ) {
        let terminal = TerminalId::new(terminal);

        network.attach_terminal(wire, device, terminal).unwrap();

        topology.attach_terminal(definitions, network, wire, device, terminal);
    }

    fn detach(
        definitions: &DefinitionRegistry,
        network: &mut Network,
        topology: &mut DerivedTopology,
        scratch: &mut TraversalScratch,
        wire: WireId,
        device: DeviceId,
        terminal: u32,
    ) {
        let terminal_id = TerminalId::new(terminal);

        network
            .detach_terminal(wire, device, TerminalId::new(terminal))
            .unwrap();

        topology.detach_terminal(definitions, network, scratch, wire, device, terminal_id);
    }

    fn remove_wire(
        definitions: &DefinitionRegistry,
        network: &mut Network,
        topology: &mut DerivedTopology,
        scratch: &mut TraversalScratch,
        wire: WireId,
    ) {
        network.remove_wire(wire).unwrap();
        topology.remove_wire(definitions, network, scratch, wire);
    }

    fn remove_device(
        definitions: &DefinitionRegistry,
        network: &mut Network,
        topology: &mut DerivedTopology,
        scratch: &mut TraversalScratch,
        device: DeviceId,
    ) -> Vec<NetId> {
        let affected_nets = topology.device_nets(network, device);

        network.remove_device(device).unwrap();

        topology.remove_device(definitions, network, scratch, device, &affected_nets);

        affected_nets
    }

    fn device_component(device: DeviceId, partition: u16) -> DeviceComponent {
        DeviceComponent::new(device, DevicePartitionId::new(partition))
    }

    fn device_island(topology: &DerivedTopology, device: DeviceId) -> IslandId {
        topology.component_island(device_component(device, 0))
    }

    #[test]
    fn device_component_stays_compact() {
        use std::mem::size_of;

        assert_eq!(size_of::<DeviceComponent>(), 8);
    }

    #[test]
    fn dense_relocation_does_not_change_unrelated_net_id() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();

        let a = wire(1);
        let b = wire(2);
        let c = wire(3);

        add_wire(&mut network, &mut topology, a);
        add_wire(&mut network, &mut topology, b);
        add_wire(&mut network, &mut topology, c);

        let c_id = topology.wire_net(c);

        connect(&definitions, &mut network, &mut topology, a, b);

        assert_eq!(topology.wire_net(c), c_id);
        assert!(topology.net(c_id).is_some());
    }

    #[test]
    fn connecting_islandless_wires_merges_only_their_nets() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();

        let a = wire(1);
        let b = wire(2);

        add_wire(&mut network, &mut topology, a);
        add_wire(&mut network, &mut topology, b);

        connect(&definitions, &mut network, &mut topology, a, b);

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
            PrimitiveElementKind::Conductance,
        );

        attach(&definitions, &mut network, &mut topology, w, d, 0);

        let net = topology.wire_net(w);
        let island = device_island(&topology, d);

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
            PrimitiveElementKind::Conductance,
        );

        add_device(
            &mut network,
            &mut topology,
            &definitions,
            db,
            PrimitiveElementKind::Conductance,
        );

        attach(&definitions, &mut network, &mut topology, a, da, 0);

        attach(&definitions, &mut network, &mut topology, b, db, 0);

        connect(&definitions, &mut network, &mut topology, a, b);

        assert_eq!(topology.wire_net(a), topology.wire_net(b));
        assert_eq!(topology.nets.len(), 1);
        assert_eq!(topology.islands.len(), 1);
    }

    #[test]
    fn redundant_wire_edge_deletion_does_not_split_net() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();
        let mut scratch = TraversalScratch::default();

        let a = wire(1);
        let b = wire(2);
        let c = wire(3);

        for id in [a, b, c] {
            add_wire(&mut network, &mut topology, id);
        }

        connect(&definitions, &mut network, &mut topology, a, b);
        connect(&definitions, &mut network, &mut topology, b, c);
        connect(&definitions, &mut network, &mut topology, a, c);

        let old = topology.wire_net(a);

        disconnect(
            &definitions,
            &mut network,
            &mut topology,
            &mut scratch,
            a,
            c,
        );

        assert_eq!(topology.nets.len(), 1);
        assert_eq!(topology.wire_net(a), old);
        assert_eq!(topology.wire_net(b), old);
        assert_eq!(topology.wire_net(c), old);
    }

    #[test]
    fn bridge_deletion_splits_net_and_keeps_old_id_on_larger_side() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();
        let mut scratch = TraversalScratch::default();

        let a = wire(1);
        let b = wire(2);
        let c = wire(3);

        for id in [a, b, c] {
            add_wire(&mut network, &mut topology, id);
        }

        connect(&definitions, &mut network, &mut topology, a, b);
        connect(&definitions, &mut network, &mut topology, b, c);

        let old = topology.wire_net(a);

        disconnect(
            &definitions,
            &mut network,
            &mut topology,
            &mut scratch,
            b,
            c,
        );

        assert_eq!(topology.nets.len(), 2);
        assert_eq!(topology.wire_net(a), old);
        assert_eq!(topology.wire_net(b), old);
        assert_ne!(topology.wire_net(c), old);
    }

    #[test]
    fn removing_articulation_wire_can_create_more_than_two_nets() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();
        let mut scratch = TraversalScratch::default();

        let center = wire(1);
        let branches = [wire(2), wire(3), wire(4)];

        add_wire(&mut network, &mut topology, center);

        for branch in branches {
            add_wire(&mut network, &mut topology, branch);
            connect(&definitions, &mut network, &mut topology, center, branch);
        }

        remove_wire(
            &definitions,
            &mut network,
            &mut topology,
            &mut scratch,
            center,
        );

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

        connect(&definitions, &mut network, &mut topology, a, b);

        add_device(
            &mut network,
            &mut topology,
            &definitions,
            bridge,
            PrimitiveElementKind::Conductance,
        );

        attach(&definitions, &mut network, &mut topology, a, bridge, 0);
        attach(&definitions, &mut network, &mut topology, b, bridge, 1);

        let island = device_island(&topology, bridge);

        disconnect(
            &definitions,
            &mut network,
            &mut topology,
            &mut scratch,
            a,
            b,
        );

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
            PrimitiveElementKind::Conductance,
        );

        add_device(
            &mut network,
            &mut topology,
            &definitions,
            db,
            PrimitiveElementKind::Conductance,
        );

        attach(&definitions, &mut network, &mut topology, a, da, 0);

        attach(&definitions, &mut network, &mut topology, b, db, 0);

        connect(&definitions, &mut network, &mut topology, a, b);

        let old_island = device_island(&topology, da);

        assert_eq!(old_island, device_island(&topology, db));

        disconnect(
            &definitions,
            &mut network,
            &mut topology,
            &mut scratch,
            a,
            b,
        );

        assert_ne!(topology.wire_net(a), topology.wire_net(b));

        assert_ne!(device_island(&topology, da), device_island(&topology, db));

        assert!(
            device_island(&topology, da) == old_island
                || device_island(&topology, db) == old_island
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
                PrimitiveElementKind::Conductance,
            );
        }

        attach(&definitions, &mut network, &mut topology, a, left, 0);
        attach(&definitions, &mut network, &mut topology, b, right, 0);
        attach(&definitions, &mut network, &mut topology, a, bridge, 0);
        attach(&definitions, &mut network, &mut topology, b, bridge, 1);

        assert_eq!(topology.islands.len(), 1);

        detach(
            &definitions,
            &mut network,
            &mut topology,
            &mut scratch,
            b,
            bridge,
            1,
        );

        assert_eq!(topology.islands.len(), 2);

        assert_eq!(
            device_island(&topology, left),
            device_island(&topology, bridge)
        );

        assert_ne!(
            device_island(&topology, left),
            device_island(&topology, right)
        );
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
                PrimitiveElementKind::Conductance,
            );
        }

        add_device(
            &mut network,
            &mut topology,
            &definitions,
            bridge,
            PrimitiveElementKind::VoltageControlledCurrentSource,
        );

        for (index, (&wire_id, &leaf)) in wires.iter().zip(leaves.iter()).enumerate() {
            attach(&definitions, &mut network, &mut topology, wire_id, leaf, 0);

            attach(
                &definitions,
                &mut network,
                &mut topology,
                wire_id,
                bridge,
                u32::try_from(index).unwrap(),
            );
        }

        assert_eq!(topology.islands.len(), 1);

        let old_island = device_island(&topology, bridge);

        let affected_nets = remove_device(
            &definitions,
            &mut network,
            &mut topology,
            &mut scratch,
            bridge,
        );

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
            device_island(&topology, leaves[0]),
            device_island(&topology, leaves[1])
        );

        assert_ne!(
            device_island(&topology, leaves[1]),
            device_island(&topology, leaves[2])
        );

        assert_ne!(
            device_island(&topology, leaves[0]),
            device_island(&topology, leaves[2])
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
            PrimitiveElementKind::VoltageControlledCurrentSource,
        );

        for (terminal, wire) in wires.into_iter().enumerate() {
            attach(
                &definitions,
                &mut network,
                &mut topology,
                wire,
                bridge,
                u32::try_from(terminal).unwrap(),
            );
        }

        let nets = wires.map(|wire| topology.wire_net(wire));
        let old_island = device_island(&topology, bridge);

        assert_eq!(topology.islands.len(), 1);

        let affected_nets = remove_device(
            &definitions,
            &mut network,
            &mut topology,
            &mut scratch,
            bridge,
        );

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
            PrimitiveElementKind::Conductance,
        );

        attach(&definitions, &mut network, &mut topology, w, d, 0);

        let net_id = topology.wire_net(w);
        let island_id = device_island(&topology, d);

        assert_eq!(topology.net_island(net_id), Some(island_id));

        detach(
            &definitions,
            &mut network,
            &mut topology,
            &mut scratch,
            w,
            d,
            0,
        );

        assert_eq!(device_island(&topology, d), island_id);

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
            PrimitiveElementKind::Conductance,
        );

        attach(&definitions, &mut network, &mut topology, w, d, 0);

        let net_id = topology.wire_net(w);
        let island_id = device_island(&topology, d);

        remove_wire(&definitions, &mut network, &mut topology, &mut scratch, w);

        assert!(topology.net(net_id).is_none());

        assert_eq!(topology.net_island_map[net_id.index()], None);

        assert_eq!(device_island(&topology, d), island_id);

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
            PrimitiveElementKind::Conductance,
        );

        attach(&definitions, &mut network, &mut topology, a, d, 0);
        attach(&definitions, &mut network, &mut topology, b, d, 1);

        assert_ne!(topology.wire_net(a), topology.wire_net(b));

        let island_id = device_island(&topology, d);

        let old_revision = topology.island(island_id).unwrap().revision();

        connect(&definitions, &mut network, &mut topology, a, b);

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

        connect(&definitions, &mut network, &mut topology, a, b);

        for device in [left_a, left_b, right] {
            add_device(
                &mut network,
                &mut topology,
                &definitions,
                device,
                PrimitiveElementKind::Conductance,
            );
        }

        attach(&definitions, &mut network, &mut topology, a, left_a, 0);
        attach(&definitions, &mut network, &mut topology, a, left_b, 0);
        attach(&definitions, &mut network, &mut topology, b, right, 0);

        let old_island = device_island(&topology, left_a);

        assert_eq!(device_island(&topology, left_b), old_island);

        assert_eq!(device_island(&topology, right), old_island);

        disconnect(
            &definitions,
            &mut network,
            &mut topology,
            &mut scratch,
            a,
            b,
        );

        assert_eq!(device_island(&topology, left_a), old_island);

        assert_eq!(device_island(&topology, left_b), old_island);

        assert_ne!(device_island(&topology, right), old_island);
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
                    PrimitiveElementKind::Conductance.into(),
                )
                .unwrap();
        }

        network
            .add_device(
                &definitions,
                bridge,
                PrimitiveElementKind::VoltageControlledCurrentSource.into(),
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

        let topology = DerivedTopology::from_network(&network, &definitions);

        topology.assert_consistent(&definitions, &network);

        assert_eq!(topology.nets.len(), 3);
        assert_eq!(topology.islands.len(), 1);

        let island = device_island(&topology, bridge);

        for leaf in leaves {
            assert_eq!(device_island(&topology, leaf), island);
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

    #[test]
    fn net_terminal_incidence_preserves_device_multiplicity() {
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
            PrimitiveElementKind::Conductance,
        );

        attach(&definitions, &mut network, &mut topology, w, d, 0);
        attach(&definitions, &mut network, &mut topology, w, d, 1);

        let net_id = topology.wire_net(w);

        assert_eq!(
            topology
                .net(net_id)
                .unwrap()
                .terminal_components
                .iter()
                .filter(|&&device_component| device_component.device == d)
                .count(),
            2
        );

        detach(
            &definitions,
            &mut network,
            &mut topology,
            &mut scratch,
            w,
            d,
            0,
        );

        assert_eq!(
            topology
                .net(net_id)
                .unwrap()
                .terminal_components
                .iter()
                .filter(|&&device_component| device_component.device == d)
                .count(),
            1
        );

        assert_eq!(
            topology.net_island(net_id),
            Some(device_island(&topology, d))
        );
    }

    #[test]
    fn island_merge_records_dirty_survivor_and_retired_source() {
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
            PrimitiveElementKind::Conductance,
        );
        add_device(
            &mut network,
            &mut topology,
            &definitions,
            db,
            PrimitiveElementKind::Conductance,
        );

        attach(&definitions, &mut network, &mut topology, a, da, 0);

        attach(&definitions, &mut network, &mut topology, b, db, 0);

        let old_a = device_island(&topology, da);
        let old_b = device_island(&topology, db);

        topology.clear_invalidation();

        connect(&definitions, &mut network, &mut topology, a, b);

        let survivor = device_island(&topology, da);
        assert_eq!(survivor, device_island(&topology, db));

        let retired = if survivor == old_a { old_b } else { old_a };

        assert_eq!(
            topology.invalidation().topology_dirty_islands(),
            &[survivor]
        );
        assert_eq!(topology.invalidation().retired_islands(), &[retired]);
        assert!(topology.invalidation().numerical_dirty_islands().is_empty());
    }

    #[test]
    fn device_component_round_trips_and_uses_option_niche() {
        use std::mem::size_of;

        let component = DeviceComponent::new(device(0x7fff_ffff), DevicePartitionId::new(u16::MAX));

        assert_eq!(component.device(), device(0x7fff_ffff));
        assert_eq!(component.partition(), DevicePartitionId::new(u16::MAX),);

        assert_eq!(size_of::<DeviceComponent>(), 8);
        assert_eq!(size_of::<Option<DeviceComponent>>(), 8);
    }

    #[test]
    fn terminal_component_uses_definition_partition_layout() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let delay = device(1);

        network
            .add_device(&definitions, delay, PrimitiveElementKind::TickDelay.into())
            .unwrap();

        for (terminal, partition) in [(0, 0), (1, 0), (2, 1), (3, 1)] {
            let component =
                terminal_component(&definitions, &network, delay, TerminalId::new(terminal));

            assert_eq!(component.device(), delay);
            assert_eq!(component.partition(), DevicePartitionId::new(partition),);
        }
    }
    #[test]
    fn device_component_option_has_no_extra_storage() {
        use std::mem::size_of;

        assert_eq!(
            size_of::<Option<DeviceComponent>>(),
            size_of::<DeviceComponent>(),
        );
    }

    #[test]
    fn multi_partition_device_starts_with_one_island_per_component() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();

        let delay = device(1);
        let definition_id = DefinitionId::from(PrimitiveElementKind::TickDelay);

        network
            .add_device(&definitions, delay, definition_id)
            .unwrap();

        let definition = definitions.get(definition_id).unwrap();

        topology.add_device(delay, definition);

        let input = DeviceComponent::new(delay, DevicePartitionId::new(0));
        let output = DeviceComponent::new(delay, DevicePartitionId::new(1));

        assert_ne!(
            topology.component_island(input),
            topology.component_island(output),
        );

        assert_eq!(topology.islands.len(), 2);
    }

    #[test]
    fn tick_delay_terminals_attach_to_their_component_islands() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();

        let input_wire = wire(1);
        let output_wire = wire(2);
        let delay = device(1);

        add_wire(&mut network, &mut topology, input_wire);
        add_wire(&mut network, &mut topology, output_wire);

        add_device(
            &mut network,
            &mut topology,
            &definitions,
            delay,
            PrimitiveElementKind::TickDelay,
        );

        let input_component = DeviceComponent::new(delay, DevicePartitionId::new(0));

        let output_component = DeviceComponent::new(delay, DevicePartitionId::new(1));

        let input_island = topology.component_island(input_component);
        let output_island = topology.component_island(output_component);

        assert_ne!(input_island, output_island);

        network
            .attach_terminal(input_wire, delay, TerminalId::new(0))
            .unwrap();

        topology.attach_terminal(
            &definitions,
            &network,
            input_wire,
            delay,
            TerminalId::new(0),
        );

        network
            .attach_terminal(output_wire, delay, TerminalId::new(2))
            .unwrap();

        topology.attach_terminal(
            &definitions,
            &network,
            output_wire,
            delay,
            TerminalId::new(2),
        );

        assert_eq!(
            topology.net_island(topology.wire_net(input_wire)),
            Some(input_island),
        );

        assert_eq!(
            topology.net_island(topology.wire_net(output_wire)),
            Some(output_island),
        );

        assert_ne!(
            topology.net_island(topology.wire_net(input_wire)),
            topology.net_island(topology.wire_net(output_wire)),
        );
    }

    #[test]
    fn multi_partition_device_islands_store_exact_components() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();

        let delay = device(1);

        add_device(
            &mut network,
            &mut topology,
            &definitions,
            delay,
            PrimitiveElementKind::TickDelay,
        );

        let input = DeviceComponent::new(delay, DevicePartitionId::new(0));
        let output = DeviceComponent::new(delay, DevicePartitionId::new(1));

        let input_island = topology.component_island(input);
        let output_island = topology.component_island(output);

        assert_eq!(
            topology.island(input_island).unwrap().components(),
            &[input],
        );

        assert_eq!(
            topology.island(output_island).unwrap().components(),
            &[output],
        );
    }

    #[test]
    fn net_terminal_incidence_preserves_device_partition() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut topology = DerivedTopology::default();

        let input_wire = wire(1);
        let output_wire = wire(2);
        let delay = device(1);

        add_wire(&mut network, &mut topology, input_wire);
        add_wire(&mut network, &mut topology, output_wire);

        add_device(
            &mut network,
            &mut topology,
            &definitions,
            delay,
            PrimitiveElementKind::TickDelay,
        );

        attach(
            &definitions,
            &mut network,
            &mut topology,
            input_wire,
            delay,
            0,
        );

        attach(
            &definitions,
            &mut network,
            &mut topology,
            output_wire,
            delay,
            2,
        );

        let input_component = DeviceComponent::new(delay, DevicePartitionId::new(0));

        let output_component = DeviceComponent::new(delay, DevicePartitionId::new(1));

        let input_net = topology.net(topology.wire_net(input_wire)).unwrap();

        let output_net = topology.net(topology.wire_net(output_wire)).unwrap();

        assert_eq!(input_net.terminal_components(), &[input_component],);

        assert_eq!(output_net.terminal_components(), &[output_component],);
    }
}
