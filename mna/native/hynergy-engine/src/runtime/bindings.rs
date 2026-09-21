use super::island::IslandRuntimeError;
use crate::compile::island::{CompiledPartitionInputs, DeviceState, IslandStateLayout};
use crate::state::PhysicalStateAddress;
use hynergy_ir::{InputSlot, StateSlot};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StateInputBinding {
    state: DeviceState,
    input: InputSlot,
    address: PhysicalStateAddress,
}

impl StateInputBinding {
    #[inline]
    const fn new(state: DeviceState, input: InputSlot, address: PhysicalStateAddress) -> Self {
        Self {
            state,
            input,
            address,
        }
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
}

impl IslandBindings {
    pub(super) fn new(
        network: &Network,
        states: &IslandStateLayout,
        partition_inputs: &[CompiledPartitionInputs],
        state_inputs: &[(StateSlot, InputSlot)],
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

            let device = state.device();

            let location = network
                .device_location(device)
                .map_err(|_| IslandRuntimeError::MissingDevice { device })?;

            bound_state_inputs.push(StateInputBinding::new(
                state,
                input,
                PhysicalStateAddress::new(location, state.state().index()),
            ));
        }

        debug_assert_eq!(
            bound_state_inputs.len(),
            state_inputs.len(),
            "state binding count must match compiled state inputs",
        );

        Ok(Self {
            parameters: parameters.into_boxed_slice(),
            state_inputs: bound_state_inputs.into_boxed_slice(),
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

        Ok(())
    }

    #[cfg(debug_assertions)]
    pub(super) fn debug_assert_valid(&self, network: &Network) {
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
        }
    }

    #[inline]
    pub(super) fn parameters(&self) -> &[ParameterInputBinding] {
        &self.parameters
    }

    #[inline]
    pub(super) fn state_inputs(&self) -> &[StateInputBinding] {
        &self.state_inputs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::definition::CompiledDefinition;
    use crate::compile::island::{
        IslandNode, IslandPartitionSpec, compile_island_parts, compile_topology_island,
    };
    use crate::topology::{DerivedTopology, DeviceComponent};
    use hynergy_model::device::definition::{
        DefinitionId, DeviceId, DevicePartitionId, PrimitiveElementKind,
    };
    use hynergy_model::device::registry::DefinitionRegistry;
    use hynergy_model::network::Network;
    use hynergy_model::parameter::ParameterId;

    fn device(raw: u32) -> DeviceId {
        DeviceId::try_from(raw).unwrap()
    }

    #[test]
    fn rebind_updates_physical_locations_without_changing_semantic_identity() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let definition_id = DefinitionId::from(PrimitiveElementKind::Capacitor);

        let first = device(1);
        let removed = device(2);
        let moved = device(3);

        for device in [first, removed, moved] {
            network
                .add_device(&definitions, device, definition_id)
                .unwrap();

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

        let partition_spec = IslandPartitionSpec::new(moved, partition, &terminal_nodes);

        let compiled = compile_island_parts(&[node_a, node_b], &[partition_spec]).unwrap();

        let parts = compiled.into_parts();

        let mut bindings = IslandBindings::new(
            &network,
            &parts.states,
            &parts.partition_inputs,
            parts.ir.state_inputs(),
        )
        .unwrap();

        assert!(!bindings.parameters().is_empty());
        assert!(!bindings.state_inputs().is_empty());

        let parameter = &bindings.parameters()[0];

        assert_eq!(parameter.device(), moved);
        assert_eq!(parameter.parameter(), ParameterId::new(0),);
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

        let parameter_input = parameter.input();
        let state_input = state.input();
        let semantic_state = state.state();

        let removal = network.remove_device(removed).unwrap();

        assert_eq!(removal.moved_device(), Some(moved),);

        bindings.rebind(&network).unwrap();

        let parameter = &bindings.parameters()[0];
        let state = &bindings.state_inputs()[0];

        assert_eq!(parameter.device(), moved);
        assert_eq!(parameter.parameter(), ParameterId::new(0),);
        assert_eq!(parameter.input(), parameter_input);
        assert_eq!(parameter.location().row(), 1);

        assert_eq!(state.state(), semantic_state);
        assert_eq!(state.input(), state_input);
        assert_eq!(state.address().location().row(), 1);
    }

    #[test]
    #[should_panic(expected = "stale parameter binding")]
    fn debug_validation_detects_stale_binding_after_row_relocation() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let definition = DefinitionId::from(PrimitiveElementKind::Capacitor);

        let first = DeviceId::try_from(1).unwrap();
        let removed = DeviceId::try_from(2).unwrap();
        let moved = DeviceId::try_from(3).unwrap();

        for device in [first, removed, moved] {
            network
                .add_device(&definitions, device, definition)
                .unwrap();

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
        )
        .unwrap();

        assert_eq!(bindings.parameters()[0].location().row(), 2,);

        network.remove_device(removed).unwrap();

        assert_eq!(network.device_location(moved).unwrap().row(), 1,);

        bindings.debug_assert_valid(&network);
    }

    #[test]
    #[should_panic(expected = "stale state binding")]
    fn debug_validation_detects_stale_state_binding_after_row_relocation() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let definition = DefinitionId::from(PrimitiveElementKind::Capacitor);

        let first = DeviceId::try_from(1).unwrap();
        let removed = DeviceId::try_from(2).unwrap();
        let moved = DeviceId::try_from(3).unwrap();

        for device in [first, removed, moved] {
            network
                .add_device(&definitions, device, definition)
                .unwrap();

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

        bindings.debug_assert_valid(&network);
    }
}
