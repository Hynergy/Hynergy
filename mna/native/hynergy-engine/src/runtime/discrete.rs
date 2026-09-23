use crate::compile::discrete::CompiledDiscretePlan;
use crate::compile::island_ir::CompiledIslandIr;
use hynergy_ir::ValueWorkspace;
use hynergy_mna::pattern::UnknownIndex;

pub(crate) const DISCRETE_CLOSURE_MAX_ROUNDS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClosureOutcome {
    Settled,
    Barrier,
    BudgetExceeded,
    InvalidPrediction,
}

#[derive(Debug)]
pub(crate) struct DiscreteScratch {
    driver_outputs: Box<[f64]>,
}

impl DiscreteScratch {
    #[inline]
    pub(crate) fn new(plan: &CompiledDiscretePlan) -> Self {
        Self {
            driver_outputs: vec![0.0; plan.drivers().len()].into_boxed_slice(),
        }
    }

    #[inline]
    fn driver_outputs_mut(&mut self) -> &mut [f64] {
        &mut self.driver_outputs
    }
}

pub(crate) fn run_discrete_closure(
    plan: &CompiledDiscretePlan,
    ir: &CompiledIslandIr,
    workspace: &mut ValueWorkspace,
    predicted: &mut [f64],
    factorized_iteration_matrix_sources: &[f64],
    rhs_barrier_reference: &[f64],
    scratch: &mut DiscreteScratch,
) -> ClosureOutcome {
    let context = DiscreteClosureContext {
        plan,
        ir,
        factorized_iteration_matrix_sources,
        rhs_barrier_reference,
    };

    context.run_with_limit(workspace, predicted, scratch, DISCRETE_CLOSURE_MAX_ROUNDS)
}

struct DiscreteClosureContext<'a> {
    plan: &'a CompiledDiscretePlan,
    ir: &'a CompiledIslandIr,
    factorized_iteration_matrix_sources: &'a [f64],
    rhs_barrier_reference: &'a [f64],
}

impl DiscreteClosureContext<'_> {
    fn run_with_limit(
        &self,
        workspace: &mut ValueWorkspace,
        predicted: &mut [f64],
        scratch: &mut DiscreteScratch,
        max_rounds: usize,
    ) -> ClosureOutcome {
        debug_assert_eq!(
            self.factorized_iteration_matrix_sources.len(),
            self.ir.iteration_matrix_sources().len(),
        );
        debug_assert_eq!(
            self.rhs_barrier_reference.len(),
            self.plan.rhs_barriers().len()
        );
        debug_assert_eq!(scratch.driver_outputs.len(), self.plan.drivers().len());

        evaluate_iteration(self.ir, workspace, predicted);

        for _ in 0..max_rounds {
            if barriers_changed(
                self.plan,
                workspace,
                self.factorized_iteration_matrix_sources,
                self.rhs_barrier_reference,
            ) {
                return ClosureOutcome::Barrier;
            }

            let driver_outputs = scratch.driver_outputs_mut();

            for (&driver, output) in self.plan.drivers().iter().zip(driver_outputs.iter_mut()) {
                let pull_up = workspace.value(driver.pull_up());
                let pull_down = workspace.value(driver.pull_down());

                let high = unknown_voltage(predicted, driver.high_rail());
                let low = unknown_voltage(predicted, driver.low_rail());

                let denominator = pull_up + pull_down;

                if !pull_up.is_finite()
                    || !pull_down.is_finite()
                    || !high.is_finite()
                    || !low.is_finite()
                    || !denominator.is_finite()
                    || denominator == 0.0
                {
                    return ClosureOutcome::InvalidPrediction;
                }

                let predicted_output = (pull_up * high + pull_down * low) / denominator;

                if !predicted_output.is_finite() {
                    return ClosureOutcome::InvalidPrediction;
                }

                *output = predicted_output;
            }

            let mut changed = false;

            for (&driver, &output) in self.plan.drivers().iter().zip(driver_outputs.iter()) {
                let destination = &mut predicted[driver.output().index()];

                changed |= *destination != output;
                *destination = output;
            }

            if !changed {
                return ClosureOutcome::Settled;
            }

            evaluate_iteration(self.ir, workspace, predicted);
        }

        ClosureOutcome::BudgetExceeded
    }
}

#[inline]
fn barriers_changed(
    plan: &CompiledDiscretePlan,
    workspace: &ValueWorkspace,
    factorized_iteration_matrix_sources: &[f64],
    rhs_barrier_reference: &[f64],
) -> bool {
    for &barrier in plan.matrix_barriers() {
        if workspace.value(barrier.source())
            != factorized_iteration_matrix_sources[barrier.factorized_source_index()]
        {
            return true;
        }
    }

    for (&source, &reference) in plan.rhs_barriers().iter().zip(rhs_barrier_reference) {
        if workspace.value(source) != reference {
            return true;
        }
    }

    false
}

#[inline]
fn unknown_voltage(solution: &[f64], unknown: Option<UnknownIndex>) -> f64 {
    unknown.map_or(0.0, |unknown| solution[unknown.index()])
}

#[inline]
fn evaluate_iteration(ir: &CompiledIslandIr, workspace: &mut ValueWorkspace, solution: &[f64]) {
    for &(unknown, input) in ir.solution_inputs() {
        workspace.set_input(input, solution[unknown.index()]);
    }

    ir.value_program().execute_iteration(workspace);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::discrete::{
        BoundComplementaryDriver, BoundDiscreteMetadata, compile_discrete_plan,
    };
    use crate::compile::island_ir::IslandIrBuilder;
    use hynergy_ir::ValueSlot;
    use hynergy_mna::pattern::{MnaPattern, PatternBuilder};
    use smallvec::{SmallVec, smallvec};

    fn request_conductance(
        builder: &mut PatternBuilder,
        a: Option<UnknownIndex>,
        b: Option<UnknownIndex>,
    ) {
        for (row, column) in [(a, a), (b, a), (a, b), (b, b)] {
            if let (Some(row), Some(column)) = (row, column) {
                builder.request(row, column).unwrap();
            }
        }
    }

    fn add_conductance(
        ir: &mut IslandIrBuilder<'_>,
        a: Option<UnknownIndex>,
        b: Option<UnknownIndex>,
        source: ValueSlot,
    ) {
        for (row, column, scale) in [(a, a, 1.0), (b, a, -1.0), (a, b, -1.0), (b, b, 1.0)] {
            let (Some(row), Some(column)) = (row, column) else {
                continue;
            };

            let destination = ir.pattern().slot(row, column).unwrap();

            ir.add_matrix(destination, source, scale);
        }
    }

    fn binary_pulls(
        ir: &mut IslandIrBuilder<'_>,
        mode: ValueSlot,
        one: ValueSlot,
    ) -> (ValueSlot, ValueSlot) {
        let pull_down = ir.sub_value(one, mode).unwrap();

        (mode, pull_down)
    }

    fn add_driver(
        metadata: &mut BoundDiscreteMetadata,
        mode: ValueSlot,
        output: UnknownIndex,
        high: UnknownIndex,
        pull_up: ValueSlot,
        pull_down: ValueSlot,
    ) {
        let mut modes = SmallVec::<[ValueSlot; 2]>::new();
        let mut drivers = SmallVec::<[BoundComplementaryDriver; 1]>::new();

        modes.push(mode);
        drivers.push(BoundComplementaryDriver::new(
            mode,
            Some(output),
            Some(high),
            None,
            pull_up,
            pull_down,
        ));

        metadata.extend(BoundDiscreteMetadata::new(modes, drivers));
    }

    fn two_level_chain() -> (
        MnaPattern,
        CompiledIslandIr,
        Box<CompiledDiscretePlan>,
        [UnknownIndex; 4],
    ) {
        let output_a = UnknownIndex::new(0);
        let output_b = UnknownIndex::new(1);
        let high = UnknownIndex::new(2);
        let input = UnknownIndex::new(3);

        let mut pattern_builder = PatternBuilder::new(4).unwrap();

        request_conductance(&mut pattern_builder, Some(high), Some(output_a));
        request_conductance(&mut pattern_builder, Some(output_a), None);
        request_conductance(&mut pattern_builder, Some(high), Some(output_b));
        request_conductance(&mut pattern_builder, Some(output_b), None);

        let pattern = pattern_builder.finish().unwrap();
        let mut ir = IslandIrBuilder::new(&pattern);

        let threshold = ir.constant_value(2.5).unwrap();
        let one = ir.constant_value(1.0).unwrap();

        let input_voltage = ir.unknown_value(Some(input)).unwrap();
        let output_a_voltage = ir.unknown_value(Some(output_a)).unwrap();

        let mode_a = ir.less_equal_value(threshold, input_voltage).unwrap();
        let mode_b = ir.less_equal_value(threshold, output_a_voltage).unwrap();

        let (pull_up_a, pull_down_a) = binary_pulls(&mut ir, mode_a, one);
        let (pull_up_b, pull_down_b) = binary_pulls(&mut ir, mode_b, one);

        add_conductance(&mut ir, Some(high), Some(output_a), pull_up_a);
        add_conductance(&mut ir, Some(output_a), None, pull_down_a);
        add_conductance(&mut ir, Some(high), Some(output_b), pull_up_b);
        add_conductance(&mut ir, Some(output_b), None, pull_down_b);

        let mut metadata = BoundDiscreteMetadata::default();

        add_driver(
            &mut metadata,
            mode_a,
            output_a,
            high,
            pull_up_a,
            pull_down_a,
        );
        add_driver(
            &mut metadata,
            mode_b,
            output_b,
            high,
            pull_up_b,
            pull_down_b,
        );

        let ir = ir.finish().unwrap();
        let plan = compile_discrete_plan(&pattern, &ir, metadata).unwrap();

        (pattern, ir, plan, [output_a, output_b, high, input])
    }

    #[test]
    fn closure_propagates_multiple_logic_levels_without_mna() {
        let (_pattern, ir, plan, [output_a, output_b, high, input]) = two_level_chain();

        assert!(plan.matrix_barriers().is_empty());
        assert!(plan.rhs_barriers().is_empty());

        let mut workspace = ir.value_program().new_workspace();
        let mut predicted = [0.0; 4];

        predicted[high.index()] = 5.0;
        predicted[input.index()] = 5.0;

        let factorized = vec![0.0; ir.iteration_matrix_sources().len()];
        let mut scratch = DiscreteScratch::new(&plan);

        let outcome = run_discrete_closure(
            &plan,
            &ir,
            &mut workspace,
            &mut predicted,
            &factorized,
            &[],
            &mut scratch,
        );

        assert_eq!(outcome, ClosureOutcome::Settled);
        assert_eq!(predicted[output_a.index()], 5.0);
        assert_eq!(predicted[output_b.index()], 5.0);
    }

    #[test]
    fn closure_rounds_are_synchronous() {
        let (_pattern, ir, plan, [output_a, output_b, high, input]) = two_level_chain();

        let mut workspace = ir.value_program().new_workspace();
        let mut predicted = [0.0; 4];

        predicted[high.index()] = 5.0;
        predicted[input.index()] = 5.0;

        let factorized = vec![0.0; ir.iteration_matrix_sources().len()];
        let mut scratch = DiscreteScratch::new(&plan);

        let context = DiscreteClosureContext {
            plan: &plan,
            ir: &ir,
            factorized_iteration_matrix_sources: &factorized,
            rhs_barrier_reference: &[],
        };

        let outcome = context.run_with_limit(&mut workspace, &mut predicted, &mut scratch, 1);

        assert_eq!(outcome, ClosureOutcome::BudgetExceeded);
        assert_eq!(predicted[output_a.index()], 5.0);
        assert_eq!(predicted[output_b.index()], 0.0);
    }

    #[test]
    fn matrix_barrier_stops_closure_before_local_output_update() {
        let output = UnknownIndex::new(0);
        let high = UnknownIndex::new(1);
        let input = UnknownIndex::new(2);
        let foreign_row = UnknownIndex::new(3);

        let mut pattern_builder = PatternBuilder::new(4).unwrap();

        request_conductance(&mut pattern_builder, Some(high), Some(output));
        request_conductance(&mut pattern_builder, Some(output), None);
        pattern_builder.request(foreign_row, foreign_row).unwrap();

        let pattern = pattern_builder.finish().unwrap();
        let mut ir = IslandIrBuilder::new(&pattern);

        let one = ir.constant_value(1.0).unwrap();
        let input_value = ir.unknown_value(Some(input)).unwrap();
        let mode = ir.less_equal_value(one, input_value).unwrap();
        let (pull_up, pull_down) = binary_pulls(&mut ir, mode, one);

        add_conductance(&mut ir, Some(high), Some(output), pull_up);
        add_conductance(&mut ir, Some(output), None, pull_down);

        let foreign_slot = ir.pattern().slot(foreign_row, foreign_row).unwrap();
        ir.add_matrix(foreign_slot, input_value, 1.0);

        let metadata = BoundDiscreteMetadata::new(
            smallvec![mode],
            smallvec![BoundComplementaryDriver::new(
                mode,
                Some(output),
                Some(high),
                None,
                pull_up,
                pull_down,
            )],
        );

        let ir = ir.finish().unwrap();
        let plan = compile_discrete_plan(&pattern, &ir, metadata).unwrap();

        assert_eq!(plan.matrix_barriers().len(), 1);

        let mut workspace = ir.value_program().new_workspace();
        let mut predicted = [0.0; 4];

        predicted[high.index()] = 5.0;
        predicted[input.index()] = 5.0;

        let factorized = vec![0.0; ir.iteration_matrix_sources().len()];
        let mut scratch = DiscreteScratch::new(&plan);

        let outcome = run_discrete_closure(
            &plan,
            &ir,
            &mut workspace,
            &mut predicted,
            &factorized,
            &[],
            &mut scratch,
        );

        assert_eq!(outcome, ClosureOutcome::Barrier);
        assert_eq!(predicted[output.index()], 0.0);
    }

    #[test]
    fn rhs_barrier_stops_closure_before_local_output_update() {
        let output = UnknownIndex::new(0);
        let high = UnknownIndex::new(1);
        let input = UnknownIndex::new(2);
        let rhs_row = UnknownIndex::new(3);

        let mut pattern_builder = PatternBuilder::new(4).unwrap();

        request_conductance(&mut pattern_builder, Some(high), Some(output));
        request_conductance(&mut pattern_builder, Some(output), None);

        let pattern = pattern_builder.finish().unwrap();
        let mut ir = IslandIrBuilder::new(&pattern);

        let one = ir.constant_value(1.0).unwrap();
        let input_value = ir.unknown_value(Some(input)).unwrap();
        let mode = ir.less_equal_value(one, input_value).unwrap();
        let (pull_up, pull_down) = binary_pulls(&mut ir, mode, one);

        add_conductance(&mut ir, Some(high), Some(output), pull_up);
        add_conductance(&mut ir, Some(output), None, pull_down);
        ir.add_rhs(rhs_row, input_value, 1.0);

        let metadata = BoundDiscreteMetadata::new(
            smallvec![mode],
            smallvec![BoundComplementaryDriver::new(
                mode,
                Some(output),
                Some(high),
                None,
                pull_up,
                pull_down,
            )],
        );

        let ir = ir.finish().unwrap();
        let plan = compile_discrete_plan(&pattern, &ir, metadata).unwrap();

        assert_eq!(plan.rhs_barriers(), &[input_value]);

        let mut workspace = ir.value_program().new_workspace();
        let mut predicted = [0.0; 4];

        predicted[high.index()] = 5.0;
        predicted[input.index()] = 5.0;

        let factorized = vec![0.0; ir.iteration_matrix_sources().len()];
        let mut scratch = DiscreteScratch::new(&plan);

        let outcome = run_discrete_closure(
            &plan,
            &ir,
            &mut workspace,
            &mut predicted,
            &factorized,
            &[0.0],
            &mut scratch,
        );

        assert_eq!(outcome, ClosureOutcome::Barrier);
        assert_eq!(predicted[output.index()], 0.0);
    }

    #[test]
    fn zero_conductance_denominator_falls_back_as_invalid_prediction() {
        let output = UnknownIndex::new(0);
        let high = UnknownIndex::new(1);
        let input = UnknownIndex::new(2);

        let mut pattern_builder = PatternBuilder::new(3).unwrap();

        request_conductance(&mut pattern_builder, Some(high), Some(output));
        request_conductance(&mut pattern_builder, Some(output), None);

        let pattern = pattern_builder.finish().unwrap();
        let mut ir = IslandIrBuilder::new(&pattern);

        let input_value = ir.unknown_value(Some(input)).unwrap();
        let mode = ir.less_equal_value(input_value, input_value).unwrap();
        let pull_up = ir.constant_value(1.0).unwrap();
        let pull_down = ir.constant_value(-1.0).unwrap();

        add_conductance(&mut ir, Some(high), Some(output), pull_up);
        add_conductance(&mut ir, Some(output), None, pull_down);

        let metadata = BoundDiscreteMetadata::new(
            smallvec![mode],
            smallvec![BoundComplementaryDriver::new(
                mode,
                Some(output),
                Some(high),
                None,
                pull_up,
                pull_down,
            )],
        );

        let ir = ir.finish().unwrap();
        let plan = compile_discrete_plan(&pattern, &ir, metadata).unwrap();

        let mut workspace = ir.value_program().new_workspace();
        let mut predicted = [0.0; 3];

        predicted[high.index()] = 5.0;

        let factorized = vec![0.0; ir.iteration_matrix_sources().len()];
        let mut scratch = DiscreteScratch::new(&plan);

        let outcome = run_discrete_closure(
            &plan,
            &ir,
            &mut workspace,
            &mut predicted,
            &factorized,
            &[],
            &mut scratch,
        );

        assert_eq!(outcome, ClosureOutcome::InvalidPrediction);
        assert_eq!(predicted[output.index()], 0.0);
    }

    #[test]
    fn nonsettling_combinational_cycle_exhausts_budget() {
        let output_a = UnknownIndex::new(0);
        let output_b = UnknownIndex::new(1);
        let high = UnknownIndex::new(2);

        let mut pattern_builder = PatternBuilder::new(3).unwrap();

        request_conductance(&mut pattern_builder, Some(high), Some(output_a));
        request_conductance(&mut pattern_builder, Some(output_a), None);
        request_conductance(&mut pattern_builder, Some(high), Some(output_b));
        request_conductance(&mut pattern_builder, Some(output_b), None);

        let pattern = pattern_builder.finish().unwrap();
        let mut ir = IslandIrBuilder::new(&pattern);

        let threshold = ir.constant_value(2.5).unwrap();
        let one = ir.constant_value(1.0).unwrap();

        let output_a_voltage = ir.unknown_value(Some(output_a)).unwrap();
        let output_b_voltage = ir.unknown_value(Some(output_b)).unwrap();

        let output_b_high = ir.less_equal_value(threshold, output_b_voltage).unwrap();
        let output_a_high = ir.less_equal_value(threshold, output_a_voltage).unwrap();

        let mode_a = ir.sub_value(one, output_b_high).unwrap();
        let mode_b = ir.sub_value(one, output_a_high).unwrap();

        let (pull_up_a, pull_down_a) = binary_pulls(&mut ir, mode_a, one);
        let (pull_up_b, pull_down_b) = binary_pulls(&mut ir, mode_b, one);

        add_conductance(&mut ir, Some(high), Some(output_a), pull_up_a);
        add_conductance(&mut ir, Some(output_a), None, pull_down_a);
        add_conductance(&mut ir, Some(high), Some(output_b), pull_up_b);
        add_conductance(&mut ir, Some(output_b), None, pull_down_b);

        let metadata = BoundDiscreteMetadata::new(
            smallvec![mode_a, mode_b],
            smallvec![
                BoundComplementaryDriver::new(
                    mode_a,
                    Some(output_a),
                    Some(high),
                    None,
                    pull_up_a,
                    pull_down_a,
                ),
                BoundComplementaryDriver::new(
                    mode_b,
                    Some(output_b),
                    Some(high),
                    None,
                    pull_up_b,
                    pull_down_b,
                ),
            ],
        );

        let ir = ir.finish().unwrap();
        let plan = compile_discrete_plan(&pattern, &ir, metadata).unwrap();

        let mut workspace = ir.value_program().new_workspace();
        let mut predicted = [0.0; 3];

        predicted[high.index()] = 5.0;

        let factorized = vec![0.0; ir.iteration_matrix_sources().len()];
        let mut scratch = DiscreteScratch::new(&plan);

        let outcome = run_discrete_closure(
            &plan,
            &ir,
            &mut workspace,
            &mut predicted,
            &factorized,
            &[],
            &mut scratch,
        );

        assert_eq!(outcome, ClosureOutcome::BudgetExceeded);
    }
}
