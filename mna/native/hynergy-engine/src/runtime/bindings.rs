use super::island::IslandRuntimeError;
use crate::compile::definition::DefinitionStateInitializer;
use crate::compile::island::{CompiledPartitionInputs, DeviceState, IslandStateLayout};
use crate::state::{PhysicalStateAddress, PhysicalStateError, PhysicalStateStore};
use hynergy_ir::{InputSlot, StateSlot, StateWrite, ValueSlot, ValueWorkspace};
use hynergy_model::device::definition::DeviceId;
use hynergy_model::network::{DeviceLocation, Network};
use hynergy_model::parameter::ParameterId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ParameterInputBinding {
    device: DeviceId,
    parameter: ParameterId,
    input: InputSlot,
    location: DeviceLocation,
}

impl ParameterInputBinding {
    #[inline]
    const fn new(
        device: DeviceId,
        parameter: ParameterId,
        input: InputSlot,
        location: DeviceLocation,
    ) -> Self {
        Self {
            device,
            parameter,
            input,
            location,
        }
    }

    #[inline]
    pub(super) const fn device(&self) -> DeviceId {
        self.device
    }

    #[inline]
    pub(super) const fn parameter(&self) -> ParameterId {
        self.parameter
    }

    #[inline]
    pub(super) const fn input(&self) -> InputSlot {
        self.input
    }

    #[inline]
    pub(super) const fn location(&self) -> DeviceLocation {
        self.location
    }

    #[inline]
    fn set_location(&mut self, location: DeviceLocation) {
        self.location = location;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct StateInputBinding {
    state: DeviceState,
    input: InputSlot,
    address: PhysicalStateAddress,
    initializer: DefinitionStateInitializer,
}

impl StateInputBinding {
    #[inline]
    const fn new(
        state: DeviceState,
        input: InputSlot,
        address: PhysicalStateAddress,
        initializer: DefinitionStateInitializer,
    ) -> Self {
        Self {
            state,
            input,
            address,
            initializer,
        }
    }

    #[inline]
    pub(super) fn read_logical(
        &self,
        network: &Network,
        physical_state: &PhysicalStateStore,
    ) -> Result<f64, IslandRuntimeError> {
        physical_state
            .read_logical_at(network, self.state.device(), self.address, self.initializer)
            .map_err(|error| match error {
                PhysicalStateError::MissingInitialParameter { device, parameter } => {
                    IslandRuntimeError::MissingParameter { device, parameter }
                }

                PhysicalStateError::StateNotInitialized { state } => {
                    IslandRuntimeError::MissingState {
                        device: state.device(),
                        state: state.state(),
                    }
                }
            })
    }

    #[inline]
    #[cfg(test)]
    pub(super) const fn initializer(&self) -> DefinitionStateInitializer {
        self.initializer
    }

    #[inline]
    pub(super) const fn state(&self) -> DeviceState {
        self.state
    }

    #[inline]
    pub(super) const fn input(&self) -> InputSlot {
        self.input
    }

    #[inline]
    #[cfg(debug_assertions)]
    pub(super) const fn address(&self) -> PhysicalStateAddress {
        self.address
    }

    #[inline]
    fn set_address(&mut self, address: PhysicalStateAddress) {
        self.address = address;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StateOutputBinding {
    state: DeviceState,
    source: ValueSlot,
    address: PhysicalStateAddress,
}

impl StateOutputBinding {
    #[inline]
    const fn new(state: DeviceState, source: ValueSlot, address: PhysicalStateAddress) -> Self {
        Self {
            state,
            source,
            address,
        }
    }

    #[inline]
    pub(super) fn validate_source(
        &self,
        workspace: &ValueWorkspace,
    ) -> Result<(), IslandRuntimeError> {
        if !self.value(workspace).is_finite() {
            return Err(IslandRuntimeError::NonFiniteState { state: self.state });
        }

        Ok(())
    }

    #[inline]
    pub(super) fn value(&self, workspace: &ValueWorkspace) -> f64 {
        workspace.value(self.source)
    }

    #[inline]
    pub(super) const fn state(&self) -> DeviceState {
        self.state
    }

    #[inline]
    #[cfg(test)]
    pub(super) const fn source(&self) -> ValueSlot {
        self.source
    }

    #[inline]
    pub(super) const fn address(&self) -> PhysicalStateAddress {
        self.address
    }

    #[inline]
    fn set_address(&mut self, address: PhysicalStateAddress) {
        self.address = address;
    }
}

#[derive(Debug)]
pub(super) struct IslandBindings {
    parameters: Box<[ParameterInputBinding]>,
    state_inputs: Box<[StateInputBinding]>,
    state_outputs: Box<[StateOutputBinding]>,
}

impl IslandBindings {
    pub(super) fn new(
        network: &Network,
        states: &IslandStateLayout,
        partition_inputs: &[CompiledPartitionInputs],
        state_inputs: &[(StateSlot, InputSlot)],
        state_writes: &[StateWrite],
    ) -> Result<Self, IslandRuntimeError> {
        let parameter_count = partition_inputs
            .iter()
            .map(|partition| partition.parameter_inputs().len())
            .sum();

        let mut parameters = Vec::<ParameterInputBinding>::with_capacity(parameter_count);

        for partition in partition_inputs {
            let device = partition.device();

            let location = network
                .device_location(device)
                .map_err(|_| IslandRuntimeError::MissingDevice { device })?;

            let definition_parameters = partition.definition_parameters();
            let parameter_inputs = partition.parameter_inputs();

            debug_assert_eq!(
                definition_parameters.len(),
                parameter_inputs.len(),
                "compiled parameter IDs and inputs must have equal lengths",
            );

            for (&parameter, &input) in definition_parameters.iter().zip(parameter_inputs) {
                parameters.push(ParameterInputBinding::new(
                    device, parameter, input, location,
                ));
            }
        }

        debug_assert_eq!(
            parameters.len(),
            parameter_count,
            "parameter binding count must match the precomputed capacity",
        );

        let mut bound_state_inputs = Vec::<StateInputBinding>::with_capacity(state_inputs.len());

        for &(slot, input) in state_inputs {
            let state = states
                .device_state(slot)
                .expect("compiled state input must have a semantic state");

            let initializer = states
                .state_initializer(slot)
                .expect("compiled state input must have an initializer");

            let device = state.device();

            let location = network
                .device_location(device)
                .map_err(|_| IslandRuntimeError::MissingDevice { device })?;

            bound_state_inputs.push(StateInputBinding::new(
                state,
                input,
                PhysicalStateAddress::new(location, state.state().index()),
                initializer,
            ));
        }

        debug_assert_eq!(
            bound_state_inputs.len(),
            state_inputs.len(),
            "state binding count must match compiled state inputs",
        );

        let mut state_outputs = Vec::<StateOutputBinding>::with_capacity(state_writes.len());

        for &write in state_writes {
            let slot = write.destination();

            let state = states
                .device_state(slot)
                .expect("compiled state output must have a semantic state");

            let device = state.device();

            let location = network
                .device_location(device)
                .map_err(|_| IslandRuntimeError::MissingDevice { device })?;

            state_outputs.push(StateOutputBinding::new(
                state,
                write.source(),
                PhysicalStateAddress::new(location, state.state().index()),
            ));
        }

        debug_assert_eq!(
            state_outputs.len(),
            state_writes.len(),
            "state output binding count must match compiled state writes",
        );

        Ok(Self {
            parameters: parameters.into_boxed_slice(),
            state_inputs: bound_state_inputs.into_boxed_slice(),
            state_outputs: state_outputs.into_boxed_slice(),
        })
    }

    pub(super) fn rebind(&mut self, network: &Network) -> Result<(), IslandRuntimeError> {
        for binding in &mut self.parameters {
            let device = binding.device();

            let location = network
                .device_location(device)
                .map_err(|_| IslandRuntimeError::MissingDevice { device })?;

            binding.set_location(location);
        }

        for binding in &mut self.state_inputs {
            let state = binding.state();
            let device = state.device();

            let location = network
                .device_location(device)
                .map_err(|_| IslandRuntimeError::MissingDevice { device })?;

            binding.set_address(PhysicalStateAddress::new(location, state.state().index()));
        }

        for binding in &mut self.state_outputs {
            let state = binding.state();
            let device = state.device();

            let location = network
                .device_location(device)
                .map_err(|_| IslandRuntimeError::MissingDevice { device })?;

            binding.set_address(PhysicalStateAddress::new(location, state.state().index()));
        }

        Ok(())
    }

    #[cfg(debug_assertions)]
    pub(super) fn debug_assert_valid(
        &self,
        network: &Network,
        physical_state: &PhysicalStateStore,
    ) {
        for binding in &self.parameters {
            let current = network
                .device_location(binding.device())
                .expect("bound parameter device must remain resident");

            debug_assert_eq!(
                binding.location(),
                current,
                "stale parameter binding for device {:?}, parameter {:?}",
                binding.device(),
                binding.parameter(),
            );

            debug_assert!(
                network
                    .parameter_at_location(binding.location(), binding.parameter(),)
                    .is_some(),
                "parameter binding must address a live parameter slot for device {:?}, parameter {:?}",
                binding.device(),
                binding.parameter(),
            );
        }

        for binding in &self.state_inputs {
            let state = binding.state();
            let address = binding.address();

            let current = network
                .device_location(state.device())
                .expect("bound state device must remain resident");

            debug_assert_eq!(
                address.location(),
                current,
                "stale state binding for device {:?}, state {:?}",
                state.device(),
                state.state(),
            );

            debug_assert_eq!(
                address.state_index(),
                state.state().index(),
                "state binding index must match its semantic state",
            );

            debug_assert!(
                physical_state.get_at(address).is_some(),
                "state binding must address a live state scalar for device {:?}, state {:?}",
                state.device(),
                state.state(),
            );
        }

        for binding in &self.state_outputs {
            let state = binding.state();
            let address = binding.address();

            let current = network
                .device_location(state.device())
                .expect("bound state output device must remain resident");

            debug_assert_eq!(
                address.location(),
                current,
                "stale state output binding for device {:?}, state {:?}",
                state.device(),
                state.state(),
            );

            debug_assert_eq!(
                address.state_index(),
                state.state().index(),
                "state output binding index must match its semantic state",
            );

            debug_assert!(
                physical_state.get_at(address).is_some(),
                "state output binding must address a live state scalar for device {:?}, state {:?}",
                state.device(),
                state.state(),
            );
        }
    }

    #[inline]
    pub(super) fn state_outputs(&self) -> &[StateOutputBinding] {
        &self.state_outputs
    }

    #[inline]
    pub(super) fn parameters(&self) -> &[ParameterInputBinding] {
        &self.parameters
    }

    #[inline]
    pub(super) fn state_inputs(&self) -> &[StateInputBinding] {
        &self.state_inputs
    }

    #[cfg(test)]
    pub(super) fn set_state_output_address_for_test(
        &mut self,
        index: usize,
        address: PhysicalStateAddress,
    ) {
        self.state_outputs[index].set_address(address);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::definition::{
        CompiledDefinition, DefinitionStateId, DefinitionStateInitializer,
    };
    use crate::compile::island::{
        IslandNode, IslandPartitionSpec, compile_island_parts, compile_topology_island,
    };
    use crate::state::PhysicalStateStore;
    use crate::topology::{DerivedTopology, DeviceComponent};
    use hynergy_ir::ValueProgramBuilder;
    use hynergy_model::device::definition::{
        DefinitionId, DeviceId, DevicePartitionId, PrimitiveElementKind,
    };
    use hynergy_model::device::registry::DefinitionRegistry;
    use hynergy_model::network::Network;
    use hynergy_model::parameter::ParameterId;

    fn device(raw: u32) -> DeviceId {
        DeviceId::try_from(raw).unwrap()
    }

    fn add_device_with_physical_state(
        definitions: &DefinitionRegistry,
        network: &mut Network,
        physical_state: &mut PhysicalStateStore,
        device: DeviceId,
        definition_id: DefinitionId,
    ) {
        let definition = definitions.get(definition_id).unwrap();

        let model_insert = network
            .prepare_add_device(definitions, device, definition_id)
            .unwrap();

        let state_insert = physical_state.prepare_add_device(definition, &model_insert);

        let insert = network.commit_add_device(model_insert);

        physical_state.commit_add_device(state_insert, insert);
    }

    #[test]
    fn rebind_updates_physical_locations_without_changing_semantic_identity() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut physical_state = PhysicalStateStore::default();

        let definition_id = DefinitionId::from(PrimitiveElementKind::Capacitor);

        let first = device(1);
        let removed = device(2);
        let moved = device(3);

        for device in [first, removed, moved] {
            add_device_with_physical_state(
                &definitions,
                &mut network,
                &mut physical_state,
                device,
                definition_id,
            );

            network
                .set_device_parameter(&definitions, device, ParameterId::new(0), 1.0e-6)
                .unwrap();
        }

        let definition = definitions.get(definition_id).unwrap();

        let compiled_definition = CompiledDefinition::compile(&definitions, definition).unwrap();

        let partition = compiled_definition
            .partition(DevicePartitionId::new(0))
            .unwrap();

        let node_a = IslandNode::net(crate::topology::NetId::try_from(1).unwrap());
        let node_b = IslandNode::net(crate::topology::NetId::try_from(2).unwrap());

        let terminal_nodes = [node_a, node_b];

        let partition_spec = IslandPartitionSpec::new(
            moved,
            partition,
            compiled_definition.state_initializers(),
            &terminal_nodes,
        );

        let compiled = compile_island_parts(&[node_a, node_b], &[partition_spec]).unwrap();
        let parts = compiled.into_parts();

        let mut bindings = IslandBindings::new(
            &network,
            &parts.states,
            &parts.partition_inputs,
            parts.ir.state_inputs(),
            parts.ir.state_transition().writes(),
        )
        .unwrap();

        assert!(!bindings.parameters().is_empty());
        assert!(!bindings.state_inputs().is_empty());
        assert!(!bindings.state_outputs().is_empty());

        let parameter = &bindings.parameters()[0];

        assert_eq!(parameter.device(), moved);
        assert_eq!(parameter.parameter(), ParameterId::new(0));
        assert_eq!(
            parameter.location(),
            network.device_location(moved).unwrap(),
        );
        assert_eq!(parameter.location().row(), 2);

        let state = &bindings.state_inputs()[0];

        assert_eq!(state.state().device(), moved);
        assert_eq!(
            state.address().location(),
            network.device_location(moved).unwrap(),
        );
        assert_eq!(state.address().location().row(), 2);

        let output = &bindings.state_outputs()[0];

        assert_eq!(output.state().device(), moved);
        assert_eq!(
            output.address().location(),
            network.device_location(moved).unwrap(),
        );
        assert_eq!(output.address().location().row(), 2);

        let parameter_input = parameter.input();
        let state_input = state.input();
        let semantic_state = state.state();
        let output_source = output.source();
        let output_state = output.state();

        let removal = network.remove_device(removed).unwrap();

        assert_eq!(removal.moved_device(), Some(moved));

        physical_state.remove_device(removal);

        assert_eq!(network.device_location(moved).unwrap().row(), 1);

        bindings.rebind(&network).unwrap();

        let parameter = &bindings.parameters()[0];

        assert_eq!(parameter.device(), moved);
        assert_eq!(parameter.parameter(), ParameterId::new(0));
        assert_eq!(parameter.input(), parameter_input);
        assert_eq!(parameter.location().row(), 1);

        let state = &bindings.state_inputs()[0];

        assert_eq!(state.state(), semantic_state);
        assert_eq!(state.input(), state_input);
        assert_eq!(
            state.address().location(),
            network.device_location(moved).unwrap(),
        );
        assert_eq!(state.address().location().row(), 1);

        let output = &bindings.state_outputs()[0];

        assert_eq!(output.state(), output_state);
        assert_eq!(output.source(), output_source);
        assert_eq!(
            output.address().location(),
            network.device_location(moved).unwrap(),
        );
        assert_eq!(output.address().location().row(), 1);
    }

    #[test]
    #[should_panic(expected = "stale parameter binding")]
    fn debug_validation_detects_stale_binding_after_row_relocation() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut physical_state = PhysicalStateStore::default();

        let definition = DefinitionId::from(PrimitiveElementKind::Capacitor);

        let first = device(1);
        let removed = device(2);
        let moved = device(3);

        for device in [first, removed, moved] {
            add_device_with_physical_state(
                &definitions,
                &mut network,
                &mut physical_state,
                device,
                definition,
            );

            network
                .set_device_parameter(&definitions, device, ParameterId::new(0), 1.0)
                .unwrap();
        }

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island = topology.component_island(
            &network,
            DeviceComponent::new(moved, DevicePartitionId::new(0)),
        );

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();
        let parts = compiled.into_parts();

        let bindings = IslandBindings::new(
            &network,
            &parts.states,
            &parts.partition_inputs,
            parts.ir.state_inputs(),
            parts.ir.state_transition().writes(),
        )
        .unwrap();

        assert_eq!(bindings.parameters()[0].location().row(), 2,);

        network.remove_device(removed).unwrap();

        assert_eq!(network.device_location(moved).unwrap().row(), 1,);

        bindings.debug_assert_valid(&network, &physical_state);
    }

    #[test]
    #[should_panic(expected = "stale state binding")]
    fn debug_validation_detects_stale_state_binding_after_row_relocation() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut physical_state = PhysicalStateStore::default();

        let definition = DefinitionId::from(PrimitiveElementKind::Capacitor);

        let first = device(1);
        let removed = device(2);
        let moved = device(3);

        for device in [first, removed, moved] {
            add_device_with_physical_state(
                &definitions,
                &mut network,
                &mut physical_state,
                device,
                definition,
            );

            network
                .set_device_parameter(&definitions, device, ParameterId::new(0), 1.0)
                .unwrap();
        }

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island = topology.component_island(
            &network,
            DeviceComponent::new(moved, DevicePartitionId::new(0)),
        );

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();
        let parts = compiled.into_parts();

        let mut bindings = IslandBindings::new(
            &network,
            &parts.states,
            &parts.partition_inputs,
            parts.ir.state_inputs(),
            parts.ir.state_transition().writes(),
        )
        .unwrap();

        assert!(
            !bindings.state_inputs().is_empty(),
            "capacitor fixture must produce a state-input binding",
        );

        bindings.parameters = Vec::new().into_boxed_slice();

        assert_eq!(bindings.state_inputs()[0].address().location().row(), 2,);

        network.remove_device(removed).unwrap();

        assert_eq!(network.device_location(moved).unwrap().row(), 1,);

        bindings.debug_assert_valid(&network, &physical_state);
    }

    #[test]
    #[should_panic(expected = "state binding must address a live state scalar")]
    fn debug_validation_detects_missing_state_sidecar_scalar() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut physical_state = PhysicalStateStore::default();

        let device = device(1);
        let definition = DefinitionId::from(PrimitiveElementKind::Capacitor);

        add_device_with_physical_state(
            &definitions,
            &mut network,
            &mut physical_state,
            device,
            definition,
        );

        network
            .set_device_parameter(&definitions, device, ParameterId::new(0), 1.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island = topology.component_island(
            &network,
            DeviceComponent::new(device, DevicePartitionId::new(0)),
        );

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();
        let parts = compiled.into_parts();

        let bindings = IslandBindings::new(
            &network,
            &parts.states,
            &parts.partition_inputs,
            parts.ir.state_inputs(),
            parts.ir.state_transition().writes(),
        )
        .unwrap();

        assert!(
            !bindings.state_inputs().is_empty(),
            "capacitor fixture must produce a state-input binding",
        );

        let missing_physical_state = PhysicalStateStore::default();
        bindings.debug_assert_valid(&network, &missing_physical_state);
    }

    #[test]
    fn state_input_binding_carries_precompiled_initializer() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut physical_state = PhysicalStateStore::default();

        let device = device(1);
        let definition = DefinitionId::from(PrimitiveElementKind::TickDelay);

        add_device_with_physical_state(
            &definitions,
            &mut network,
            &mut physical_state,
            device,
            definition,
        );

        network
            .set_device_parameter(&definitions, device, ParameterId::new(0), 3.25)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island = topology.component_island(
            &network,
            DeviceComponent::new(device, DevicePartitionId::new(1)),
        );

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();
        let parts = compiled.into_parts();

        let bindings = IslandBindings::new(
            &network,
            &parts.states,
            &parts.partition_inputs,
            parts.ir.state_inputs(),
            parts.ir.state_transition().writes(),
        )
        .unwrap();

        assert_eq!(bindings.state_inputs().len(), 1);

        let binding = &bindings.state_inputs()[0];

        assert_eq!(binding.state().device(), device,);

        assert_eq!(
            binding.initializer(),
            DefinitionStateInitializer::Parameter(ParameterId::new(0),),
        );

        assert_eq!(
            binding.read_logical(&network, &physical_state,).unwrap(),
            3.25,
        );

        assert!(
            !physical_state.is_initialized_at(binding.address().location(),),
            "runtime logical reads must not commit state initialization",
        );
    }

    #[test]
    fn state_output_binding_retains_transition_source_and_physical_destination() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();
        let mut physical_state = PhysicalStateStore::default();

        let device = device(1);
        let definition_id = DefinitionId::from(PrimitiveElementKind::Capacitor);

        add_device_with_physical_state(
            &definitions,
            &mut network,
            &mut physical_state,
            device,
            definition_id,
        );

        network
            .set_device_parameter(&definitions, device, ParameterId::new(0), 1.0e-6)
            .unwrap();

        let definition = definitions.get(definition_id).unwrap();
        let compiled_definition = CompiledDefinition::compile(&definitions, definition).unwrap();

        let partition = compiled_definition
            .partition(DevicePartitionId::new(0))
            .unwrap();

        let node_a = IslandNode::net(crate::topology::NetId::try_from(1).unwrap());
        let node_b = IslandNode::net(crate::topology::NetId::try_from(2).unwrap());

        let terminal_nodes = [node_a, node_b];

        let partition_spec = IslandPartitionSpec::new(
            device,
            partition,
            compiled_definition.state_initializers(),
            &terminal_nodes,
        );

        let compiled = compile_island_parts(&[node_a, node_b], &[partition_spec]).unwrap();
        let parts = compiled.into_parts();

        assert_eq!(parts.ir.state_transition().writes().len(), 1,);

        let write = parts.ir.state_transition().writes()[0];

        let bindings = IslandBindings::new(
            &network,
            &parts.states,
            &parts.partition_inputs,
            parts.ir.state_inputs(),
            parts.ir.state_transition().writes(),
        )
        .unwrap();

        assert_eq!(bindings.state_outputs().len(), 1,);

        let binding = &bindings.state_outputs()[0];

        assert_eq!(
            binding.state(),
            DeviceState::new(device, DefinitionStateId::new(0),),
        );

        assert_eq!(binding.source(), write.source(),);

        assert_eq!(
            binding.address(),
            PhysicalStateAddress::new(network.device_location(device).unwrap(), 0,),
        );

        let workspace = parts.ir.value_program().new_workspace();

        assert_eq!(binding.value(&workspace), workspace.value(write.source()),);
    }

    #[test]
    fn state_output_binding_rejects_non_finite_source_value() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let device = device(1);

        network
            .add_device(
                &definitions,
                device,
                DefinitionId::from(PrimitiveElementKind::TickDelay),
            )
            .unwrap();

        let location = network.device_location(device).unwrap();
        let state = DeviceState::new(device, DefinitionStateId::new(0));

        let mut values = ValueProgramBuilder::new();

        let source = values.constant(f64::NAN).unwrap();
        let workspace = values.finish().new_workspace();

        let binding = StateOutputBinding {
            state,
            source,
            address: PhysicalStateAddress::new(location, 0),
        };

        assert_eq!(
            binding.validate_source(&workspace),
            Err(IslandRuntimeError::NonFiniteState { state },),
        );
    }
}
