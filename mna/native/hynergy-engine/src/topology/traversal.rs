use super::{DerivedTopology, IslandId, NetId};
use hynergy_model::device::definition::DeviceId;
use hynergy_model::network::{Network, WireId};
use smallvec::SmallVec;
use std::num::NonZeroU32;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct IslandComponent {
    pub(super) nets: SmallVec<[NetId; 4]>,
    pub(super) devices: SmallVec<[DeviceId; 4]>,
}

impl IslandComponent {
    #[inline]
    pub(super) fn rewrite_cost(&self) -> usize {
        self.nets.len() + self.devices.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IslandVertexType {
    Device,
    Net,
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
struct IslandVertex(NonZeroU32);

impl IslandVertex {
    const TYPE_BIT: u32 = 1 << 31;
    const ID_MASK: u32 = Self::TYPE_BIT - 1;

    #[inline]
    fn as_net(self) -> Option<NetId> {
        (self.vertex_type() == IslandVertexType::Net).then(|| {
            let id = NonZeroU32::new(self.0.get() & Self::ID_MASK)
                .expect("packed island vertex IDs are non-zero");

            NetId::from(id)
        })
    }

    #[inline]
    #[allow(dead_code)]
    unsafe fn net_unchecked(self) -> NetId {
        debug_assert_eq!(self.0.get() & Self::TYPE_BIT, 0);

        NetId::from(self.0)
    }

    #[inline]
    fn as_device(self) -> Option<DeviceId> {
        (self.vertex_type() == IslandVertexType::Device).then(|| {
            let id = NonZeroU32::new(self.0.get() & Self::ID_MASK)
                .expect("packed island vertex IDs are non-zero");

            DeviceId::from(id)
        })
    }

    #[inline]
    #[allow(dead_code)]
    unsafe fn device_unchecked(self) -> DeviceId {
        debug_assert_ne!(self.0.get() & Self::TYPE_BIT, 0);

        let raw = unsafe { NonZeroU32::new_unchecked(self.0.get() & Self::ID_MASK) };

        DeviceId::from(raw)
    }

    #[inline]
    fn vertex_type(self) -> IslandVertexType {
        if self.0.get() & Self::TYPE_BIT != 0 {
            IslandVertexType::Device
        } else {
            IslandVertexType::Net
        }
    }
}

impl From<NetId> for IslandVertex {
    #[inline]
    fn from(id: NetId) -> Self {
        let raw: NonZeroU32 = id.into();

        debug_assert!(raw.get() <= Self::ID_MASK);

        Self(raw)
    }
}

impl From<DeviceId> for IslandVertex {
    #[inline]
    fn from(id: DeviceId) -> Self {
        let id: NonZeroU32 = id.into();

        debug_assert!(id.get() <= Self::ID_MASK);

        Self(
            NonZeroU32::new(id.get() | Self::TYPE_BIT)
                .expect("tagging non-zero DeviceId remains non-zero"),
        )
    }
}

pub(super) fn wire_components(
    topology: &DerivedTopology,
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

                if let Some((device, _)) = connection.as_terminal() {
                    component.terminal_devices.push(device);
                }
            }
        }

        components.push(component);
    }

    components
}

pub(super) fn island_components(
    topology: &DerivedTopology,
    network: &Network,
    scratch: &mut TraversalScratch,
    island_id: IslandId,
    affected_nets: &[NetId],
) -> Vec<IslandComponent> {
    scratch.begin_island_traversal(topology.nets.slot_count(), network.devices().len());

    let island = topology
        .islands
        .get(island_id)
        .expect("island repair requires a live island");

    let mut components = Vec::new();

    for &device_id in &island.devices {
        let index = device_id.index();

        if !scratch.visit_device(index) {
            continue;
        }

        debug_assert_eq!(topology.device_island_map[index], Some(island_id));
        debug_assert!(network.devices()[index].is_some());

        scratch.island_stack.push(device_id.into());

        components.push(walk_island_component(topology, network, island_id, scratch));

        debug_assert!(scratch.island_stack.is_empty());
    }

    for &net_id in affected_nets {
        let index = net_id.index();

        if !scratch.visit_net(index) {
            continue;
        }

        debug_assert_eq!(topology.net_island_map[index], Some(island_id));
        debug_assert!(topology.nets.get(net_id).is_some());

        scratch.island_stack.push(net_id.into());

        components.push(walk_island_component(topology, network, island_id, scratch));

        debug_assert!(scratch.island_stack.is_empty());
    }

    components
}

fn walk_island_component(
    topology: &DerivedTopology,
    network: &Network,
    island_id: IslandId,
    scratch: &mut TraversalScratch,
) -> IslandComponent {
    let mut component = IslandComponent::default();

    while let Some(vertex) = scratch.island_stack.pop() {
        match vertex.vertex_type() {
            IslandVertexType::Net => {
                let net_id = unsafe { vertex.as_net().unwrap_unchecked() };

                component.nets.push(net_id);

                let net = topology
                    .nets
                    .get(net_id)
                    .expect("visited island net must be live");

                for &device_id in &net.terminal_devices {
                    let index = device_id.index();

                    if !scratch.visit_device(index) {
                        continue;
                    }

                    debug_assert_eq!(topology.device_island_map[index], Some(island_id));

                    scratch.island_stack.push(device_id.into());
                }
            }

            IslandVertexType::Device => {
                let device_id = unsafe { vertex.as_device().unwrap_unchecked() };

                component.devices.push(device_id);

                let device = network.devices()[device_id.index()]
                    .as_ref()
                    .expect("visited island device must be live");

                for connection in device.terminals().iter().flatten().copied() {
                    if let Some(wire_id) = connection.as_wire() {
                        let net_id = topology.wire_net_map[wire_id.index()]
                            .expect("live attached wire must have a NetId");

                        let index = net_id.index();

                        if !scratch.visit_net(index) {
                            continue;
                        }

                        debug_assert_eq!(topology.net_island_map[index], Some(island_id));

                        scratch.island_stack.push(net_id.into());
                        continue;
                    }

                    if let Some((other_device, _)) = connection.as_terminal() {
                        let index = other_device.index();

                        if !scratch.visit_device(index) {
                            continue;
                        }

                        debug_assert_eq!(topology.device_island_map[index], Some(island_id));

                        scratch.island_stack.push(other_device.into());
                    }
                }
            }
        }
    }

    component
}

#[derive(Debug, Default)]
pub(crate) struct TraversalScratch {
    wire_seen: Vec<bool>,
    net_seen: Vec<bool>,
    device_seen: Vec<bool>,

    touched_wires: Vec<usize>,
    touched_nets: Vec<usize>,
    touched_devices: Vec<usize>,

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
    fn begin_island_traversal(&mut self, net_slots: usize, device_slots: usize) {
        Self::reset_marks(&mut self.net_seen, &mut self.touched_nets, net_slots);

        Self::reset_marks(
            &mut self.device_seen,
            &mut self.touched_devices,
            device_slots,
        );

        self.island_stack.clear();
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

    #[inline]
    fn visit_device(&mut self, index: usize) -> bool {
        if self.device_seen[index] {
            return false;
        }

        self.device_seen[index] = true;
        self.touched_devices.push(index);
        true
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct WireComponent {
    pub(super) wires: SmallVec<[WireId; 4]>,
    pub(super) terminal_devices: SmallVec<[DeviceId; 2]>,
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

        // Growing the scratch storage must preserve the reset behavior.
        scratch.begin_wire_traversal(8);

        assert!(scratch.visit_wire(1));
        assert!(scratch.visit_wire(7));

        scratch.begin_island_traversal(4, 4);

        assert!(scratch.visit_net(1));
        assert!(scratch.visit_device(2));

        assert!(!scratch.visit_net(1));
        assert!(!scratch.visit_device(2));

        scratch.begin_island_traversal(8, 8);

        assert!(scratch.visit_net(1));
        assert!(scratch.visit_device(2));

        assert!(scratch.visit_net(7));
        assert!(scratch.visit_device(7));
    }
}
