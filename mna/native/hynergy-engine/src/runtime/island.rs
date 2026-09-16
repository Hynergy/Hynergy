use crate::compile::definition::DefinitionStateId;
use crate::compile::island::{
    CompiledIsland, CompiledIslandParts, CompiledObserverOutput, CompiledPartitionInputs,
    DeviceObserver, DeviceState, IslandNode, IslandStateLayout, IslandUnknownLayout,
};
use crate::compile::island_ir::CompiledIslandIr;
use hynergy_ir::ValueWorkspace;
use hynergy_mna::system::{MnaError, MnaSystem};
use hynergy_model::device::definition::DeviceId;
use hynergy_model::network::Network;
use hynergy_model::parameter::ParameterId;
use thiserror::Error;

const NONLINEAR_ABSOLUTE_TOLERANCE: f64 = 1.0e-9;
const NONLINEAR_RELATIVE_TOLERANCE: f64 = 1.0e-6;
const NONLINEAR_MAX_ITERATIONS: usize = 32;

#[derive(Debug)]
struct NonlinearScratch {
    current: Box<[f64]>,
    next: Box<[f64]>,
    stability: Box<[f64]>,
}

impl NonlinearScratch {
    fn new(dimension: usize, stability_count: usize) -> Self {
        Self {
            current: vec![0.0; dimension].into_boxed_slice(),
            next: vec![0.0; dimension].into_boxed_slice(),
            stability: vec![0.0; stability_count].into_boxed_slice(),
        }
    }
}

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

    #[error("nonlinear island did not converge after {iterations} iterations")]
    NonlinearDidNotConverge { iterations: usize },

    #[error("island matrix contains a non-finite value")]
    NonFiniteMatrix,

    #[error("island solution contains a non-finite value")]
    NonFiniteSolution,
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
    static_inputs_dirty: bool,
    nonlinear_scratch: Option<Box<NonlinearScratch>>,
    observer_outputs: Box<[CompiledObserverOutput]>,

    #[cfg(test)]
    matrix_stamp_count: usize,
    #[cfg(test)]
    solve_count: usize,
}

impl IslandRuntime {
    pub(crate) fn new(compiled: CompiledIsland, timestep: f64) -> Result<Self, IslandRuntimeError> {
        debug_assert!(timestep.is_finite());
        debug_assert!(timestep > 0.0);

        let CompiledIslandParts {
            pattern,
            ir,
            unknowns,
            states,
            partition_inputs,
            observer_outputs,
        } = compiled.into_parts();

        let dimension = pattern.dimension();

        let mut workspace = ir.value_program().new_workspace();

        if let Some(input) = ir.timestep_input() {
            workspace.set_input(input, timestep);
        }

        let system = MnaSystem::new(pattern)?;

        let nonlinear_scratch = ir.requires_nonlinear_iteration().then(|| {
            Box::new(NonlinearScratch::new(
                dimension,
                ir.iteration_stability_values().len(),
            ))
        });

        Ok(Self {
            system,
            ir,
            unknowns,
            states,
            partition_inputs,
            observer_outputs,
            workspace,
            solution: vec![0.0; dimension].into_boxed_slice(),
            solution_valid: false,
            static_initialized: false,
            matrix_dirty: true,
            static_inputs_dirty: true,
            nonlinear_scratch,
            #[cfg(test)]
            matrix_stamp_count: 0,
            #[cfg(test)]
            solve_count: 0,
        })
    }

    pub(crate) fn observer_value(&self, observer: DeviceObserver) -> Option<f64> {
        if !self.solution_valid {
            return None;
        }

        let index = self
            .observer_outputs
            .binary_search_by_key(&observer, |output| output.observer())
            .ok()?;

        Some(self.workspace.value(self.observer_outputs[index].value()))
    }

    fn factorize_matrix_if_dirty(&mut self) -> Result<(), IslandRuntimeError> {
        if self.matrix_dirty {
            {
                let mut matrix = self.system.values_mut();

                matrix.clear();

                self.ir
                    .matrix_program()
                    .execute(&mut matrix, self.workspace.values());
            }

            if !all_finite(self.system.values()) {
                return Err(IslandRuntimeError::NonFiniteMatrix);
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

        Ok(())
    }

    fn solve_linear(&mut self) -> Result<(), IslandRuntimeError> {
        self.factorize_matrix_if_dirty()?;

        self.ir
            .rhs_program()
            .execute(&mut self.solution, self.workspace.values());

        self.system.solve_in_place(&mut self.solution)?;

        #[cfg(test)]
        {
            self.solve_count += 1;
        }

        if !all_finite(&self.solution) {
            return Err(IslandRuntimeError::NonFiniteSolution);
        }

        evaluate_iteration(&self.ir, &mut self.workspace, &self.solution);

        self.solution_valid = true;

        Ok(())
    }

    fn solve_nonlinear_candidate(
        &mut self,
        candidate: &mut [f64],
    ) -> Result<(), IslandRuntimeError> {
        self.factorize_matrix_if_dirty()?;

        self.ir
            .rhs_program()
            .execute(candidate, self.workspace.values());

        self.system.solve_in_place(candidate)?;

        #[cfg(test)]
        {
            self.solve_count += 1;
        }

        if !all_finite(candidate) {
            return Err(IslandRuntimeError::NonFiniteSolution);
        }

        Ok(())
    }

    fn solve_nonlinear(
        &mut self,
        scratch: &mut NonlinearScratch,
    ) -> Result<(), IslandRuntimeError> {
        let iteration_affects_matrix = self.ir.iteration_affects_matrix();

        evaluate_iteration(&self.ir, &mut self.workspace, &self.solution);

        capture_iteration_stability(&self.ir, &self.workspace, &mut scratch.stability);
        advance_iteration_latches(&self.ir, &mut self.workspace);

        if iteration_affects_matrix {
            self.matrix_dirty = true;
        }

        self.solve_nonlinear_candidate(&mut scratch.current)?;

        evaluate_iteration(&self.ir, &mut self.workspace, &scratch.current);

        if solutions_converged(&self.solution, &scratch.current)
            && self.iteration_stability_matches(&scratch.stability)
        {
            std::mem::swap(&mut self.solution, &mut scratch.current);

            self.solution_valid = true;

            return Ok(());
        }

        for _ in 1..NONLINEAR_MAX_ITERATIONS {
            capture_iteration_stability(&self.ir, &self.workspace, &mut scratch.stability);
            advance_iteration_latches(&self.ir, &mut self.workspace);

            if iteration_affects_matrix {
                self.matrix_dirty = true;
            }

            self.solve_nonlinear_candidate(&mut scratch.next)?;

            evaluate_iteration(&self.ir, &mut self.workspace, &scratch.next);

            if solutions_converged(&scratch.current, &scratch.next)
                && self.iteration_stability_matches(&scratch.stability)
            {
                std::mem::swap(&mut self.solution, &mut scratch.next);

                self.solution_valid = true;

                return Ok(());
            }

            std::mem::swap(&mut scratch.current, &mut scratch.next);
        }

        Err(IslandRuntimeError::NonlinearDidNotConverge {
            iterations: NONLINEAR_MAX_ITERATIONS,
        })
    }

    pub(crate) fn solve_tick<F>(
        &mut self,
        network: &Network,
        mut old_state: F,
    ) -> Result<Vec<StagedStateWrite>, IslandRuntimeError>
    where
        F: FnMut(DeviceState) -> Option<f64>,
    {
        self.solution_valid = false;

        self.prepare_static(network)?;

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

        initialize_iteration_latches(&self.ir, &mut self.workspace);

        if self.nonlinear_scratch.is_none() {
            self.solve_linear()?;
        } else {
            let mut scratch = self
                .nonlinear_scratch
                .take()
                .expect("nonlinear scratch was checked above");

            let result = self.solve_nonlinear(&mut scratch);

            self.nonlinear_scratch = Some(scratch);

            result?;
        }

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

    fn load_parameters(&mut self, network: &Network) -> Result<StaticChanges, IslandRuntimeError> {
        let mut changes = StaticChanges::default();

        for partition in &self.partition_inputs {
            let device = partition.device();

            let device_slot = network
                .devices()
                .get(device.index())
                .and_then(Option::as_ref)
                .ok_or(IslandRuntimeError::MissingDevice { device })?;

            let parameter_ids = partition.definition_parameters();

            let parameter_inputs = partition.parameter_inputs();

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

                changes.record(self.ir.static_input_affects_matrix(input));
            }
        }

        Ok(changes)
    }

    fn prepare_static(&mut self, network: &Network) -> Result<(), IslandRuntimeError> {
        let mut changes = StaticChanges::default();

        if self.static_inputs_dirty {
            changes = self.load_parameters(network)?;
            self.static_inputs_dirty = false;
        }

        let initialize = !self.static_initialized;

        if initialize || changes.values {
            self.ir.value_program().execute_static(&mut self.workspace);
            self.static_initialized = true;
        }

        if initialize || changes.matrix {
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

    #[inline]
    pub(crate) fn mark_numerical_dirty(&mut self) {
        self.static_inputs_dirty = true;
    }

    #[inline]
    fn iteration_stability_matches(&self, expected: &[f64]) -> bool {
        iteration_stability_matches(
            expected,
            self.ir
                .iteration_stability_values()
                .iter()
                .map(|&slot| self.workspace.value(slot)),
        )
    }

    #[cfg(test)]
    #[inline]
    fn matrix_stamp_count(&self) -> usize {
        self.matrix_stamp_count
    }

    #[cfg(test)]
    #[inline]
    fn solve_count(&self) -> usize {
        self.solve_count
    }
}

#[inline]
fn initialize_iteration_latches(ir: &CompiledIslandIr, workspace: &mut ValueWorkspace) {
    for &latch in ir.iteration_latches() {
        let initial = workspace.value(latch.initial());

        workspace.set_input(latch.input(), initial);
    }
}

#[inline]
fn advance_iteration_latches(ir: &CompiledIslandIr, workspace: &mut ValueWorkspace) {
    for &latch in ir.iteration_latches() {
        let next = workspace.value(latch.update());

        workspace.set_input(latch.input(), next);
    }
}

#[inline]
fn evaluate_iteration(ir: &CompiledIslandIr, workspace: &mut ValueWorkspace, solution: &[f64]) {
    for &(unknown, input) in ir.solution_inputs() {
        workspace.set_input(input, solution[unknown.index()]);
    }

    ir.value_program().execute_iteration(workspace);
}

#[inline]
fn capture_iteration_stability(
    ir: &CompiledIslandIr,
    workspace: &ValueWorkspace,
    target: &mut [f64],
) {
    debug_assert_eq!(target.len(), ir.iteration_stability_values().len(),);

    for (target, &slot) in target.iter_mut().zip(ir.iteration_stability_values()) {
        *target = workspace.value(slot);
    }
}

#[inline]
fn iteration_stability_matches(
    expected: &[f64],
    actual: impl ExactSizeIterator<Item = f64>,
) -> bool {
    debug_assert_eq!(expected.len(), actual.len());

    expected
        .iter()
        .copied()
        .zip(actual)
        .all(|(expected, actual)| expected == actual)
}

#[inline]
fn solutions_converged(previous: &[f64], current: &[f64]) -> bool {
    debug_assert_eq!(previous.len(), current.len());
    debug_assert!(all_finite(previous));
    debug_assert!(all_finite(current));

    previous.iter().zip(current).all(|(&previous, &current)| {
        let delta = (current - previous).abs();

        let limit = NONLINEAR_ABSOLUTE_TOLERANCE
            + NONLINEAR_RELATIVE_TOLERANCE * current.abs().max(previous.abs());

        delta <= limit
    })
}

#[inline]
fn all_finite(values: &[f64]) -> bool {
    values.iter().all(|value| value.is_finite())
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

#[derive(Debug, Default, Clone, Copy)]
struct StaticChanges {
    values: bool,
    matrix: bool,
}

impl StaticChanges {
    #[inline]
    fn record(&mut self, affects_matrix: bool) {
        self.values = true;
        self.matrix |= affects_matrix;
    }
}

#[cfg(test)]
mod test {
    use crate::compile::definition::DefinitionStateId;
    use crate::compile::island::{
        DeviceObserver, DeviceState, IslandNode, compile_topology_island,
    };
    use crate::compile::island_ir::IslandIrBuilder;
    use crate::runtime::island::{
        IslandRuntime, advance_iteration_latches, initialize_iteration_latches,
        iteration_stability_matches, solutions_converged,
    };
    use crate::topology::{DerivedTopology, DeviceComponent};
    use hynergy_ir::StateSlot;
    use hynergy_mna::pattern::PatternBuilder;
    use hynergy_model::circuit::{Element, ValueRef};
    use hynergy_model::device::builder::DeviceDefinitionBuilder;
    use hynergy_model::device::definition::{
        DefinitionId, DefinitionObserverId, DeviceId, DevicePartitionId, PrimitiveElementKind,
        TerminalId,
    };
    use hynergy_model::device::registry::DefinitionRegistry;
    use hynergy_model::network::{Network, WireId};
    use hynergy_model::parameter::ParameterId;

    const DEFAULT_TIMESTEP: f64 = 1.0;

    fn voltage_source_island() -> (Network, crate::compile::island::CompiledIsland) {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let negative = WireId::try_from(1).unwrap();
        let positive = WireId::try_from(2).unwrap();

        let source = DeviceId::try_from(1).unwrap();

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
            .attach_terminal(positive, source, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(negative, source, TerminalId::new(1))
            .unwrap();

        network
            .set_device_parameter(&definitions, source, ParameterId::new(0), 5.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island =
            topology.component_island(DeviceComponent::new(source, DevicePartitionId::new(0)));

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        (network, compiled)
    }

    #[test]
    fn linear_island_solves_once() {
        let (network, compiled) = voltage_source_island();

        let mut runtime = IslandRuntime::new(compiled, DEFAULT_TIMESTEP).unwrap();

        runtime.solve_tick(&network, |_| None).unwrap();

        assert_eq!(runtime.solve_count(), 1);
    }

    #[test]
    fn nonlinear_rhs_iteration_reuses_matrix_factorization() {
        let (network, mut compiled) = voltage_source_island();

        compiled.force_nonlinear_iteration_for_test(false);

        let mut runtime = IslandRuntime::new(compiled, DEFAULT_TIMESTEP).unwrap();

        runtime.solve_tick(&network, |_| None).unwrap();

        assert_eq!(runtime.solve_count(), 2);
        assert_eq!(runtime.matrix_stamp_count(), 1);
    }

    #[test]
    fn nonlinear_matrix_iteration_restamps_matrix() {
        let (network, mut compiled) = voltage_source_island();

        compiled.force_nonlinear_iteration_for_test(true);

        let mut runtime = IslandRuntime::new(compiled, DEFAULT_TIMESTEP).unwrap();

        runtime.solve_tick(&network, |_| None).unwrap();

        assert_eq!(runtime.solve_count(), 2);
        assert_eq!(runtime.matrix_stamp_count(), 2);
    }

    #[test]
    fn nonlinear_island_reuses_previous_solution_as_guess() {
        let (network, mut compiled) = voltage_source_island();

        compiled.force_nonlinear_iteration_for_test(false);

        let mut runtime = IslandRuntime::new(compiled, DEFAULT_TIMESTEP).unwrap();

        runtime.solve_tick(&network, |_| None).unwrap();

        assert_eq!(runtime.solve_count(), 2);

        runtime.solve_tick(&network, |_| None).unwrap();

        assert_eq!(runtime.solve_count(), 3);
    }

    #[test]
    fn voltage_source_and_conductance_solve_node_voltage() {
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
            .set_device_parameter(&definitions, conductance, ParameterId::new(0), 2.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island =
            topology.component_island(DeviceComponent::new(source, DevicePartitionId::new(0)));

        let positive_node = IslandNode::net(topology.wire_net(positive));
        let negative_node = IslandNode::net(topology.wire_net(negative));

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        let mut runtime = IslandRuntime::new(compiled, DEFAULT_TIMESTEP).unwrap();

        let writes = runtime.solve_tick(&network, |_| None).unwrap();

        assert!(writes.is_empty());

        let voltage = runtime.node_voltage(positive_node).unwrap()
            - runtime.node_voltage(negative_node).unwrap();

        assert!((voltage - 5.0).abs() < 1.0e-12);
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

        let island =
            topology.component_island(DeviceComponent::new(capacitor, DevicePartitionId::new(0)));

        let node_a = IslandNode::net(topology.wire_net(wire_a));
        let node_b = IslandNode::net(topology.wire_net(wire_b));

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        let mut runtime = IslandRuntime::new(compiled, 0.5).unwrap();

        let capacitor_state = DeviceState::new(capacitor, DefinitionStateId::new(0));

        let writes = runtime
            .solve_tick(&network, |state| (state == capacitor_state).then_some(3.0))
            .unwrap();

        let voltage = runtime.node_voltage(node_a).unwrap() - runtime.node_voltage(node_b).unwrap();

        assert!((voltage - 2.8).abs() < 1.0e-12);

        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].state(), capacitor_state);
        assert!((writes[0].value() - 2.8).abs() < 1.0e-12);
    }

    #[test]
    fn fixed_timestep_capacitor_reuses_matrix_factorization() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let a = WireId::try_from(1).unwrap();
        let b = WireId::try_from(2).unwrap();

        let conductance = DeviceId::try_from(1).unwrap();
        let capacitor = DeviceId::try_from(2).unwrap();

        network.add_wire(a).unwrap();
        network.add_wire(b).unwrap();

        network
            .add_device(
                &definitions,
                conductance,
                PrimitiveElementKind::Conductance.into(),
            )
            .unwrap();

        network
            .add_device(
                &definitions,
                capacitor,
                PrimitiveElementKind::Capacitor.into(),
            )
            .unwrap();

        for device in [conductance, capacitor] {
            network
                .attach_terminal(a, device, TerminalId::new(0))
                .unwrap();

            network
                .attach_terminal(b, device, TerminalId::new(1))
                .unwrap();
        }

        network
            .set_device_parameter(&definitions, conductance, ParameterId::new(0), 1.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, capacitor, ParameterId::new(0), 2.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island =
            topology.component_island(DeviceComponent::new(capacitor, DevicePartitionId::new(0)));

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        let mut runtime = IslandRuntime::new(compiled, 0.5).unwrap();

        let state = DeviceState::new(capacitor, DefinitionStateId::new(0));

        runtime
            .solve_tick(&network, |candidate| (candidate == state).then_some(0.0))
            .unwrap();

        assert_eq!(runtime.matrix_stamp_count(), 1);

        runtime
            .solve_tick(&network, |candidate| (candidate == state).then_some(0.0))
            .unwrap();

        assert_eq!(runtime.matrix_stamp_count(), 1);
    }

    #[test]
    fn matrix_affecting_parameter_change_restamps_matrix() {
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

        let mut runtime = IslandRuntime::new(compiled, DEFAULT_TIMESTEP).unwrap();

        runtime.solve_tick(&network, |_| None).unwrap();

        assert_eq!(runtime.matrix_stamp_count(), 1);

        runtime.solve_tick(&network, |_| None).unwrap();

        assert_eq!(runtime.matrix_stamp_count(), 1);

        network
            .set_device_parameter(&definitions, conductance, ParameterId::new(0), 2.0)
            .unwrap();

        runtime.mark_numerical_dirty();

        runtime.solve_tick(&network, |_| None).unwrap();

        assert_eq!(runtime.matrix_stamp_count(), 2);
    }

    #[test]
    fn rhs_only_parameter_change_reuses_matrix_factorization() {
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

        let positive_node = IslandNode::net(topology.wire_net(positive));
        let negative_node = IslandNode::net(topology.wire_net(negative));

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        let mut runtime = IslandRuntime::new(compiled, DEFAULT_TIMESTEP).unwrap();

        runtime.solve_tick(&network, |_| None).unwrap();

        assert_eq!(runtime.matrix_stamp_count(), 1);

        network
            .set_device_parameter(&definitions, source, ParameterId::new(0), 9.0)
            .unwrap();

        runtime.mark_numerical_dirty();

        runtime.solve_tick(&network, |_| None).unwrap();

        assert_eq!(runtime.matrix_stamp_count(), 1);

        let voltage = runtime.node_voltage(positive_node).unwrap()
            - runtime.node_voltage(negative_node).unwrap();

        assert!((voltage - 9.0).abs() < 1.0e-12);
    }

    #[test]
    fn controlled_conductance_converges_to_self_controlled_operating_point() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let common = WireId::try_from(1).unwrap();
        let output = WireId::try_from(2).unwrap();

        let controlled = DeviceId::try_from(1).unwrap();
        let current_source = DeviceId::try_from(2).unwrap();

        network.add_wire(common).unwrap();
        network.add_wire(output).unwrap();

        network
            .add_device(
                &definitions,
                controlled,
                PrimitiveElementKind::VoltageControlledConductance.into(),
            )
            .unwrap();

        network
            .add_device(
                &definitions,
                current_source,
                PrimitiveElementKind::CurrentSource.into(),
            )
            .unwrap();

        network
            .attach_terminal(output, controlled, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(common, controlled, TerminalId::new(1))
            .unwrap();

        // Control voltage is the output voltage.
        network
            .attach_terminal(output, controlled, TerminalId::new(2))
            .unwrap();

        // Inject 6 A from common into output.
        network
            .attach_terminal(common, current_source, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(output, current_source, TerminalId::new(1))
            .unwrap();

        // threshold = 2
        // transition = 2
        // G_min = 1
        // G_max = 5
        for (index, value) in [2.0, 2.0, 1.0, 5.0].into_iter().enumerate() {
            network
                .set_device_parameter(
                    &definitions,
                    controlled,
                    ParameterId::new(index as u32),
                    value,
                )
                .unwrap();
        }

        network
            .set_device_parameter(&definitions, current_source, ParameterId::new(0), 6.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island =
            topology.component_island(DeviceComponent::new(controlled, DevicePartitionId::new(0)));

        let output_node = IslandNode::net(topology.wire_net(output));
        let common_node = IslandNode::net(topology.wire_net(common));

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        let mut runtime = IslandRuntime::new(compiled, DEFAULT_TIMESTEP).unwrap();

        runtime.solve_tick(&network, |_| None).unwrap();

        let voltage =
            runtime.node_voltage(output_node).unwrap() - runtime.node_voltage(common_node).unwrap();

        // Inside the transition:
        //
        // G(V) = 1 + 2 * (V - 1)
        //      = 2V - 1
        //
        // 6 = G(V) * V
        //   = (2V - 1)V
        //
        // V = 2 is the positive operating point.
        assert!((voltage - 2.0).abs() < 1.0e-6);

        assert!(runtime.solve_count() > 1);
    }

    #[test]
    fn iteration_stability_requires_exact_values() {
        assert!(solutions_converged(&[0.0], &[5.0e-10]));

        assert!(!iteration_stability_matches(&[0.0], [5.0e-10].into_iter(),));

        assert!(iteration_stability_matches(
            &[0.0, 1.0],
            [0.0, 1.0].into_iter(),
        ));
    }

    #[test]
    fn iteration_latch_advances_to_computed_value() {
        let pattern = PatternBuilder::new(1).unwrap().finish().unwrap();

        let mut builder = IslandIrBuilder::new(&pattern);

        let initial = builder.state_value(StateSlot::new(0)).unwrap();
        let latch = builder.iteration_latch(initial).unwrap();
        let one = builder.constant_value(1.0).unwrap();
        let update = builder.sub_value(one, latch).unwrap();

        builder.update_iteration_latch(latch, update);

        let ir = builder.finish().unwrap();
        let mut workspace = ir.value_program().new_workspace();

        let state_input = ir.state_inputs()[0].1;

        workspace.set_input(state_input, 0.0);

        ir.value_program().execute_tick(&mut workspace);

        initialize_iteration_latches(&ir, &mut workspace);

        ir.value_program().execute_iteration(&mut workspace);

        assert_eq!(workspace.value(update), 1.0);

        advance_iteration_latches(&ir, &mut workspace);

        ir.value_program().execute_iteration(&mut workspace);

        assert_eq!(workspace.value(update), 0.0);
    }

    fn voltage_controlled_switch_island() -> (
        DefinitionRegistry,
        Network,
        crate::compile::island::CompiledIsland,
        IslandNode,
        IslandNode,
        DeviceId,
        DeviceId,
    ) {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let common = WireId::try_from(1).unwrap();
        let output = WireId::try_from(2).unwrap();
        let control = WireId::try_from(3).unwrap();

        let switch = DeviceId::try_from(1).unwrap();
        let control_source = DeviceId::try_from(2).unwrap();
        let current_source = DeviceId::try_from(3).unwrap();

        network.add_wire(common).unwrap();
        network.add_wire(output).unwrap();
        network.add_wire(control).unwrap();

        network
            .add_device(
                &definitions,
                switch,
                PrimitiveElementKind::VoltageControlledSwitch.into(),
            )
            .unwrap();

        network
            .add_device(
                &definitions,
                control_source,
                PrimitiveElementKind::VoltageSource.into(),
            )
            .unwrap();

        network
            .add_device(
                &definitions,
                current_source,
                PrimitiveElementKind::CurrentSource.into(),
            )
            .unwrap();

        network
            .attach_terminal(output, switch, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(common, switch, TerminalId::new(1))
            .unwrap();

        network
            .attach_terminal(control, switch, TerminalId::new(2))
            .unwrap();

        network
            .attach_terminal(common, switch, TerminalId::new(3))
            .unwrap();

        network
            .attach_terminal(control, control_source, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(common, control_source, TerminalId::new(1))
            .unwrap();

        // Inject 8 A into the output.
        network
            .attach_terminal(common, current_source, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(output, current_source, TerminalId::new(1))
            .unwrap();

        // threshold = 2
        // hysteresis = 2
        // lower = 1
        // upper = 3
        // G_max = 4
        // G_min = 1
        for (index, value) in [2.0, 2.0, 4.0, 1.0].into_iter().enumerate() {
            network
                .set_device_parameter(&definitions, switch, ParameterId::new(index as u32), value)
                .unwrap();
        }

        network
            .set_device_parameter(&definitions, control_source, ParameterId::new(0), 0.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, current_source, ParameterId::new(0), 8.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island =
            topology.component_island(DeviceComponent::new(switch, DevicePartitionId::new(0)));

        let output_node = IslandNode::net(topology.wire_net(output));
        let common_node = IslandNode::net(topology.wire_net(common));

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        (
            definitions,
            network,
            compiled,
            output_node,
            common_node,
            switch,
            control_source,
        )
    }

    #[test]
    fn voltage_controlled_switch_applies_hysteresis() {
        let (definitions, mut network, compiled, output_node, common_node, switch, control_source) =
            voltage_controlled_switch_island();

        let mut runtime = IslandRuntime::new(compiled, DEFAULT_TIMESTEP).unwrap();

        let state = DeviceState::new(switch, DefinitionStateId::new(0));

        let mut mode = 0.0;

        // Below lower threshold: remain OFF.
        let writes = runtime
            .solve_tick(&network, |candidate| (candidate == state).then_some(mode))
            .unwrap();

        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].state(), state);
        assert_eq!(writes[0].value(), 0.0);

        let voltage =
            runtime.node_voltage(output_node).unwrap() - runtime.node_voltage(common_node).unwrap();

        assert!((voltage - 8.0).abs() < 1.0e-9);

        mode = writes[0].value();

        // Above upper threshold: turn ON.
        network
            .set_device_parameter(&definitions, control_source, ParameterId::new(0), 4.0)
            .unwrap();

        runtime.mark_numerical_dirty();

        let writes = runtime
            .solve_tick(&network, |candidate| (candidate == state).then_some(mode))
            .unwrap();

        assert_eq!(writes[0].value(), 1.0);

        let voltage =
            runtime.node_voltage(output_node).unwrap() - runtime.node_voltage(common_node).unwrap();

        assert!((voltage - 2.0).abs() < 1.0e-9);

        mode = writes[0].value();

        // Inside the deadband: retain ON.
        network
            .set_device_parameter(&definitions, control_source, ParameterId::new(0), 2.0)
            .unwrap();

        runtime.mark_numerical_dirty();

        let writes = runtime
            .solve_tick(&network, |candidate| (candidate == state).then_some(mode))
            .unwrap();

        assert_eq!(writes[0].value(), 1.0);

        let voltage =
            runtime.node_voltage(output_node).unwrap() - runtime.node_voltage(common_node).unwrap();

        assert!((voltage - 2.0).abs() < 1.0e-9);

        mode = writes[0].value();

        // Below lower threshold: turn OFF.
        network
            .set_device_parameter(&definitions, control_source, ParameterId::new(0), 0.0)
            .unwrap();

        runtime.mark_numerical_dirty();

        let writes = runtime
            .solve_tick(&network, |candidate| (candidate == state).then_some(mode))
            .unwrap();

        assert_eq!(writes[0].value(), 0.0);

        let voltage =
            runtime.node_voltage(output_node).unwrap() - runtime.node_voltage(common_node).unwrap();

        assert!((voltage - 8.0).abs() < 1.0e-9);
    }

    #[test]
    fn stateless_composite_solves_through_island_runtime() {
        let mut definitions = DefinitionRegistry::new();

        let resistance = DefinitionId::from(PrimitiveElementKind::Resistance);

        let resistance_constraint = definitions.get(resistance).unwrap().parameters()[0];

        let composite_definition = {
            let mut builder = DeviceDefinitionBuilder::new(&definitions);

            let positive = builder.add_terminal().unwrap();
            let negative = builder.add_terminal().unwrap();
            let middle = builder.add_node().unwrap();

            let r2 = builder.add_parameter(resistance_constraint).unwrap();

            builder
                .add_element(Element::new(
                    resistance,
                    vec![positive, middle],
                    vec![ValueRef::Literal(2.0)],
                ))
                .unwrap();

            builder
                .add_element(Element::new(
                    resistance,
                    vec![middle, negative],
                    vec![ValueRef::Parameter(r2)],
                ))
                .unwrap();

            builder.build_definition().unwrap()
        };

        let composite_definition = definitions.register(composite_definition).unwrap();

        let mut network = Network::new();

        let common = WireId::try_from(1).unwrap();
        let output = WireId::try_from(2).unwrap();

        let composite = DeviceId::try_from(1).unwrap();
        let source = DeviceId::try_from(2).unwrap();

        network.add_wire(common).unwrap();
        network.add_wire(output).unwrap();

        network
            .add_device(&definitions, composite, composite_definition)
            .unwrap();

        network
            .add_device(
                &definitions,
                source,
                PrimitiveElementKind::CurrentSource.into(),
            )
            .unwrap();

        network
            .attach_terminal(output, composite, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(common, composite, TerminalId::new(1))
            .unwrap();

        // Inject current from common into output.
        network
            .attach_terminal(common, source, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(output, source, TerminalId::new(1))
            .unwrap();

        // R2 = 3 ohm, so total resistance is 5 ohm.
        network
            .set_device_parameter(&definitions, composite, ParameterId::new(0), 3.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, source, ParameterId::new(0), 2.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island =
            topology.component_island(DeviceComponent::new(composite, DevicePartitionId::new(0)));

        let output_node = IslandNode::net(topology.wire_net(output));
        let common_node = IslandNode::net(topology.wire_net(common));

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        let mut runtime = IslandRuntime::new(compiled, DEFAULT_TIMESTEP).unwrap();

        let writes = runtime.solve_tick(&network, |_| None).unwrap();

        assert!(writes.is_empty());

        let voltage =
            runtime.node_voltage(output_node).unwrap() - runtime.node_voltage(common_node).unwrap();

        // 2 A * (2 + 3) ohm = 10 V.
        assert!((voltage - 10.0).abs() < 1.0e-12);

        assert_eq!(runtime.matrix_stamp_count(), 1);

        // R2 = 8 ohm, so total resistance becomes 10 ohm.
        network
            .set_device_parameter(&definitions, composite, ParameterId::new(0), 8.0)
            .unwrap();

        runtime.mark_numerical_dirty();

        let writes = runtime.solve_tick(&network, |_| None).unwrap();

        assert!(writes.is_empty());

        let voltage =
            runtime.node_voltage(output_node).unwrap() - runtime.node_voltage(common_node).unwrap();

        // 2 A * (2 + 8) ohm = 20 V.
        assert!((voltage - 20.0).abs() < 1.0e-12);

        assert_eq!(runtime.matrix_stamp_count(), 2);
    }

    #[test]
    fn composite_voltage_observer_is_available_after_solve() {
        let mut definitions = DefinitionRegistry::new();

        let observed_definition = {
            let mut builder = DeviceDefinitionBuilder::new(&definitions);

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

        let observed_definition = definitions.register(observed_definition).unwrap();

        let mut network = Network::new();

        let negative = WireId::try_from(1).unwrap();
        let positive = WireId::try_from(2).unwrap();

        let observed = DeviceId::try_from(1).unwrap();
        let source = DeviceId::try_from(2).unwrap();

        network.add_wire(negative).unwrap();
        network.add_wire(positive).unwrap();

        network
            .add_device(&definitions, observed, observed_definition)
            .unwrap();

        network
            .add_device(
                &definitions,
                source,
                PrimitiveElementKind::VoltageSource.into(),
            )
            .unwrap();

        for device in [observed, source] {
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

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island =
            topology.component_island(DeviceComponent::new(observed, DevicePartitionId::new(0)));

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        let mut runtime = IslandRuntime::new(compiled, 1.0).unwrap();

        let observer = DeviceObserver::new(observed, DefinitionObserverId::new(0));

        assert_eq!(runtime.observer_value(observer), None);

        runtime.solve_tick(&network, |_| None).unwrap();

        let value = runtime.observer_value(observer).unwrap();

        assert!((value - 5.0).abs() < 1.0e-12);
    }
}
