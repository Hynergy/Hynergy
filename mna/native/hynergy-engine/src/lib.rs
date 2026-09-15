mod compile;
mod topology;

use crate::topology::{DerivedTopology, TraversalScratch};
use hynergy_model::device::definition::{DefinitionId, DeviceDefinition, DeviceId, TerminalId};
use hynergy_model::device::registry::{DefinitionRegistry, RegisterDeviceError};
use hynergy_model::network::{Network, NetworkModelError, WireId};
use hynergy_model::parameter::ParameterId;

pub struct Engine {
    definition_registry: DefinitionRegistry,

    universe: Vec<Option<World>>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldManagementError {
    UnknownWorld,
    WorldIdExhausted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldCommandApplyError {
    UnknownWorld,
    Model(NetworkModelError),
}

impl From<NetworkModelError> for WorldCommandApplyError {
    #[inline]
    fn from(error: NetworkModelError) -> Self {
        Self::Model(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorldCommand {
    AddWire {
        wire: WireId,
    },
    RemoveWire {
        wire: WireId,
    },
    ConnectWires {
        wire_a: WireId,
        wire_b: WireId,
    },
    DisconnectWires {
        wire_a: WireId,
        wire_b: WireId,
    },
    AddDevice {
        device: DeviceId,
        definition: DefinitionId,
    },
    RemoveDevice {
        device: DeviceId,
    },
    AttachTerminal {
        wire: WireId,
        device: DeviceId,
        terminal: TerminalId,
    },
    DetachTerminal {
        wire: WireId,
        device: DeviceId,
        terminal: TerminalId,
    },
    SetDeviceParameter {
        device: DeviceId,
        parameter: ParameterId,
        value: f64,
    },
}

impl Engine {
    pub const COMPOSITE_DEFINITION_ID_BASE: u32 = DefinitionRegistry::COMPOSITE_DEFINITION_ID_BASE;

    pub fn new() -> Self {
        Self {
            definition_registry: DefinitionRegistry::new(),
            universe: Vec::new(),
        }
    }

    #[inline]
    pub fn definitions(&self) -> &DefinitionRegistry {
        &self.definition_registry
    }

    #[inline]
    pub fn register_definition(
        &mut self,
        definition: DeviceDefinition,
    ) -> Result<DefinitionId, RegisterDeviceError> {
        self.definition_registry.register(definition)
    }

    pub fn new_world(&mut self) -> Result<u32, WorldManagementError> {
        let id = u32::try_from(self.universe.len())
            .map_err(|_| WorldManagementError::WorldIdExhausted)?;

        self.universe.push(Some(World::default()));
        Ok(id)
    }

    pub fn destroy_world(&mut self, world_id: u32) -> Result<(), WorldManagementError> {
        let world = self
            .universe
            .get_mut(world_id as usize)
            .ok_or(WorldManagementError::UnknownWorld)?;

        if world.take().is_none() {
            return Err(WorldManagementError::UnknownWorld);
        }

        Ok(())
    }

    #[inline]
    pub fn world(&self, world_id: u32) -> Option<&World> {
        self.universe
            .get(world_id as usize)
            .and_then(Option::as_ref)
    }

    #[allow(dead_code)]
    #[inline]
    fn world_mut(&mut self, world_id: u32) -> Option<&mut World> {
        self.universe
            .get_mut(world_id as usize)
            .and_then(Option::as_mut)
    }

    #[inline]
    pub fn contains_world(&self, world_id: u32) -> bool {
        self.world(world_id).is_some()
    }

    pub fn apply_world_command(
        &mut self,
        world_id: u32,
        command: WorldCommand,
    ) -> Result<(), WorldCommandApplyError> {
        let definitions = &self.definition_registry;
        let world = self
            .universe
            .get_mut(world_id as usize)
            .and_then(Option::as_mut)
            .ok_or(WorldCommandApplyError::UnknownWorld)?;

        match command {
            WorldCommand::AddWire { wire } => {
                world.add_wire(wire)?;
            }
            WorldCommand::RemoveWire { wire } => {
                world.remove_wire(definitions, wire)?;
            }
            WorldCommand::ConnectWires { wire_a, wire_b } => {
                world.connect_wires(definitions, wire_a, wire_b)?;
            }
            WorldCommand::DisconnectWires { wire_a, wire_b } => {
                world.disconnect_wires(definitions, wire_a, wire_b)?;
            }
            WorldCommand::AddDevice { device, definition } => {
                world.add_device(definitions, device, definition)?;
            }
            WorldCommand::RemoveDevice { device } => {
                world.remove_device(definitions, device)?;
            }
            WorldCommand::AttachTerminal {
                wire,
                device,
                terminal,
            } => {
                world.attach_terminal(definitions, wire, device, terminal)?;
            }
            WorldCommand::DetachTerminal {
                wire,
                device,
                terminal,
            } => {
                world.detach_terminal(definitions, wire, device, terminal)?;
            }
            WorldCommand::SetDeviceParameter {
                device,
                parameter,
                value,
            } => {
                world.set_device_parameter(definitions, device, parameter, value)?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct World {
    network: Network,
    derived_topology: DerivedTopology,
    topology_scratch: TraversalScratch,
}

impl Clone for World {
    fn clone(&self) -> Self {
        Self {
            network: self.network.clone(),
            derived_topology: self.derived_topology.clone(),
            topology_scratch: TraversalScratch::default(),
        }
    }
}

impl World {
    #[inline]
    pub fn network(&self) -> &Network {
        &self.network
    }

    pub fn add_wire(&mut self, wire: WireId) -> Result<(), NetworkModelError> {
        self.network.add_wire(wire)?;
        self.derived_topology.add_wire(wire);

        Ok(())
    }

    pub fn remove_wire(
        &mut self,
        definitions: &DefinitionRegistry,
        wire: WireId,
    ) -> Result<(), NetworkModelError> {
        self.network.remove_wire(wire)?;
        self.derived_topology.remove_wire(
            definitions,
            &self.network,
            &mut self.topology_scratch,
            wire,
        );

        Ok(())
    }

    pub fn connect_wires(
        &mut self,
        definitions: &DefinitionRegistry,
        wire_a: WireId,
        wire_b: WireId,
    ) -> Result<(), NetworkModelError> {
        self.network.connect_wires(wire_a, wire_b)?;
        self.derived_topology
            .connect_wires(definitions, &self.network, wire_a, wire_b);

        Ok(())
    }

    pub fn disconnect_wires(
        &mut self,
        definitions: &DefinitionRegistry,
        wire_a: WireId,
        wire_b: WireId,
    ) -> Result<(), NetworkModelError> {
        self.network.disconnect_wires(wire_a, wire_b)?;
        self.derived_topology.disconnect_wires(
            definitions,
            &self.network,
            &mut self.topology_scratch,
            wire_a,
            wire_b,
        );

        Ok(())
    }

    pub fn add_device(
        &mut self,
        definitions: &DefinitionRegistry,
        device: DeviceId,
        definition: DefinitionId,
    ) -> Result<(), NetworkModelError> {
        self.network.add_device(definitions, device, definition)?;

        let definition = definitions
            .get(definition)
            .expect("successfully added device definition must remain registered");

        self.derived_topology.add_device(device, definition);

        Ok(())
    }

    pub fn remove_device(
        &mut self,
        definitions: &DefinitionRegistry,
        device: DeviceId,
    ) -> Result<(), NetworkModelError> {
        let affected_nets = self.derived_topology.device_nets(&self.network, device);

        self.network.remove_device(device)?;

        self.derived_topology.remove_device(
            definitions,
            &self.network,
            &mut self.topology_scratch,
            device,
            &affected_nets,
        );

        Ok(())
    }

    pub fn attach_terminal(
        &mut self,
        definitions: &DefinitionRegistry,
        wire: WireId,
        device: DeviceId,
        terminal: TerminalId,
    ) -> Result<(), NetworkModelError> {
        self.network.attach_terminal(wire, device, terminal)?;

        self.derived_topology
            .attach_terminal(definitions, &self.network, wire, device, terminal);

        Ok(())
    }

    pub fn detach_terminal(
        &mut self,

        definitions: &DefinitionRegistry,
        wire: WireId,
        device: DeviceId,
        terminal: TerminalId,
    ) -> Result<(), NetworkModelError> {
        self.network.detach_terminal(wire, device, terminal)?;
        self.derived_topology.detach_terminal(
            definitions,
            &self.network,
            &mut self.topology_scratch,
            wire,
            device,
            terminal,
        );

        Ok(())
    }

    #[inline]
    pub fn set_device_parameter(
        &mut self,
        definitions: &DefinitionRegistry,
        device: DeviceId,
        parameter: ParameterId,
        value: f64,
    ) -> Result<(), NetworkModelError> {
        self.network
            .set_device_parameter(definitions, device, parameter, value)?;

        self.derived_topology.mark_device_numerical_dirty(device);

        Ok(())
    }

    #[inline]
    fn debug_validate_topology(&self, definitions: &DefinitionRegistry) {
        #[cfg(debug_assertions)]
        self.derived_topology
            .assert_consistent(definitions, &self.network);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::DeviceComponent;
    use hynergy_model::device::definition::{DevicePartitionId, PrimitiveElementKind};
    use hynergy_model::device::registry::DefinitionRegistry;
    use hynergy_model::parameter::{ParameterConstraintError, ParameterId};

    fn wire(raw: u32) -> WireId {
        WireId::try_from(raw).unwrap()
    }

    fn device(raw: u32) -> DeviceId {
        DeviceId::try_from(raw).unwrap()
    }

    fn admittance() -> DefinitionId {
        PrimitiveElementKind::Conductance.into()
    }

    #[test]
    fn world_updates_network_and_topology_consistently() {
        let definitions = DefinitionRegistry::new();
        let mut world = World::default();
        let a = wire(1);
        let b = wire(2);
        let d = device(1);

        world.add_wire(a).unwrap();
        world.add_wire(b).unwrap();
        world.connect_wires(&definitions, a, b).unwrap();
        world.add_device(&definitions, d, admittance()).unwrap();
        world
            .attach_terminal(&definitions, a, d, TerminalId::new(0))
            .unwrap();
        world
            .detach_terminal(&definitions, a, d, TerminalId::new(0))
            .unwrap();
        world.disconnect_wires(&definitions, a, b).unwrap();
        world.remove_device(&definitions, d).unwrap();
        world.remove_wire(&definitions, b).unwrap();
        world.remove_wire(&definitions, a).unwrap();

        world
            .derived_topology
            .assert_consistent(&definitions, &world.network);
    }

    #[test]
    fn failed_network_mutation_does_not_change_topology() {
        let definitions = DefinitionRegistry::new();

        let mut world = World::default();
        let a = wire(1);
        let b = wire(2);

        world.add_wire(a).unwrap();
        world.add_wire(b).unwrap();
        world.connect_wires(&definitions, a, b).unwrap();

        let before = world.derived_topology.clone();

        assert_eq!(
            world.connect_wires(&definitions, a, b),
            Err(NetworkModelError::AlreadyConnected)
        );
        assert_eq!(world.derived_topology, before);

        world
            .derived_topology
            .assert_consistent(&definitions, &world.network);
    }

    #[test]
    fn parameter_changes_invalidate_numerics_without_changing_topology() {
        let definitions = DefinitionRegistry::new();
        let mut world = World::default();
        let d = device(1);

        world.add_device(&definitions, d, admittance()).unwrap();

        let component = DeviceComponent::new(d, DevicePartitionId::new(0));
        let island = world.derived_topology.component_island(component);

        let revision = world.derived_topology.island(island).unwrap().revision();

        world.derived_topology.clear_invalidation();

        let before = world.derived_topology.clone();

        world
            .set_device_parameter(&definitions, d, ParameterId::new(0), 1.0)
            .unwrap();

        assert_eq!(world.derived_topology, before);

        assert_eq!(
            world.derived_topology.island(island).unwrap().revision(),
            revision
        );

        assert!(
            world
                .derived_topology
                .invalidation()
                .topology_dirty_islands()
                .is_empty()
        );

        assert_eq!(
            world
                .derived_topology
                .invalidation()
                .numerical_dirty_islands(),
            &[island]
        );

        let invalidation_before_failure = world.derived_topology.invalidation().clone();

        assert_eq!(
            world.set_device_parameter(&definitions, d, ParameterId::new(0), -1.0,),
            Err(NetworkModelError::ParameterConstraint {
                parameter: ParameterId::new(0),
                source: ParameterConstraintError::OutOfRange,
            })
        );

        assert_eq!(
            world.derived_topology.invalidation(),
            &invalidation_before_failure
        );
    }

    #[test]
    fn every_world_command_variant_dispatches() {
        let mut engine = Engine::new();
        let definitions = DefinitionRegistry::new();
        let world = engine.new_world().unwrap();

        let a = wire(1);
        let b = wire(2);
        let d = device(1);

        engine
            .apply_world_command(world, WorldCommand::AddWire { wire: a })
            .unwrap();

        engine
            .apply_world_command(world, WorldCommand::AddWire { wire: b })
            .unwrap();

        engine
            .apply_world_command(
                world,
                WorldCommand::ConnectWires {
                    wire_a: a,
                    wire_b: b,
                },
            )
            .unwrap();

        engine
            .apply_world_command(
                world,
                WorldCommand::DisconnectWires {
                    wire_a: a,
                    wire_b: b,
                },
            )
            .unwrap();

        engine
            .apply_world_command(
                world,
                WorldCommand::AddDevice {
                    device: d,
                    definition: admittance(),
                },
            )
            .unwrap();

        engine
            .apply_world_command(
                world,
                WorldCommand::AttachTerminal {
                    wire: a,
                    device: d,
                    terminal: TerminalId::new(0),
                },
            )
            .unwrap();

        engine
            .apply_world_command(
                world,
                WorldCommand::DetachTerminal {
                    wire: a,
                    device: d,
                    terminal: TerminalId::new(0),
                },
            )
            .unwrap();

        engine
            .apply_world_command(
                world,
                WorldCommand::SetDeviceParameter {
                    device: d,
                    parameter: ParameterId::new(0),
                    value: 1.0,
                },
            )
            .unwrap();

        engine
            .apply_world_command(world, WorldCommand::RemoveDevice { device: d })
            .unwrap();

        engine
            .apply_world_command(world, WorldCommand::RemoveWire { wire: b })
            .unwrap();

        engine
            .apply_world_command(world, WorldCommand::RemoveWire { wire: a })
            .unwrap();

        let world = engine.world(world).unwrap();

        assert!(world.network.wires().iter().all(Option::is_none));
        assert!(world.network.devices().iter().all(Option::is_none));

        world
            .derived_topology
            .assert_consistent(&definitions, &world.network);
    }

    #[test]
    fn command_dispatch_preserves_model_errors() {
        let mut engine = Engine::new();
        let world = engine.new_world().unwrap();
        let wire = wire(1);

        engine
            .apply_world_command(world, WorldCommand::AddWire { wire })
            .unwrap();

        assert_eq!(
            engine.apply_world_command(world, WorldCommand::AddWire { wire },),
            Err(WorldCommandApplyError::Model(
                NetworkModelError::IdAlreadyAssigned { id: wire.id() }
            ))
        );
    }
}
