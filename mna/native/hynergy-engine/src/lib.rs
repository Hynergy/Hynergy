mod compile;
#[cfg(feature = "solver-profiling")]
mod profiling;
mod runtime;
mod state;
mod topology;

use crate::compile::island::{DeviceObserver, IslandCompileError, compile_topology_island};
#[cfg(feature = "solver-profiling")]
pub use crate::profiling::{SolverIslandProfile, SolverIterationProfile, SolverTickProfile};
use crate::runtime::island::{IslandRuntime, IslandRuntimeError};
pub use crate::runtime::subscription::{SubscriptionError, SubscriptionId};
use crate::runtime::subscription::{SubscriptionRegistry, SubscriptionUpdate};
use crate::state::{PhysicalStateError, PhysicalStateStore};
use crate::topology::{DerivedTopology, TraversalScratch};
use hynergy_mna::system::MnaError;
use hynergy_model::device::definition::{
    DefinitionId, DefinitionObserverId, DeviceDefinition, DeviceId, TerminalId,
};
use hynergy_model::device::registry::{DefinitionRegistry, RegisterDeviceError};
use hynergy_model::network::{Network, NetworkModelError, WireId};
use hynergy_model::parameter::ParameterId;
use std::num::NonZeroU32;
use thiserror::Error;

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum EngineTickError {
    #[error("world does not exist")]
    UnknownWorld,

    #[error("device {device:?} parameter {parameter:?} is not assigned")]
    MissingParameter {
        device: DeviceId,
        parameter: ParameterId,
    },

    #[error("island matrix is singular")]
    Singular,

    #[error("nonlinear island did not converge after {iterations} iterations")]
    NonlinearDidNotConverge { iterations: usize },

    #[error("island matrix contains a non-finite value")]
    NonFiniteMatrix,

    #[error("island solution contains a non-finite value")]
    NonFiniteSolution,

    #[error("simulation resource limit was exceeded")]
    ResourceExhausted,

    #[error("MNA backend failed")]
    BackendFailure,

    #[error("island compilation failed")]
    CompilationFailed,

    #[error("simulation internal invariant failed")]
    InternalInvariant,
}

impl From<WorldTickError> for EngineTickError {
    fn from(error: WorldTickError) -> Self {
        match error {
            WorldTickError::Compile(_) => Self::CompilationFailed,

            WorldTickError::Runtime(error) => match error {
                IslandRuntimeError::MissingParameter { device, parameter } => {
                    Self::MissingParameter { device, parameter }
                }

                IslandRuntimeError::NonlinearDidNotConverge { iterations } => {
                    Self::NonlinearDidNotConverge { iterations }
                }

                IslandRuntimeError::NonFiniteMatrix => Self::NonFiniteMatrix,

                IslandRuntimeError::NonFiniteSolution
                | IslandRuntimeError::NonFiniteState { .. } => Self::NonFiniteSolution,

                IslandRuntimeError::Mna(error) => match error {
                    MnaError::Singular { .. } => Self::Singular,

                    MnaError::IndexOverflow | MnaError::OutOfMemory => Self::ResourceExhausted,

                    MnaError::BackendFailure => Self::BackendFailure,

                    MnaError::NotFactorized | MnaError::RhsLengthMismatch { .. } => {
                        Self::InternalInvariant
                    }
                },

                IslandRuntimeError::MissingDevice { .. }
                | IslandRuntimeError::MissingState { .. } => Self::InternalInvariant,
            },

            WorldTickError::State(error) => match error {
                PhysicalStateError::MissingInitialParameter { device, parameter } => {
                    Self::MissingParameter { device, parameter }
                }

                PhysicalStateError::StateNotInitialized { .. } => Self::InternalInvariant,
            },
        }
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngineConfig {
    max_worker_threads: u32,
}

impl EngineConfig {
    pub fn new(max_worker_threads: u32) -> Self {
        Self { max_worker_threads }
    }

    pub fn max_worker_threads(&self) -> u32 {
        self.max_worker_threads
    }
}

#[allow(dead_code)]
pub struct Engine {
    config: EngineConfig,

    definition_registry: DefinitionRegistry,
    universe: Vec<Option<World>>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(EngineConfig {
            max_worker_threads: 1,
        })
    }
}

impl Engine {
    pub const COMPOSITE_DEFINITION_ID_BASE: u32 = DefinitionRegistry::COMPOSITE_DEFINITION_ID_BASE;

    pub fn new(config: EngineConfig) -> Self {
        Self {
            config,
            definition_registry: DefinitionRegistry::new(),
            universe: Vec::new(),
        }
    }

    pub fn subscription_count(&self, world_id: u32) -> Result<usize, SubscriptionError> {
        let world = self
            .universe
            .get(world_id as usize)
            .and_then(Option::as_ref)
            .ok_or(SubscriptionError::UnknownWorld)?;

        Ok(world.subscriptions.subscriptions().len())
    }

    pub fn tick_world(&mut self, world_id: u32) -> Result<(), EngineTickError> {
        let definitions = &self.definition_registry;

        let world = self
            .universe
            .get_mut(world_id as usize)
            .and_then(Option::as_mut)
            .ok_or(EngineTickError::UnknownWorld)?;

        world.tick(definitions).map_err(EngineTickError::from)
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

    pub fn new_world(&mut self, config: WorldConfig) -> Result<u32, WorldManagementError> {
        let id = u32::try_from(self.universe.len())
            .map_err(|_| WorldManagementError::WorldIdExhausted)?;

        self.universe.push(Some(World::new(config)));

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

    #[cfg(feature = "solver-profiling")]
    #[inline]
    pub fn solver_tick_profile(&self, world_id: u32) -> Option<SolverTickProfile> {
        self.world(world_id).map(World::solver_tick_profile)
    }

    #[cfg(test)]
    #[cfg(debug_assertions)]
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

        #[cfg(debug_assertions)]
        world.debug_validate_storage(definitions);

        Ok(())
    }

    #[inline]
    pub fn subscribe_observer(
        &mut self,
        world_id: u32,
        device: DeviceId,
        observer: DefinitionObserverId,
    ) -> Result<SubscriptionId, SubscriptionError> {
        let definitions = &self.definition_registry;

        let world = self
            .universe
            .get_mut(world_id as usize)
            .and_then(Option::as_mut)
            .ok_or(SubscriptionError::UnknownWorld)?;

        world.subscribe_observer(definitions, device, observer)
    }

    #[inline]
    pub fn unsubscribe(
        &mut self,
        world_id: u32,
        subscription: SubscriptionId,
    ) -> Result<(), SubscriptionError> {
        let world = self
            .universe
            .get_mut(world_id as usize)
            .and_then(Option::as_mut)
            .ok_or(SubscriptionError::UnknownWorld)?;

        world.unsubscribe(subscription)
    }

    #[inline]
    pub fn subscription_updates(
        &self,
        world_id: u32,
    ) -> Result<&[SubscriptionUpdate], SubscriptionError> {
        let world = self
            .universe
            .get(world_id as usize)
            .and_then(Option::as_ref)
            .ok_or(SubscriptionError::UnknownWorld)?;

        Ok(world.subscription_updates())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldConfig {
    tick_frequency_hz: NonZeroU32,
    timestep: f64,
}

impl WorldConfig {
    #[inline]
    pub fn new(tick_frequency_hz: NonZeroU32) -> Self {
        Self {
            tick_frequency_hz,
            timestep: 1.0 / f64::from(tick_frequency_hz.get()),
        }
    }

    #[inline]
    pub const fn tick_frequency_hz(self) -> NonZeroU32 {
        self.tick_frequency_hz
    }

    #[inline]
    pub(crate) const fn timestep(self) -> f64 {
        self.timestep
    }
}

#[derive(Debug)]
pub struct World {
    config: WorldConfig,
    network: Network,
    subscriptions: SubscriptionRegistry,
    derived_topology: DerivedTopology,
    topology_scratch: TraversalScratch,
    physical_state: PhysicalStateStore,
    island_runtimes: Vec<Option<IslandRuntime>>,
    subscription_updates: Vec<SubscriptionUpdate>,
}

impl World {
    fn new(config: WorldConfig) -> Self {
        Self {
            config,
            network: Network::new(),
            subscriptions: SubscriptionRegistry::new(),
            derived_topology: DerivedTopology::default(),
            topology_scratch: TraversalScratch::default(),
            physical_state: PhysicalStateStore::default(),
            island_runtimes: Vec::new(),
            subscription_updates: Vec::new(),
        }
    }

    pub(crate) fn tick(&mut self, definitions: &DefinitionRegistry) -> Result<(), WorldTickError> {
        self.subscription_updates.clear();

        self.sync_island_runtimes(definitions)?;

        {
            let topology = &self.derived_topology;
            let network = &self.network;
            let physical_state = &self.physical_state;
            let runtimes = &mut self.island_runtimes;

            for (island, _) in topology.islands() {
                let runtime = runtimes
                    .get_mut(island.index())
                    .and_then(Option::as_mut)
                    .expect("live island must have a runtime after synchronization");

                runtime.prepare_tick_state_inputs(network, physical_state)?;
            }
        }

        {
            let topology = &self.derived_topology;
            let network = &self.network;
            let runtimes = &mut self.island_runtimes;

            for (island, _) in topology.islands() {
                let runtime = runtimes
                    .get_mut(island.index())
                    .and_then(Option::as_mut)
                    .expect("live island must have a runtime after synchronization");

                runtime.solve_prepared_tick(network)?;
            }
        }

        self.commit_runtime_state_outputs()?;

        self.collect_subscription_updates();

        Ok(())
    }

    #[inline]
    pub fn subscription_updates(&self) -> &[SubscriptionUpdate] {
        &self.subscription_updates
    }

    #[cfg(feature = "solver-profiling")]
    pub fn solver_tick_profile(&self) -> SolverTickProfile {
        let islands = self
            .derived_topology
            .islands()
            .filter_map(|(island, _)| {
                self.island_runtimes
                    .get(island.index())
                    .and_then(Option::as_ref)
                    .map(|runtime| {
                        runtime
                            .solver_tick_profile()
                            .clone()
                            .with_island_index(island.index())
                    })
            })
            .collect();

        SolverTickProfile::from_islands(islands)
    }

    fn collect_subscription_updates(&mut self) {
        self.subscription_updates.clear();

        {
            let network = &self.network;
            let topology = &self.derived_topology;
            let runtimes = &self.island_runtimes;
            let updates = &mut self.subscription_updates;

            for subscription in self.subscriptions.subscriptions_mut() {
                let island = topology.component_island(network, subscription.component());

                let runtime = runtimes
                    .get(island.index())
                    .and_then(Option::as_ref)
                    .expect("live subscription component must have an island runtime");

                if subscription.published_bits().is_some() && !runtime.observer_outputs_dirty() {
                    continue;
                }

                let value = runtime
                    .observer_value(subscription.observer())
                    .expect("successful tick must make subscribed observer available");

                let bits = value.to_bits();

                if subscription.published_bits() == Some(bits) {
                    continue;
                }

                subscription.set_published_bits(bits);

                updates.push(SubscriptionUpdate::new(subscription.id(), value));
            }
        }

        for runtime in self.island_runtimes.iter_mut().flatten() {
            runtime.mark_observer_outputs_clean();
        }
    }

    fn subscribe_observer(
        &mut self,
        definitions: &DefinitionRegistry,
        device: DeviceId,
        observer: DefinitionObserverId,
    ) -> Result<SubscriptionId, SubscriptionError> {
        let definition_id = self
            .network
            .device_definition_id(device)
            .map_err(|_| SubscriptionError::UnknownDevice { device })?;

        let definition = definitions
            .get(definition_id)
            .expect("live device definition must remain registered");

        let definition_observer = definition
            .observer(observer)
            .ok_or(SubscriptionError::UnknownObserver { device, observer })?;

        self.subscriptions
            .insert(
                DeviceObserver::new(device, observer),
                definition_observer.partition(),
            )
            .ok_or(SubscriptionError::IdExhausted)
    }

    fn unsubscribe(&mut self, subscription: SubscriptionId) -> Result<(), SubscriptionError> {
        if !self.subscriptions.remove(subscription) {
            return Err(SubscriptionError::UnknownSubscription { subscription });
        }

        Ok(())
    }

    #[inline]
    pub fn add_wire(&mut self, wire: WireId) -> Result<(), NetworkModelError> {
        self.network.add_wire(wire)?;
        self.derived_topology.add_wire(wire);

        Ok(())
    }

    #[inline]
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

    #[inline]
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

    #[inline]
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

    #[inline]
    pub fn add_device(
        &mut self,
        definitions: &DefinitionRegistry,
        device: DeviceId,
        definition: DefinitionId,
    ) -> Result<(), NetworkModelError> {
        let model_insert = self
            .network
            .prepare_add_device(definitions, device, definition)?;

        let definition = definitions
            .get(definition)
            .expect("prepared device definition must remain registered");

        let state_insert = self
            .physical_state
            .prepare_add_device(definition, &model_insert);

        let topology_insert = self
            .derived_topology
            .prepare_add_device(definition, &model_insert);

        let insert = self.network.commit_add_device(model_insert);

        self.physical_state.commit_add_device(state_insert, insert);
        self.derived_topology
            .commit_add_device(device, definition, topology_insert, insert);

        Ok(())
    }

    #[inline]
    pub fn remove_device(
        &mut self,
        definitions: &DefinitionRegistry,
        device: DeviceId,
    ) -> Result<(), NetworkModelError> {
        let topology_removal = self
            .derived_topology
            .prepare_device_removal(&self.network, device);

        let removal = self.network.remove_device(device)?;

        self.physical_state.remove_device(removal);

        self.derived_topology.remove_device(
            definitions,
            &self.network,
            &mut self.topology_scratch,
            device,
            topology_removal,
            removal,
        );

        if let Some(moved) = removal.moved_device() {
            self.derived_topology
                .mark_device_binding_dirty(&self.network, moved);
        }

        if let Some(relocation) = removal.chunk_relocation() {
            let moved_devices = self
                .network
                .device_ids_in_chunk(relocation.to_chunk())
                .expect("relocated model chunk must remain resident");

            for &moved in moved_devices {
                self.derived_topology
                    .mark_device_binding_dirty(&self.network, moved);
            }
        }

        self.subscriptions.remove_device(device);

        Ok(())
    }

    #[inline]
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

    #[inline]
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

        self.derived_topology
            .mark_device_numerical_dirty(&self.network, device);

        Ok(())
    }

    fn sync_island_runtimes(
        &mut self,
        definitions: &DefinitionRegistry,
    ) -> Result<(), WorldTickError> {
        let timestep = self.config.timestep();

        {
            let topology = &self.derived_topology;
            let invalidation = topology.invalidation();

            let topology_dirty = invalidation.topology_dirty_islands();
            let binding_dirty = invalidation.binding_dirty_islands();
            let retired = invalidation.retired_islands();
            let numerical_dirty = invalidation.numerical_dirty_islands();

            let network = &self.network;
            #[cfg(debug_assertions)]
            let physical_state = &self.physical_state;
            let runtimes = &mut self.island_runtimes;

            for &island in retired {
                if let Some(runtime) = runtimes.get_mut(island.index()) {
                    *runtime = None;
                }
            }

            for (island, _) in topology.islands() {
                if runtimes.len() <= island.index() {
                    runtimes.resize_with(island.index() + 1, || None);
                }

                let needs_compile =
                    runtimes[island.index()].is_none() || topology_dirty.contains(&island);

                if !needs_compile {
                    continue;
                }

                let compiled = compile_topology_island(definitions, network, topology, island)?;
                let runtime = IslandRuntime::new(compiled, network, timestep)?;

                runtimes[island.index()] = Some(runtime);
            }

            for &island in binding_dirty {
                let Some(runtime) = runtimes.get_mut(island.index()).and_then(Option::as_mut)
                else {
                    continue;
                };

                runtime.rebind(network)?;
            }

            for &island in numerical_dirty {
                let Some(runtime) = runtimes.get_mut(island.index()).and_then(Option::as_mut)
                else {
                    continue;
                };

                runtime.mark_numerical_dirty();
            }

            #[cfg(debug_assertions)]
            {
                for (island, _) in topology.islands() {
                    let runtime = runtimes
                        .get(island.index())
                        .and_then(Option::as_ref)
                        .expect("live island must have a runtime after synchronization");

                    runtime.debug_assert_bindings_valid(network, physical_state);
                }
            }
        }

        self.derived_topology.clear_invalidation();

        Ok(())
    }

    #[inline]
    fn validate_runtime_state_outputs(&self) -> Result<(), IslandRuntimeError> {
        for (island, _) in self.derived_topology.islands() {
            let runtime = self
                .island_runtimes
                .get(island.index())
                .and_then(Option::as_ref)
                .expect("live island must have a runtime after synchronization");

            runtime.validate_state_outputs(&self.physical_state)?;
        }

        Ok(())
    }

    #[inline]
    fn scatter_runtime_state_outputs(&mut self) {
        let topology = &self.derived_topology;
        let runtimes = &self.island_runtimes;
        let physical_state = &mut self.physical_state;

        for (island, _) in topology.islands() {
            let runtime = runtimes
                .get(island.index())
                .and_then(Option::as_ref)
                .expect("live island must have a runtime after synchronization");

            runtime.scatter_state_outputs(physical_state);
        }
    }

    #[inline]
    fn finalize_runtime_state_outputs(&mut self) {
        let topology = &self.derived_topology;
        let runtimes = &self.island_runtimes;
        let physical_state = &mut self.physical_state;

        for (island, _) in topology.islands() {
            let runtime = runtimes
                .get(island.index())
                .and_then(Option::as_ref)
                .expect("live island must have a runtime after synchronization");

            runtime.finalize_state_outputs(physical_state);
        }
    }

    #[inline]
    fn commit_runtime_state_outputs(&mut self) -> Result<(), IslandRuntimeError> {
        self.validate_runtime_state_outputs()?;

        self.scatter_runtime_state_outputs();
        self.finalize_runtime_state_outputs();

        Ok(())
    }

    #[cfg(debug_assertions)]
    #[inline]
    fn debug_validate_storage(&self, definitions: &DefinitionRegistry) {
        self.derived_topology
            .assert_consistent(definitions, &self.network);
        self.physical_state
            .assert_aligned(definitions, &self.network);
    }

    #[inline]
    pub fn network(&self) -> &Network {
        &self.network
    }

    #[inline]
    pub const fn config(&self) -> WorldConfig {
        self.config
    }
}

#[derive(Debug, Error, PartialEq)]
pub(crate) enum WorldTickError {
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
    use crate::compile::definition::{DefinitionStateId, DefinitionStateInitializer};
    use crate::compile::island::{DeviceState, IslandNode};
    use crate::state::{PhysicalStateAddress, PhysicalStateStore};
    use crate::topology::DeviceComponent;
    use hynergy_model::circuit::{Element, ValueRef};
    use hynergy_model::device::builder::DeviceDefinitionBuilder;
    use hynergy_model::device::definition::{DevicePartitionId, PrimitiveElementKind};
    use hynergy_model::device::registry::DefinitionRegistry;
    use hynergy_model::parameter::{ParameterConstraintError, ParameterId};
    use std::num::NonZeroU32;

    fn world_config() -> WorldConfig {
        WorldConfig::new(NonZeroU32::new(30).unwrap())
    }

    fn wire(raw: u32) -> WireId {
        WireId::try_from(raw).unwrap()
    }

    fn device(raw: u32) -> DeviceId {
        DeviceId::try_from(raw).unwrap()
    }

    fn admittance() -> DefinitionId {
        PrimitiveElementKind::Conductance.into()
    }

    fn register_observed_conductance(engine: &mut Engine) -> DefinitionId {
        let definition = {
            let mut builder = DeviceDefinitionBuilder::new(engine.definitions());

            let positive = builder.add_terminal().unwrap();
            let negative = builder.add_terminal().unwrap();

            builder
                .add_element(Element::new(
                    PrimitiveElementKind::Conductance.into(),
                    vec![positive, negative],
                    vec![ValueRef::Literal(1.0)],
                ))
                .unwrap();

            builder.add_voltage_observer(positive, negative).unwrap();
            builder.build_definition().unwrap()
        };

        engine.register_definition(definition).unwrap()
    }

    fn observed_voltage_world() -> (Engine, u32, DeviceId, DeviceId, DefinitionObserverId) {
        let mut engine = Engine::default();

        let observed_definition = {
            let mut builder = DeviceDefinitionBuilder::new(engine.definitions());

            let positive = builder.add_terminal().unwrap();
            let negative = builder.add_terminal().unwrap();

            builder
                .add_element(Element::new(
                    PrimitiveElementKind::Conductance.into(),
                    vec![positive, negative],
                    vec![ValueRef::Literal(1.0)],
                ))
                .unwrap();

            let observer = builder.add_voltage_observer(positive, negative).unwrap();

            assert_eq!(observer, DefinitionObserverId::new(0));

            builder.build_definition().unwrap()
        };

        let observed_definition = engine.register_definition(observed_definition).unwrap();

        let world = engine.new_world(world_config()).unwrap();

        let negative = wire(1);
        let positive = wire(2);

        let observed = device(1);
        let source = device(2);

        engine
            .apply_world_command(world, WorldCommand::AddWire { wire: negative })
            .unwrap();

        engine
            .apply_world_command(world, WorldCommand::AddWire { wire: positive })
            .unwrap();

        engine
            .apply_world_command(
                world,
                WorldCommand::AddDevice {
                    device: observed,
                    definition: observed_definition,
                },
            )
            .unwrap();

        engine
            .apply_world_command(
                world,
                WorldCommand::AddDevice {
                    device: source,
                    definition: PrimitiveElementKind::VoltageSource.into(),
                },
            )
            .unwrap();

        for device in [observed, source] {
            engine
                .apply_world_command(
                    world,
                    WorldCommand::AttachTerminal {
                        wire: positive,
                        device,
                        terminal: TerminalId::new(0),
                    },
                )
                .unwrap();

            engine
                .apply_world_command(
                    world,
                    WorldCommand::AttachTerminal {
                        wire: negative,
                        device,
                        terminal: TerminalId::new(1),
                    },
                )
                .unwrap();
        }

        engine
            .apply_world_command(
                world,
                WorldCommand::SetDeviceParameter {
                    device: source,
                    parameter: ParameterId::new(0),
                    value: 5.0,
                },
            )
            .unwrap();

        (
            engine,
            world,
            observed,
            source,
            DefinitionObserverId::new(0),
        )
    }

    fn add_device_with_physical_state(
        definitions: &DefinitionRegistry,
        network: &mut Network,
        state: &mut PhysicalStateStore,
        device: DeviceId,
        definition_id: DefinitionId,
    ) {
        let definition = definitions.get(definition_id).unwrap();

        let model_insert = network
            .prepare_add_device(definitions, device, definition_id)
            .unwrap();

        let state_insert = state.prepare_add_device(definition, &model_insert);
        let insert = network.commit_add_device(model_insert);

        state.commit_add_device(state_insert, insert);
    }

    fn add_stateful_device(
        definitions: &DefinitionRegistry,
        network: &mut Network,
        state: &mut PhysicalStateStore,
        device: DeviceId,
        kind: PrimitiveElementKind,
    ) {
        let definition_id = DefinitionId::from(kind);
        let definition = definitions.get(definition_id).unwrap();

        let model_insert = network
            .prepare_add_device(definitions, device, definition_id)
            .unwrap();

        let state_insert = state.prepare_add_device(definition, &model_insert);
        let insert = network.commit_add_device(model_insert);

        state.commit_add_device(state_insert, insert);
    }

    #[test]
    fn sleeping_island_does_not_reevaluate_existing_subscription() {
        let (mut engine, world_id, observed, _, observer) = observed_voltage_world();

        engine
            .subscribe_observer(world_id, observed, observer)
            .unwrap();

        let component = DeviceComponent::new(observed, DevicePartitionId::new(0));

        let world = engine.world(world_id).unwrap();

        let island = world
            .derived_topology
            .component_island(world.network(), component);

        engine.tick_world(world_id).unwrap();

        {
            let runtime = engine.world(world_id).unwrap().island_runtimes[island.index()]
                .as_ref()
                .unwrap();

            assert_eq!(runtime.observer_read_count(), 1);
        }

        assert_eq!(engine.subscription_updates(world_id).unwrap().len(), 1,);

        engine.tick_world(world_id).unwrap();

        assert!(engine.subscription_updates(world_id).unwrap().is_empty(),);

        let runtime = engine.world(world_id).unwrap().island_runtimes[island.index()]
            .as_ref()
            .unwrap();

        assert_eq!(
            runtime.observer_read_count(),
            1,
            "existing subscription on a sleeping island \
         should not be reevaluated",
        );
    }

    #[test]
    fn subscription_ids_are_monotonic_and_not_reused() {
        let mut engine = Engine::default();

        let definition = register_observed_conductance(&mut engine);

        let world = engine
            .new_world(WorldConfig::new(NonZeroU32::new(20).unwrap()))
            .unwrap();

        let device = DeviceId::try_from(1).unwrap();

        engine
            .apply_world_command(world, WorldCommand::AddDevice { device, definition })
            .unwrap();

        let observer = DefinitionObserverId::new(0);

        let first = engine.subscribe_observer(world, device, observer).unwrap();
        let second = engine.subscribe_observer(world, device, observer).unwrap();

        assert_eq!(first, SubscriptionId::try_from(1).unwrap());
        assert_eq!(second, SubscriptionId::try_from(2).unwrap());

        engine.unsubscribe(world, first).unwrap();

        let third = engine.subscribe_observer(world, device, observer).unwrap();

        assert_eq!(third, SubscriptionId::try_from(3).unwrap());
    }

    #[test]
    fn subscribing_unknown_definition_observer_is_rejected() {
        let mut engine = Engine::default();

        let definition = register_observed_conductance(&mut engine);

        let world = engine
            .new_world(WorldConfig::new(NonZeroU32::new(20).unwrap()))
            .unwrap();

        let device = DeviceId::try_from(1).unwrap();

        engine
            .apply_world_command(world, WorldCommand::AddDevice { device, definition })
            .unwrap();

        let observer = DefinitionObserverId::new(1);

        assert_eq!(
            engine.subscribe_observer(world, device, observer),
            Err(SubscriptionError::UnknownObserver { device, observer }),
        );
    }

    #[test]
    fn removing_device_destroys_its_subscriptions() {
        let mut engine = Engine::default();
        let definition = register_observed_conductance(&mut engine);

        let world = engine
            .new_world(WorldConfig::new(NonZeroU32::new(20).unwrap()))
            .unwrap();

        let device = DeviceId::try_from(1).unwrap();

        engine
            .apply_world_command(world, WorldCommand::AddDevice { device, definition })
            .unwrap();

        let subscription = engine
            .subscribe_observer(world, device, DefinitionObserverId::new(0))
            .unwrap();

        engine
            .apply_world_command(world, WorldCommand::RemoveDevice { device })
            .unwrap();

        assert_eq!(
            engine.unsubscribe(world, subscription),
            Err(SubscriptionError::UnknownSubscription { subscription }),
        );
    }

    #[test]
    fn observer_subscription_publishes_initial_value_once() {
        let (mut engine, world, observed, _, observer) = observed_voltage_world();

        let subscription = engine
            .subscribe_observer(world, observed, observer)
            .unwrap();

        assert!(engine.subscription_updates(world).unwrap().is_empty());

        engine.tick_world(world).unwrap();

        let updates = engine.subscription_updates(world).unwrap();

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].subscription(), subscription);
        assert_eq!(updates[0].value().to_bits(), 5.0f64.to_bits(),);

        engine.tick_world(world).unwrap();

        assert!(engine.subscription_updates(world).unwrap().is_empty());
    }

    #[test]
    fn observer_subscription_publishes_changed_value() {
        let (mut engine, world, observed, source, observer) = observed_voltage_world();

        let subscription = engine
            .subscribe_observer(world, observed, observer)
            .unwrap();

        engine.tick_world(world).unwrap();

        assert_eq!(engine.subscription_updates(world).unwrap().len(), 1,);

        engine.tick_world(world).unwrap();

        assert!(engine.subscription_updates(world).unwrap().is_empty());

        engine
            .apply_world_command(
                world,
                WorldCommand::SetDeviceParameter {
                    device: source,
                    parameter: ParameterId::new(0),
                    value: 7.0,
                },
            )
            .unwrap();

        engine.tick_world(world).unwrap();

        let updates = engine.subscription_updates(world).unwrap();

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].subscription(), subscription);
        assert_eq!(updates[0].value().to_bits(), 7.0f64.to_bits(),);

        engine.tick_world(world).unwrap();

        assert!(engine.subscription_updates(world).unwrap().is_empty());
    }

    #[test]
    fn failed_tick_does_not_advance_subscription_baseline() {
        let (mut engine, world, observed, source, observer) = observed_voltage_world();

        let subscription = engine
            .subscribe_observer(world, observed, observer)
            .unwrap();

        engine.tick_world(world).unwrap();

        {
            let updates = engine.subscription_updates(world).unwrap();

            assert_eq!(updates.len(), 1);
            assert_eq!(updates[0].subscription(), subscription);
            assert_eq!(updates[0].value().to_bits(), 5.0f64.to_bits(),);
        }

        engine
            .apply_world_command(
                world,
                WorldCommand::SetDeviceParameter {
                    device: source,
                    parameter: ParameterId::new(0),
                    value: 7.0,
                },
            )
            .unwrap();

        let failing_negative = wire(3);
        let failing_positive = wire(4);
        let failing_source = device(3);

        engine
            .apply_world_command(
                world,
                WorldCommand::AddWire {
                    wire: failing_negative,
                },
            )
            .unwrap();

        engine
            .apply_world_command(
                world,
                WorldCommand::AddWire {
                    wire: failing_positive,
                },
            )
            .unwrap();

        engine
            .apply_world_command(
                world,
                WorldCommand::AddDevice {
                    device: failing_source,
                    definition: PrimitiveElementKind::VoltageSource.into(),
                },
            )
            .unwrap();

        engine
            .apply_world_command(
                world,
                WorldCommand::AttachTerminal {
                    wire: failing_positive,
                    device: failing_source,
                    terminal: TerminalId::new(0),
                },
            )
            .unwrap();

        engine
            .apply_world_command(
                world,
                WorldCommand::AttachTerminal {
                    wire: failing_negative,
                    device: failing_source,
                    terminal: TerminalId::new(1),
                },
            )
            .unwrap();

        assert_eq!(
            engine.tick_world(world),
            Err(EngineTickError::MissingParameter {
                device: failing_source,
                parameter: ParameterId::new(0),
            }),
        );

        assert!(engine.subscription_updates(world).unwrap().is_empty());

        engine
            .apply_world_command(
                world,
                WorldCommand::SetDeviceParameter {
                    device: failing_source,
                    parameter: ParameterId::new(0),
                    value: 1.0,
                },
            )
            .unwrap();

        engine.tick_world(world).unwrap();

        let updates = engine.subscription_updates(world).unwrap();

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].subscription(), subscription);
        assert_eq!(updates[0].value().to_bits(), 7.0f64.to_bits(),);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "live wire is missing from derived nets")]
    fn successful_world_command_checks_derived_topology() {
        let mut engine = Engine::default();
        let world_id = engine.new_world(world_config()).unwrap();

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
        let mut world = World::new(world_config());
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

        let mut world = World::new(world_config());
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
        let mut world = World::new(world_config());
        let d = device(1);

        world.add_device(&definitions, d, admittance()).unwrap();

        let component = DeviceComponent::new(d, DevicePartitionId::new(0));
        let island = world
            .derived_topology
            .component_island(world.network(), component);

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
            Err(NetworkModelError::ParameterConstraintViolation {
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
        let mut engine = Engine::default();
        let definitions = DefinitionRegistry::new();
        let world = engine.new_world(world_config()).unwrap();

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
        assert!(world.network.iter_devices().next().is_none());

        world
            .derived_topology
            .assert_consistent(&definitions, &world.network);
    }

    #[test]
    fn command_dispatch_preserves_model_errors() {
        let mut engine = Engine::default();
        let world = engine.new_world(world_config()).unwrap();
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
        let mut world = World::new(world_config());
        let delay = device(1);

        world
            .add_device(&definitions, delay, PrimitiveElementKind::TickDelay.into())
            .unwrap();

        let input_component = DeviceComponent::new(delay, DevicePartitionId::new(0));
        let output_component = DeviceComponent::new(delay, DevicePartitionId::new(1));

        let input_island = world
            .derived_topology
            .component_island(world.network(), input_component);
        let output_island = world
            .derived_topology
            .component_island(world.network(), output_component);

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
    fn removing_device_removes_physical_state() {
        let definitions = DefinitionRegistry::new();
        let mut world = World::new(world_config());

        let capacitor = device(1);

        world
            .add_device(
                &definitions,
                capacitor,
                PrimitiveElementKind::Capacitor.into(),
            )
            .unwrap();

        let state = DeviceState::new(capacitor, DefinitionStateId::new(0));

        assert_eq!(world.physical_state.get(&world.network, state,), Some(0.0),);

        world.remove_device(&definitions, capacitor).unwrap();

        assert_eq!(world.physical_state.get(&world.network, state,), None,);
    }

    #[test]
    fn world_tick_delays_value_across_separate_islands() {
        let definitions = DefinitionRegistry::new();
        let mut world = World::new(world_config());

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

        let input_island = world.derived_topology.component_island(
            world.network(),
            DeviceComponent::new(delay, DevicePartitionId::new(0)),
        );

        let output_island = world.derived_topology.component_island(
            world.network(),
            DeviceComponent::new(delay, DevicePartitionId::new(1)),
        );

        assert_ne!(input_island, output_island,);

        let positive_node = IslandNode::net(world.derived_topology.wire_net(output_positive));
        let negative_node = IslandNode::net(world.derived_topology.wire_net(output_negative));

        let physical_state = DeviceState::new(delay, DefinitionStateId::new(0));

        world.tick(&definitions).unwrap();

        let runtime = world.island_runtimes[output_island.index()]
            .as_ref()
            .unwrap();

        let first_output = runtime.node_voltage(positive_node).unwrap()
            - runtime.node_voltage(negative_node).unwrap();

        assert!((first_output - 4.0).abs() < 1.0e-12);

        assert_eq!(
            world.physical_state.get(&world.network, physical_state),
            Some(9.0),
        );

        world.tick(&definitions).unwrap();

        let runtime = world.island_runtimes[output_island.index()]
            .as_ref()
            .unwrap();

        let second_output = runtime.node_voltage(positive_node).unwrap()
            - runtime.node_voltage(negative_node).unwrap();

        assert!((second_output - 9.0).abs() < 1.0e-12);
    }

    #[test]
    fn failed_initialization_does_not_freeze_tick_delay_initial_state() {
        let definitions = DefinitionRegistry::new();
        let mut world = World::new(world_config());

        let input_negative = wire(1);
        let input_positive = wire(2);
        let output_negative = wire(3);
        let output_positive = wire(4);

        let source = device(1);
        let delay = device(2);
        let load = device(3);
        let blocker = device(4);

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
            .add_device(
                &definitions,
                blocker,
                PrimitiveElementKind::TickDelay.into(),
            )
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
            .set_device_parameter(&definitions, source, ParameterId::new(0), 5.0)
            .unwrap();

        world
            .set_device_parameter(&definitions, delay, ParameterId::new(0), 4.25)
            .unwrap();

        world
            .set_device_parameter(&definitions, load, ParameterId::new(0), 1.0)
            .unwrap();

        assert_eq!(
            world.tick(&definitions),
            Err(WorldTickError::Runtime(
                IslandRuntimeError::MissingParameter {
                    device: blocker,
                    parameter: ParameterId::new(0),
                },
            )),
        );

        let location = world.network.device_location(delay).unwrap();

        assert!(
            !world.physical_state.is_initialized_at(location),
            "a failed tick must not commit first-time state initialization",
        );

        world
            .set_device_parameter(&definitions, delay, ParameterId::new(0), 9.0)
            .unwrap();

        world.remove_device(&definitions, blocker).unwrap();

        let output_island = world.derived_topology.component_island(
            world.network(),
            DeviceComponent::new(delay, DevicePartitionId::new(1)),
        );

        let positive_node = IslandNode::net(world.derived_topology.wire_net(output_positive));
        let negative_node = IslandNode::net(world.derived_topology.wire_net(output_negative));

        world.tick(&definitions).unwrap();

        let runtime = world.island_runtimes[output_island.index()]
            .as_ref()
            .unwrap();

        let output = runtime.node_voltage(positive_node).unwrap()
            - runtime.node_voltage(negative_node).unwrap();

        assert!(
            (output - 9.0).abs() < 1.0e-12,
            "successful retry must re-evaluate the current TickDelay initial parameter",
        );

        assert!(
            world.physical_state.is_initialized_at(location),
            "successful tick must commit the state row",
        );
    }

    #[test]
    fn failed_state_validation_does_not_modify_state() {
        let definitions = DefinitionRegistry::new();
        let mut world = World::new(world_config());

        let capacitor = device(1);

        world
            .add_device(
                &definitions,
                capacitor,
                PrimitiveElementKind::Capacitor.into(),
            )
            .unwrap();

        let state = DeviceState::new(capacitor, DefinitionStateId::new(0));
        let missing = DeviceState::new(capacitor, DefinitionStateId::new(1));
        let location = world.network.device_location(capacitor).unwrap();
        let valid_address = PhysicalStateAddress::new(location, 0);
        let missing_address = PhysicalStateAddress::new(location, 1);

        assert_eq!(world.physical_state.get_at(valid_address), Some(0.0),);
        assert!(!world.physical_state.is_initialized_at(location),);

        world
            .physical_state
            .validate_address(state, valid_address)
            .unwrap();

        let error = world
            .physical_state
            .validate_address(missing, missing_address)
            .unwrap_err();

        assert_eq!(
            error,
            PhysicalStateError::StateNotInitialized { state: missing },
        );

        assert_eq!(
            world.physical_state.get_at(valid_address),
            Some(0.0),
            "validation failure must not modify an earlier valid state",
        );

        assert!(
            !world.physical_state.is_initialized_at(location),
            "validation failure must not initialize the row",
        );
    }

    #[test]
    fn parameter_change_refreshes_cached_island_runtime() {
        let definitions = DefinitionRegistry::new();
        let mut world = World::new(world_config());

        let negative = wire(1);
        let positive = wire(2);

        let source = device(1);
        let conductance = device(2);

        world.add_wire(negative).unwrap();
        world.add_wire(positive).unwrap();

        world
            .add_device(
                &definitions,
                source,
                PrimitiveElementKind::VoltageSource.into(),
            )
            .unwrap();

        world
            .add_device(
                &definitions,
                conductance,
                PrimitiveElementKind::Conductance.into(),
            )
            .unwrap();

        for device in [source, conductance] {
            world
                .attach_terminal(&definitions, positive, device, TerminalId::new(0))
                .unwrap();

            world
                .attach_terminal(&definitions, negative, device, TerminalId::new(1))
                .unwrap();
        }

        world
            .set_device_parameter(&definitions, source, ParameterId::new(0), 5.0)
            .unwrap();

        world
            .set_device_parameter(&definitions, conductance, ParameterId::new(0), 1.0)
            .unwrap();

        world.tick(&definitions).unwrap();

        let island = world.derived_topology.component_island(
            world.network(),
            DeviceComponent::new(source, DevicePartitionId::new(0)),
        );

        let positive_node = IslandNode::net(world.derived_topology.wire_net(positive));
        let negative_node = IslandNode::net(world.derived_topology.wire_net(negative));

        let runtime = world.island_runtimes[island.index()].as_ref().unwrap();

        let voltage = runtime.node_voltage(positive_node).unwrap()
            - runtime.node_voltage(negative_node).unwrap();

        assert!((voltage - 5.0).abs() < 1.0e-12);

        world
            .set_device_parameter(&definitions, source, ParameterId::new(0), 9.0)
            .unwrap();

        world.tick(&definitions).unwrap();

        let runtime = world.island_runtimes[island.index()].as_ref().unwrap();

        let voltage = runtime.node_voltage(positive_node).unwrap()
            - runtime.node_voltage(negative_node).unwrap();

        assert!((voltage - 9.0).abs() < 1.0e-12);
    }

    #[test]
    fn schmitt_buffer_initializes_low() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let switch = device(1);

        let mut states = PhysicalStateStore::default();

        add_device_with_physical_state(
            &definitions,
            &mut network,
            &mut states,
            switch,
            PrimitiveElementKind::SchmittBuffer.into(),
        );

        assert_eq!(
            states.get(
                &network,
                DeviceState::new(switch, DefinitionStateId::new(0),)
            ),
            Some(0.0),
        );
    }

    #[test]
    fn engine_ticks_configured_world() {
        let mut engine = Engine::default();

        assert_eq!(engine.tick_world(0), Err(EngineTickError::UnknownWorld),);

        let world = engine.new_world(world_config()).unwrap();

        assert_eq!(engine.tick_world(world), Ok(()),);
    }

    #[test]
    fn nonconvergent_stateless_switch_reports_convergence_failure() {
        let definitions = DefinitionRegistry::new();
        let mut world = World::new(world_config());

        let common = wire(1);
        let output = wire(2);

        let switch = device(1);
        let source = device(2);

        world.add_wire(common).unwrap();
        world.add_wire(output).unwrap();

        world
            .add_device(
                &definitions,
                switch,
                PrimitiveElementKind::VoltageControlledSwitch.into(),
            )
            .unwrap();

        world
            .add_device(
                &definitions,
                source,
                PrimitiveElementKind::CurrentSource.into(),
            )
            .unwrap();

        world
            .attach_terminal(&definitions, output, switch, TerminalId::new(0))
            .unwrap();

        world
            .attach_terminal(&definitions, common, switch, TerminalId::new(1))
            .unwrap();

        world
            .attach_terminal(&definitions, output, switch, TerminalId::new(2))
            .unwrap();

        world
            .attach_terminal(&definitions, common, switch, TerminalId::new(3))
            .unwrap();

        world
            .attach_terminal(&definitions, common, source, TerminalId::new(0))
            .unwrap();

        world
            .attach_terminal(&definitions, output, source, TerminalId::new(1))
            .unwrap();

        for (index, value) in [5.0, 4.0, 1.0].into_iter().enumerate() {
            world
                .set_device_parameter(&definitions, switch, ParameterId::new(index as u32), value)
                .unwrap();
        }

        world
            .set_device_parameter(&definitions, source, ParameterId::new(0), 8.0)
            .unwrap();

        let error = world.tick(&definitions).unwrap_err();

        assert!(matches!(
            error,
            WorldTickError::Runtime(IslandRuntimeError::NonlinearDidNotConverge { .. })
        ));

        assert_eq!(
            world.physical_state.get(
                &world.network,
                DeviceState::new(switch, DefinitionStateId::new(0))
            ),
            None,
        );
    }

    #[test]
    fn engine_tick_reports_missing_device_parameter() {
        let mut engine = Engine::default();

        let world_id = engine.new_world(world_config()).unwrap();
        let device = device(1);

        engine
            .apply_world_command(
                world_id,
                WorldCommand::AddDevice {
                    device,
                    definition: PrimitiveElementKind::Conductance.into(),
                },
            )
            .unwrap();

        assert_eq!(
            engine.tick_world(world_id),
            Err(EngineTickError::MissingParameter {
                device,
                parameter: ParameterId::new(0),
            }),
        );
    }

    #[test]
    fn composite_tick_delay_uses_old_state_until_next_tick() {
        let mut definitions = DefinitionRegistry::new();

        let tick_delay = DefinitionId::from(PrimitiveElementKind::TickDelay);

        let initial_constraint = definitions.get(tick_delay).unwrap().parameters()[0];

        let composite = {
            let mut builder = DeviceDefinitionBuilder::new(&definitions);

            let input_positive = builder.add_terminal().unwrap();

            let input_negative = builder.add_terminal().unwrap();

            let output_positive = builder.add_terminal().unwrap();

            let output_negative = builder.add_terminal().unwrap();

            let initial = builder.add_parameter(initial_constraint).unwrap();

            builder
                .add_element(Element::new(
                    tick_delay,
                    vec![
                        input_positive,
                        input_negative,
                        output_positive,
                        output_negative,
                    ],
                    vec![ValueRef::Parameter(initial)],
                ))
                .unwrap();

            builder.build_definition().unwrap()
        };

        assert_eq!(composite.state_count(), 1);
        assert_eq!(composite.partition_count(), 2);

        let composite = definitions.register(composite).unwrap();

        let mut world = World::new(world_config());

        let input_negative = WireId::try_from(1).unwrap();
        let input_positive = WireId::try_from(2).unwrap();
        let output_negative = WireId::try_from(3).unwrap();
        let output_positive = WireId::try_from(4).unwrap();

        for wire in [
            input_negative,
            input_positive,
            output_negative,
            output_positive,
        ] {
            world.add_wire(wire).unwrap();
        }

        let delay = DeviceId::try_from(1).unwrap();
        let source = DeviceId::try_from(2).unwrap();

        world.add_device(&definitions, delay, composite).unwrap();

        world
            .add_device(
                &definitions,
                source,
                PrimitiveElementKind::VoltageSource.into(),
            )
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
            .attach_terminal(&definitions, input_positive, source, TerminalId::new(0))
            .unwrap();

        world
            .attach_terminal(&definitions, input_negative, source, TerminalId::new(1))
            .unwrap();

        world
            .set_device_parameter(&definitions, source, ParameterId::new(0), 5.0)
            .unwrap();

        world
            .set_device_parameter(&definitions, delay, ParameterId::new(0), 1.5)
            .unwrap();

        let output_component = DeviceComponent::new(delay, DevicePartitionId::new(1));

        let output_island = world
            .derived_topology
            .component_island(world.network(), output_component);

        let output_positive_node =
            IslandNode::net(world.derived_topology.wire_net(output_positive));

        let output_negative_node =
            IslandNode::net(world.derived_topology.wire_net(output_negative));

        let state = DeviceState::new(delay, DefinitionStateId::new(0));

        world.tick(&definitions).unwrap();

        let output_runtime = world
            .island_runtimes
            .get(output_island.index())
            .and_then(Option::as_ref)
            .unwrap();

        let voltage = output_runtime.node_voltage(output_positive_node).unwrap()
            - output_runtime.node_voltage(output_negative_node).unwrap();

        assert!((voltage - 1.5).abs() < 1.0e-12);
        assert_eq!(world.physical_state.get(&world.network, state), Some(5.0),);

        world.tick(&definitions).unwrap();

        let output_runtime = world
            .island_runtimes
            .get(output_island.index())
            .and_then(Option::as_ref)
            .unwrap();

        let voltage = output_runtime.node_voltage(output_positive_node).unwrap()
            - output_runtime.node_voltage(output_negative_node).unwrap();

        assert!((voltage - 5.0).abs() < 1.0e-12);

        assert_eq!(world.physical_state.get(&world.network, state), Some(5.0),);
    }

    #[test]
    fn world_uses_configured_fixed_tick_frequency() {
        let config = WorldConfig::new(NonZeroU32::new(20).unwrap());

        assert_eq!(config.tick_frequency_hz(), NonZeroU32::new(20).unwrap(),);

        assert!((config.timestep() - 0.05).abs() < 1.0e-12);

        let mut engine = Engine::default();

        let world_id = engine.new_world(config).unwrap();

        let world = engine.world(world_id).unwrap();

        assert_eq!(world.config(), config);
    }

    #[test]
    fn new_subscription_on_sleeping_island_publishes_cached_value_once() {
        let (mut engine, world_id, observed, _, observer) = observed_voltage_world();

        let component = DeviceComponent::new(observed, DevicePartitionId::new(0));

        let world = engine.world(world_id).unwrap();

        let island = world
            .derived_topology
            .component_island(world.network(), component);

        engine.tick_world(world_id).unwrap();

        {
            let runtime = engine.world(world_id).unwrap().island_runtimes[island.index()]
                .as_ref()
                .unwrap();

            assert_eq!(runtime.observer_read_count(), 0);
        }

        let subscription = engine
            .subscribe_observer(world_id, observed, observer)
            .unwrap();

        engine.tick_world(world_id).unwrap();

        {
            let updates = engine.subscription_updates(world_id).unwrap();

            assert_eq!(updates.len(), 1);
            assert_eq!(updates[0].subscription(), subscription);
            assert_eq!(updates[0].value().to_bits(), 5.0f64.to_bits());
        }

        {
            let runtime = engine.world(world_id).unwrap().island_runtimes[island.index()]
                .as_ref()
                .unwrap();

            assert_eq!(runtime.observer_read_count(), 1);
        }

        engine.tick_world(world_id).unwrap();

        assert!(engine.subscription_updates(world_id).unwrap().is_empty(),);

        let runtime = engine.world(world_id).unwrap().island_runtimes[island.index()]
            .as_ref()
            .unwrap();

        assert_eq!(runtime.observer_read_count(), 1);
    }

    #[test]
    fn topology_change_recompiles_and_wakes_sleeping_island() {
        let mut engine = Engine::default();

        let fixed_load = {
            let definition = {
                let mut builder = DeviceDefinitionBuilder::new(engine.definitions());

                let positive = builder.add_terminal().unwrap();
                let negative = builder.add_terminal().unwrap();

                builder
                    .add_element(Element::new(
                        PrimitiveElementKind::Conductance.into(),
                        vec![positive, negative],
                        vec![ValueRef::Literal(1.0)],
                    ))
                    .unwrap();

                builder.build_definition().unwrap()
            };

            engine.register_definition(definition).unwrap()
        };

        let world = engine.new_world(world_config()).unwrap();

        let negative = wire(1);
        let positive = wire(2);

        let source = device(1);
        let first_load = device(2);
        let second_load = device(3);

        for wire in [negative, positive] {
            engine
                .apply_world_command(world, WorldCommand::AddWire { wire })
                .unwrap();
        }

        engine
            .apply_world_command(
                world,
                WorldCommand::AddDevice {
                    device: source,
                    definition: PrimitiveElementKind::VoltageSource.into(),
                },
            )
            .unwrap();

        engine
            .apply_world_command(
                world,
                WorldCommand::AddDevice {
                    device: first_load,
                    definition: fixed_load,
                },
            )
            .unwrap();

        for device in [source, first_load] {
            engine
                .apply_world_command(
                    world,
                    WorldCommand::AttachTerminal {
                        wire: positive,
                        device,
                        terminal: TerminalId::new(0),
                    },
                )
                .unwrap();

            engine
                .apply_world_command(
                    world,
                    WorldCommand::AttachTerminal {
                        wire: negative,
                        device,
                        terminal: TerminalId::new(1),
                    },
                )
                .unwrap();
        }

        engine
            .apply_world_command(
                world,
                WorldCommand::SetDeviceParameter {
                    device: source,
                    parameter: ParameterId::new(0),
                    value: 5.0,
                },
            )
            .unwrap();

        let subscription = engine
            .subscribe_observer(world, source, DefinitionObserverId::new(1))
            .unwrap();

        engine.tick_world(world).unwrap();

        {
            let updates = engine.subscription_updates(world).unwrap();

            assert_eq!(updates.len(), 1);
            assert_eq!(updates[0].subscription(), subscription);
            assert_eq!(updates[0].value().to_bits(), (-5.0f64).to_bits());
        }

        engine.tick_world(world).unwrap();

        assert!(engine.subscription_updates(world).unwrap().is_empty(),);

        engine
            .apply_world_command(
                world,
                WorldCommand::AddDevice {
                    device: second_load,
                    definition: fixed_load,
                },
            )
            .unwrap();

        engine
            .apply_world_command(
                world,
                WorldCommand::AttachTerminal {
                    wire: positive,
                    device: second_load,
                    terminal: TerminalId::new(0),
                },
            )
            .unwrap();

        engine
            .apply_world_command(
                world,
                WorldCommand::AttachTerminal {
                    wire: negative,
                    device: second_load,
                    terminal: TerminalId::new(1),
                },
            )
            .unwrap();

        engine.tick_world(world).unwrap();

        {
            let updates = engine.subscription_updates(world).unwrap();

            assert_eq!(updates.len(), 1);
            assert_eq!(updates[0].subscription(), subscription);
            assert_eq!(updates[0].value().to_bits(), (-10.0f64).to_bits(),);
        }

        engine.tick_world(world).unwrap();

        assert!(engine.subscription_updates(world).unwrap().is_empty(),);
    }

    #[test]
    fn parameter_change_wakes_sleeping_island_and_it_sleeps_again() {
        let (mut engine, world_id, observed, source, observer) = observed_voltage_world();

        let component = DeviceComponent::new(observed, DevicePartitionId::new(0));

        let world = engine.world(world_id).unwrap();

        let island = world
            .derived_topology
            .component_island(world.network(), component);

        let subscription = engine
            .subscribe_observer(world_id, observed, observer)
            .unwrap();

        engine.tick_world(world_id).unwrap();

        {
            let updates = engine.subscription_updates(world_id).unwrap();

            assert_eq!(updates.len(), 1);
            assert_eq!(updates[0].subscription(), subscription);
            assert_eq!(updates[0].value().to_bits(), 5.0f64.to_bits());
        }

        {
            let runtime = engine.world(world_id).unwrap().island_runtimes[island.index()]
                .as_ref()
                .unwrap();

            assert_eq!(runtime.observer_read_count(), 1);
        }

        engine.tick_world(world_id).unwrap();

        assert!(engine.subscription_updates(world_id).unwrap().is_empty(),);

        {
            let runtime = engine.world(world_id).unwrap().island_runtimes[island.index()]
                .as_ref()
                .unwrap();

            assert_eq!(runtime.observer_read_count(), 1);
        }

        engine
            .apply_world_command(
                world_id,
                WorldCommand::SetDeviceParameter {
                    device: source,
                    parameter: ParameterId::new(0),
                    value: 7.0,
                },
            )
            .unwrap();

        engine.tick_world(world_id).unwrap();

        {
            let updates = engine.subscription_updates(world_id).unwrap();

            assert_eq!(updates.len(), 1);
            assert_eq!(updates[0].subscription(), subscription);
            assert_eq!(updates[0].value().to_bits(), 7.0f64.to_bits());
        }

        {
            let runtime = engine.world(world_id).unwrap().island_runtimes[island.index()]
                .as_ref()
                .unwrap();

            assert_eq!(runtime.observer_read_count(), 2);
        }

        engine.tick_world(world_id).unwrap();

        assert!(engine.subscription_updates(world_id).unwrap().is_empty(),);

        let runtime = engine.world(world_id).unwrap().island_runtimes[island.index()]
            .as_ref()
            .unwrap();

        assert_eq!(runtime.observer_read_count(), 2);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn state_and_topology_sidecars_stay_aligned_through_row_and_chunk_relocation() {
        let definitions = DefinitionRegistry::new();
        let mut world = World::new(world_config());

        let stateless = device(1);

        world
            .add_device(
                &definitions,
                stateless,
                DefinitionId::from(PrimitiveElementKind::Conductance),
            )
            .unwrap();

        world.debug_validate_storage(&definitions);

        for raw in 2..=66 {
            world
                .add_device(
                    &definitions,
                    device(raw),
                    DefinitionId::from(PrimitiveElementKind::TickDelay),
                )
                .unwrap();

            world.debug_validate_storage(&definitions);
        }

        let middle = device(10);

        world.remove_device(&definitions, middle).unwrap();
        world.debug_validate_storage(&definitions);

        world.remove_device(&definitions, stateless).unwrap();
        world.debug_validate_storage(&definitions);

        assert_eq!(world.network().iter_device_ids().count(), 64);
        assert!(world.network().device(middle).is_err());

        let relocated = world.network().device_location(device(66)).unwrap();

        assert_eq!(relocated.chunk_index(), 0);
        assert_eq!(relocated.row(), 0);
    }

    #[test]
    fn warmed_stateful_tick_reads_committed_state_only_through_physical_bindings() {
        let definitions = DefinitionRegistry::new();

        let mut world = World::new(WorldConfig::new(NonZeroU32::new(30).unwrap()));

        let ground = WireId::try_from(1).unwrap();
        let node = WireId::try_from(2).unwrap();
        let device = DeviceId::try_from(1).unwrap();

        world.add_wire(ground).unwrap();
        world.add_wire(node).unwrap();

        world
            .add_device(
                &definitions,
                device,
                DefinitionId::from(PrimitiveElementKind::Capacitor),
            )
            .unwrap();

        world
            .set_device_parameter(&definitions, device, ParameterId::new(0), 1.0e-6)
            .unwrap();

        world
            .attach_terminal(&definitions, ground, device, TerminalId::new(0))
            .unwrap();

        world
            .attach_terminal(&definitions, node, device, TerminalId::new(1))
            .unwrap();

        world.tick(&definitions).unwrap();

        world.physical_state.reset_read_counts();

        world.tick(&definitions).unwrap();

        assert_eq!(
            world.physical_state.semantic_read_count(),
            0,
            "warmed runtime must not resolve committed state through DeviceId",
        );

        assert!(
            world.physical_state.physical_read_count() > 0,
            "stateful runtime must read committed state through a physical binding",
        );
    }

    #[test]
    fn row_relocation_rebinds_survivor_runtime_without_topology_recompile() {
        let definitions = DefinitionRegistry::new();

        let mut world = World::new(WorldConfig::new(NonZeroU32::new(30).unwrap()));

        let definition = DefinitionId::from(PrimitiveElementKind::VoltageSource);

        let first = DeviceId::try_from(1).unwrap();
        let removed = DeviceId::try_from(2).unwrap();
        let moved = DeviceId::try_from(3).unwrap();

        for (index, device) in [first, removed, moved].into_iter().enumerate() {
            world.add_device(&definitions, device, definition).unwrap();

            world
                .set_device_parameter(
                    &definitions,
                    device,
                    ParameterId::new(0),
                    (index + 1) as f64,
                )
                .unwrap();
        }

        let moved_component = DeviceComponent::new(moved, DevicePartitionId::new(0));

        let moved_island = world
            .derived_topology
            .component_island(&world.network, moved_component);

        world.tick(&definitions).unwrap();

        let revision_before = world
            .derived_topology
            .island(moved_island)
            .unwrap()
            .revision();

        let rebinds_before = world.island_runtimes[moved_island.index()]
            .as_ref()
            .unwrap()
            .binding_rebind_count();

        assert_eq!(world.network.device_location(moved).unwrap().row(), 2,);

        world.remove_device(&definitions, removed).unwrap();

        assert_eq!(
            world.network.device_location(moved).unwrap().row(),
            1,
            "removing the middle physical row must swap-move the final device",
        );

        assert_eq!(
            world
                .derived_topology
                .island(moved_island)
                .unwrap()
                .revision(),
            revision_before,
            "physical row relocation must not alter topology",
        );

        assert!(
            world
                .derived_topology
                .invalidation()
                .binding_dirty_islands()
                .contains(&moved_island),
            "the moved survivor's runtime binding must be invalidated",
        );

        world.tick(&definitions).unwrap();

        assert_eq!(
            world
                .derived_topology
                .island(moved_island)
                .unwrap()
                .revision(),
            revision_before,
            "binding repair must not recompile topology",
        );

        assert_eq!(
            world.island_runtimes[moved_island.index()]
                .as_ref()
                .unwrap()
                .binding_rebind_count(),
            rebinds_before + 1,
            "the existing runtime must be rebound exactly once",
        );
    }

    #[test]
    fn chunk_relocation_rebinds_every_survivor_runtime_without_topology_recompile() {
        let definitions = DefinitionRegistry::new();

        let mut world = World::new(WorldConfig::new(NonZeroU32::new(30).unwrap()));

        let removed = DeviceId::try_from(1).unwrap();

        world
            .add_device(
                &definitions,
                removed,
                DefinitionId::from(PrimitiveElementKind::Conductance),
            )
            .unwrap();

        world
            .set_device_parameter(&definitions, removed, ParameterId::new(0), 1.0)
            .unwrap();

        let moved = [
            DeviceId::try_from(2).unwrap(),
            DeviceId::try_from(3).unwrap(),
            DeviceId::try_from(4).unwrap(),
        ];

        for (index, device) in moved.into_iter().enumerate() {
            world
                .add_device(
                    &definitions,
                    device,
                    DefinitionId::from(PrimitiveElementKind::VoltageSource),
                )
                .unwrap();

            world
                .set_device_parameter(
                    &definitions,
                    device,
                    ParameterId::new(0),
                    (index + 1) as f64,
                )
                .unwrap();

            assert_eq!(
                world.network.device_location(device).unwrap().chunk_index(),
                1,
            );
        }

        let components =
            moved.map(|device| DeviceComponent::new(device, DevicePartitionId::new(0)));

        let islands = components.map(|component| {
            world
                .derived_topology
                .component_island(&world.network, component)
        });

        assert_ne!(islands[0], islands[1]);
        assert_ne!(islands[1], islands[2]);
        assert_ne!(islands[0], islands[2]);

        world.tick(&definitions).unwrap();

        let revisions =
            islands.map(|island| world.derived_topology.island(island).unwrap().revision());

        let rebinds = islands.map(|island| {
            world.island_runtimes[island.index()]
                .as_ref()
                .unwrap()
                .binding_rebind_count()
        });

        world.remove_device(&definitions, removed).unwrap();

        for device in moved {
            assert_eq!(
                world.network.device_location(device).unwrap().chunk_index(),
                0,
            );
        }

        for (index, island) in islands.into_iter().enumerate() {
            assert_eq!(
                world.derived_topology.island(island).unwrap().revision(),
                revisions[index],
                "physical chunk relocation must not modify topology",
            );

            assert!(
                world
                    .derived_topology
                    .invalidation()
                    .binding_dirty_islands()
                    .contains(&island),
                "every island in the relocated chunk must become binding-dirty",
            );
        }

        world.tick(&definitions).unwrap();

        for (index, island) in islands.into_iter().enumerate() {
            assert_eq!(
                world.derived_topology.island(island).unwrap().revision(),
                revisions[index],
            );

            assert_eq!(
                world.island_runtimes[island.index()]
                    .as_ref()
                    .unwrap()
                    .binding_rebind_count(),
                rebinds[index] + 1,
                "each relocated survivor runtime must be rebound exactly once",
            );
        }
    }

    #[test]
    #[should_panic(expected = "stale parameter binding")]
    fn debug_runtime_sync_detects_unconsumed_stale_binding() {
        let definitions = DefinitionRegistry::new();

        let mut world = World::new(WorldConfig::new(NonZeroU32::new(30).unwrap()));

        let definition = DefinitionId::from(PrimitiveElementKind::VoltageSource);

        let first = DeviceId::try_from(1).unwrap();
        let removed = DeviceId::try_from(2).unwrap();
        let moved = DeviceId::try_from(3).unwrap();

        for (index, device) in [first, removed, moved].into_iter().enumerate() {
            world.add_device(&definitions, device, definition).unwrap();

            world
                .set_device_parameter(
                    &definitions,
                    device,
                    ParameterId::new(0),
                    (index + 1) as f64,
                )
                .unwrap();
        }

        world.tick(&definitions).unwrap();

        assert_eq!(world.network.device_location(moved).unwrap().row(), 2,);

        world.remove_device(&definitions, removed).unwrap();

        assert_eq!(world.network.device_location(moved).unwrap().row(), 1,);

        assert!(
            !world
                .derived_topology
                .invalidation()
                .binding_dirty_islands()
                .is_empty(),
        );

        world.derived_topology.clear_invalidation();
        world.tick(&definitions).unwrap();
    }

    #[test]
    fn uninitialized_physical_read_returns_literal_without_committing_row() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut state = PhysicalStateStore::default();

        let device = device(1);

        add_stateful_device(
            &definitions,
            &mut network,
            &mut state,
            device,
            PrimitiveElementKind::Capacitor,
        );

        let location = network.device_location(device).unwrap();

        let address = PhysicalStateAddress::new(location, 0);

        assert!(!state.is_initialized_at(location));

        assert_eq!(
            state
                .read_logical_at(
                    &network,
                    device,
                    address,
                    DefinitionStateInitializer::Literal(0.0),
                )
                .unwrap(),
            0.0,
        );

        assert!(
            !state.is_initialized_at(location),
            "logical initial-state reads must not commit initialization",
        );
    }

    #[test]
    fn uninitialized_parameter_backed_state_is_re_evaluated_until_commit() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut state = PhysicalStateStore::default();

        let device = device(1);

        add_stateful_device(
            &definitions,
            &mut network,
            &mut state,
            device,
            PrimitiveElementKind::TickDelay,
        );

        network
            .set_device_parameter(&definitions, device, ParameterId::new(0), 3.25)
            .unwrap();

        let location = network.device_location(device).unwrap();

        let address = PhysicalStateAddress::new(location, 0);

        let initializer = DefinitionStateInitializer::Parameter(ParameterId::new(0));

        assert_eq!(
            state
                .read_logical_at(&network, device, address, initializer,)
                .unwrap(),
            3.25,
        );

        assert!(!state.is_initialized_at(location));

        network
            .set_device_parameter(&definitions, device, ParameterId::new(0), 7.5)
            .unwrap();

        assert_eq!(
            state
                .read_logical_at(&network, device, address, initializer,)
                .unwrap(),
            7.5,
            "uncommitted initial state must follow the current initial parameter",
        );

        assert!(!state.is_initialized_at(location));
    }

    #[test]
    fn committed_state_overrides_logical_initializer() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut state = PhysicalStateStore::default();

        let device = device(1);

        add_stateful_device(
            &definitions,
            &mut network,
            &mut state,
            device,
            PrimitiveElementKind::TickDelay,
        );

        network
            .set_device_parameter(&definitions, device, ParameterId::new(0), 3.25)
            .unwrap();

        let location = network.device_location(device).unwrap();
        let address = PhysicalStateAddress::new(location, 0);
        let semantic = DeviceState::new(device, DefinitionStateId::new(0));

        state.validate_address(semantic, address).unwrap();
        state.write_prevalidated(address, 11.0);
        state.mark_initialized_prevalidated(location);

        assert!(state.is_initialized_at(location));

        network
            .set_device_parameter(&definitions, device, ParameterId::new(0), 7.5)
            .unwrap();

        assert_eq!(
            state
                .read_logical_at(
                    &network,
                    device,
                    address,
                    DefinitionStateInitializer::Parameter(ParameterId::new(0),),
                )
                .unwrap(),
            11.0,
            "committed history must take precedence over the initializer",
        );
    }

    #[test]
    fn missing_logical_initial_parameter_does_not_initialize_state() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut state = PhysicalStateStore::default();

        let device = device(1);

        add_stateful_device(
            &definitions,
            &mut network,
            &mut state,
            device,
            PrimitiveElementKind::TickDelay,
        );

        let location = network.device_location(device).unwrap();

        let address = PhysicalStateAddress::new(location, 0);

        let error = state
            .read_logical_at(
                &network,
                device,
                address,
                DefinitionStateInitializer::Parameter(ParameterId::new(0)),
            )
            .unwrap_err();

        assert_eq!(
            error,
            PhysicalStateError::MissingInitialParameter {
                device,
                parameter: ParameterId::new(0),
            },
        );

        assert!(!state.is_initialized_at(location),);
    }

    #[test]
    fn failed_global_state_validation_prevents_all_scatter() {
        let definitions = DefinitionRegistry::new();
        let mut world = World::new(world_config());

        let first_a = wire(1);
        let first_b = wire(2);
        let second_a = wire(3);
        let second_b = wire(4);

        for wire in [first_a, first_b, second_a, second_b] {
            world.add_wire(wire).unwrap();
        }

        let first_conductance = device(1);
        let first_capacitor = device(2);
        let second_conductance = device(3);
        let second_capacitor = device(4);

        for (device, kind) in [
            (first_conductance, PrimitiveElementKind::Conductance),
            (first_capacitor, PrimitiveElementKind::Capacitor),
            (second_conductance, PrimitiveElementKind::Conductance),
            (second_capacitor, PrimitiveElementKind::Capacitor),
        ] {
            world.add_device(&definitions, device, kind.into()).unwrap();

            world
                .set_device_parameter(&definitions, device, ParameterId::new(0), 1.0)
                .unwrap();
        }

        world.sync_island_runtimes(&definitions).unwrap();

        let first_state = DeviceState::new(first_capacitor, DefinitionStateId::new(0));
        let first_location = world.network.device_location(first_capacitor).unwrap();
        let first_address = PhysicalStateAddress::new(first_location, 0);

        world
            .physical_state
            .validate_address(first_state, first_address)
            .unwrap();

        world.physical_state.write_prevalidated(first_address, 7.0);

        assert_eq!(world.physical_state.get_at(first_address), Some(7.0),);
        assert!(!world.physical_state.is_initialized_at(first_location),);

        let second_island = world.derived_topology.component_island(
            &world.network,
            DeviceComponent::new(second_capacitor, DevicePartitionId::new(0)),
        );

        let second_location = world.network.device_location(second_capacitor).unwrap();

        world.island_runtimes[second_island.index()]
            .as_mut()
            .unwrap()
            .set_state_output_address_for_test(0, PhysicalStateAddress::new(second_location, 1));

        let error = world.commit_runtime_state_outputs().unwrap_err();

        assert_eq!(
            error,
            IslandRuntimeError::MissingState {
                device: second_capacitor,
                state: DefinitionStateId::new(0),
            },
        );

        assert_eq!(
            world.physical_state.get_at(first_address),
            Some(7.0),
            "failure in another island must occur before any earlier scalar is scattered",
        );

        assert!(
            !world.physical_state.is_initialized_at(first_location),
            "failed global validation must not finalize any earlier row",
        );
    }
}
