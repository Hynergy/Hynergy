use super::{DerivedTopology, DeviceComponent, IslandId, NetId, terminal_component};
use hynergy_model::device::registry::DefinitionRegistry;
use hynergy_model::network::{Network, WireId};
use smallvec::SmallVec;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct IslandComponent {
    pub(super) nets: SmallVec<[NetId; 4]>,
    pub(super) components: SmallVec<[DeviceComponent; 2]>,
}

impl IslandComponent {
    #[inline]
    pub(super) fn rewrite_cost(&self) -> usize {
        self.nets.len() + self.components.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IslandVertex {
    Net(NetId),
    Component(DeviceComponent),
}

pub(super) fn wire_components(
    topology: &DerivedTopology,
    definitions: &DefinitionRegistry,
    network: &Network,
    net_id: NetId,
    scratch: &mut TraversalScratch,
) -> Vec<WireComponent> {
    scratch.begin_wire_traversal(network.wires().len());

    let candidates = &topology
        .nets
        .get(net_id)
        .expect("net repair requires a live net")
        .wires;

    let mut components = Vec::new();

    for &start in candidates {
        let index = start.index();

        if topology.wire_net_map[index] != Some(net_id) {
            continue;
        }

        if !scratch.visit_wire(index) {
            continue;
        }

        scratch.wire_stack.push(start);

        let mut component = WireComponent::default();

        while let Some(wire) = scratch.wire_stack.pop() {
            component.wires.push(wire);

            for connection in network
                .wire_connections(wire)
                .expect("visited repair wire must still be live")
            {
                if let Some(neighbor) = connection.as_wire() {
                    let index = neighbor.index();

                    debug_assert_eq!(
                        topology.wire_net_map[index],
                        Some(net_id),
                        "destructive mutation cannot create a cross-net wire edge"
                    );

                    if scratch.visit_wire(index) {
                        scratch.wire_stack.push(neighbor);
                    }

                    continue;
                }

                if let Some((device, terminal)) = connection.as_terminal() {
                    component.terminal_components.push(terminal_component(
                        definitions,
                        network,
                        device,
                        terminal,
                    ));
                }
            }
        }

        components.push(component);
    }

    components
}

pub(super) fn island_components(
    topology: &DerivedTopology,
    definitions: &DefinitionRegistry,
    network: &Network,
    scratch: &mut TraversalScratch,
    island_id: IslandId,
    affected_nets: &[NetId],
) -> Vec<IslandComponent> {
    scratch.begin_island_traversal(
        topology.nets.slot_count(),
        topology
            .device_chunks
            .iter()
            .map(|chunk| chunk.row_count() * chunk.partition_count()),
    );

    let island = topology
        .islands
        .get(island_id)
        .expect("island repair requires a live island");

    let mut pieces = Vec::new();

    for &device_component in &island.components {
        let (chunk_index, component_index) =
            topology.component_storage_index(network, device_component);

        if !scratch.visit_component(chunk_index, component_index) {
            continue;
        }

        debug_assert_eq!(
            topology.component_island(network, device_component,),
            island_id,
        );

        debug_assert!(network.device(device_component.device()).is_ok());
        scratch
            .island_stack
            .push(IslandVertex::Component(device_component));

        pieces.push(walk_island_component(
            topology,
            definitions,
            network,
            island_id,
            scratch,
        ));

        debug_assert!(scratch.island_stack.is_empty());
    }

    for &net_id in affected_nets {
        let index = net_id.index();

        if !scratch.visit_net(index) {
            continue;
        }

        debug_assert_eq!(topology.net_island_map[index], Some(island_id),);

        debug_assert!(topology.nets.get(net_id).is_some());

        scratch.island_stack.push(IslandVertex::Net(net_id));

        pieces.push(walk_island_component(
            topology,
            definitions,
            network,
            island_id,
            scratch,
        ));

        debug_assert!(scratch.island_stack.is_empty());
    }

    pieces
}

fn walk_island_component(
    topology: &DerivedTopology,
    definitions: &DefinitionRegistry,
    network: &Network,
    island_id: IslandId,
    scratch: &mut TraversalScratch,
) -> IslandComponent {
    let mut result = IslandComponent::default();

    while let Some(vertex) = scratch.island_stack.pop() {
        match vertex {
            IslandVertex::Net(net_id) => {
                result.nets.push(net_id);

                let net = topology
                    .nets
                    .get(net_id)
                    .expect("visited island net must be live");

                for &neighbor in &net.terminal_components {
                    let (chunk_index, component_index) =
                        topology.component_storage_index(network, neighbor);

                    if !scratch.visit_component(chunk_index, component_index) {
                        continue;
                    }

                    debug_assert_eq!(topology.component_island(network, neighbor), island_id,);

                    scratch.island_stack.push(IslandVertex::Component(neighbor));
                }
            }

            IslandVertex::Component(device_component) => {
                result.components.push(device_component);

                let device_id = device_component.device();

                let device = network
                    .device(device_id)
                    .expect("visited island component device must be live");

                let definition = definitions
                    .get(device.definition_id())
                    .expect("visited component definition must remain registered");

                for (terminal_index, connection) in device.terminals().iter().enumerate() {
                    if definition.terminal_partitions()[terminal_index]
                        != device_component.partition()
                    {
                        continue;
                    }

                    let Some(connection) = *connection else {
                        continue;
                    };

                    if let Some(wire_id) = connection.as_wire() {
                        let net_id = topology.wire_net_map[wire_id.index()]
                            .expect("live attached wire must have a NetId");

                        let index = net_id.index();

                        if !scratch.visit_net(index) {
                            continue;
                        }

                        debug_assert_eq!(topology.net_island_map[index], Some(island_id),);

                        scratch.island_stack.push(IslandVertex::Net(net_id));

                        continue;
                    }

                    if let Some((other_device, other_terminal)) = connection.as_terminal() {
                        let neighbor =
                            terminal_component(definitions, network, other_device, other_terminal);

                        let (chunk_index, component_index) =
                            topology.component_storage_index(network, neighbor);

                        if !scratch.visit_component(chunk_index, component_index) {
                            continue;
                        }

                        debug_assert_eq!(topology.component_island(network, neighbor), island_id,);

                        scratch.island_stack.push(IslandVertex::Component(neighbor));
                    }
                }
            }
        }
    }

    result
}

#[derive(Debug, Default)]
pub(crate) struct TraversalScratch {
    wire_seen: Vec<bool>,
    net_seen: Vec<bool>,

    component_seen: Vec<Vec<u64>>,

    touched_wires: Vec<usize>,
    touched_nets: Vec<usize>,
    touched_component_words: Vec<(usize, usize)>,

    wire_stack: Vec<WireId>,
    island_stack: Vec<IslandVertex>,
}

impl TraversalScratch {
    #[inline]
    fn reset_marks(seen: &mut Vec<bool>, touched: &mut Vec<usize>, required_len: usize) {
        for index in touched.drain(..) {
            seen[index] = false;
        }

        if seen.len() < required_len {
            seen.resize(required_len, false);
        }
    }

    #[inline]
    fn begin_wire_traversal(&mut self, wire_slots: usize) {
        Self::reset_marks(&mut self.wire_seen, &mut self.touched_wires, wire_slots);

        self.wire_stack.clear();
    }

    #[inline]
    fn begin_island_traversal(
        &mut self,
        net_slots: usize,
        component_slots: impl IntoIterator<Item = usize>,
    ) {
        Self::reset_marks(&mut self.net_seen, &mut self.touched_nets, net_slots);

        for (chunk_index, word_index) in self.touched_component_words.drain(..) {
            self.component_seen[chunk_index][word_index] = 0;
        }

        for (chunk_index, slot_count) in component_slots.into_iter().enumerate() {
            if self.component_seen.len() <= chunk_index {
                self.component_seen.resize_with(chunk_index + 1, Vec::new);
            }

            let required_words = slot_count.div_ceil(64);

            if self.component_seen[chunk_index].len() < required_words {
                self.component_seen[chunk_index].resize(required_words, 0);
            }
        }

        self.island_stack.clear();
    }

    #[inline]
    fn visit_component(&mut self, chunk_index: usize, index: usize) -> bool {
        let word_index = index / 64;
        let mask = 1_u64 << (index % 64);

        let word = self.component_seen[chunk_index][word_index];

        if word & mask != 0 {
            return false;
        }

        if word == 0 {
            self.touched_component_words.push((chunk_index, word_index));
        }

        self.component_seen[chunk_index][word_index] = word | mask;

        true
    }

    #[inline]
    fn visit_wire(&mut self, index: usize) -> bool {
        if self.wire_seen[index] {
            return false;
        }

        self.wire_seen[index] = true;
        self.touched_wires.push(index);
        true
    }

    #[inline]
    fn visit_net(&mut self, index: usize) -> bool {
        if self.net_seen[index] {
            return false;
        }

        self.net_seen[index] = true;
        self.touched_nets.push(index);
        true
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct WireComponent {
    pub(super) wires: SmallVec<[WireId; 4]>,
    pub(super) terminal_components: SmallVec<[DeviceComponent; 2]>,
}

#[cfg(test)]
mod tests {
    use super::TraversalScratch;

    #[test]
    fn scratch_marks_are_reusable_between_traversals() {
        let mut scratch = TraversalScratch::default();

        scratch.begin_wire_traversal(4);

        assert!(scratch.visit_wire(1));
        assert!(scratch.visit_wire(3));

        assert!(!scratch.visit_wire(1));
        assert!(!scratch.visit_wire(3));

        scratch.begin_wire_traversal(4);

        assert!(scratch.visit_wire(1));
        assert!(scratch.visit_wire(3));

        scratch.begin_wire_traversal(8);

        assert!(scratch.visit_wire(1));
        assert!(scratch.visit_wire(7));

        scratch.begin_island_traversal(4, [4]);

        assert!(scratch.visit_net(1));
        assert!(scratch.visit_component(0, 2));

        assert!(!scratch.visit_net(1));
        assert!(!scratch.visit_component(0, 2));

        scratch.begin_island_traversal(8, [8]);

        assert!(scratch.visit_net(1));
        assert!(scratch.visit_component(0, 2));

        assert!(scratch.visit_net(7));
        assert!(scratch.visit_component(0, 7));
    }
}
