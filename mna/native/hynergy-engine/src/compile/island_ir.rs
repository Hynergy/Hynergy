use hynergy_ir::{
    EvaluationRate, InputSlot, MatrixAdd, MatrixProgram, RhsAdd, RhsProgram, StateProgramError,
    StateSlot, StateTransitionProgram, StateWrite, ValueBuildError, ValueProgram,
    ValueProgramBuilder, ValueSlot,
};
use std::collections::BTreeMap;
use thiserror::Error;

use hynergy_mna::pattern::{MatrixSlot, MnaPattern, UnknownIndex};

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IslandIrBuildError {
    #[error(transparent)]
    Values(#[from] ValueBuildError),
    #[error(transparent)]
    State(#[from] StateProgramError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IterationStampDependency {
    None,
    Rhs,
    Matrix,
}

impl IterationStampDependency {
    #[inline]
    const fn requires_iteration(self) -> bool {
        !matches!(self, Self::None)
    }

    #[inline]
    #[cfg(test)]
    const fn affects_matrix(self) -> bool {
        matches!(self, Self::Matrix)
    }
}

#[derive(Debug, Clone, Copy)]
struct PendingIterationLatch {
    input: InputSlot,
    initial: ValueSlot,
    update: Option<ValueSlot>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct IterationLatch {
    input: InputSlot,
    initial: ValueSlot,
    update: ValueSlot,
}

impl IterationLatch {
    #[inline]
    pub(crate) const fn input(self) -> InputSlot {
        self.input
    }

    #[inline]
    pub(crate) const fn initial(self) -> ValueSlot {
        self.initial
    }

    #[inline]
    pub(crate) const fn update(self) -> ValueSlot {
        self.update
    }
}

#[derive(Debug)]
pub(crate) struct IslandIrBuilder<'a> {
    pattern: &'a MnaPattern,
    values: ValueProgramBuilder,
    matrix_static_inputs: Vec<InputSlot>,
    matrix_ops: Vec<MatrixAdd>,
    rhs_ops: Vec<RhsAdd>,
    state_writes: Vec<StateWrite>,
    timestep_input: Option<InputSlot>,
    zero_value: Option<ValueSlot>,
    solution_inputs: BTreeMap<UnknownIndex, InputSlot>,
    state_inputs: BTreeMap<StateSlot, InputSlot>,
    iteration_stamp_dependency: IterationStampDependency,
    iteration_stability_values: Vec<ValueSlot>,
    iteration_latches: Vec<PendingIterationLatch>,
}

impl<'a> IslandIrBuilder<'a> {
    pub(crate) fn new(pattern: &'a MnaPattern) -> Self {
        Self {
            pattern,
            values: ValueProgramBuilder::new(),
            matrix_static_inputs: Vec::new(),
            matrix_ops: Vec::new(),
            rhs_ops: Vec::new(),
            state_writes: Vec::new(),
            timestep_input: None,
            zero_value: None,
            solution_inputs: BTreeMap::new(),
            state_inputs: BTreeMap::new(),
            iteration_stamp_dependency: IterationStampDependency::None,
            iteration_stability_values: Vec::new(),
            iteration_latches: Vec::new(),
        }
    }

    pub(crate) fn iteration_latch(
        &mut self,
        initial: ValueSlot,
    ) -> Result<ValueSlot, ValueBuildError> {
        let rate = self.values.rate(initial)?;

        debug_assert!(
            rate <= EvaluationRate::Tick,
            "iteration latch initial value must be available before iteration",
        );

        let input = self.values.iteration_input()?;

        self.iteration_latches.push(PendingIterationLatch {
            input,
            initial,
            update: None,
        });

        Ok(input.value())
    }

    pub(crate) fn update_iteration_latch(&mut self, latch: ValueSlot, update: ValueSlot) {
        debug_assert_eq!(
            self.values
                .rate(update)
                .expect("iteration latch update must belong to its value program"),
            EvaluationRate::Iteration,
        );

        let pending = self
            .iteration_latches
            .iter_mut()
            .find(|candidate| candidate.input.value() == latch)
            .expect("iteration latch value must belong to this builder");

        assert!(
            pending.update.replace(update).is_none(),
            "iteration latch must have exactly one update",
        );
    }

    #[inline]
    pub(crate) fn parameter_input(
        &mut self,
        affects_matrix: bool,
    ) -> Result<InputSlot, ValueBuildError> {
        let input = self.values.static_input()?;

        if affects_matrix {
            self.mark_matrix_static_input(input);
        }

        Ok(input)
    }

    #[inline]
    pub(crate) fn constant_value(&mut self, value: f64) -> Result<ValueSlot, ValueBuildError> {
        self.values.constant(value)
    }

    pub(crate) fn timestep_value(
        &mut self,
        affects_matrix: bool,
    ) -> Result<ValueSlot, ValueBuildError> {
        if let Some(input) = self.timestep_input {
            if affects_matrix {
                self.mark_matrix_static_input(input);
            }

            return Ok(input.value());
        }

        let input = self.values.static_input()?;

        self.timestep_input = Some(input);

        if affects_matrix {
            self.mark_matrix_static_input(input);
        }

        Ok(input.value())
    }

    pub(crate) fn state_value(&mut self, state: StateSlot) -> Result<ValueSlot, ValueBuildError> {
        if let Some(&input) = self.state_inputs.get(&state) {
            return Ok(input.value());
        }

        let input = self.values.tick_input()?;

        self.state_inputs.insert(state, input);

        Ok(input.value())
    }

    pub(crate) fn unknown_value(
        &mut self,
        unknown: Option<UnknownIndex>,
    ) -> Result<ValueSlot, ValueBuildError> {
        let Some(unknown) = unknown else {
            return self.reference_value();
        };

        if let Some(&input) = self.solution_inputs.get(&unknown) {
            return Ok(input.value());
        }

        let input = self.values.iteration_input()?;

        self.solution_inputs.insert(unknown, input);

        Ok(input.value())
    }

    #[inline]
    fn reference_value(&mut self) -> Result<ValueSlot, ValueBuildError> {
        if let Some(value) = self.zero_value {
            return Ok(value);
        }

        let value = self.values.constant(0.0)?;

        self.zero_value = Some(value);

        Ok(value)
    }

    #[inline]
    pub(crate) fn add_value(
        &mut self,
        lhs: ValueSlot,
        rhs: ValueSlot,
    ) -> Result<ValueSlot, ValueBuildError> {
        self.values.add(lhs, rhs)
    }

    #[inline]
    pub(crate) fn sub_value(
        &mut self,
        lhs: ValueSlot,
        rhs: ValueSlot,
    ) -> Result<ValueSlot, ValueBuildError> {
        self.values.sub(lhs, rhs)
    }

    #[inline]
    pub(crate) fn mul_value(
        &mut self,
        lhs: ValueSlot,
        rhs: ValueSlot,
    ) -> Result<ValueSlot, ValueBuildError> {
        self.values.mul(lhs, rhs)
    }

    #[inline]
    pub(crate) fn div_value(
        &mut self,
        lhs: ValueSlot,
        rhs: ValueSlot,
    ) -> Result<ValueSlot, ValueBuildError> {
        self.values.div(lhs, rhs)
    }

    #[inline]
    pub(crate) fn neg_value(&mut self, operand: ValueSlot) -> Result<ValueSlot, ValueBuildError> {
        self.values.neg(operand)
    }

    #[inline]
    pub(crate) fn less_equal_value(
        &mut self,
        lhs: ValueSlot,
        rhs: ValueSlot,
    ) -> Result<ValueSlot, ValueBuildError> {
        self.values.less_equal(lhs, rhs)
    }

    #[inline]
    pub(crate) fn add_matrix(&mut self, destination: MatrixSlot, source: ValueSlot, scale: f64) {
        self.record_iteration_stamp_dependency(source, true);

        self.matrix_ops
            .push(MatrixAdd::new(destination, source, scale));
    }

    #[inline]
    pub(crate) fn add_rhs(&mut self, destination: UnknownIndex, source: ValueSlot, scale: f64) {
        self.record_iteration_stamp_dependency(source, false);
        self.rhs_ops.push(RhsAdd::new(destination, source, scale));
    }

    #[inline]
    pub(crate) fn write_state(&mut self, destination: StateSlot, source: ValueSlot) {
        self.state_writes.push(StateWrite::new(destination, source));
    }

    pub(crate) fn finish(mut self) -> Result<CompiledIslandIr, IslandIrBuildError> {
        let state_transition = StateTransitionProgram::new(self.state_writes)?;

        let solution_inputs = self
            .solution_inputs
            .into_iter()
            .collect::<Vec<_>>()
            .into_boxed_slice();

        let state_inputs = self
            .state_inputs
            .into_iter()
            .collect::<Vec<_>>()
            .into_boxed_slice();

        let iteration_latches = self
            .iteration_latches
            .into_iter()
            .map(|latch| IterationLatch {
                input: latch.input,
                initial: latch.initial,
                update: latch.update.expect("iteration latch must have an update"),
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();

        let matrix_program = MatrixProgram::new(self.matrix_ops);

        let mut iteration_matrix_sources = matrix_program
            .ops()
            .iter()
            .map(|op| op.source())
            .filter(|&source| {
                self.values
                    .rate(source)
                    .expect("island IR matrix source must belong to its value program")
                    == EvaluationRate::Iteration
            })
            .collect::<Vec<_>>();

        iteration_matrix_sources.sort_unstable();
        iteration_matrix_sources.dedup();

        self.matrix_static_inputs.sort_unstable();
        self.matrix_static_inputs.dedup();

        self.iteration_stability_values.sort_unstable();
        self.iteration_stability_values.dedup();

        Ok(CompiledIslandIr {
            value_program: self.values.finish(),
            matrix_program,
            rhs_program: RhsProgram::new(self.rhs_ops),
            state_transition,
            matrix_static_inputs: self.matrix_static_inputs.into_boxed_slice(),
            iteration_matrix_sources: iteration_matrix_sources.into_boxed_slice(),
            iteration_stamp_dependency: self.iteration_stamp_dependency,
            iteration_stability_values: self.iteration_stability_values.into_boxed_slice(),
            timestep_input: self.timestep_input,
            solution_inputs,
            state_inputs,
            iteration_latches,
        })
    }

    #[inline]
    fn mark_matrix_static_input(&mut self, input: InputSlot) {
        self.matrix_static_inputs.push(input);
    }

    #[inline]
    fn record_iteration_stamp_dependency(&mut self, source: ValueSlot, affects_matrix: bool) {
        let rate = self
            .values
            .rate(source)
            .expect("island IR stamp source must belong to its value program");

        if rate != EvaluationRate::Iteration {
            return;
        }

        if affects_matrix {
            self.iteration_stamp_dependency = IterationStampDependency::Matrix;
        } else if self.iteration_stamp_dependency == IterationStampDependency::None {
            self.iteration_stamp_dependency = IterationStampDependency::Rhs;
        }
    }

    #[inline]
    pub(crate) fn require_iteration_stability(&mut self, value: ValueSlot) {
        debug_assert_eq!(
            self.values
                .rate(value)
                .expect("stability value must belong to its value program"),
            EvaluationRate::Iteration,
            "iteration stability values must be iteration-rate values",
        );

        self.iteration_stability_values.push(value);
    }

    #[inline]
    pub(crate) const fn pattern(&self) -> &MnaPattern {
        self.pattern
    }
}

#[derive(Debug)]
pub(crate) struct CompiledIslandIr {
    value_program: ValueProgram,
    matrix_program: MatrixProgram,
    rhs_program: RhsProgram,
    state_transition: StateTransitionProgram,
    matrix_static_inputs: Box<[InputSlot]>,
    iteration_matrix_sources: Box<[ValueSlot]>,
    iteration_stamp_dependency: IterationStampDependency,
    iteration_stability_values: Box<[ValueSlot]>,
    iteration_latches: Box<[IterationLatch]>,
    timestep_input: Option<InputSlot>,
    solution_inputs: Box<[(UnknownIndex, InputSlot)]>,
    state_inputs: Box<[(StateSlot, InputSlot)]>,
}

impl CompiledIslandIr {
    #[inline]
    pub(crate) fn value_program(&self) -> &ValueProgram {
        &self.value_program
    }

    #[inline]
    pub(crate) fn matrix_program(&self) -> &MatrixProgram {
        &self.matrix_program
    }

    #[inline]
    pub(crate) fn rhs_program(&self) -> &RhsProgram {
        &self.rhs_program
    }

    #[inline]
    pub(crate) fn state_transition(&self) -> &StateTransitionProgram {
        &self.state_transition
    }

    #[inline]
    pub(crate) fn static_input_affects_matrix(&self, input: InputSlot) -> bool {
        self.matrix_static_inputs.binary_search(&input).is_ok()
    }

    #[inline]
    pub(crate) const fn timestep_input(&self) -> Option<InputSlot> {
        self.timestep_input
    }

    #[inline]
    pub(crate) fn solution_inputs(&self) -> &[(UnknownIndex, InputSlot)] {
        &self.solution_inputs
    }

    #[inline]
    pub(crate) fn state_inputs(&self) -> &[(StateSlot, InputSlot)] {
        &self.state_inputs
    }

    #[inline]
    pub(crate) const fn requires_nonlinear_iteration(&self) -> bool {
        self.iteration_stamp_dependency.requires_iteration()
    }

    #[inline]
    #[cfg(test)]
    pub(crate) const fn iteration_affects_matrix(&self) -> bool {
        self.iteration_stamp_dependency.affects_matrix()
    }

    #[inline]
    pub(crate) fn iteration_matrix_sources(&self) -> &[ValueSlot] {
        &self.iteration_matrix_sources
    }

    #[inline]
    pub(crate) fn iteration_stability_values(&self) -> &[ValueSlot] {
        &self.iteration_stability_values
    }

    #[inline]
    pub(crate) fn iteration_latches(&self) -> &[IterationLatch] {
        &self.iteration_latches
    }

    #[cfg(test)]
    #[inline]
    pub(crate) fn force_nonlinear_iteration_for_test(&mut self, affects_matrix: bool) {
        self.iteration_stamp_dependency = if affects_matrix {
            IterationStampDependency::Matrix
        } else {
            IterationStampDependency::Rhs
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use hynergy_mna::pattern::PatternBuilder;

    fn empty_pattern(dimension: usize) -> MnaPattern {
        PatternBuilder::new(dimension).unwrap().finish().unwrap()
    }

    #[test]
    fn references_to_same_unknown_share_solution_input() {
        let pattern = empty_pattern(2);
        let unknown = UnknownIndex::new(1);

        let mut builder = IslandIrBuilder::new(&pattern);

        let first = builder.unknown_value(Some(unknown)).unwrap();
        let second = builder.unknown_value(Some(unknown)).unwrap();

        assert_eq!(first, second);

        let ir = builder.finish().unwrap();

        assert_eq!(ir.solution_inputs().len(), 1);
        assert_eq!(ir.solution_inputs()[0].0, unknown,);

        let input = ir.solution_inputs()[0].1;

        assert_eq!(input.value(), first);

        let mut workspace = ir.value_program().new_workspace();

        workspace.set_input(input, 4.25);

        ir.value_program().execute_iteration(&mut workspace);

        assert_eq!(workspace.value(first), 4.25,);
    }

    #[test]
    fn reference_node_is_shared_constant_zero() {
        let pattern = empty_pattern(1);
        let mut builder = IslandIrBuilder::new(&pattern);

        let first = builder.unknown_value(None).unwrap();
        let second = builder.unknown_value(None).unwrap();

        assert_eq!(first, second);

        let ir = builder.finish().unwrap();

        assert!(ir.solution_inputs().is_empty());

        let workspace = ir.value_program().new_workspace();

        assert_eq!(workspace.value(first), 0.0);
    }

    #[test]
    fn timestep_is_shared_across_all_references() {
        let pattern = empty_pattern(0);
        let mut builder = IslandIrBuilder::new(&pattern);

        let first = builder.timestep_value(false).unwrap();
        let second = builder.timestep_value(false).unwrap();

        assert_eq!(first, second);

        let ir = builder.finish().unwrap();
        let input = ir.timestep_input().unwrap();

        assert_eq!(input.value(), first);

        let mut workspace = ir.value_program().new_workspace();

        workspace.set_input(input, 0.125);
        ir.value_program().execute_static(&mut workspace);

        assert_eq!(workspace.value(first), 0.125,);
    }

    #[test]
    fn references_to_same_state_share_tick_input() {
        let pattern = empty_pattern(0);
        let state = StateSlot::new(7);

        let mut builder = IslandIrBuilder::new(&pattern);

        let first = builder.state_value(state).unwrap();
        let second = builder.state_value(state).unwrap();

        assert_eq!(first, second);

        let ir = builder.finish().unwrap();

        assert_eq!(ir.state_inputs().len(), 1);
        assert_eq!(ir.state_inputs()[0].0, state,);

        let input = ir.state_inputs()[0].1;
        let mut workspace = ir.value_program().new_workspace();

        workspace.set_input(input, -3.5);

        ir.value_program().execute_tick(&mut workspace);

        assert_eq!(workspace.value(first), -3.5,);
    }

    #[test]
    fn duplicate_state_producers_are_rejected_at_finish() {
        let pattern = empty_pattern(0);
        let mut builder = IslandIrBuilder::new(&pattern);
        let first = builder.constant_value(1.0).unwrap();
        let second = builder.constant_value(2.0).unwrap();
        let state = StateSlot::new(0);

        builder.write_state(state, first);
        builder.write_state(state, second);

        assert_eq!(
            builder.finish().unwrap_err(),
            IslandIrBuildError::State(StateProgramError::DuplicateDestination {
                destination: state,
            },),
        );
    }

    #[test]
    fn iteration_dependent_rhs_marks_island_nonlinear() {
        let pattern = empty_pattern(1);
        let unknown = UnknownIndex::new(0);

        let mut builder = IslandIrBuilder::new(&pattern);

        let solution = builder.unknown_value(Some(unknown)).unwrap();

        builder.add_rhs(unknown, solution, 1.0);

        let ir = builder.finish().unwrap();

        assert!(ir.requires_nonlinear_iteration());
        assert!(!ir.iteration_affects_matrix());
    }

    #[test]
    fn iteration_dependent_matrix_marks_nonlinear_matrix() {
        let unknown = UnknownIndex::new(0);

        let mut pattern_builder = PatternBuilder::new(1).unwrap();

        pattern_builder.request(unknown, unknown).unwrap();

        let pattern = pattern_builder.finish().unwrap();
        let mut builder = IslandIrBuilder::new(&pattern);

        let solution = builder.unknown_value(Some(unknown)).unwrap();
        let destination = pattern.slot(unknown, unknown).unwrap();

        builder.add_matrix(destination, solution, 1.0);

        let ir = builder.finish().unwrap();

        assert!(ir.requires_nonlinear_iteration());
        assert!(ir.iteration_affects_matrix());
        assert_eq!(ir.iteration_matrix_sources(), &[solution]);
    }

    #[test]
    fn cancelled_iteration_matrix_contributions_are_not_tracked_as_sources() {
        let unknown = UnknownIndex::new(0);

        let mut pattern_builder = PatternBuilder::new(1).unwrap();

        pattern_builder.request(unknown, unknown).unwrap();

        let pattern = pattern_builder.finish().unwrap();
        let mut builder = IslandIrBuilder::new(&pattern);

        let solution = builder.unknown_value(Some(unknown)).unwrap();
        let destination = pattern.slot(unknown, unknown).unwrap();

        builder.add_matrix(destination, solution, 1.0);
        builder.add_matrix(destination, solution, -1.0);

        let ir = builder.finish().unwrap();

        assert!(ir.iteration_matrix_sources().is_empty());
    }

    #[test]
    fn iteration_stability_values_are_deduplicated() {
        let pattern = empty_pattern(1);
        let unknown = UnknownIndex::new(0);

        let mut builder = IslandIrBuilder::new(&pattern);

        let value = builder.unknown_value(Some(unknown)).unwrap();

        builder.require_iteration_stability(value);
        builder.require_iteration_stability(value);

        let ir = builder.finish().unwrap();

        assert_eq!(ir.iteration_stability_values(), &[value]);
    }

    #[test]
    fn iteration_latch_records_initial_and_update_values() {
        let pattern = empty_pattern(1);

        let mut builder = IslandIrBuilder::new(&pattern);

        let initial = builder.state_value(StateSlot::new(0)).unwrap();
        let latch = builder.iteration_latch(initial).unwrap();

        let unknown = builder.unknown_value(Some(UnknownIndex::new(0))).unwrap();
        let update = builder.less_equal_value(latch, unknown).unwrap();

        builder.update_iteration_latch(latch, update);

        let ir = builder.finish().unwrap();

        assert_eq!(ir.iteration_latches().len(), 1);

        let compiled = ir.iteration_latches()[0];

        assert_eq!(compiled.initial(), initial);
        assert_eq!(compiled.input().value(), latch);
        assert_eq!(compiled.update(), update);
    }
}
