use thiserror::Error;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueSlot(u32);

impl ValueSlot {
    #[cfg(test)]
    #[inline]
    pub(crate) const fn new(index: u32) -> Self {
        Self(index)
    }

    #[inline]
    pub const fn get(self) -> u32 {
        self.0
    }

    #[inline]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputSlot(u32);

impl InputSlot {
    #[inline]
    pub const fn value(self) -> ValueSlot {
        ValueSlot(self.0)
    }

    #[inline]
    const fn index(self) -> usize {
        self.0 as usize
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EvaluationRate {
    Constant = 0,
    Static = 1,
    Tick = 2,
    Iteration = 3,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ValueBuildError {
    #[error("the value-slot range is exhausted")]
    SlotExhausted,
    #[error("unknown value slot {slot:?}")]
    UnknownValue { slot: ValueSlot },
}

#[derive(Debug, Clone, Copy)]
struct SlotInfo {
    rate: EvaluationRate,
    constant: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
enum ValueOp {
    Add {
        destination: ValueSlot,
        lhs: ValueSlot,
        rhs: ValueSlot,
    },

    Sub {
        destination: ValueSlot,
        lhs: ValueSlot,
        rhs: ValueSlot,
    },

    Mul {
        destination: ValueSlot,
        lhs: ValueSlot,
        rhs: ValueSlot,
    },

    Div {
        destination: ValueSlot,
        lhs: ValueSlot,
        rhs: ValueSlot,
    },

    Neg {
        destination: ValueSlot,
        operand: ValueSlot,
    },

    LessEqual {
        destination: ValueSlot,
        lhs: ValueSlot,
        rhs: ValueSlot,
    },
}

impl ValueOp {
    #[inline]
    fn destination(self) -> ValueSlot {
        match self {
            Self::Add { destination, .. }
            | Self::Sub { destination, .. }
            | Self::Mul { destination, .. }
            | Self::Div { destination, .. }
            | Self::LessEqual { destination, .. }
            | Self::Neg { destination, .. } => destination,
        }
    }

    #[inline]
    fn execute(self, values: &mut [f64]) {
        match self {
            Self::Add {
                destination,
                lhs,
                rhs,
            } => {
                values[destination.index()] = values[lhs.index()] + values[rhs.index()];
            }

            Self::Sub {
                destination,
                lhs,
                rhs,
            } => {
                values[destination.index()] = values[lhs.index()] - values[rhs.index()];
            }

            Self::Mul {
                destination,
                lhs,
                rhs,
            } => {
                values[destination.index()] = values[lhs.index()] * values[rhs.index()];
            }

            Self::Div {
                destination,
                lhs,
                rhs,
            } => {
                values[destination.index()] = values[lhs.index()] / values[rhs.index()];
            }

            Self::LessEqual {
                destination,
                lhs,
                rhs,
            } => {
                values[destination.index()] = if values[lhs.index()] <= values[rhs.index()] {
                    1.0
                } else {
                    0.0
                };
            }

            Self::Neg {
                destination,
                operand,
            } => {
                values[destination.index()] = -values[operand.index()];
            }
        }
    }

    #[inline]
    fn visit_dependencies(self, visit: &mut impl FnMut(ValueSlot, ValueSlot)) {
        match self {
            Self::Add {
                destination,
                lhs,
                rhs,
            }
            | Self::Sub {
                destination,
                lhs,
                rhs,
            }
            | Self::Mul {
                destination,
                lhs,
                rhs,
            }
            | Self::Div {
                destination,
                lhs,
                rhs,
            }
            | Self::LessEqual {
                destination,
                lhs,
                rhs,
            } => {
                visit(destination, lhs);
                visit(destination, rhs);
            }

            Self::Neg {
                destination,
                operand,
            } => {
                visit(destination, operand);
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    LessEqual,
}

#[derive(Debug, Default)]
pub struct ValueProgramBuilder {
    slots: Vec<SlotInfo>,
    initial_values: Vec<f64>,

    static_ops: Vec<ValueOp>,
    tick_ops: Vec<ValueOp>,
    iteration_ops: Vec<ValueOp>,
}

impl ValueProgramBuilder {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn with_capacity(value_count: usize, op_count: usize) -> Self {
        Self {
            slots: Vec::with_capacity(value_count),
            initial_values: Vec::with_capacity(value_count),

            static_ops: Vec::with_capacity(op_count),
            tick_ops: Vec::new(),
            iteration_ops: Vec::new(),
        }
    }

    pub fn constant(&mut self, value: f64) -> Result<ValueSlot, ValueBuildError> {
        self.allocate_slot(
            SlotInfo {
                rate: EvaluationRate::Constant,
                constant: Some(value),
            },
            value,
        )
    }

    pub fn static_input(&mut self) -> Result<InputSlot, ValueBuildError> {
        self.input(EvaluationRate::Static)
    }

    pub fn tick_input(&mut self) -> Result<InputSlot, ValueBuildError> {
        self.input(EvaluationRate::Tick)
    }

    pub fn iteration_input(&mut self) -> Result<InputSlot, ValueBuildError> {
        self.input(EvaluationRate::Iteration)
    }

    pub fn add(&mut self, lhs: ValueSlot, rhs: ValueSlot) -> Result<ValueSlot, ValueBuildError> {
        self.binary(BinaryOp::Add, lhs, rhs)
    }

    pub fn sub(&mut self, lhs: ValueSlot, rhs: ValueSlot) -> Result<ValueSlot, ValueBuildError> {
        self.binary(BinaryOp::Sub, lhs, rhs)
    }

    pub fn mul(&mut self, lhs: ValueSlot, rhs: ValueSlot) -> Result<ValueSlot, ValueBuildError> {
        self.binary(BinaryOp::Mul, lhs, rhs)
    }

    pub fn div(&mut self, lhs: ValueSlot, rhs: ValueSlot) -> Result<ValueSlot, ValueBuildError> {
        self.binary(BinaryOp::Div, lhs, rhs)
    }

    pub fn less_equal(
        &mut self,
        lhs: ValueSlot,
        rhs: ValueSlot,
    ) -> Result<ValueSlot, ValueBuildError> {
        self.binary(BinaryOp::LessEqual, lhs, rhs)
    }

    pub fn neg(&mut self, operand: ValueSlot) -> Result<ValueSlot, ValueBuildError> {
        let info = self.slot_info(operand)?;

        if let Some(value) = info.constant {
            return self.constant(-value);
        }

        let destination = self.allocate_computed(info.rate)?;

        self.push_op(
            info.rate,
            ValueOp::Neg {
                destination,
                operand,
            },
        );

        Ok(destination)
    }

    #[inline]
    pub fn rate(&self, slot: ValueSlot) -> Result<EvaluationRate, ValueBuildError> {
        Ok(self.slot_info(slot)?.rate)
    }

    #[inline]
    pub fn value_count(&self) -> usize {
        self.slots.len()
    }

    pub fn finish(self) -> ValueProgram {
        debug_assert_eq!(self.slots.len(), self.initial_values.len());

        ValueProgram {
            initial_values: self.initial_values.into_boxed_slice(),
            static_ops: self.static_ops.into_boxed_slice(),
            tick_ops: self.tick_ops.into_boxed_slice(),
            iteration_ops: self.iteration_ops.into_boxed_slice(),
        }
    }

    fn input(&mut self, rate: EvaluationRate) -> Result<InputSlot, ValueBuildError> {
        debug_assert!(rate != EvaluationRate::Constant);

        let value = self.allocate_slot(
            SlotInfo {
                rate,
                constant: None,
            },
            0.0,
        )?;

        Ok(InputSlot(value.get()))
    }

    fn binary(
        &mut self,
        op: BinaryOp,
        lhs: ValueSlot,
        rhs: ValueSlot,
    ) -> Result<ValueSlot, ValueBuildError> {
        let lhs_info = self.slot_info(lhs)?;
        let rhs_info = self.slot_info(rhs)?;

        if let (Some(lhs), Some(rhs)) = (lhs_info.constant, rhs_info.constant) {
            let value = match op {
                BinaryOp::Add => lhs + rhs,
                BinaryOp::Sub => lhs - rhs,
                BinaryOp::Mul => lhs * rhs,
                BinaryOp::Div => lhs / rhs,
                BinaryOp::LessEqual => {
                    if lhs <= rhs {
                        1.0
                    } else {
                        0.0
                    }
                }
            };

            return self.constant(value);
        }

        let rate = lhs_info.rate.max(rhs_info.rate);
        let destination = self.allocate_computed(rate)?;

        let op = match op {
            BinaryOp::Add => ValueOp::Add {
                destination,
                lhs,
                rhs,
            },

            BinaryOp::Sub => ValueOp::Sub {
                destination,
                lhs,
                rhs,
            },

            BinaryOp::Mul => ValueOp::Mul {
                destination,
                lhs,
                rhs,
            },

            BinaryOp::LessEqual => ValueOp::LessEqual {
                destination,
                lhs,
                rhs,
            },

            BinaryOp::Div => ValueOp::Div {
                destination,
                lhs,
                rhs,
            },
        };

        self.push_op(rate, op);

        Ok(destination)
    }

    #[inline]
    fn allocate_computed(&mut self, rate: EvaluationRate) -> Result<ValueSlot, ValueBuildError> {
        debug_assert!(rate != EvaluationRate::Constant);

        self.allocate_slot(
            SlotInfo {
                rate,
                constant: None,
            },
            0.0,
        )
    }

    fn allocate_slot(
        &mut self,
        info: SlotInfo,
        initial_value: f64,
    ) -> Result<ValueSlot, ValueBuildError> {
        let index = u32::try_from(self.slots.len()).map_err(|_| ValueBuildError::SlotExhausted)?;

        self.slots.push(info);
        self.initial_values.push(initial_value);

        Ok(ValueSlot(index))
    }

    #[inline]
    fn slot_info(&self, slot: ValueSlot) -> Result<SlotInfo, ValueBuildError> {
        self.slots
            .get(slot.index())
            .copied()
            .ok_or(ValueBuildError::UnknownValue { slot })
    }

    #[inline]
    fn push_op(&mut self, rate: EvaluationRate, op: ValueOp) {
        match rate {
            EvaluationRate::Constant => {
                unreachable!("constant values must be folded")
            }

            EvaluationRate::Static => {
                self.static_ops.push(op);
            }

            EvaluationRate::Tick => {
                self.tick_ops.push(op);
            }

            EvaluationRate::Iteration => {
                self.iteration_ops.push(op);
            }
        }
    }
}

#[derive(Debug)]
pub struct ValueProgram {
    initial_values: Box<[f64]>,

    static_ops: Box<[ValueOp]>,
    tick_ops: Box<[ValueOp]>,
    iteration_ops: Box<[ValueOp]>,
}

/// Direct iteration-operation users of each value, compiled for one program.
/// This metadata is optional; ordinary full evaluation does not allocate it.
#[derive(Debug)]
pub struct IterationDependencyPlan {
    offsets: Box<[u32]>,
    dependent_ops: Box<[u32]>,
    operation_count: usize,
}

/// Reusable dirty-operation bits for an iteration dependency plan.
#[derive(Debug)]
pub struct IterationScratch {
    dirty_words: Box<[u64]>,
    pending_count: usize,
    operation_count: usize,
}

impl IterationDependencyPlan {
    pub fn new_scratch(&self) -> IterationScratch {
        IterationScratch {
            dirty_words: vec![0; self.operation_count.div_ceil(64)].into_boxed_slice(),
            pending_count: 0,
            operation_count: self.operation_count,
        }
    }

    /// Mark direct users after changing an iteration input in the workspace.
    /// Use scratch made by this plan and mark every changed iteration input.
    pub fn mark_input(&self, input: InputSlot, scratch: &mut IterationScratch) {
        self.assert_scratch(scratch);
        self.mark_users(input.value(), scratch);
    }

    #[inline]
    fn mark_users(&self, value: ValueSlot, scratch: &mut IterationScratch) {
        let start = self.offsets[value.index()] as usize;
        let end = self.offsets[value.index() + 1] as usize;
        for &operation in &self.dependent_ops[start..end] {
            let operation = operation as usize;
            let mask = 1_u64 << (operation % 64);
            let word = &mut scratch.dirty_words[operation / 64];
            if *word & mask == 0 {
                *word |= mask;
                scratch.pending_count += 1;
            }
        }
    }

    #[inline]
    fn assert_scratch(&self, scratch: &IterationScratch) {
        assert_eq!(
            self.operation_count, scratch.operation_count,
            "IterationScratch must match the iteration dependency plan"
        );
    }
}

impl ValueProgram {
    /// Compile a compact reverse graph without copying operations or values.
    pub fn compile_iteration_dependencies(&self) -> IterationDependencyPlan {
        let mut offsets = vec![
            0_u32;
            self.value_count()
                .checked_add(1)
                .expect("value count overflow")
        ];
        for &op in &self.iteration_ops {
            op.visit_dependencies(&mut |_, source| {
                let count = &mut offsets[source.index() + 1];
                *count = count
                    .checked_add(1)
                    .expect("iteration dependency count exceeds u32");
            });
        }
        for index in 1..offsets.len() {
            offsets[index] = offsets[index]
                .checked_add(offsets[index - 1])
                .expect("iteration dependency offsets exceed u32");
        }
        let mut dependent_ops = vec![0; offsets[self.value_count()] as usize];
        let mut cursors = offsets[..self.value_count()].to_vec();
        for (index, &op) in self.iteration_ops.iter().enumerate() {
            let index = u32::try_from(index).expect("iteration operation count exceeds u32");
            op.visit_dependencies(&mut |_, source| {
                let cursor = &mut cursors[source.index()];
                dependent_ops[*cursor as usize] = index;
                *cursor += 1;
            });
        }
        IterationDependencyPlan {
            offsets: offsets.into_boxed_slice(),
            dependent_ops: dependent_ops.into_boxed_slice(),
            operation_count: self.iteration_ops.len(),
        }
    }

    /// Refresh marked iteration inputs and their transitive users.
    ///
    /// The plan must be compiled from this program. First fully evaluate the
    /// workspace, then set and mark every changed iteration input. Changes to
    /// static or tick inputs require a fresh full evaluation before reuse.
    /// Returns the number of operations executed and leaves scratch empty.
    pub fn execute_iteration_incremental(
        &self,
        plan: &IterationDependencyPlan,
        workspace: &mut ValueWorkspace,
        scratch: &mut IterationScratch,
    ) -> usize {
        self.assert_workspace(workspace);
        assert_eq!(
            self.value_count(),
            plan.offsets.len() - 1,
            "iteration plan value count mismatch"
        );
        assert_eq!(
            self.iteration_ops.len(),
            plan.operation_count,
            "iteration plan operation count mismatch"
        );
        plan.assert_scratch(scratch);
        if scratch.pending_count == 0 {
            return 0;
        }
        if scratch.pending_count >= self.iteration_ops.len().div_ceil(4) {
            execute_ops(&self.iteration_ops, &mut workspace.values);
            scratch.dirty_words.fill(0);
            scratch.pending_count = 0;
            return self.iteration_ops.len();
        }

        let mut executed = 0;
        // Builder order is topological. Re-read each word because an operation
        // can mark a successor in the same word while it is being drained.
        for word_index in 0..scratch.dirty_words.len() {
            while scratch.dirty_words[word_index] != 0 {
                let bit = scratch.dirty_words[word_index].trailing_zeros() as usize;
                let index = word_index * 64 + bit;
                scratch.dirty_words[word_index] &= !(1_u64 << bit);
                scratch.pending_count -= 1;
                let op = self.iteration_ops[index];
                op.execute(&mut workspace.values);
                executed += 1;
                // Do not stop at numerically equal results: signed zero and
                // NaN payloads must retain the full evaluator's arithmetic.
                plan.mark_users(op.destination(), scratch);
            }
        }
        debug_assert_eq!(scratch.pending_count, 0);
        executed
    }

    #[inline]
    pub fn value_count(&self) -> usize {
        self.initial_values.len()
    }

    #[inline]
    pub fn static_op_count(&self) -> usize {
        self.static_ops.len()
    }

    #[inline]
    pub fn tick_op_count(&self) -> usize {
        self.tick_ops.len()
    }

    #[inline]
    pub fn iteration_op_count(&self) -> usize {
        self.iteration_ops.len()
    }

    #[inline]
    pub fn visit_iteration_dependencies(&self, mut visit: impl FnMut(ValueSlot, ValueSlot)) {
        for &op in &self.iteration_ops {
            op.visit_dependencies(&mut visit);
        }
    }

    pub fn new_workspace(&self) -> ValueWorkspace {
        ValueWorkspace {
            values: self.initial_values.to_vec().into_boxed_slice(),
        }
    }

    #[inline]
    pub fn execute_static(&self, workspace: &mut ValueWorkspace) {
        self.assert_workspace(workspace);
        execute_ops(&self.static_ops, &mut workspace.values);
    }

    #[inline]
    pub fn execute_tick(&self, workspace: &mut ValueWorkspace) {
        self.assert_workspace(workspace);
        execute_ops(&self.tick_ops, &mut workspace.values);
    }

    #[inline]
    pub fn execute_iteration(&self, workspace: &mut ValueWorkspace) {
        self.assert_workspace(workspace);
        execute_ops(&self.iteration_ops, &mut workspace.values);
    }

    #[inline]
    fn assert_workspace(&self, workspace: &ValueWorkspace) {
        assert_eq!(
            workspace.values.len(),
            self.initial_values.len(),
            "ValueWorkspace does not belong to a compatible ValueProgram",
        );
    }
}

#[derive(Debug)]
pub struct ValueWorkspace {
    values: Box<[f64]>,
}

impl ValueWorkspace {
    #[inline]
    pub fn values(&self) -> &[f64] {
        &self.values
    }

    #[inline]
    pub fn value(&self, slot: ValueSlot) -> f64 {
        self.values[slot.index()]
    }

    #[inline]
    pub fn set_input(&mut self, slot: InputSlot, value: f64) {
        self.values[slot.index()] = value;
    }
}

#[inline]
fn execute_ops(ops: &[ValueOp], values: &mut [f64]) {
    for &op in ops {
        op.execute(values);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_fully_constant_expression() {
        let mut builder = ValueProgramBuilder::new();

        let four = builder.constant(4.0).unwrap();
        let three = builder.constant(3.0).unwrap();

        let product = builder.mul(four, three).unwrap();
        let result = builder.neg(product).unwrap();

        assert_eq!(builder.rate(result).unwrap(), EvaluationRate::Constant);

        let program = builder.finish();

        assert_eq!(program.static_op_count(), 0);
        assert_eq!(program.tick_op_count(), 0);
        assert_eq!(program.iteration_op_count(), 0);

        let workspace = program.new_workspace();

        assert_eq!(workspace.value(product), 12.0);
        assert_eq!(workspace.value(result), -12.0);
    }

    #[test]
    fn static_expression_executes_only_in_static_stage() {
        let mut builder = ValueProgramBuilder::new();

        let parameter = builder.static_input().unwrap();
        let timestep = builder.static_input().unwrap();

        let ratio = builder.div(parameter.value(), timestep.value()).unwrap();

        assert_eq!(builder.rate(ratio).unwrap(), EvaluationRate::Static);

        let program = builder.finish();

        assert_eq!(program.static_op_count(), 1);
        assert_eq!(program.tick_op_count(), 0);
        assert_eq!(program.iteration_op_count(), 0);

        let mut workspace = program.new_workspace();

        workspace.set_input(parameter, 4.0);
        workspace.set_input(timestep, 2.0);

        program.execute_static(&mut workspace);

        assert_eq!(workspace.value(ratio), 2.0);
    }

    #[test]
    fn tick_expression_can_depend_on_static_result() {
        let mut builder = ValueProgramBuilder::new();

        let capacitance = builder.static_input().unwrap();
        let timestep = builder.static_input().unwrap();
        let old_voltage = builder.tick_input().unwrap();

        let conductance = builder.div(capacitance.value(), timestep.value()).unwrap();

        let history = builder.mul(conductance, old_voltage.value()).unwrap();

        assert_eq!(builder.rate(conductance).unwrap(), EvaluationRate::Static);

        assert_eq!(builder.rate(history).unwrap(), EvaluationRate::Tick);

        let program = builder.finish();

        assert_eq!(program.static_op_count(), 1);
        assert_eq!(program.tick_op_count(), 1);

        let mut workspace = program.new_workspace();

        workspace.set_input(capacitance, 6.0);
        workspace.set_input(timestep, 2.0);

        program.execute_static(&mut workspace);

        assert_eq!(workspace.value(conductance), 3.0);

        workspace.set_input(old_voltage, 5.0);

        program.execute_tick(&mut workspace);

        assert_eq!(workspace.value(history), 15.0);
    }

    #[test]
    fn iteration_expression_can_depend_on_tick_and_static_results() {
        let mut builder = ValueProgramBuilder::new();

        let parameter = builder.static_input().unwrap();
        let old_state = builder.tick_input().unwrap();
        let solution = builder.iteration_input().unwrap();

        let history = builder.mul(parameter.value(), old_state.value()).unwrap();

        let result = builder.add(history, solution.value()).unwrap();

        assert_eq!(builder.rate(history).unwrap(), EvaluationRate::Tick);

        assert_eq!(builder.rate(result).unwrap(), EvaluationRate::Iteration);

        let program = builder.finish();

        let mut workspace = program.new_workspace();

        workspace.set_input(parameter, 2.0);
        program.execute_static(&mut workspace);

        workspace.set_input(old_state, 3.0);
        program.execute_tick(&mut workspace);

        workspace.set_input(solution, 4.0);
        program.execute_iteration(&mut workspace);

        assert_eq!(workspace.value(history), 6.0);
        assert_eq!(workspace.value(result), 10.0);
    }

    #[test]
    fn operations_preserve_ssa_dependency_order_within_stage() {
        let mut builder = ValueProgramBuilder::new();

        let input = builder.tick_input().unwrap();
        let two = builder.constant(2.0).unwrap();

        let doubled = builder.mul(input.value(), two).unwrap();
        let quadrupled = builder.mul(doubled, two).unwrap();
        let negated = builder.neg(quadrupled).unwrap();

        let program = builder.finish();

        assert_eq!(program.tick_op_count(), 3);

        let mut workspace = program.new_workspace();

        workspace.set_input(input, 3.0);
        program.execute_tick(&mut workspace);

        assert_eq!(workspace.value(doubled), 6.0);
        assert_eq!(workspace.value(quadrupled), 12.0);
        assert_eq!(workspace.value(negated), -12.0);
    }

    #[test]
    fn rejects_unknown_value_slots_during_build() {
        let mut builder = ValueProgramBuilder::new();

        let valid = builder.constant(1.0).unwrap();
        let unknown = ValueSlot::new(10);

        assert_eq!(
            builder.add(valid, unknown),
            Err(ValueBuildError::UnknownValue { slot: unknown })
        );
    }

    #[test]
    fn tick_stage_does_not_reexecute_static_operations() {
        let mut builder = ValueProgramBuilder::new();

        let static_input = builder.static_input().unwrap();
        let tick_input = builder.tick_input().unwrap();

        let two = builder.constant(2.0).unwrap();

        let static_value = builder.mul(static_input.value(), two).unwrap();

        let tick_value = builder.add(static_value, tick_input.value()).unwrap();

        let program = builder.finish();
        let mut workspace = program.new_workspace();

        workspace.set_input(static_input, 3.0);
        program.execute_static(&mut workspace);

        assert_eq!(workspace.value(static_value), 6.0);

        workspace.set_input(static_input, 100.0);
        workspace.set_input(tick_input, 5.0);

        program.execute_tick(&mut workspace);

        assert_eq!(workspace.value(static_value), 6.0);
        assert_eq!(workspace.value(tick_value), 11.0);
    }

    #[test]
    fn less_equal_folds_constant_operands() {
        let mut builder = ValueProgramBuilder::new();

        let two = builder.constant(2.0).unwrap();
        let three = builder.constant(3.0).unwrap();

        let less = builder.less_equal(two, three).unwrap();
        let equal = builder.less_equal(two, two).unwrap();
        let greater = builder.less_equal(three, two).unwrap();

        assert_eq!(builder.rate(less).unwrap(), EvaluationRate::Constant);
        assert_eq!(builder.rate(equal).unwrap(), EvaluationRate::Constant);
        assert_eq!(builder.rate(greater).unwrap(), EvaluationRate::Constant,);

        let program = builder.finish();

        assert_eq!(program.static_op_count(), 0);
        assert_eq!(program.tick_op_count(), 0);
        assert_eq!(program.iteration_op_count(), 0);

        let workspace = program.new_workspace();

        assert_eq!(workspace.value(less), 1.0);
        assert_eq!(workspace.value(equal), 1.0);
        assert_eq!(workspace.value(greater), 0.0);
    }

    #[test]
    fn less_equal_executes_at_highest_operand_rate() {
        let mut builder = ValueProgramBuilder::new();

        let limit = builder.static_input().unwrap();
        let value = builder.iteration_input().unwrap();

        let comparison = builder.less_equal(value.value(), limit.value()).unwrap();

        assert_eq!(builder.rate(comparison).unwrap(), EvaluationRate::Iteration,);

        let program = builder.finish();
        let mut workspace = program.new_workspace();

        workspace.set_input(limit, 2.0);
        program.execute_static(&mut workspace);

        workspace.set_input(value, 1.0);
        program.execute_iteration(&mut workspace);
        assert_eq!(workspace.value(comparison), 1.0);

        workspace.set_input(value, 2.0);
        program.execute_iteration(&mut workspace);
        assert_eq!(workspace.value(comparison), 1.0);

        workspace.set_input(value, 3.0);
        program.execute_iteration(&mut workspace);
        assert_eq!(workspace.value(comparison), 0.0);
    }

    #[test]
    fn iteration_dependency_visitor_reports_only_iteration_edges() {
        let mut builder = ValueProgramBuilder::new();

        let static_input = builder.static_input().unwrap();
        let tick_input = builder.tick_input().unwrap();
        let iteration_input = builder.iteration_input().unwrap();
        let two = builder.constant(2.0).unwrap();

        let static_value = builder.mul(static_input.value(), two).unwrap();
        let tick_value = builder.add(static_value, tick_input.value()).unwrap();
        let mixed = builder.add(iteration_input.value(), tick_value).unwrap();
        let negated = builder.neg(mixed).unwrap();
        let compared = builder.less_equal(negated, static_value).unwrap();

        let program = builder.finish();
        let mut edges = Vec::new();

        program.visit_iteration_dependencies(|destination, source| {
            edges.push((destination, source));
        });

        assert_eq!(
            edges,
            vec![
                (mixed, iteration_input.value()),
                (mixed, tick_value),
                (negated, mixed),
                (compared, negated),
                (compared, static_value),
            ],
        );
    }

    #[test]
    fn value_op_remains_compact() {
        assert_eq!(size_of::<ValueOp>(), 16);
    }

    #[test]
    fn incremental_execution_prunes_unrelated_ops() {
        let mut builder = ValueProgramBuilder::new();
        let source = builder.iteration_input().unwrap();
        let one = builder.constant(1.0).unwrap();
        let two = builder.constant(2.0).unwrap();
        let first = builder.add(source.value(), one).unwrap();
        let second = builder.mul(first, two).unwrap();
        let result = builder.neg(second).unwrap();

        let unrelated = (0..5)
            .map(|_| {
                let input = builder.iteration_input().unwrap();
                let output = builder.add(input.value(), one).unwrap();
                (input, output)
            })
            .collect::<Vec<_>>();
        let program = builder.finish();
        assert_eq!(program.iteration_op_count(), 8);

        let plan = program.compile_iteration_dependencies();
        let mut scratch = plan.new_scratch();
        let mut workspace = program.new_workspace();
        workspace.set_input(source, 1.0);
        for (input, _) in &unrelated {
            workspace.set_input(*input, 10.0);
        }
        program.execute_iteration(&mut workspace);

        workspace.set_input(source, 3.0);
        plan.mark_input(source, &mut scratch);
        assert_eq!(
            program.execute_iteration_incremental(&plan, &mut workspace, &mut scratch),
            3
        );
        assert_eq!(workspace.value(result), -8.0);
        for (_, output) in unrelated {
            assert_eq!(workspace.value(output), 11.0);
        }
        assert_eq!(scratch.pending_count, 0);
    }

    #[test]
    fn incremental_execution_rereads_dirty_words_across_word_boundary() {
        let mut builder = ValueProgramBuilder::new();
        let one = builder.constant(1.0).unwrap();
        let fillers = (0..62)
            .map(|_| {
                let input = builder.iteration_input().unwrap();
                builder.add(input.value(), one).unwrap();
                input
            })
            .collect::<Vec<_>>();
        let source = builder.iteration_input().unwrap();
        let first = builder.add(source.value(), one).unwrap();
        let second = builder.mul(first, one).unwrap();
        let result = builder.neg(second).unwrap();
        let program = builder.finish();
        assert_eq!(program.iteration_op_count(), 65);

        let plan = program.compile_iteration_dependencies();
        let mut scratch = plan.new_scratch();
        let mut workspace = program.new_workspace();
        for input in fillers {
            workspace.set_input(input, 0.0);
        }
        workspace.set_input(source, 1.0);
        program.execute_iteration(&mut workspace);

        workspace.set_input(source, 2.0);
        plan.mark_input(source, &mut scratch);
        assert_eq!(
            program.execute_iteration_incremental(&plan, &mut workspace, &mut scratch),
            3
        );
        assert_eq!(workspace.value(result), -3.0);
    }

    #[test]
    fn incremental_execution_deduplicates_reconverging_roots() {
        let mut builder = ValueProgramBuilder::new();
        let one = builder.constant(1.0).unwrap();
        let two = builder.constant(2.0).unwrap();
        let left_root = builder.iteration_input().unwrap();
        let right_root = builder.iteration_input().unwrap();
        let left_first = builder.add(left_root.value(), one).unwrap();
        let left = builder.mul(left_first, two).unwrap();
        let right = builder.sub(right_root.value(), one).unwrap();
        let joined = builder.add(left, right).unwrap();
        let fillers = (0..8)
            .map(|_| {
                let input = builder.iteration_input().unwrap();
                builder.add(input.value(), one).unwrap();
                input
            })
            .collect::<Vec<_>>();
        let program = builder.finish();

        let plan = program.compile_iteration_dependencies();
        let mut scratch = plan.new_scratch();
        let mut workspace = program.new_workspace();
        workspace.set_input(left_root, 1.0);
        workspace.set_input(right_root, 5.0);
        for input in fillers {
            workspace.set_input(input, 10.0);
        }
        program.execute_iteration(&mut workspace);

        workspace.set_input(left_root, 3.0);
        workspace.set_input(right_root, 8.0);
        plan.mark_input(left_root, &mut scratch);
        plan.mark_input(right_root, &mut scratch);
        assert_eq!(
            program.execute_iteration_incremental(&plan, &mut workspace, &mut scratch),
            4
        );
        assert_eq!(workspace.value(joined), 15.0);
    }

    #[test]
    fn incremental_execution_drains_repeated_rounds_and_handles_empty_program() {
        let mut builder = ValueProgramBuilder::new();
        let source = builder.iteration_input().unwrap();
        let one = builder.constant(1.0).unwrap();
        let first = builder.add(source.value(), one).unwrap();
        let result = builder.neg(first).unwrap();
        let program = builder.finish();
        let plan = program.compile_iteration_dependencies();
        let mut scratch = plan.new_scratch();
        let mut workspace = program.new_workspace();

        workspace.set_input(source, 2.0);
        program.execute_iteration(&mut workspace);
        workspace.set_input(source, 3.0);
        plan.mark_input(source, &mut scratch);
        assert_eq!(
            program.execute_iteration_incremental(&plan, &mut workspace, &mut scratch),
            2
        );
        assert_eq!(workspace.value(result), -4.0);
        assert_eq!(
            program.execute_iteration_incremental(&plan, &mut workspace, &mut scratch),
            0
        );

        workspace.set_input(source, 4.0);
        plan.mark_input(source, &mut scratch);
        assert_eq!(
            program.execute_iteration_incremental(&plan, &mut workspace, &mut scratch),
            2
        );
        assert_eq!(workspace.value(result), -5.0);

        let empty_program = ValueProgramBuilder::new().finish();
        let empty_plan = empty_program.compile_iteration_dependencies();
        let mut empty_scratch = empty_plan.new_scratch();
        let mut empty_workspace = empty_program.new_workspace();
        assert_eq!(
            empty_program.execute_iteration_incremental(
                &empty_plan,
                &mut empty_workspace,
                &mut empty_scratch,
            ),
            0
        );
    }

    #[test]
    fn incremental_execution_uses_full_loop_for_dense_initial_frontier() {
        let mut builder = ValueProgramBuilder::new();
        let one = builder.constant(1.0).unwrap();
        let inputs = (0..8)
            .map(|_| {
                let input = builder.iteration_input().unwrap();
                builder.add(input.value(), one).unwrap();
                input
            })
            .collect::<Vec<_>>();
        let program = builder.finish();
        let plan = program.compile_iteration_dependencies();
        let mut scratch = plan.new_scratch();
        let mut workspace = program.new_workspace();
        program.execute_iteration(&mut workspace);

        for (index, input) in inputs.iter().copied().take(2).enumerate() {
            workspace.set_input(input, (index + 1) as f64);
            plan.mark_input(input, &mut scratch);
        }
        assert_eq!(
            program.execute_iteration_incremental(&plan, &mut workspace, &mut scratch),
            program.iteration_op_count()
        );
        assert_eq!(
            program.execute_iteration_incremental(&plan, &mut workspace, &mut scratch),
            0
        );
    }

    #[test]
    fn incremental_execution_matches_full_bits_for_ieee_values_and_all_ops() {
        let mut builder = ValueProgramBuilder::new();
        let numerator = builder.static_input().unwrap();
        let denominator = builder.static_input().unwrap();
        let tick = builder.tick_input().unwrap();
        let x = builder.iteration_input().unwrap();
        let y = builder.iteration_input().unwrap();
        let z = builder.iteration_input().unwrap();

        let static_value = builder.div(numerator.value(), denominator.value()).unwrap();
        let tick_value = builder.add(tick.value(), static_value).unwrap();
        let added = builder.add(x.value(), tick_value).unwrap();
        let subtracted = builder.sub(y.value(), tick_value).unwrap();
        let multiplied = builder.mul(added, subtracted).unwrap();
        let divided = builder.div(multiplied, z.value()).unwrap();
        let negated = builder.neg(divided).unwrap();
        let compared = builder.less_equal(negated, x.value()).unwrap();
        let one = builder.constant(1.0).unwrap();
        let fillers = (0..24)
            .map(|_| {
                let input = builder.iteration_input().unwrap();
                builder.add(input.value(), one).unwrap();
                input
            })
            .collect::<Vec<_>>();
        let program = builder.finish();
        let plan = program.compile_iteration_dependencies();
        let mut scratch = plan.new_scratch();
        let mut full_workspace = program.new_workspace();
        let mut incremental_workspace = program.new_workspace();

        for workspace in [&mut full_workspace, &mut incremental_workspace] {
            workspace.set_input(numerator, 1.0);
            workspace.set_input(denominator, 1.0);
            workspace.set_input(tick, 0.0);
            for input in &fillers {
                workspace.set_input(*input, 0.0);
            }
            program.execute_static(workspace);
            program.execute_tick(workspace);
            program.execute_iteration(workspace);
        }

        let cases = [
            (0.0, -0.0, -0.0),
            (-0.0, 0.0, 0.0),
            (f64::from_bits(0x7ff8_0000_0000_0042), -1.0, 0.0),
            (f64::INFINITY, f64::NEG_INFINITY, 1.0),
            (f64::NEG_INFINITY, f64::INFINITY, -1.0),
        ];

        for (x_value, y_value, z_value) in cases {
            for workspace in [&mut full_workspace, &mut incremental_workspace] {
                workspace.set_input(x, x_value);
                workspace.set_input(y, y_value);
                workspace.set_input(z, z_value);
            }
            plan.mark_input(x, &mut scratch);
            plan.mark_input(y, &mut scratch);
            plan.mark_input(z, &mut scratch);

            program.execute_iteration(&mut full_workspace);
            assert_eq!(
                program.execute_iteration_incremental(
                    &plan,
                    &mut incremental_workspace,
                    &mut scratch,
                ),
                6
            );
            assert_eq!(
                incremental_workspace
                    .values()
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                full_workspace
                    .values()
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                "workspace values differ for x={x_value:?}, y={y_value:?}, z={z_value:?}"
            );
            assert_eq!(
                incremental_workspace.value(negated).to_bits(),
                full_workspace.value(negated).to_bits()
            );
            assert_eq!(
                incremental_workspace.value(compared).to_bits(),
                full_workspace.value(compared).to_bits()
            );
        }

        for workspace in [&mut full_workspace, &mut incremental_workspace] {
            workspace.set_input(numerator, 3.0);
            workspace.set_input(denominator, 2.0);
            workspace.set_input(tick, 0.5);
            workspace.set_input(x, 3.0);
            workspace.set_input(y, 4.0);
            workspace.set_input(z, 2.0);
            program.execute_static(workspace);
            program.execute_tick(workspace);
            program.execute_iteration(workspace);
            assert_eq!(workspace.value(negated), -5.0);
            workspace.set_input(x, 4.0);
        }
        plan.mark_input(x, &mut scratch);
        program.execute_iteration(&mut full_workspace);
        assert_eq!(
            program.execute_iteration_incremental(&plan, &mut incremental_workspace, &mut scratch),
            5
        );
        assert_eq!(incremental_workspace.value(negated), -6.0);
        assert_eq!(incremental_workspace.values, full_workspace.values);
    }

    #[test]
    fn incremental_input_without_users_does_no_work() {
        let mut builder = ValueProgramBuilder::new();
        let unused = builder.iteration_input().unwrap();
        let used = builder.iteration_input().unwrap();
        let result = builder.neg(used.value()).unwrap();
        let program = builder.finish();
        let plan = program.compile_iteration_dependencies();
        let mut scratch = plan.new_scratch();
        let mut workspace = program.new_workspace();
        workspace.set_input(used, 2.0);
        program.execute_iteration(&mut workspace);
        workspace.set_input(unused, 3.0);
        plan.mark_input(unused, &mut scratch);
        assert_eq!(
            program.execute_iteration_incremental(&plan, &mut workspace, &mut scratch),
            0
        );
        assert_eq!(workspace.value(result), -2.0);
    }
}
