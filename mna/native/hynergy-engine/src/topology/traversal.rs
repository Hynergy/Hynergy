use super::{DerivedTopology, IslandId, NetId};
use hynergy_model::device::definition::DeviceId;
use hynergy_model::network::{Network, WireId};
use std::num::NonZeroU32;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct IslandComponent {
    pub(super) nets: Vec<NetId>,
    pub(super) devices: Vec<DeviceId>,
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
struct IslandVertex(NonZeroU32);

impl IslandVertex {
    const TYPE_BIT: u32 = 1 << 31;
    const ID_MASK: u32 = Self::TYPE_BIT - 1;

    #[inline]
    fn new(id: impl Into<NonZeroU32>, ty: IslandVertexType) -> Option<Self> {
        let id = id.into();

        if id.get() > Self::ID_MASK {
            return None;
        }

        let raw = match ty {
            IslandVertexType::Device => id.get() | Self::TYPE_BIT,
            IslandVertexType::Net => id.get(),
        };

        Some(Self(NonZeroU32::new(raw).expect(
            "a non-zero vertex ID produces a non-zero island vertex",
        )))
    }

    #[inline]
    fn as_net(self) -> Option<NetId> {
        (self.vertex_type() == IslandVertexType::Net).then(|| {
            let id = NonZeroU32::new(self.0.get() & Self::ID_MASK)
                .expect("packed island vertex IDs are non-zero");

            NetId::from(id)
        })
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
    fn vertex_type(self) -> IslandVertexType {
        if self.0.get() & Self::TYPE_BIT != 0 {
            IslandVertexType::Device
        } else {
            IslandVertexType::Net
        }
    }
}

impl TryFrom<NetId> for IslandVertex {
    type Error = ();

    #[inline]
    fn try_from(id: NetId) -> Result<Self, Self::Error> {
        Self::new(id, IslandVertexType::Net).ok_or(())
    }
}

impl TryFrom<DeviceId> for IslandVertex {
    type Error = ();

    #[inline]
    fn try_from(id: DeviceId) -> Result<Self, Self::Error> {
        Self::new(id, IslandVertexType::Device).ok_or(())
    }
}

pub(super) fn wire_components(network: &Network, candidates: &[WireId]) -> Vec<Vec<WireId>> {
    let mut allowed = vec![false; network.wires().len()];
    for &wire in candidates {
        if network
            .wires()
            .get(wire.index())
            .is_some_and(|slot| slot.is_some())
        {
            allowed[wire.index()] = true;
        }
    }

    let mut visited = vec![false; allowed.len()];
    let mut stack = Vec::new();
    let mut components = Vec::new();

    for &start in candidates {
        if start.index() >= allowed.len() || !allowed[start.index()] || visited[start.index()] {
            continue;
        }

        visited[start.index()] = true;
        stack.push(start);
        let mut component = Vec::new();

        while let Some(wire) = stack.pop() {
            component.push(wire);

            let connections = network
                .wire_connections(wire)
                .expect("candidate wire must still exist in the network");

            for connection in connections {
                let Some(neighbor) = connection.as_wire() else {
                    continue;
                };
                let index = neighbor.index();

                if index < allowed.len() && allowed[index] && !visited[index] {
                    visited[index] = true;
                    stack.push(neighbor);
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
    island_id: IslandId,
) -> Vec<IslandComponent> {
    let island = topology
        .islands
        .get(island_id)
        .expect("island repair requires a live island");

    let mut allowed_nets = vec![false; topology.nets.slot_count()];
    for &net_id in &island.nets {
        if topology
            .nets
            .get(net_id)
            .is_some_and(|net| net.island == Some(island_id))
        {
            allowed_nets[net_id.index()] = true;
        }
    }

    let mut allowed_devices = vec![false; network.devices().len()];
    for &device_id in &island.devices {
        if network
            .devices()
            .get(device_id.index())
            .is_some_and(|slot| slot.is_some())
            && topology
                .device_island_map
                .get(device_id.index())
                .copied()
                .flatten()
                == Some(island_id)
        {
            allowed_devices[device_id.index()] = true;
        }
    }

    let mut visited_nets = vec![false; allowed_nets.len()];
    let mut visited_devices = vec![false; allowed_devices.len()];
    let mut stack = Vec::new();
    let mut components = Vec::new();

    for &device_id in &island.devices {
        let index = device_id.index();
        if index >= allowed_devices.len() || !allowed_devices[index] || visited_devices[index] {
            continue;
        }

        visited_devices[index] = true;
        stack.push(
            IslandVertex::try_from(device_id)
                .expect("DeviceId should never exceed 31-bit non-zero representation"),
        );
        components.push(walk_island_component(
            topology,
            network,
            &allowed_nets,
            &allowed_devices,
            &mut visited_nets,
            &mut visited_devices,
            &mut stack,
        ));
    }

    for &net_id in &island.nets {
        let index = net_id.index();
        if index >= allowed_nets.len() || !allowed_nets[index] || visited_nets[index] {
            continue;
        }

        visited_nets[index] = true;
        stack.push(
            IslandVertex::try_from(net_id)
                .expect("NetId should never exceed 31-bit non-zero representation"),
        );
        components.push(walk_island_component(
            topology,
            network,
            &allowed_nets,
            &allowed_devices,
            &mut visited_nets,
            &mut visited_devices,
            &mut stack,
        ));
    }

    components
}

fn walk_island_component(
    topology: &DerivedTopology,
    network: &Network,
    allowed_nets: &[bool],
    allowed_devices: &[bool],
    visited_nets: &mut [bool],
    visited_devices: &mut [bool],
    stack: &mut Vec<IslandVertex>,
) -> IslandComponent {
    let mut component = IslandComponent::default();

    while let Some(vertex) = stack.pop() {
        match vertex.vertex_type() {
            IslandVertexType::Net => {
                let net_id = unsafe { vertex.as_net().unwrap_unchecked() };

                component.nets.push(net_id);
                let net = topology
                    .nets
                    .get(net_id)
                    .expect("allowed island net must be live");

                for &wire in &net.wires {
                    let Ok(connections) = network.wire_connections(wire) else {
                        continue;
                    };

                    for connection in connections {
                        let Some((device_id, _)) = connection.as_terminal() else {
                            continue;
                        };
                        let index = device_id.index();

                        if index < allowed_devices.len()
                            && allowed_devices[index]
                            && !visited_devices[index]
                        {
                            visited_devices[index] = true;
                            stack.push(IslandVertex::try_from(device_id).expect(
                                "DeviceId should never exceed 31-bit non-zero representation",
                            ));
                        }
                    }
                }
            }
            IslandVertexType::Device => {
                let device_id = unsafe { vertex.as_device().unwrap_unchecked() };

                component.devices.push(device_id);
                let device = network.devices()[device_id.index()]
                    .as_ref()
                    .expect("allowed island device must be live");

                for connection in device.terminals().iter().flatten().copied() {
                    if let Some(wire_id) = connection.as_wire() {
                        let Some(net_id) = topology
                            .wire_net_map
                            .get(wire_id.index())
                            .copied()
                            .flatten()
                        else {
                            continue;
                        };
                        let index = net_id.index();

                        if index < allowed_nets.len() && allowed_nets[index] && !visited_nets[index]
                        {
                            visited_nets[index] = true;
                            stack.push(IslandVertex::try_from(net_id).expect(
                                "NetId should never exceed 31-bit non-zero representation",
                            ));
                        }
                        continue;
                    }

                    if let Some((other_device, _)) = connection.as_terminal() {
                        let index = other_device.index();
                        if index < allowed_devices.len()
                            && allowed_devices[index]
                            && !visited_devices[index]
                        {
                            visited_devices[index] = true;
                            stack.push(IslandVertex::try_from(other_device).expect(
                                "DeviceId should never exceed 31-bit non-zero representation",
                            ));
                        }
                    }
                }
            }
        }
    }

    component
}
