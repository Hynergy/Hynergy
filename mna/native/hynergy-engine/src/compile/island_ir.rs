use hynergy_ir::{
    InputSlot, MatrixAdd, MatrixProgram, RhsAdd, RhsProgram, StateProgramError, StateSlot,
    StateTransitionProgram, StateWrite, ValueBuildError, ValueProgram, ValueProgramBuilder,
    ValueSlot,
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

#[derive(Debug)]
pub(crate) struct IslandIrBuilder<'a> {
    pattern: &'a MnaPattern,

    values: ValueProgramBuilder,

    matrix_ops: Vec<MatrixAdd>,
    rhs_ops: Vec<RhsAdd>,
    state_writes: Vec<StateWrite>,

    timestep_input: Option<InputSlot>,
    zero_value: Option<ValueSlot>,

    solution_inputs: BTreeMap<UnknownIndex, InputSlot>,
    state_inputs: BTreeMap<StateSlot, InputSlot>,
}

impl<'a> IslandIrBuilder<'a> {
    pub(crate) fn new(pattern: &'a MnaPattern) -> Self {
        Self {
            pattern,

            values: ValueProgramBuilder::new(),

            matrix_ops: Vec::new(),
            rhs_ops: Vec::new(),
            state_writes: Vec::new(),

            timestep_input: None,
            zero_value: None,

            solution_inputs: BTreeMap::new(),
            state_inputs: BTreeMap::new(),
        }
    }

    #[inline]
    pub(crate) const fn pattern(&self) -> &MnaPattern {
        self.pattern
    }

    #[inline]
    pub(crate) fn parameter_input(&mut self) -> Result<InputSlot, ValueBuildError> {
        self.values.static_input()
    }

    #[inline]
    pub(crate) fn constant_value(&mut self, value: f64) -> Result<ValueSlot, ValueBuildError> {
        self.values.constant(value)
    }

    pub(crate) fn timestep_value(&mut self) -> Result<ValueSlot, ValueBuildError> {
        if let Some(input) = self.timestep_input {
            return Ok(input.value());
        }

        let input = self.values.static_input()?;

        self.timestep_input = Some(input);

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
    pub(crate) fn add_matrix(&mut self, destination: MatrixSlot, source: ValueSlot, scale: f64) {
        self.matrix_ops
            .push(MatrixAdd::new(destination, source, scale));
    }

    #[inline]
    pub(crate) fn add_rhs(&mut self, destination: UnknownIndex, source: ValueSlot, scale: f64) {
        self.rhs_ops.push(RhsAdd::new(destination, source, scale));
    }

    #[inline]
    pub(crate) fn write_state(&mut self, destination: StateSlot, source: ValueSlot) {
        self.state_writes.push(StateWrite::new(destination, source));
    }

    pub(crate) fn finish(self) -> Result<CompiledIslandIr, IslandIrBuildError> {
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

        Ok(CompiledIslandIr {
            value_program: self.values.finish(),

            matrix_program: MatrixProgram::new(self.matrix_ops),

            rhs_program: RhsProgram::new(self.rhs_ops),

            state_transition,

            timestep_input: self.timestep_input,

            solution_inputs,
            state_inputs,
        })
    }
}

#[derive(Debug)]
pub(crate) struct CompiledIslandIr {
    value_program: ValueProgram,

    matrix_program: MatrixProgram,
    rhs_program: RhsProgram,

    state_transition: StateTransitionProgram,

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

        let first = builder.timestep_value().unwrap();
        let second = builder.timestep_value().unwrap();

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
}
