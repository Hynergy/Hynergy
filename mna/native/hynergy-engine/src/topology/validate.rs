use crate::topology::{DerivedTopology, NetId};
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
            self.net_island_map.len(),
            self.nets.slot_count(),
            "net -> island map must mirror the stable NetId slot space"
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

        for (net_id, net) in self.nets.iter() {
            assert!(
                !net.wires.is_empty(),
                "live nets must contain at least one wire"
            );

            let island_id = self.net_island_map[net_id.index()];

            if let Some(island_id) = island_id {
                assert!(
                    self.islands.get(island_id).is_some(),
                    "live net references a retired island"
                );
            }

            let mut expected_terminal_devices = Vec::new();

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

                for connection in network
                    .wire_connections(wire)
                    .expect("wire in live net must exist")
                {
                    if let Some((device, _)) = connection.as_terminal() {
                        expected_terminal_devices.push(device);
                    }
                }

                seen_wires[wire.index()] = true;
            }

            let mut actual_terminal_devices = net.terminal_devices.clone();

            expected_terminal_devices.sort_unstable_by_key(|device| device.index());
            actual_terminal_devices.sort_unstable_by_key(|device| device.index());

            assert_eq!(
                actual_terminal_devices.as_slice(),
                expected_terminal_devices.as_slice(),
                "Net terminal incidence disagrees with Network wire connections"
            );

            if !net.terminal_devices.is_empty() {
                assert!(
                    island_id.is_some(),
                    "a net with attached terminals must belong to an island"
                );
            }
        }

        for index in 0..self.nets.slot_count() {
            let net_id = net_id(index);

            if self.nets.get(net_id).is_none() {
                assert_eq!(
                    self.net_island_map[index], None,
                    "retired NetId still has an IslandId"
                );
            }
        }

        for (index, slot) in network.wires().iter().enumerate() {
            match slot {
                Some(_) => {
                    assert!(seen_wires[index], "live wire is missing from derived nets");

                    let net_id =
                        self.wire_net_map[index].expect("live wire must have a derived NetId");

                    assert!(
                        self.nets.get(net_id).is_some(),
                        "live wire references a retired NetId"
                    );
                }

                None => {
                    assert_eq!(
                        self.wire_net_map[index], None,
                        "removed wire still has a derived NetId"
                    );
                }
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
        }

        for (index, slot) in network.devices().iter().enumerate() {
            match slot {
                Some(_) => {
                    assert!(
                        seen_devices[index],
                        "live device is missing from derived islands"
                    );

                    let island_id = self.device_island_map[index]
                        .expect("live device must have a derived IslandId");

                    assert!(
                        self.islands.get(island_id).is_some(),
                        "live device references a retired IslandId"
                    );
                }

                None => {
                    assert_eq!(
                        self.device_island_map[index], None,
                        "removed device still has a derived IslandId"
                    );
                }
            }
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

            let start = wire_id(index);

            let expected_net = self.wire_net_map[index].expect("live wire must have a NetId");

            assert!(
                self.nets.get(expected_net).is_some(),
                "wire component references a retired NetId"
            );

            assert!(
                !seen_components[expected_net.index()],
                "one NetId represents multiple disconnected wire components"
            );

            seen_components[expected_net.index()] = true;
            component_count += 1;

            visited[index] = true;
            stack.push(start);

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
                self.islands.get(expected_island).is_some(),
                "device references a retired IslandId"
            );

            assert!(
                !seen_islands[expected_island.index()],
                "one IslandId represents multiple disconnected primitive components"
            );

            seen_islands[expected_island.index()] = true;
            component_count += 1;

            visited_devices[index] = true;

            stack.push(
                PrimitiveVertex::try_from(device)
                    .expect("DeviceId must fit the packed 31-bit vertex representation"),
            );

            while let Some(vertex) = stack.pop() {
                match vertex.vertex_type() {
                    PrimitiveVertexType::Wire => {
                        let wire = vertex.as_wire().expect("wire vertex must decode as WireId");

                        let net_id = self.wire_net_map[wire.index()]
                            .expect("live wire must have a derived NetId");

                        assert!(
                            self.nets.get(net_id).is_some(),
                            "wire reachable from a device references a retired NetId"
                        );

                        assert_eq!(
                            self.net_island_map[net_id.index()],
                            Some(expected_island),
                            "net reachable from a device belongs to the wrong island"
                        );

                        for connection in network
                            .wire_connections(wire)
                            .expect("visited wire must exist")
                        {
                            if let Some(neighbor) = connection.as_wire() {
                                if !visited_wires[neighbor.index()] {
                                    visited_wires[neighbor.index()] = true;

                                    stack.push(PrimitiveVertex::try_from(neighbor).expect(
                                        "WireId must fit the packed 31-bit vertex representation",
                                    ));
                                }

                                continue;
                            }

                            if let Some((neighbor, _)) = connection.as_terminal()
                                && !visited_devices[neighbor.index()]
                            {
                                visited_devices[neighbor.index()] = true;

                                stack.push(PrimitiveVertex::try_from(neighbor).expect(
                                    "DeviceId must fit the packed 31-bit vertex representation",
                                ));
                            }
                        }
                    }

                    PrimitiveVertexType::Device => {
                        let current = vertex
                            .as_device()
                            .expect("device vertex must decode as DeviceId");

                        assert_eq!(
                            self.device_island_map[current.index()],
                            Some(expected_island),
                            "connected devices disagree on IslandId"
                        );

                        let device_slot = network
                            .devices()
                            .get(current.index())
                            .and_then(Option::as_ref)
                            .expect("visited device must exist");

                        for connection in device_slot.terminals().iter().flatten().copied() {
                            if let Some(wire) = connection.as_wire()
                                && !visited_wires[wire.index()]
                            {
                                visited_wires[wire.index()] = true;

                                stack.push(PrimitiveVertex::try_from(wire).expect(
                                    "WireId must fit the packed 31-bit vertex representation",
                                ));

                                continue;
                            }

                            if let Some((neighbor, _)) = connection.as_terminal()
                                && !visited_devices[neighbor.index()]
                            {
                                visited_devices[neighbor.index()] = true;

                                stack.push(PrimitiveVertex::try_from(neighbor).expect(
                                    "DeviceId must fit the packed 31-bit vertex representation",
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

            let net_id = self.wire_net_map[index].expect("live wire must have a derived NetId");

            assert!(
                self.nets.get(net_id).is_some(),
                "islandless wire references a retired NetId"
            );

            assert_eq!(
                self.net_island_map[net_id.index()],
                None,
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

#[inline]
fn wire_id(index: usize) -> WireId {
    WireId::try_from(u32::try_from(index + 1).expect("wire index must fit WireId"))
        .expect("wire IDs are one-based")
}

#[inline]
fn device_id(index: usize) -> DeviceId {
    DeviceId::try_from(u32::try_from(index + 1).expect("device index must fit DeviceId"))
        .expect("device IDs are one-based")
}

#[inline]
fn net_id(index: usize) -> NetId {
    NetId::try_from(u32::try_from(index + 1).expect("net index must fit NetId"))
        .expect("net IDs are one-based")
}
