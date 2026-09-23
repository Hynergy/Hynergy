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

impl ValueProgram {
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
}
