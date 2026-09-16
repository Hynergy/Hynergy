mod compile;
mod runtime;
mod topology;

use crate::compile::island::{
    DeviceObserver, DeviceState, IslandCompileError, compile_topology_island,
};
use crate::runtime::island::{IslandRuntime, IslandRuntimeError, StagedStateWrite};
pub use crate::runtime::subscription::{SubscriptionError, SubscriptionId};
use crate::runtime::subscription::{SubscriptionRegistry, SubscriptionUpdate};
use crate::topology::{DerivedTopology, TraversalScratch};
use hynergy_mna::system::MnaError;
use hynergy_model::circuit::ValueRef;
use hynergy_model::device::definition::{
    DefinitionId, DefinitionObserverId, DeviceBody, DeviceDefinition, DeviceId,
    PrimitiveElementKind, TerminalId,
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

                IslandRuntimeError::NonFiniteSolution => Self::NonFiniteSolution,

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

                PhysicalStateError::StateNotInitialized { .. }
                | PhysicalStateError::DuplicateWrite { .. } => Self::InternalInvariant,
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

pub struct Engine {
    definition_registry: DefinitionRegistry,

    universe: Vec<Option<World>>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub const COMPOSITE_DEFINITION_ID_BASE: u32 = DefinitionRegistry::COMPOSITE_DEFINITION_ID_BASE;

    pub fn new() -> Self {
        Self {
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
            physical_state: PhysicalStateStore::new(),
            island_runtimes: Vec::new(),
            subscription_updates: Vec::new(),
        }
    }

    pub(crate) fn tick(&mut self, definitions: &DefinitionRegistry) -> Result<(), WorldTickError> {
        self.subscription_updates.clear();

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

            let writes = runtime.solve_tick(network, |state| old_state.get(state))?;

            staged.extend(writes);
        }

        self.physical_state.commit_staged(&staged)?;

        self.collect_subscription_updates();

        Ok(())
    }

    #[inline]
    pub fn subscription_updates(&self) -> &[SubscriptionUpdate] {
        &self.subscription_updates
    }

    fn collect_subscription_updates(&mut self) {
        self.subscription_updates.clear();

        let topology = &self.derived_topology;
        let runtimes = &self.island_runtimes;
        let updates = &mut self.subscription_updates;

        for subscription in self.subscriptions.subscriptions_mut() {
            let island = topology.component_island(subscription.component());

            let runtime = runtimes
                .get(island.index())
                .and_then(Option::as_ref)
                .expect("live subscription component must have an island runtime");

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
        self.network.add_device(definitions, device, definition)?;

        let definition = definitions
            .get(definition)
            .expect("successfully added device definition must remain registered");

        self.derived_topology.add_device(device, definition);

        Ok(())
    }

    #[inline]
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

        self.derived_topology.mark_device_numerical_dirty(device);

        Ok(())
    }

    #[inline]
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
        let timestep = self.config.timestep();

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

        let numerical_dirty = self
            .derived_topology
            .invalidation()
            .numerical_dirty_islands()
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

            let runtime = IslandRuntime::new(compiled, timestep)?;

            self.island_runtimes[island.index()] = Some(runtime);
        }

        for island in numerical_dirty {
            let Some(runtime) = self
                .island_runtimes
                .get_mut(island.index())
                .and_then(Option::as_mut)
            else {
                continue;
            };

            runtime.mark_numerical_dirty();
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

    #[inline]
    pub const fn config(&self) -> WorldConfig {
        self.config
    }
}

#[derive(Debug, Clone, Copy)]
enum InitialParameterValue {
    Value(f64),
    Unassigned(ParameterId),
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhysicalStateError {
    #[error("device {device:?} initial parameter {parameter:?} is not assigned")]
    MissingInitialParameter {
        device: DeviceId,
        parameter: ParameterId,
    },

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

        let device_slot = network
            .devices()
            .get(device.index())
            .and_then(Option::as_ref)
            .expect("state initialization device must exist");

        let parameters = device_slot
            .parameters()
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let parameter = ParameterId::new(
                    u32::try_from(index).expect("definition parameter index must fit ParameterId"),
                );

                match value {
                    Some(value) => InitialParameterValue::Value(*value),

                    None => InitialParameterValue::Unassigned(parameter),
                }
            })
            .collect::<Vec<_>>();

        let mut state = Vec::with_capacity(definition.state_count());

        initialize_definition_state(definitions, definition, &parameters, device, &mut state)?;

        debug_assert_eq!(
            state.len(),
            definition.state_count(),
            "recursive state initialization must produce exactly \
         the definition state count",
        );

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

            let exists = self
                .devices
                .get(state.device().index())
                .and_then(Option::as_ref)
                .and_then(|values| values.get(state.state().index()))
                .is_some();

            if !exists {
                return Err(PhysicalStateError::StateNotInitialized { state });
            }
        }

        for write in writes {
            let state = write.state();

            let value = self.devices[state.device().index()]
                .as_mut()
                .expect("validated state device must remain initialized")
                .get_mut(state.state().index())
                .expect("validated state slot must remain initialized");

            *value = write.value();
        }

        Ok(())
    }
}

fn initialize_definition_state(
    definitions: &DefinitionRegistry,
    definition: &DeviceDefinition,
    parameters: &[InitialParameterValue],
    device: DeviceId,
    state: &mut Vec<f64>,
) -> Result<(), PhysicalStateError> {
    debug_assert_eq!(
        parameters.len(),
        definition.parameters().len(),
        "initial parameter mapping must match definition",
    );

    match definition.body() {
        DeviceBody::Primitive(PrimitiveElementKind::TickDelay) => {
            debug_assert_eq!(definition.state_count(), 1,);

            let initial = match parameters[0] {
                InitialParameterValue::Value(value) => value,

                InitialParameterValue::Unassigned(parameter) => {
                    return Err(PhysicalStateError::MissingInitialParameter { device, parameter });
                }
            };

            state.push(initial);
        }

        DeviceBody::Primitive(
            PrimitiveElementKind::Capacitor
            | PrimitiveElementKind::Inductor
            | PrimitiveElementKind::VoltageControlledSwitch,
        ) => {
            debug_assert_eq!(definition.state_count(), 1,);

            state.push(0.0);
        }

        DeviceBody::Primitive(_) => {
            debug_assert_eq!(definition.state_count(), 0,);
        }

        DeviceBody::Composite(circuit) => {
            for element in circuit.elements() {
                let child = definitions
                    .get(element.definition())
                    .expect("registered composite child must remain registered");

                let mut child_parameters = Vec::with_capacity(element.parameters().len());

                for value in element.parameters() {
                    let value = match *value {
                        ValueRef::Literal(value) => InitialParameterValue::Value(value),

                        ValueRef::Parameter(parameter) => parameters[parameter.index()],
                    };

                    child_parameters.push(value);
                }

                let before = state.len();

                initialize_definition_state(definitions, child, &child_parameters, device, state)?;

                debug_assert_eq!(
                    state.len() - before,
                    child.state_count(),
                    "child state initialization must match \
                     child definition state count",
                );
            }
        }
    }

    Ok(())
}

#[derive(Debug, Error)]
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
    use crate::compile::definition::DefinitionStateId;
    use crate::compile::island::IslandNode;
    use crate::runtime::island::StagedStateWrite;
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
        let mut engine = Engine::new();

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

    #[test]
    fn subscription_ids_are_monotonic_and_not_reused() {
        let mut engine = Engine::new();

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
        let mut engine = Engine::new();

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
        let mut engine = Engine::new();
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
        let mut engine = Engine::new();
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
        assert!(world.network.devices().iter().all(Option::is_none));

        world
            .derived_topology
            .assert_consistent(&definitions, &world.network);
    }

    #[test]
    fn command_dispatch_preserves_model_errors() {
        let mut engine = Engine::new();
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
        let mut world = World::new(world_config());

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
        let mut world = World::new(world_config());

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

        world.tick(&definitions).unwrap();

        let runtime = world.island_runtimes[output_island.index()]
            .as_ref()
            .unwrap();

        let first_output = runtime.node_voltage(positive_node).unwrap()
            - runtime.node_voltage(negative_node).unwrap();

        assert!((first_output - 4.0).abs() < 1.0e-12);

        assert_eq!(world.physical_state.get(physical_state), Some(9.0),);

        world.tick(&definitions).unwrap();

        let runtime = world.island_runtimes[output_island.index()]
            .as_ref()
            .unwrap();

        let second_output = runtime.node_voltage(positive_node).unwrap()
            - runtime.node_voltage(negative_node).unwrap();

        assert!((second_output - 9.0).abs() < 1.0e-12);
    }

    #[test]
    fn failed_state_commit_does_not_modify_state() {
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

        world
            .physical_state
            .initialize_device(&definitions, &world.network, capacitor)
            .unwrap();

        let state = DeviceState::new(capacitor, DefinitionStateId::new(0));

        assert_eq!(world.physical_state.get(state), Some(0.0),);

        let error = world
            .physical_state
            .commit_staged(&[
                StagedStateWrite::new(state, 3.0),
                StagedStateWrite::new(state, 7.0),
            ])
            .unwrap_err();

        assert_eq!(error, PhysicalStateError::DuplicateWrite { state },);

        assert_eq!(world.physical_state.get(state), Some(0.0),);
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

        let island = world
            .derived_topology
            .component_island(DeviceComponent::new(source, DevicePartitionId::new(0)));

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
    fn voltage_controlled_switch_initializes_off() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let switch = device(1);

        network
            .add_device(
                &definitions,
                switch,
                PrimitiveElementKind::VoltageControlledSwitch.into(),
            )
            .unwrap();

        let mut states = PhysicalStateStore::new();

        states
            .initialize_device(&definitions, &network, switch)
            .unwrap();

        assert_eq!(
            states.get(DeviceState::new(switch, DefinitionStateId::new(0),)),
            Some(0.0),
        );
    }

    #[test]
    fn engine_ticks_configured_world() {
        let mut engine = Engine::new();

        assert_eq!(engine.tick_world(0), Err(EngineTickError::UnknownWorld),);

        let world = engine.new_world(world_config()).unwrap();

        assert_eq!(engine.tick_world(world), Ok(()),);
    }

    #[test]
    fn nonconvergent_switch_does_not_commit_physical_state() {
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

        // Switch output.
        world
            .attach_terminal(&definitions, output, switch, TerminalId::new(0))
            .unwrap();

        world
            .attach_terminal(&definitions, common, switch, TerminalId::new(1))
            .unwrap();

        // Self-control:
        //
        // Vc = Vout.
        world
            .attach_terminal(&definitions, output, switch, TerminalId::new(2))
            .unwrap();

        world
            .attach_terminal(&definitions, common, switch, TerminalId::new(3))
            .unwrap();

        // Inject 8 A into output.
        world
            .attach_terminal(&definitions, common, source, TerminalId::new(0))
            .unwrap();

        world
            .attach_terminal(&definitions, output, source, TerminalId::new(1))
            .unwrap();

        // threshold = 5
        // hysteresis = 2
        // lower = 4
        // upper = 6
        // G_max = 4
        // G_min = 1
        //
        // OFF:
        //     Vout = 8 / 1 = 8 V
        //     8 >= upper -> ON
        //
        // ON:
        //     Vout = 8 / 4 = 2 V
        //     2 <= lower -> OFF
        //
        // Therefore no stable discrete operating point exists.
        for (index, value) in [5.0, 2.0, 4.0, 1.0].into_iter().enumerate() {
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

        let state = DeviceState::new(switch, DefinitionStateId::new(0));

        // initialize_physical_state() ran before the solve, so the
        // state exists. The failed solve must not have changed OFF -> ON.
        assert_eq!(world.physical_state.get(state), Some(0.0),);
    }

    #[test]
    fn engine_tick_reports_missing_device_parameter() {
        let mut engine = Engine::new();

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
    fn nested_composite_initializes_flattened_state_in_element_order() {
        let mut definitions = DefinitionRegistry::new();

        let tick_delay = DefinitionId::from(PrimitiveElementKind::TickDelay);

        let delay_parameter_constraint = definitions.get(tick_delay).unwrap().parameters()[0];

        /*
         * Inner composite:
         *
         * parameter 0
         *     ↓
         * TickDelay initial value
         */
        let inner = {
            let mut builder = DeviceDefinitionBuilder::new(&definitions);

            let input_positive = builder.add_terminal().unwrap();
            let input_negative = builder.add_terminal().unwrap();
            let output_positive = builder.add_terminal().unwrap();
            let output_negative = builder.add_terminal().unwrap();

            let initial = builder.add_parameter(delay_parameter_constraint).unwrap();

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

        let inner = definitions.register(inner).unwrap();

        let inner_parameter_constraint = definitions.get(inner).unwrap().parameters()[0];

        /*
         * Outer state order:
         *
         * state 0 = capacitor
         * state 1 = inner TickDelay using outer parameter
         * state 2 = inner TickDelay using literal
         */
        let outer = {
            let mut builder = DeviceDefinitionBuilder::new(&definitions);

            let input_positive = builder.add_terminal().unwrap();
            let input_negative = builder.add_terminal().unwrap();
            let output_positive = builder.add_terminal().unwrap();
            let output_negative = builder.add_terminal().unwrap();

            let initial = builder.add_parameter(inner_parameter_constraint).unwrap();

            builder
                .add_element(Element::new(
                    PrimitiveElementKind::Capacitor.into(),
                    vec![input_positive, input_negative],
                    vec![ValueRef::Literal(2.0)],
                ))
                .unwrap();

            builder
                .add_element(Element::new(
                    inner,
                    vec![
                        input_positive,
                        input_negative,
                        output_positive,
                        output_negative,
                    ],
                    vec![ValueRef::Parameter(initial)],
                ))
                .unwrap();

            builder
                .add_element(Element::new(
                    inner,
                    vec![
                        input_positive,
                        input_negative,
                        output_positive,
                        output_negative,
                    ],
                    vec![ValueRef::Literal(1.5)],
                ))
                .unwrap();

            builder.build_definition().unwrap()
        };

        assert_eq!(outer.state_count(), 3);

        let outer = definitions.register(outer).unwrap();

        let mut network = Network::new();

        let device = DeviceId::try_from(1).unwrap();

        network.add_device(&definitions, device, outer).unwrap();

        network
            .set_device_parameter(&definitions, device, ParameterId::new(0), 7.25)
            .unwrap();

        let mut store = PhysicalStateStore::new();

        store
            .initialize_device(&definitions, &network, device)
            .unwrap();

        assert_eq!(
            store.get(DeviceState::new(device, DefinitionStateId::new(0),)),
            Some(0.0),
        );

        assert_eq!(
            store.get(DeviceState::new(device, DefinitionStateId::new(1),)),
            Some(7.25),
        );

        assert_eq!(
            store.get(DeviceState::new(device, DefinitionStateId::new(2),)),
            Some(1.5),
        );
    }

    #[test]
    fn nested_composite_missing_initial_parameter_reports_outer_parameter() {
        let mut definitions = DefinitionRegistry::new();

        let tick_delay = DefinitionId::from(PrimitiveElementKind::TickDelay);

        let constraint = definitions.get(tick_delay).unwrap().parameters()[0];

        let inner = {
            let mut builder = DeviceDefinitionBuilder::new(&definitions);

            let a = builder.add_terminal().unwrap();
            let b = builder.add_terminal().unwrap();
            let c = builder.add_terminal().unwrap();
            let d = builder.add_terminal().unwrap();

            let parameter = builder.add_parameter(constraint).unwrap();

            builder
                .add_element(Element::new(
                    tick_delay,
                    vec![a, b, c, d],
                    vec![ValueRef::Parameter(parameter)],
                ))
                .unwrap();

            builder.build_definition().unwrap()
        };

        let inner = definitions.register(inner).unwrap();

        let constraint = definitions.get(inner).unwrap().parameters()[0];

        let outer = {
            let mut builder = DeviceDefinitionBuilder::new(&definitions);

            let a = builder.add_terminal().unwrap();
            let b = builder.add_terminal().unwrap();
            let c = builder.add_terminal().unwrap();
            let d = builder.add_terminal().unwrap();

            let parameter = builder.add_parameter(constraint).unwrap();

            builder
                .add_element(Element::new(
                    inner,
                    vec![a, b, c, d],
                    vec![ValueRef::Parameter(parameter)],
                ))
                .unwrap();

            builder.build_definition().unwrap()
        };

        let outer = definitions.register(outer).unwrap();

        let mut network = Network::new();

        let device = DeviceId::try_from(1).unwrap();

        network.add_device(&definitions, device, outer).unwrap();

        let mut store = PhysicalStateStore::new();

        assert_eq!(
            store.initialize_device(&definitions, &network, device,),
            Err(PhysicalStateError::MissingInitialParameter {
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

        /*
         * Composite TickDelay:
         *
         * terminals 0,1 = input partition
         * terminals 2,3 = output partition
         */
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

        /*
         * Drive the input to 5 V.
         */
        world
            .attach_terminal(&definitions, input_positive, source, TerminalId::new(0))
            .unwrap();

        world
            .attach_terminal(&definitions, input_negative, source, TerminalId::new(1))
            .unwrap();

        world
            .set_device_parameter(&definitions, source, ParameterId::new(0), 5.0)
            .unwrap();

        /*
         * Initial TickDelay output = 1.5 V.
         */
        world
            .set_device_parameter(&definitions, delay, ParameterId::new(0), 1.5)
            .unwrap();

        let output_component = DeviceComponent::new(delay, DevicePartitionId::new(1));

        let output_island = world.derived_topology.component_island(output_component);

        let output_positive_node =
            IslandNode::net(world.derived_topology.wire_net(output_positive));

        let output_negative_node =
            IslandNode::net(world.derived_topology.wire_net(output_negative));

        let state = DeviceState::new(delay, DefinitionStateId::new(0));

        /*
         * Tick 1:
         *
         * output partition must read initial old state = 1.5 V.
         *
         * input partition sees 5 V and stages state = 5 V.
         *
         * Only after every island succeeds may 5 V be committed.
         */
        world.tick(&definitions).unwrap();

        let output_runtime = world
            .island_runtimes
            .get(output_island.index())
            .and_then(Option::as_ref)
            .unwrap();

        let voltage = output_runtime.node_voltage(output_positive_node).unwrap()
            - output_runtime.node_voltage(output_negative_node).unwrap();

        assert!((voltage - 1.5).abs() < 1.0e-12);

        /*
         * The staged 5 V input has now committed to physical state.
         */
        assert_eq!(world.physical_state.get(state), Some(5.0),);

        /*
         * Tick 2:
         *
         * output now reads the state committed by tick 1.
         */
        world.tick(&definitions).unwrap();

        let output_runtime = world
            .island_runtimes
            .get(output_island.index())
            .and_then(Option::as_ref)
            .unwrap();

        let voltage = output_runtime.node_voltage(output_positive_node).unwrap()
            - output_runtime.node_voltage(output_negative_node).unwrap();

        assert!((voltage - 5.0).abs() < 1.0e-12);

        assert_eq!(world.physical_state.get(state), Some(5.0),);
    }

    #[test]
    fn world_uses_configured_fixed_tick_frequency() {
        let config = WorldConfig::new(NonZeroU32::new(20).unwrap());

        assert_eq!(config.tick_frequency_hz(), NonZeroU32::new(20).unwrap(),);

        assert!((config.timestep() - 0.05).abs() < 1.0e-12);

        let mut engine = Engine::new();

        let world_id = engine.new_world(config).unwrap();

        let world = engine.world(world_id).unwrap();

        assert_eq!(world.config(), config);
    }
}
