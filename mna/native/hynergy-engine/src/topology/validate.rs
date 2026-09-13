use crate::topology::DerivedTopology;
use hynergy_model::device::definition::DeviceId;
use hynergy_model::network::{Network, WireId};
use std::num::NonZeroU32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrimitiveVertexType {
    Wire,
    Device,
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PrimitiveVertex(NonZeroU32);

impl PrimitiveVertex {
    const TYPE_BIT: u32 = 1 << 31;
    const ID_MASK: u32 = Self::TYPE_BIT - 1;

    #[inline]
    fn new(id: impl Into<NonZeroU32>, ty: PrimitiveVertexType) -> Option<Self> {
        let id = id.into();

        if id.get() > Self::ID_MASK {
            return None;
        }

        let raw = match ty {
            PrimitiveVertexType::Wire => id.get(),
            PrimitiveVertexType::Device => id.get() | Self::TYPE_BIT,
        };

        Some(Self(NonZeroU32::new(raw).expect(
            "a non-zero vertex ID produces a non-zero primitive vertex",
        )))
    }

    #[inline]
    fn as_wire(self) -> Option<WireId> {
        (self.vertex_type() == PrimitiveVertexType::Wire).then(|| {
            WireId::from(
                NonZeroU32::new(self.0.get() & Self::ID_MASK)
                    .expect("packed primitive vertex ID is non-zero"),
            )
        })
    }

    #[inline]
    fn as_device(self) -> Option<DeviceId> {
        (self.vertex_type() == PrimitiveVertexType::Device).then(|| {
            DeviceId::from(
                NonZeroU32::new(self.0.get() & Self::ID_MASK)
                    .expect("packed primitive vertex ID is non-zero"),
            )
        })
    }

    #[inline]
    fn vertex_type(self) -> PrimitiveVertexType {
        if self.0.get() & Self::TYPE_BIT != 0 {
            PrimitiveVertexType::Device
        } else {
            PrimitiveVertexType::Wire
        }
    }
}

impl TryFrom<WireId> for PrimitiveVertex {
    type Error = ();

    #[inline]
    fn try_from(id: WireId) -> Result<Self, Self::Error> {
        Self::new(id, PrimitiveVertexType::Wire).ok_or(())
    }
}

impl TryFrom<DeviceId> for PrimitiveVertex {
    type Error = ();

    #[inline]
    fn try_from(id: DeviceId) -> Result<Self, Self::Error> {
        Self::new(id, PrimitiveVertexType::Device).ok_or(())
    }
}

impl DerivedTopology {
    pub(crate) fn assert_consistent(&self, network: &Network) {
        assert_eq!(
            self.wire_net_map.len(),
            network.wires().len(),
            "wire map must mirror Network wire slots"
        );
        assert_eq!(
            self.device_island_map.len(),
            network.devices().len(),
            "device map must mirror Network device slots"
        );

        self.assert_membership_maps(network);
        self.assert_net_partition_matches_network(network);
        self.assert_island_partition_matches_network(network);
    }

    fn assert_membership_maps(&self, network: &Network) {
        let mut seen_wires = vec![false; network.wires().len()];
        let mut seen_devices = vec![false; network.devices().len()];
        let mut seen_island_nets = vec![false; self.nets.slot_count()];

        for (net_id, net) in self.nets.iter() {
            assert!(
                !net.wires.is_empty(),
                "live nets must contain at least one wire"
            );

            for &wire in &net.wires {
                assert!(
                    network
                        .wires()
                        .get(wire.index())
                        .is_some_and(|slot| slot.is_some()),
                    "net references a removed or out-of-range wire"
                );
                assert_eq!(
                    self.wire_net_map[wire.index()],
                    Some(net_id),
                    "wire -> net map disagrees with Net::wires"
                );
                assert!(
                    !seen_wires[wire.index()],
                    "wire appears in more than one net"
                );
                seen_wires[wire.index()] = true;
            }

            if let Some(island_id) = net.island {
                let island = self
                    .islands
                    .get(island_id)
                    .expect("net references a retired island");
                assert!(
                    island.nets.contains(&net_id),
                    "Net::island disagrees with IslandTopology::nets"
                );
            }
        }

        for (index, slot) in network.wires().iter().enumerate() {
            match slot {
                Some(_) => {
                    assert!(seen_wires[index], "live wire is missing from derived nets");
                    assert!(
                        self.wire_net_map[index].is_some(),
                        "live wire has no derived NetId"
                    );
                }
                None => assert_eq!(
                    self.wire_net_map[index], None,
                    "removed wire still has a derived NetId"
                ),
            }
        }

        for (island_id, island) in self.islands.iter() {
            assert!(
                !island.devices.is_empty(),
                "live islands must contain at least one device"
            );

            for &device in &island.devices {
                assert!(
                    network
                        .devices()
                        .get(device.index())
                        .is_some_and(|slot| slot.is_some()),
                    "island references a removed or out-of-range device"
                );
                assert_eq!(
                    self.device_island_map[device.index()],
                    Some(island_id),
                    "device -> island map disagrees with IslandTopology::devices"
                );
                assert!(
                    !seen_devices[device.index()],
                    "device appears in more than one island"
                );
                seen_devices[device.index()] = true;
            }

            for &net_id in &island.nets {
                let net = self
                    .nets
                    .get(net_id)
                    .expect("island references a retired net");
                assert_eq!(
                    net.island,
                    Some(island_id),
                    "IslandTopology::nets disagrees with Net::island"
                );
                assert!(
                    !seen_island_nets[net_id.index()],
                    "net appears in more than one island"
                );
                seen_island_nets[net_id.index()] = true;
            }
        }

        for (index, slot) in network.devices().iter().enumerate() {
            match slot {
                Some(_) => {
                    assert!(
                        seen_devices[index],
                        "live device is missing from derived islands"
                    );
                    assert!(
                        self.device_island_map[index].is_some(),
                        "live device has no derived IslandId"
                    );
                }
                None => assert_eq!(
                    self.device_island_map[index], None,
                    "removed device still has a derived IslandId"
                ),
            }
        }

        for (net_id, net) in self.nets.iter() {
            assert_eq!(
                seen_island_nets[net_id.index()],
                net.island.is_some(),
                "island membership presence disagrees with Net::island"
            );
        }
    }

    fn assert_net_partition_matches_network(&self, network: &Network) {
        let mut visited = vec![false; network.wires().len()];
        let mut seen_components = vec![false; self.nets.slot_count()];
        let mut stack = Vec::new();
        let mut component_count = 0usize;

        for (index, slot) in network.wires().iter().enumerate() {
            if slot.is_none() || visited[index] {
                continue;
            }

            let wire = wire_id(index);
            let expected_net = self.wire_net_map[index].expect("live wire must have a NetId");
            assert!(
                !seen_components[expected_net.index()],
                "one NetId represents multiple disconnected wire components"
            );
            seen_components[expected_net.index()] = true;
            component_count += 1;

            visited[index] = true;
            stack.push(wire);

            while let Some(current) = stack.pop() {
                assert_eq!(
                    self.wire_net_map[current.index()],
                    Some(expected_net),
                    "wire-connected vertices disagree on NetId"
                );

                for connection in network
                    .wire_connections(current)
                    .expect("visited wire must exist")
                {
                    let Some(neighbor) = connection.as_wire() else {
                        continue;
                    };
                    if !visited[neighbor.index()] {
                        visited[neighbor.index()] = true;
                        stack.push(neighbor);
                    }
                }
            }
        }

        assert_eq!(
            component_count,
            self.nets.len(),
            "derived net count differs from wire-graph connected-component count"
        );
    }

    fn assert_island_partition_matches_network(&self, network: &Network) {
        let mut visited_wires = vec![false; network.wires().len()];
        let mut visited_devices = vec![false; network.devices().len()];
        let mut seen_islands = vec![false; self.islands.slot_count()];
        let mut stack = Vec::new();
        let mut component_count = 0usize;

        for (index, slot) in network.devices().iter().enumerate() {
            if slot.is_none() || visited_devices[index] {
                continue;
            }

            let device = device_id(index);
            let expected_island =
                self.device_island_map[index].expect("live device must have a derived IslandId");
            assert!(
                !seen_islands[expected_island.index()],
                "one IslandId represents multiple disconnected primitive components"
            );
            seen_islands[expected_island.index()] = true;
            component_count += 1;

            visited_devices[index] = true;
            stack.push(
                PrimitiveVertex::try_from(device)
                    .expect("DeviceId should never exceed 31-bit non-zero representation"),
            );

            while let Some(vertex) = stack.pop() {
                match vertex.vertex_type() {
                    PrimitiveVertexType::Wire => {
                        let wire = vertex.as_wire().expect("vertex is a wire");

                        let net_id = self.wire_net_map[wire.index()]
                            .expect("live wire must have a derived NetId");
                        let net = self.nets.get(net_id).expect("wire NetId must be live");
                        assert_eq!(
                            net.island,
                            Some(expected_island),
                            "wire reachable from a device belongs to the wrong island"
                        );

                        for connection in network
                            .wire_connections(wire)
                            .expect("visited wire must exist")
                        {
                            if let Some(neighbor) = connection.as_wire() {
                                if !visited_wires[neighbor.index()] {
                                    visited_wires[neighbor.index()] = true;
                                    stack.push(PrimitiveVertex::try_from(neighbor).expect(
                                        "WireId should never exceed 31-bit non-zero representation",
                                    ));
                                }
                            } else if let Some((neighbor, _)) = connection.as_terminal()
                                && !visited_devices[neighbor.index()]
                            {
                                visited_devices[neighbor.index()] = true;
                                stack.push(PrimitiveVertex::try_from(neighbor).expect(
                                    "DeviceId should never exceed 31-bit non-zero representation",
                                ));
                            }
                        }
                    }
                    PrimitiveVertexType::Device => {
                        let current = vertex.as_device().expect("vertex is a device");

                        assert_eq!(
                            self.device_island_map[current.index()],
                            Some(expected_island),
                            "connected devices disagree on IslandId"
                        );

                        let device_slot = network.devices()[current.index()]
                            .as_ref()
                            .expect("visited device must exist");
                        for connection in device_slot.terminals().iter().flatten().copied() {
                            if let Some(wire) = connection.as_wire()
                                && !visited_wires[wire.index()]
                            {
                                visited_wires[wire.index()] = true;
                                stack.push(PrimitiveVertex::try_from(wire).expect(
                                    "WireId should never exceed 31-bit non-zero representation",
                                ));
                            } else if let Some((neighbor, _)) = connection.as_terminal()
                                && !visited_devices[neighbor.index()]
                            {
                                visited_devices[neighbor.index()] = true;
                                stack.push(PrimitiveVertex::try_from(neighbor).expect(
                                    "DeviceId should never exceed 31-bit non-zero representation",
                                ));
                            }
                        }
                    }
                }
            }
        }

        for (index, slot) in network.wires().iter().enumerate() {
            if slot.is_none() || visited_wires[index] {
                continue;
            }

            let net_id = self.wire_net_map[index].expect("live wire must have a NetId");
            let net = self.nets.get(net_id).expect("wire NetId must be live");
            assert_eq!(
                net.island, None,
                "wire component with no reachable device must be islandless"
            );
        }

        assert_eq!(
            component_count,
            self.islands.len(),
            "derived island count differs from primitive connected components containing devices"
        );
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
