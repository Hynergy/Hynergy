use crate::topology::device_topology::DeviceTopologyChunk;
use crate::topology::{DerivedTopology, DeviceComponent, NetId, terminal_component};
use hynergy_model::device::definition::DevicePartitionId;
use hynergy_model::device::registry::DefinitionRegistry;
use hynergy_model::network::{Network, WireId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrimitiveVertex {
    Wire(WireId),
    Component(DeviceComponent),
}

impl DerivedTopology {
    pub(crate) fn assert_consistent(&self, definitions: &DefinitionRegistry, network: &Network) {
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

        self.assert_device_chunks_match_network(definitions, network);
        self.assert_membership_maps(definitions, network);
        self.assert_net_partition_matches_network(network);
        self.assert_island_partition_matches_network(definitions, network);
    }

    fn assert_device_chunks_match_network(
        &self,
        definitions: &DefinitionRegistry,
        network: &Network,
    ) {
        let mut seen_rows = self
            .device_chunks
            .iter()
            .map(|chunk| vec![false; chunk.row_count()])
            .collect::<Vec<_>>();

        let mut live_devices = 0usize;

        for (device, device_view) in network.iter_devices() {
            let location = network
                .device_location(device)
                .expect("iterated live device must have a location");

            let chunk_index = location.chunk_index() as usize;
            let row = location.row() as usize;

            let chunk = self
                .device_chunks
                .get(chunk_index)
                .expect("live model chunk must have a topology chunk");

            let definition = definitions
                .get(device_view.definition_id())
                .expect("live device definition must remain registered");

            assert_eq!(
                chunk.partition_count(),
                definition.partition_count(),
                "topology chunk partition stride disagrees with model definition",
            );

            assert!(
                row < chunk.row_count(),
                "live model row is outside topology chunk",
            );

            assert!(
                !seen_rows[chunk_index][row],
                "two live devices resolve to one physical topology row",
            );

            seen_rows[chunk_index][row] = true;
            live_devices += 1;
        }

        assert_eq!(
            self.device_chunks
                .iter()
                .map(DeviceTopologyChunk::row_count)
                .sum::<usize>(),
            live_devices,
            "topology row count must equal live model device count",
        );

        for (chunk_index, chunk) in self.device_chunks.iter().enumerate() {
            assert_ne!(
                chunk.row_count(),
                0,
                "resident topology chunks must not be empty"
            );

            assert!(
                seen_rows[chunk_index].iter().all(|seen| *seen),
                "topology chunk contains an orphaned physical row",
            );
        }
    }

    fn assert_membership_maps(&self, definitions: &DefinitionRegistry, network: &Network) {
        let mut seen_wires = vec![false; network.wires().len()];

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

            let mut expected_terminal_components = Vec::new();

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
                    if let Some((device, terminal)) = connection.as_terminal() {
                        expected_terminal_components.push(terminal_component(
                            definitions,
                            network,
                            device,
                            terminal,
                        ));
                    }
                }

                seen_wires[wire.index()] = true;
            }

            let mut actual_terminal_components = net.terminal_components.clone();

            expected_terminal_components.sort_unstable();
            actual_terminal_components.sort_unstable();

            assert_eq!(
                actual_terminal_components.as_slice(),
                expected_terminal_components.as_slice(),
                "Net terminal incidence disagrees with Network wire connections",
            );

            if !net.terminal_components.is_empty() {
                assert!(
                    island_id.is_some(),
                    "a net with attached terminals must belong to an island",
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

        let mut seen_components = self
            .device_chunks
            .iter()
            .map(|chunk| vec![false; chunk.row_count() * chunk.partition_count()])
            .collect::<Vec<_>>();

        for (island_id, island) in self.islands.iter() {
            assert!(
                !island.components.is_empty(),
                "live islands must contain at least one device component",
            );

            for &component in &island.components {
                assert!(
                    network.device(component.device()).is_ok(),
                    "island references a removed device",
                );

                let (chunk_index, component_index) =
                    self.component_storage_index(network, component);

                assert_eq!(
                    self.component_island(network, component),
                    island_id,
                    "component -> island sidecar disagrees with IslandTopology",
                );

                assert!(
                    !seen_components[chunk_index][component_index],
                    "device component appears in more than one island",
                );

                seen_components[chunk_index][component_index] = true;
            }
        }

        for (device, device_view) in network.iter_devices() {
            let definition = definitions
                .get(device_view.definition_id())
                .expect("live device definition must remain registered");

            for partition_index in 0..definition.partition_count() {
                let partition = DevicePartitionId::new(
                    u16::try_from(partition_index)
                        .expect("partition index must fit DevicePartitionId"),
                );

                let component = DeviceComponent::new(device, partition);

                let (chunk_index, component_index) =
                    self.component_storage_index(network, component);

                assert!(
                    seen_components[chunk_index][component_index],
                    "live device component is missing from islands",
                );

                let island = self.component_island(network, component);

                assert!(
                    self.islands.get(island).is_some(),
                    "device component references a retired island",
                );
            }
        }

        for chunk in seen_components {
            assert!(
                chunk.into_iter().all(|seen| seen),
                "topology component sidecar contains an orphaned component",
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

    fn assert_island_partition_matches_network(
        &self,
        definitions: &DefinitionRegistry,
        network: &Network,
    ) {
        let mut visited_wires = vec![false; network.wires().len()];

        let mut visited_components = self
            .device_chunks
            .iter()
            .map(|chunk| vec![false; chunk.row_count() * chunk.partition_count()])
            .collect::<Vec<_>>();

        let mut seen_islands = vec![false; self.islands.slot_count()];
        let mut stack = Vec::new();
        let mut connected_component_count = 0usize;

        for (device, device_view) in network.iter_devices() {
            let definition = definitions
                .get(device_view.definition_id())
                .expect("live definition must remain registered");

            for partition_index in 0..definition.partition_count() {
                let partition = DevicePartitionId::new(
                    u16::try_from(partition_index)
                        .expect("partition index must fit DevicePartitionId"),
                );

                let component = DeviceComponent::new(device, partition);

                let (chunk_index, component_index) =
                    self.component_storage_index(network, component);

                if visited_components[chunk_index][component_index] {
                    continue;
                }

                let expected_island = self.component_island(network, component);

                assert!(self.islands.get(expected_island).is_some(),);

                assert!(
                    !seen_islands[expected_island.index()],
                    "one IslandId represents multiple disconnected electrical components",
                );

                seen_islands[expected_island.index()] = true;
                connected_component_count += 1;

                visited_components[chunk_index][component_index] = true;

                stack.push(PrimitiveVertex::Component(component));

                while let Some(vertex) = stack.pop() {
                    match vertex {
                        PrimitiveVertex::Wire(wire) => {
                            let net_id = self.wire_net_map[wire.index()]
                                .expect("live wire must have a derived NetId");

                            assert!(
                                self.nets.get(net_id).is_some(),
                                "wire reachable from a component references a retired NetId"
                            );

                            assert_eq!(
                                self.net_island_map[net_id.index()],
                                Some(expected_island),
                                "net reachable from a component belongs to the wrong island"
                            );

                            for connection in network
                                .wire_connections(wire)
                                .expect("visited wire must exist")
                            {
                                if let Some(neighbor) = connection.as_wire() {
                                    if !visited_wires[neighbor.index()] {
                                        visited_wires[neighbor.index()] = true;
                                        stack.push(PrimitiveVertex::Wire(neighbor));
                                    }

                                    continue;
                                }

                                if let Some((neighbor_device, neighbor_terminal)) =
                                    connection.as_terminal()
                                {
                                    let neighbor = terminal_component(
                                        definitions,
                                        network,
                                        neighbor_device,
                                        neighbor_terminal,
                                    );

                                    let (neighbor_chunk, neighbor_index) =
                                        self.component_storage_index(network, neighbor);

                                    if !visited_components[neighbor_chunk][neighbor_index] {
                                        visited_components[neighbor_chunk][neighbor_index] = true;

                                        stack.push(PrimitiveVertex::Component(neighbor));
                                    }
                                }
                            }
                        }

                        PrimitiveVertex::Component(component) => {
                            assert_eq!(
                                self.component_island(network, component,),
                                expected_island,
                                "connected components disagree on IslandId",
                            );

                            let device = component.device();

                            let device_view = network
                                .device(device)
                                .expect("visited component device must exist");

                            let definition = definitions
                                .get(device_view.definition_id())
                                .expect("visited component definition must remain registered");

                            for (terminal_index, connection) in
                                device_view.terminals().iter().enumerate()
                            {
                                if definition.terminal_partitions()[terminal_index]
                                    != component.partition()
                                {
                                    continue;
                                }

                                let Some(connection) = *connection else {
                                    continue;
                                };

                                if let Some(wire) = connection.as_wire() {
                                    if !visited_wires[wire.index()] {
                                        visited_wires[wire.index()] = true;
                                        stack.push(PrimitiveVertex::Wire(wire));
                                    }

                                    continue;
                                }

                                if let Some((neighbor_device, neighbor_terminal)) =
                                    connection.as_terminal()
                                {
                                    let neighbor = terminal_component(
                                        definitions,
                                        network,
                                        neighbor_device,
                                        neighbor_terminal,
                                    );

                                    let (neighbor_chunk, neighbor_index) =
                                        self.component_storage_index(network, neighbor);

                                    if !visited_components[neighbor_chunk][neighbor_index] {
                                        visited_components[neighbor_chunk][neighbor_index] = true;

                                        stack.push(PrimitiveVertex::Component(neighbor));
                                    }
                                }
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
                "wire component with no reachable device component must be islandless"
            );
        }

        assert_eq!(
            connected_component_count,
            self.islands.len(),
            "derived island count differs from electrical connected-component count"
        );
    }
}

#[inline]
fn wire_id(index: usize) -> WireId {
    WireId::try_from(u32::try_from(index + 1).expect("wire index must fit WireId"))
        .expect("wire IDs are one-based")
}

#[inline]
fn net_id(index: usize) -> NetId {
    NetId::try_from(u32::try_from(index + 1).expect("net index must fit NetId"))
        .expect("net IDs are one-based")
}
