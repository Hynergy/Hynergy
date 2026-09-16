use crate::compile::CompiledIslandIr;
use crate::compile::definition::DefinitionStateId;
use crate::compile::island::{
    CompiledIsland, CompiledIslandParts, CompiledPartitionInputs, DeviceState, IslandNode,
    IslandStateLayout, IslandUnknownLayout,
};
use hynergy_ir::ValueWorkspace;
use hynergy_mna::system::{MnaError, MnaSystem};
use hynergy_model::device::definition::DeviceId;
use hynergy_model::network::Network;
use hynergy_model::parameter::ParameterId;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum IslandRuntimeError {
    #[error("device {device:?} no longer exists")]
    MissingDevice { device: DeviceId },

    #[error("device {device:?} parameter {parameter:?} is not assigned")]
    MissingParameter {
        device: DeviceId,
        parameter: ParameterId,
    },

    #[error(transparent)]
    Mna(#[from] MnaError),

    #[error("state {state:?} for device {device:?} is not available")]
    MissingState {
        device: DeviceId,
        state: DefinitionStateId,
    },

    #[error("timestep must be finite and greater than zero")]
    InvalidTimestep,
}

#[derive(Debug)]
pub(crate) struct IslandRuntime {
    system: MnaSystem,
    ir: CompiledIslandIr,
    unknowns: IslandUnknownLayout,
    states: IslandStateLayout,
    partition_inputs: Box<[CompiledPartitionInputs]>,
    workspace: ValueWorkspace,
    solution: Box<[f64]>,
    solution_valid: bool,
    static_initialized: bool,
    matrix_dirty: bool,
    last_timestep: f64,
    #[cfg(test)]
    matrix_stamp_count: usize,
}

impl IslandRuntime {
    pub(crate) fn new(compiled: CompiledIsland) -> Result<Self, IslandRuntimeError> {
        let CompiledIslandParts {
            pattern,
            ir,
            unknowns,
            states,
            partition_inputs,
        } = compiled.into_parts();

        let dimension = pattern.dimension();

        let workspace = ir.value_program().new_workspace();

        let system = MnaSystem::new(pattern)?;

        Ok(Self {
            system,
            ir,
            unknowns,
            states,
            partition_inputs,

            workspace,

            solution: vec![0.0; dimension].into_boxed_slice(),

            solution_valid: false,

            static_initialized: false,
            matrix_dirty: true,
            last_timestep: f64::NAN,

            #[cfg(test)]
            matrix_stamp_count: 0,
        })
    }

    fn solve_current_values(&mut self) -> Result<(), IslandRuntimeError> {
        if self.matrix_dirty {
            {
                let mut matrix = self.system.values_mut();

                matrix.clear();

                self.ir
                    .matrix_program()
                    .execute(&mut matrix, self.workspace.values());
            }

            self.system.factorize()?;

            self.matrix_dirty = false;

            #[cfg(test)]
            {
                self.matrix_stamp_count += 1;
            }
        }

        debug_assert!(
            self.system.is_factorized(),
            "clean island matrix must remain factorized",
        );

        self.ir
            .rhs_program()
            .execute(&mut self.solution, self.workspace.values());

        self.system.solve_in_place(&mut self.solution)?;

        for &(unknown, input) in self.ir.solution_inputs() {
            self.workspace
                .set_input(input, self.solution[unknown.index()]);
        }

        self.ir
            .value_program()
            .execute_iteration(&mut self.workspace);

        self.solution_valid = true;

        Ok(())
    }

    pub(crate) fn solve_tick<F>(
        &mut self,
        network: &Network,
        timestep: f64,
        mut old_state: F,
    ) -> Result<Vec<StagedStateWrite>, IslandRuntimeError>
    where
        F: FnMut(DeviceState) -> Option<f64>,
    {
        self.solution_valid = false;

        self.prepare_static(network, timestep)?;

        for &(slot, input) in self.ir.state_inputs() {
            let state = self
                .states
                .device_state(slot)
                .expect("compiled state input must have a physical state");

            let value = old_state(state).ok_or(IslandRuntimeError::MissingState {
                device: state.device(),

                state: state.state(),
            })?;

            self.workspace.set_input(input, value);
        }

        self.ir.value_program().execute_tick(&mut self.workspace);

        self.solve_current_values()?;

        let mut next_state = vec![0.0; self.states.state_count()];

        self.ir
            .state_transition()
            .execute(&mut next_state, self.workspace.values());

        let mut writes = Vec::with_capacity(self.ir.state_transition().len());

        for write in self.ir.state_transition().writes() {
            let slot = write.destination();

            let state = self
                .states
                .device_state(slot)
                .expect("compiled state write must have a physical state");

            writes.push(StagedStateWrite::new(state, next_state[slot.index()]));
        }

        Ok(writes)
    }

    fn load_parameters(&mut self, network: &Network) -> Result<bool, IslandRuntimeError> {
        let mut changed = false;

        for partition in &self.partition_inputs {
            let device = partition.device();

            let device_slot = network
                .devices()
                .get(device.index())
                .and_then(Option::as_ref)
                .ok_or(IslandRuntimeError::MissingDevice { device })?;

            let parameter_ids = partition.definition_parameters();

            let parameter_inputs = partition.inputs().parameters();

            debug_assert_eq!(parameter_ids.len(), parameter_inputs.len(),);

            for (&parameter, &input) in parameter_ids.iter().zip(parameter_inputs) {
                let value = device_slot
                    .parameters()
                    .get(parameter.index())
                    .copied()
                    .flatten()
                    .ok_or(IslandRuntimeError::MissingParameter { device, parameter })?;

                if self.workspace.value(input.value()) == value {
                    continue;
                }

                self.workspace.set_input(input, value);

                changed = true;
            }
        }

        Ok(changed)
    }

    fn prepare_static(
        &mut self,
        network: &Network,
        timestep: f64,
    ) -> Result<(), IslandRuntimeError> {
        let mut changed = self.load_parameters(network)?;

        if let Some(input) = self.ir.timestep_input() {
            if !timestep.is_finite() || timestep <= 0.0 {
                return Err(IslandRuntimeError::InvalidTimestep);
            }

            if self.last_timestep != timestep {
                self.workspace.set_input(input, timestep);

                self.last_timestep = timestep;

                changed = true;
            }
        }

        if !self.static_initialized || changed {
            self.ir.value_program().execute_static(&mut self.workspace);

            self.static_initialized = true;
            self.matrix_dirty = true;
        }

        Ok(())
    }

    pub(crate) fn node_voltage(&self, node: IslandNode) -> Option<f64> {
        if !self.solution_valid {
            return None;
        }

        Some(match self.unknowns.node_unknown(node) {
            None => 0.0,

            Some(unknown) => self.solution[unknown.index()],
        })
    }

    #[cfg(test)]
    #[inline]
    fn matrix_stamp_count(&self) -> usize {
        self.matrix_stamp_count
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct StagedStateWrite {
    state: DeviceState,
    value: f64,
}

impl StagedStateWrite {
    #[inline]
    pub(crate) const fn new(state: DeviceState, value: f64) -> Self {
        Self { state, value }
    }

    #[inline]
    pub(crate) const fn state(self) -> DeviceState {
        self.state
    }

    #[inline]
    pub(crate) const fn value(self) -> f64 {
        self.value
    }
}

#[cfg(test)]
mod test {
    use crate::compile::definition::DefinitionStateId;
    use crate::compile::island::{DeviceState, IslandNode, compile_topology_island};
    use crate::runtime::island::IslandRuntime;
    use crate::topology::{DerivedTopology, DeviceComponent};
    use hynergy_model::device::definition::{
        DefinitionId, DeviceId, DevicePartitionId, PrimitiveElementKind, TerminalId,
    };
    use hynergy_model::device::registry::DefinitionRegistry;
    use hynergy_model::network::{Network, WireId};
    use hynergy_model::parameter::ParameterId;

    #[test]
    fn voltage_source_and_conductance_solve_node_voltage() {
        let definitions = DefinitionRegistry::new();

        let mut network = Network::new();

        let wire_negative = WireId::try_from(1).unwrap();

        let wire_positive = WireId::try_from(2).unwrap();

        let source = DeviceId::try_from(1).unwrap();

        let conductance = DeviceId::try_from(2).unwrap();

        network.add_wire(wire_negative).unwrap();

        network.add_wire(wire_positive).unwrap();

        network
            .add_device(
                &definitions,
                source,
                DefinitionId::from(PrimitiveElementKind::VoltageSource),
            )
            .unwrap();

        network
            .add_device(
                &definitions,
                conductance,
                DefinitionId::from(PrimitiveElementKind::Conductance),
            )
            .unwrap();

        network
            .attach_terminal(wire_positive, source, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(wire_negative, source, TerminalId::new(1))
            .unwrap();

        network
            .attach_terminal(wire_positive, conductance, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(wire_negative, conductance, TerminalId::new(1))
            .unwrap();

        network
            .set_device_parameter(&definitions, source, ParameterId::new(0), 5.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, conductance, ParameterId::new(0), 2.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island_id =
            topology.component_island(DeviceComponent::new(source, DevicePartitionId::new(0)));

        let positive = IslandNode::net(topology.wire_net(wire_positive));

        let negative = IslandNode::net(topology.wire_net(wire_negative));

        let compiled =
            compile_topology_island(&definitions, &network, &topology, island_id).unwrap();

        let mut runtime = IslandRuntime::new(compiled).unwrap();

        let writes = runtime.solve_tick(&network, 1.0, |_| None).unwrap();

        assert!(writes.is_empty());

        let positive_voltage = runtime.node_voltage(positive).unwrap();

        let negative_voltage = runtime.node_voltage(negative).unwrap();

        assert!((positive_voltage - negative_voltage - 5.0).abs() < 1.0e-12);
    }

    #[test]
    fn capacitor_tick_reads_old_state_and_stages_next_state() {
        let definitions = DefinitionRegistry::new();

        let mut network = Network::new();

        let wire_a = WireId::try_from(1).unwrap();

        let wire_b = WireId::try_from(2).unwrap();

        let conductance = DeviceId::try_from(1).unwrap();

        let capacitor = DeviceId::try_from(2).unwrap();

        let source = DeviceId::try_from(3).unwrap();

        network.add_wire(wire_a).unwrap();

        network.add_wire(wire_b).unwrap();

        network
            .add_device(
                &definitions,
                conductance,
                DefinitionId::from(PrimitiveElementKind::Conductance),
            )
            .unwrap();

        network
            .add_device(
                &definitions,
                capacitor,
                DefinitionId::from(PrimitiveElementKind::Capacitor),
            )
            .unwrap();

        network
            .add_device(
                &definitions,
                source,
                DefinitionId::from(PrimitiveElementKind::CurrentSource),
            )
            .unwrap();

        network
            .attach_terminal(wire_a, conductance, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(wire_b, conductance, TerminalId::new(1))
            .unwrap();

        network
            .attach_terminal(wire_a, capacitor, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(wire_b, capacitor, TerminalId::new(1))
            .unwrap();

        // Current flows from B to A.
        network
            .attach_terminal(wire_b, source, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(wire_a, source, TerminalId::new(1))
            .unwrap();

        network
            .set_device_parameter(&definitions, conductance, ParameterId::new(0), 1.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, capacitor, ParameterId::new(0), 2.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, source, ParameterId::new(0), 2.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island_id =
            topology.component_island(DeviceComponent::new(capacitor, DevicePartitionId::new(0)));

        let node_a = IslandNode::net(topology.wire_net(wire_a));

        let node_b = IslandNode::net(topology.wire_net(wire_b));

        let compiled =
            compile_topology_island(&definitions, &network, &topology, island_id).unwrap();

        let mut runtime = IslandRuntime::new(compiled).unwrap();

        let capacitor_state = DeviceState::new(capacitor, DefinitionStateId::new(0));

        let writes = runtime
            .solve_tick(&network, 0.5, |state| {
                (state == capacitor_state).then_some(3.0)
            })
            .unwrap();

        let voltage = runtime.node_voltage(node_a).unwrap() - runtime.node_voltage(node_b).unwrap();

        assert!((voltage - 2.8).abs() < 1.0e-12);

        assert_eq!(writes.len(), 1,);

        assert_eq!(writes[0].state(), capacitor_state,);

        assert!((writes[0].value() - 2.8).abs() < 1.0e-12);
    }

    #[test]
    fn unchanged_static_inputs_reuse_matrix_factorization() {
        let definitions = DefinitionRegistry::new();

        let mut network = Network::new();

        let negative = WireId::try_from(1).unwrap();

        let positive = WireId::try_from(2).unwrap();

        let source = DeviceId::try_from(1).unwrap();

        let conductance = DeviceId::try_from(2).unwrap();

        network.add_wire(negative).unwrap();

        network.add_wire(positive).unwrap();

        network
            .add_device(
                &definitions,
                source,
                PrimitiveElementKind::VoltageSource.into(),
            )
            .unwrap();

        network
            .add_device(
                &definitions,
                conductance,
                PrimitiveElementKind::Conductance.into(),
            )
            .unwrap();

        for device in [source, conductance] {
            network
                .attach_terminal(positive, device, TerminalId::new(0))
                .unwrap();

            network
                .attach_terminal(negative, device, TerminalId::new(1))
                .unwrap();
        }

        network
            .set_device_parameter(&definitions, source, ParameterId::new(0), 5.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, conductance, ParameterId::new(0), 1.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island =
            topology.component_island(DeviceComponent::new(source, DevicePartitionId::new(0)));

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        let mut runtime = IslandRuntime::new(compiled).unwrap();

        let writes = runtime.solve_tick(&network, 1.0, |_| None).unwrap();

        assert!(writes.is_empty());

        assert_eq!(runtime.matrix_stamp_count(), 1,);

        let writes = runtime.solve_tick(&network, 1.0, |_| None).unwrap();

        assert!(writes.is_empty());

        assert_eq!(runtime.matrix_stamp_count(), 1,);

        network
            .set_device_parameter(&definitions, conductance, ParameterId::new(0), 2.0)
            .unwrap();

        let writes = runtime.solve_tick(&network, 1.0, |_| None).unwrap();

        assert!(writes.is_empty());

        assert_eq!(runtime.matrix_stamp_count(), 2,);
    }
}
