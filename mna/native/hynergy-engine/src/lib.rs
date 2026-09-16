mod compile;
mod runtime;
mod topology;

use crate::compile::island::{DeviceState, IslandCompileError, compile_topology_island};
use crate::runtime::island::{IslandRuntime, IslandRuntimeError, StagedStateWrite};
use crate::topology::{DerivedTopology, TraversalScratch};
use hynergy_model::device::definition::{
    DefinitionId, DeviceBody, DeviceDefinition, DeviceId, PrimitiveElementKind, TerminalId,
};
use hynergy_model::device::registry::{DefinitionRegistry, RegisterDeviceError};
use hynergy_model::network::{Network, NetworkModelError, WireId};
use hynergy_model::parameter::ParameterId;
use thiserror::Error;

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

    #[cfg(test)]
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

        world.debug_validate_topology(definitions);

        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct World {
    network: Network,
    derived_topology: DerivedTopology,
    topology_scratch: TraversalScratch,
    physical_state: PhysicalStateStore,
    island_runtimes: Vec<Option<IslandRuntime>>,
}

impl Clone for World {
    fn clone(&self) -> Self {
        Self {
            network: self.network.clone(),

            derived_topology: self.derived_topology.clone(),

            topology_scratch: TraversalScratch::default(),

            physical_state: self.physical_state.clone(),

            island_runtimes: Vec::new(),
        }
    }
}

impl World {
    pub(crate) fn tick(
        &mut self,
        definitions: &DefinitionRegistry,
        timestep: f64,
    ) -> Result<(), WorldTickError> {
        if !timestep.is_finite() || timestep <= 0.0 {
            return Err(WorldTickError::InvalidTimestep);
        }

        self.sync_island_runtimes(definitions)?;

        self.initialize_physical_state(definitions)?;

        let live_islands = self
            .derived_topology
            .islands()
            .map(|(island, _)| island)
            .collect::<Vec<_>>();

        let network = &self.network;

        let old_state = &self.physical_state;

        let runtimes = &mut self.island_runtimes;

        let mut staged = Vec::<StagedStateWrite>::new();

        for island in live_islands {
            let runtime = runtimes
                .get_mut(island.index())
                .and_then(Option::as_mut)
                .expect("live island must have a runtime after synchronization");

            let writes = runtime.solve_tick(network, timestep, |state| old_state.get(state))?;

            staged.extend(writes);
        }

        self.physical_state.commit_staged(&staged)?;

        Ok(())
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

        self.physical_state.remove_device(device);

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

    fn initialize_physical_state(
        &mut self,
        definitions: &DefinitionRegistry,
    ) -> Result<(), PhysicalStateError> {
        for (index, slot) in self.network.devices().iter().enumerate() {
            if slot.is_none() {
                continue;
            }

            let raw = u32::try_from(index + 1).expect("device index must fit DeviceId");

            let device = DeviceId::try_from(raw).expect("device IDs are one-based");

            self.physical_state
                .initialize_device(definitions, &self.network, device)?;
        }

        Ok(())
    }

    fn sync_island_runtimes(
        &mut self,
        definitions: &DefinitionRegistry,
    ) -> Result<(), WorldTickError> {
        let live_islands = self
            .derived_topology
            .islands()
            .map(|(island, _)| island)
            .collect::<Vec<_>>();

        let topology_dirty = self
            .derived_topology
            .invalidation()
            .topology_dirty_islands()
            .to_vec();

        let retired = self
            .derived_topology
            .invalidation()
            .retired_islands()
            .to_vec();

        for island in retired {
            if let Some(runtime) = self.island_runtimes.get_mut(island.index()) {
                *runtime = None;
            }
        }

        for island in live_islands {
            if self.island_runtimes.len() <= island.index() {
                self.island_runtimes
                    .resize_with(island.index() + 1, || None);
            }

            let needs_compile =
                self.island_runtimes[island.index()].is_none() || topology_dirty.contains(&island);

            if !needs_compile {
                continue;
            }

            let compiled = compile_topology_island(
                definitions,
                &self.network,
                &self.derived_topology,
                island,
            )?;

            let runtime = IslandRuntime::new(compiled)?;

            self.island_runtimes[island.index()] = Some(runtime);
        }

        self.derived_topology.clear_invalidation();

        Ok(())
    }

    #[inline]
    fn debug_validate_topology(&self, definitions: &DefinitionRegistry) {
        #[cfg(debug_assertions)]
        self.derived_topology
            .assert_consistent(definitions, &self.network);
    }

    #[inline]
    pub fn network(&self) -> &Network {
        &self.network
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhysicalStateError {
    #[error("device {device:?} initial parameter {parameter:?} is not assigned")]
    MissingInitialParameter {
        device: DeviceId,
        parameter: ParameterId,
    },

    #[error("composite device state initialization is not implemented")]
    CompositeNotYetSupported,

    #[error("state {state:?} is not initialized")]
    StateNotInitialized { state: DeviceState },

    #[error("state {state:?} has more than one staged write")]
    DuplicateWrite { state: DeviceState },
}

#[derive(Debug, Default, Clone)]
pub(crate) struct PhysicalStateStore {
    devices: Vec<Option<Box<[f64]>>>,
}

impl PhysicalStateStore {
    #[inline]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn initialize_device(
        &mut self,
        definitions: &DefinitionRegistry,
        network: &Network,
        device: DeviceId,
    ) -> Result<(), PhysicalStateError> {
        if self
            .devices
            .get(device.index())
            .is_some_and(Option::is_some)
        {
            return Ok(());
        }

        if self.devices.len() <= device.index() {
            self.devices.resize_with(device.index() + 1, || None);
        }

        let definition_id = network
            .device_definition_id(device)
            .expect("state initialization device must exist");

        let definition = definitions
            .get(definition_id)
            .expect("state initialization definition must remain registered");

        let mut state = vec![0.0; definition.state_count()];

        match definition.body() {
            DeviceBody::Primitive(PrimitiveElementKind::TickDelay) => {
                debug_assert_eq!(state.len(), 1,);

                let parameter = ParameterId::new(0);

                let device_slot = network
                    .devices()
                    .get(device.index())
                    .and_then(Option::as_ref)
                    .expect("state initialization device must exist");

                let initial = device_slot
                    .parameters()
                    .get(parameter.index())
                    .copied()
                    .flatten()
                    .ok_or(PhysicalStateError::MissingInitialParameter { device, parameter })?;

                state[0] = initial;
            }

            DeviceBody::Primitive(
                PrimitiveElementKind::Capacitor | PrimitiveElementKind::Inductor,
            ) => {
                debug_assert_eq!(state.len(), 1,);

                // Zero initial voltage/current.
            }

            DeviceBody::Primitive(_) => {
                debug_assert!(state.is_empty(),);
            }

            DeviceBody::Composite(_) => {
                if !state.is_empty() {
                    return Err(PhysicalStateError::CompositeNotYetSupported);
                }
            }
        }

        self.devices[device.index()] = Some(state.into_boxed_slice());

        Ok(())
    }

    #[inline]
    pub(crate) fn get(&self, state: DeviceState) -> Option<f64> {
        self.devices
            .get(state.device().index())
            .and_then(Option::as_ref)
            .and_then(|values| values.get(state.state().index()))
            .copied()
    }

    #[inline]
    pub(crate) fn remove_device(&mut self, device: DeviceId) {
        if let Some(slot) = self.devices.get_mut(device.index()) {
            *slot = None;
        }
    }

    pub(crate) fn commit_staged(
        &mut self,
        writes: &[StagedStateWrite],
    ) -> Result<(), PhysicalStateError> {
        for (index, write) in writes.iter().enumerate() {
            let state = write.state();

            if writes[..index]
                .iter()
                .any(|previous| previous.state() == state)
            {
                return Err(PhysicalStateError::DuplicateWrite { state });
            }

            let value = self
                .devices
                .get_mut(state.device().index())
                .and_then(Option::as_mut)
                .and_then(|values| values.get_mut(state.state().index()))
                .ok_or(PhysicalStateError::StateNotInitialized { state })?;

            *value = write.value();
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub(crate) enum WorldTickError {
    #[error("timestep must be finite and greater than zero")]
    InvalidTimestep,

    #[error(transparent)]
    Compile(#[from] IslandCompileError),

    #[error(transparent)]
    Runtime(#[from] IslandRuntimeError),

    #[error(transparent)]
    State(#[from] PhysicalStateError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::definition::DefinitionStateId;
    use crate::compile::island::IslandNode;
    use crate::runtime::island::StagedStateWrite;
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
    #[cfg(debug_assertions)]
    #[should_panic(expected = "live wire is missing from derived nets")]
    fn successful_world_command_checks_derived_topology() {
        let mut engine = Engine::new();
        let world_id = engine.new_world().unwrap();

        let untracked = wire(1);
        let added = wire(2);

        engine
            .world_mut(world_id)
            .unwrap()
            .network
            .add_wire(untracked)
            .unwrap();

        engine
            .apply_world_command(world_id, WorldCommand::AddWire { wire: added })
            .unwrap();
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

    #[test]
    fn parameter_change_invalidates_all_component_islands() {
        let definitions = DefinitionRegistry::new();
        let mut world = World::default();
        let delay = device(1);

        world
            .add_device(&definitions, delay, PrimitiveElementKind::TickDelay.into())
            .unwrap();

        let input_component = DeviceComponent::new(delay, DevicePartitionId::new(0));
        let output_component = DeviceComponent::new(delay, DevicePartitionId::new(1));

        let input_island = world.derived_topology.component_island(input_component);
        let output_island = world.derived_topology.component_island(output_component);

        assert_ne!(input_island, output_island);

        let input_revision = world
            .derived_topology
            .island(input_island)
            .unwrap()
            .revision();

        let output_revision = world
            .derived_topology
            .island(output_island)
            .unwrap()
            .revision();

        world.derived_topology.clear_invalidation();

        world
            .set_device_parameter(&definitions, delay, ParameterId::new(0), 1.0)
            .unwrap();

        assert_eq!(
            world
                .derived_topology
                .island(input_island)
                .unwrap()
                .revision(),
            input_revision,
        );

        assert_eq!(
            world
                .derived_topology
                .island(output_island)
                .unwrap()
                .revision(),
            output_revision,
        );

        assert!(
            world
                .derived_topology
                .invalidation()
                .topology_dirty_islands()
                .is_empty()
        );

        let dirty = world
            .derived_topology
            .invalidation()
            .numerical_dirty_islands();

        assert_eq!(dirty.len(), 2);
        assert!(dirty.contains(&input_island));
        assert!(dirty.contains(&output_island));
    }

    #[test]
    fn tick_delay_state_initializes_once_from_parameter() {
        let definitions = DefinitionRegistry::new();

        let mut world = World::default();

        let delay = device(1);

        world
            .add_device(&definitions, delay, PrimitiveElementKind::TickDelay.into())
            .unwrap();

        world
            .set_device_parameter(&definitions, delay, ParameterId::new(0), 4.25)
            .unwrap();

        world
            .physical_state
            .initialize_device(&definitions, &world.network, delay)
            .unwrap();

        let state = DeviceState::new(delay, DefinitionStateId::new(0));

        assert_eq!(world.physical_state.get(state), Some(4.25),);

        // The parameter is only the initial value.
        world
            .set_device_parameter(&definitions, delay, ParameterId::new(0), 9.0)
            .unwrap();

        world
            .physical_state
            .initialize_device(&definitions, &world.network, delay)
            .unwrap();

        assert_eq!(world.physical_state.get(state), Some(4.25),);

        world
            .physical_state
            .commit_staged(&[StagedStateWrite::new(state, 7.5)])
            .unwrap();

        assert_eq!(world.physical_state.get(state), Some(7.5),);
    }

    #[test]
    fn removing_device_removes_physical_state() {
        let definitions = DefinitionRegistry::new();

        let mut world = World::default();

        let capacitor = device(1);

        world
            .add_device(
                &definitions,
                capacitor,
                PrimitiveElementKind::Capacitor.into(),
            )
            .unwrap();

        world
            .physical_state
            .initialize_device(&definitions, &world.network, capacitor)
            .unwrap();

        let state = DeviceState::new(capacitor, DefinitionStateId::new(0));

        assert_eq!(world.physical_state.get(state), Some(0.0),);

        world.remove_device(&definitions, capacitor).unwrap();

        assert_eq!(world.physical_state.get(state), None,);
    }

    #[test]
    fn world_tick_delays_value_across_separate_islands() {
        let definitions = DefinitionRegistry::new();

        let mut world = World::default();

        let input_negative = wire(1);
        let input_positive = wire(2);
        let output_negative = wire(3);
        let output_positive = wire(4);

        let source = device(1);
        let delay = device(2);
        let load = device(3);

        for wire in [
            input_negative,
            input_positive,
            output_negative,
            output_positive,
        ] {
            world.add_wire(wire).unwrap();
        }

        world
            .add_device(
                &definitions,
                source,
                PrimitiveElementKind::VoltageSource.into(),
            )
            .unwrap();

        world
            .add_device(&definitions, delay, PrimitiveElementKind::TickDelay.into())
            .unwrap();

        world
            .add_device(&definitions, load, PrimitiveElementKind::Conductance.into())
            .unwrap();

        world
            .attach_terminal(&definitions, input_positive, source, TerminalId::new(0))
            .unwrap();

        world
            .attach_terminal(&definitions, input_negative, source, TerminalId::new(1))
            .unwrap();

        world
            .attach_terminal(&definitions, input_positive, delay, TerminalId::new(0))
            .unwrap();

        world
            .attach_terminal(&definitions, input_negative, delay, TerminalId::new(1))
            .unwrap();

        world
            .attach_terminal(&definitions, output_positive, delay, TerminalId::new(2))
            .unwrap();

        world
            .attach_terminal(&definitions, output_negative, delay, TerminalId::new(3))
            .unwrap();

        world
            .attach_terminal(&definitions, output_positive, load, TerminalId::new(0))
            .unwrap();

        world
            .attach_terminal(&definitions, output_negative, load, TerminalId::new(1))
            .unwrap();

        world
            .set_device_parameter(&definitions, source, ParameterId::new(0), 9.0)
            .unwrap();

        world
            .set_device_parameter(&definitions, delay, ParameterId::new(0), 4.0)
            .unwrap();

        world
            .set_device_parameter(&definitions, load, ParameterId::new(0), 1.0)
            .unwrap();

        let input_island = world
            .derived_topology
            .component_island(DeviceComponent::new(delay, DevicePartitionId::new(0)));

        let output_island = world
            .derived_topology
            .component_island(DeviceComponent::new(delay, DevicePartitionId::new(1)));

        assert_ne!(input_island, output_island,);

        let positive_node = IslandNode::net(world.derived_topology.wire_net(output_positive));

        let negative_node = IslandNode::net(world.derived_topology.wire_net(output_negative));

        let physical_state = DeviceState::new(delay, DefinitionStateId::new(0));

        world.tick(&definitions, 1.0).unwrap();

        let runtime = world.island_runtimes[output_island.index()]
            .as_ref()
            .unwrap();

        let first_output = runtime.node_voltage(positive_node).unwrap()
            - runtime.node_voltage(negative_node).unwrap();

        assert!((first_output - 4.0).abs() < 1.0e-12);

        assert_eq!(world.physical_state.get(physical_state), Some(9.0),);

        world.tick(&definitions, 1.0).unwrap();

        let runtime = world.island_runtimes[output_island.index()]
            .as_ref()
            .unwrap();

        let second_output = runtime.node_voltage(positive_node).unwrap()
            - runtime.node_voltage(negative_node).unwrap();

        assert!((second_output - 9.0).abs() < 1.0e-12);
    }
}
