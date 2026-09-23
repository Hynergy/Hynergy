use crate::compile::island_ir::CompiledIslandIr;
use hynergy_ir::{MatrixProgram, RhsProgram, ValueSlot};
use hynergy_mna::pattern::{MnaPattern, UnknownIndex};
use smallvec::SmallVec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BoundComplementaryDriver {
    mode: ValueSlot,

    output: Option<UnknownIndex>,
    high_rail: Option<UnknownIndex>,
    low_rail: Option<UnknownIndex>,

    pull_up: ValueSlot,
    pull_down: ValueSlot,
}

impl BoundComplementaryDriver {
    #[inline]
    pub(crate) const fn new(
        mode: ValueSlot,
        output: Option<UnknownIndex>,
        high_rail: Option<UnknownIndex>,
        low_rail: Option<UnknownIndex>,
        pull_up: ValueSlot,
        pull_down: ValueSlot,
    ) -> Self {
        Self {
            mode,
            output,
            high_rail,
            low_rail,
            pull_up,
            pull_down,
        }
    }

    #[inline]
    #[cfg(test)]
    pub(crate) const fn mode(self) -> ValueSlot {
        self.mode
    }

    #[inline]
    pub(crate) const fn output(self) -> Option<UnknownIndex> {
        self.output
    }

    #[inline]
    pub(crate) const fn high_rail(self) -> Option<UnknownIndex> {
        self.high_rail
    }

    #[inline]
    pub(crate) const fn low_rail(self) -> Option<UnknownIndex> {
        self.low_rail
    }

    #[inline]
    pub(crate) const fn pull_up(self) -> ValueSlot {
        self.pull_up
    }

    #[inline]
    pub(crate) const fn pull_down(self) -> ValueSlot {
        self.pull_down
    }
}

#[derive(Debug, Default)]
pub(crate) struct BoundDiscreteMetadata {
    modes: SmallVec<[ValueSlot; 2]>,
    complementary_drivers: SmallVec<[BoundComplementaryDriver; 1]>,
}

impl BoundDiscreteMetadata {
    #[inline]
    pub(crate) fn new(
        modes: SmallVec<[ValueSlot; 2]>,
        complementary_drivers: SmallVec<[BoundComplementaryDriver; 1]>,
    ) -> Self {
        Self {
            modes,
            complementary_drivers,
        }
    }

    #[inline]
    #[cfg(test)]
    pub(crate) fn modes(&self) -> &[ValueSlot] {
        &self.modes
    }

    #[inline]
    pub(crate) fn complementary_drivers(&self) -> &[BoundComplementaryDriver] {
        &self.complementary_drivers
    }

    #[inline]
    pub(crate) fn extend(&mut self, other: Self) {
        let (modes, complementary_drivers) = other.into_parts();

        self.modes.extend(modes);
        self.complementary_drivers.extend(complementary_drivers);
    }

    #[inline]
    pub(crate) fn into_parts(
        self,
    ) -> (
        SmallVec<[ValueSlot; 2]>,
        SmallVec<[BoundComplementaryDriver; 1]>,
    ) {
        (self.modes, self.complementary_drivers)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct QualifiedComplementaryDriver {
    output: UnknownIndex,

    high_rail: Option<UnknownIndex>,
    low_rail: Option<UnknownIndex>,

    pull_up: ValueSlot,
    pull_down: ValueSlot,
}

impl QualifiedComplementaryDriver {
    #[inline]
    pub(crate) const fn output(self) -> UnknownIndex {
        self.output
    }

    #[inline]
    pub(crate) const fn high_rail(self) -> Option<UnknownIndex> {
        self.high_rail
    }

    #[inline]
    pub(crate) const fn low_rail(self) -> Option<UnknownIndex> {
        self.low_rail
    }

    #[inline]
    pub(crate) const fn pull_up(self) -> ValueSlot {
        self.pull_up
    }

    #[inline]
    pub(crate) const fn pull_down(self) -> ValueSlot {
        self.pull_down
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MatrixBarrierSource {
    source: ValueSlot,
    factorized_source_index: u32,
}

impl MatrixBarrierSource {
    #[inline]
    pub(crate) const fn source(self) -> ValueSlot {
        self.source
    }

    #[inline]
    pub(crate) const fn factorized_source_index(self) -> usize {
        self.factorized_source_index as usize
    }
}

#[derive(Debug)]
pub(crate) struct CompiledDiscretePlan {
    drivers: Box<[QualifiedComplementaryDriver]>,
    matrix_barriers: Box<[MatrixBarrierSource]>,
    rhs_barriers: Box<[ValueSlot]>,
}

impl CompiledDiscretePlan {
    #[inline]
    pub(crate) fn drivers(&self) -> &[QualifiedComplementaryDriver] {
        &self.drivers
    }

    #[inline]
    pub(crate) fn matrix_barriers(&self) -> &[MatrixBarrierSource] {
        &self.matrix_barriers
    }

    #[inline]
    pub(crate) fn rhs_barriers(&self) -> &[ValueSlot] {
        &self.rhs_barriers
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CanonicalMatrixTerm {
    row: UnknownIndex,
    column: UnknownIndex,
    source: ValueSlot,
    scale: f64,
}

impl CanonicalMatrixTerm {
    #[inline]
    const fn new(row: UnknownIndex, column: UnknownIndex, source: ValueSlot, scale: f64) -> Self {
        Self {
            row,
            column,
            source,
            scale,
        }
    }
}

pub(crate) fn compile_discrete_plan(
    pattern: &MnaPattern,
    ir: &CompiledIslandIr,
    metadata: BoundDiscreteMetadata,
) -> Option<Box<CompiledDiscretePlan>> {
    if !ir.iteration_latches().is_empty() {
        return None;
    }

    let (_, candidates) = metadata.into_parts();

    if candidates.is_empty() {
        return None;
    }

    let mut candidate_outputs = candidates
        .iter()
        .filter_map(|candidate| candidate.output())
        .collect::<Vec<_>>();

    candidate_outputs.sort_unstable();

    let mut duplicate_outputs = candidate_outputs
        .windows(2)
        .filter_map(|outputs| (outputs[0] == outputs[1]).then_some(outputs[0]))
        .collect::<Vec<_>>();

    duplicate_outputs.dedup();
    candidate_outputs.dedup();

    let (mut terms_by_row, mut terms_by_source) =
        collect_matrix_terms(pattern, ir.matrix_program(), &candidate_outputs);

    terms_by_row.sort_unstable_by_key(|term| (term.row, term.column, term.source));
    terms_by_source.sort_unstable_by_key(|term| (term.source, term.column, term.row));

    let mut qualified = Vec::with_capacity(candidates.len());

    for candidate in candidates {
        let Some(output) = candidate.output() else {
            continue;
        };

        if candidate.high_rail() == Some(output) || candidate.low_rail() == Some(output) {
            continue;
        }

        if duplicate_outputs.binary_search(&output).is_ok() {
            continue;
        }

        if rhs_has_destination(ir.rhs_program(), output) {
            continue;
        }

        let mut expected_row = Vec::with_capacity(4);

        push_conductance_footprint(
            &mut expected_row,
            candidate.high_rail(),
            Some(output),
            candidate.pull_up(),
        );

        push_conductance_footprint(
            &mut expected_row,
            Some(output),
            candidate.low_rail(),
            candidate.pull_down(),
        );

        canonicalize_matrix_terms(&mut expected_row);
        expected_row.retain(|term| term.row == output);
        expected_row.sort_unstable_by_key(|term| (term.row, term.column, term.source));

        let actual_row = row_terms(&terms_by_row, output);

        if actual_row != expected_row.as_slice() {
            continue;
        }

        qualified.push(QualifiedComplementaryDriver {
            output,
            high_rail: candidate.high_rail(),
            low_rail: candidate.low_rail(),
            pull_up: candidate.pull_up(),
            pull_down: candidate.pull_down(),
        });
    }

    if qualified.is_empty() {
        return None;
    }

    let mut expected_footprints = Vec::with_capacity(qualified.len() * 8);

    for driver in &qualified {
        push_conductance_footprint(
            &mut expected_footprints,
            driver.high_rail(),
            Some(driver.output()),
            driver.pull_up(),
        );

        push_conductance_footprint(
            &mut expected_footprints,
            Some(driver.output()),
            driver.low_rail(),
            driver.pull_down(),
        );
    }

    canonicalize_matrix_terms(&mut expected_footprints);
    expected_footprints.sort_unstable_by_key(|term| (term.source, term.column, term.row));

    let mut matrix_barriers = Vec::new();

    for (index, &source) in ir.iteration_matrix_sources().iter().enumerate() {
        let actual = source_terms(&terms_by_source, source);
        let expected = source_terms(&expected_footprints, source);

        if actual == expected {
            continue;
        }

        matrix_barriers.push(MatrixBarrierSource {
            source,
            factorized_source_index: u32::try_from(index)
                .expect("iteration matrix source index must fit u32"),
        });
    }

    let mut rhs_barriers = ir
        .rhs_program()
        .ops()
        .iter()
        .map(|op| op.source())
        .collect::<Vec<_>>();

    rhs_barriers.sort_unstable();
    rhs_barriers.dedup();

    Some(Box::new(CompiledDiscretePlan {
        drivers: qualified.into_boxed_slice(),
        matrix_barriers: matrix_barriers.into_boxed_slice(),
        rhs_barriers: rhs_barriers.into_boxed_slice(),
    }))
}

fn collect_matrix_terms(
    pattern: &MnaPattern,
    matrix: &MatrixProgram,
    candidate_outputs: &[UnknownIndex],
) -> (Vec<CanonicalMatrixTerm>, Vec<CanonicalMatrixTerm>) {
    let mut terms_by_row = Vec::new();
    let mut terms_by_source = Vec::with_capacity(matrix.len());

    for &op in matrix.ops() {
        let (row, column) = pattern
            .coordinate(op.destination())
            .expect("compiled matrix operation must reference the final MNA pattern");

        let term = CanonicalMatrixTerm::new(row, column, op.source(), op.scale());

        if candidate_outputs.binary_search(&row).is_ok() {
            terms_by_row.push(term);
        }

        terms_by_source.push(term);
    }

    (terms_by_row, terms_by_source)
}

fn push_conductance_footprint(
    terms: &mut Vec<CanonicalMatrixTerm>,
    a: Option<UnknownIndex>,
    b: Option<UnknownIndex>,
    source: ValueSlot,
) {
    push_matrix_term(terms, a, a, source, 1.0);
    push_matrix_term(terms, b, a, source, -1.0);
    push_matrix_term(terms, a, b, source, -1.0);
    push_matrix_term(terms, b, b, source, 1.0);
}

#[inline]
fn push_matrix_term(
    terms: &mut Vec<CanonicalMatrixTerm>,
    row: Option<UnknownIndex>,
    column: Option<UnknownIndex>,
    source: ValueSlot,
    scale: f64,
) {
    let (Some(row), Some(column)) = (row, column) else {
        return;
    };

    terms.push(CanonicalMatrixTerm::new(row, column, source, scale));
}

fn canonicalize_matrix_terms(terms: &mut Vec<CanonicalMatrixTerm>) {
    terms.retain(|term| term.scale != 0.0);
    terms.sort_unstable_by_key(|term| (term.source, term.column, term.row));

    if terms.len() < 2 {
        return;
    }

    let mut write = 0usize;

    for read in 1..terms.len() {
        if terms[write].row == terms[read].row
            && terms[write].column == terms[read].column
            && terms[write].source == terms[read].source
        {
            terms[write].scale += terms[read].scale;
        } else {
            write += 1;

            if write != read {
                terms[write] = terms[read];
            }
        }
    }

    terms.truncate(write + 1);
    terms.retain(|term| term.scale != 0.0);
}

fn row_terms(terms: &[CanonicalMatrixTerm], row: UnknownIndex) -> &[CanonicalMatrixTerm] {
    let start = terms.partition_point(|term| term.row < row);
    let end = start + terms[start..].partition_point(|term| term.row == row);

    &terms[start..end]
}

fn source_terms(terms: &[CanonicalMatrixTerm], source: ValueSlot) -> &[CanonicalMatrixTerm] {
    let start = terms.partition_point(|term| term.source < source);
    let end = start + terms[start..].partition_point(|term| term.source == source);

    &terms[start..end]
}

fn rhs_has_destination(rhs: &RhsProgram, destination: UnknownIndex) -> bool {
    let ops = rhs.ops();
    let index = ops.partition_point(|op| op.destination() < destination);

    ops.get(index)
        .is_some_and(|op| op.destination() == destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::island_ir::IslandIrBuilder;
    use hynergy_mna::pattern::PatternBuilder;
    use smallvec::smallvec;

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

    #[test]
    fn qualifies_exact_complementary_output_and_covers_its_matrix_sources() {
        let output = UnknownIndex::new(0);
        let high = UnknownIndex::new(1);
        let input = UnknownIndex::new(2);

        let mut pattern_builder = PatternBuilder::new(3).unwrap();

        request_conductance(&mut pattern_builder, Some(high), Some(output));
        request_conductance(&mut pattern_builder, Some(output), None);

        let pattern = pattern_builder.finish().unwrap();
        let mut ir = IslandIrBuilder::new(&pattern);

        let mode = ir.unknown_value(Some(input)).unwrap();
        let one = ir.constant_value(1.0).unwrap();
        let pull_up = ir.add_value(mode, one).unwrap();
        let pull_down = ir.sub_value(one, mode).unwrap();

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

        assert_eq!(plan.drivers().len(), 1);
        assert_eq!(plan.drivers()[0].output(), output);
        assert_eq!(plan.drivers()[0].high_rail(), Some(high));
        assert_eq!(plan.drivers()[0].low_rail(), None);
        assert_eq!(plan.drivers()[0].pull_up(), pull_up);
        assert_eq!(plan.drivers()[0].pull_down(), pull_down);
        assert!(plan.matrix_barriers().is_empty());
        assert!(plan.rhs_barriers().is_empty());
    }

    #[test]
    fn output_column_voltage_sensing_does_not_disqualify_driver() {
        let output = UnknownIndex::new(0);
        let high = UnknownIndex::new(1);
        let input = UnknownIndex::new(2);
        let sensed_row = UnknownIndex::new(3);

        let mut pattern_builder = PatternBuilder::new(4).unwrap();

        request_conductance(&mut pattern_builder, Some(high), Some(output));
        request_conductance(&mut pattern_builder, Some(output), None);
        pattern_builder.request(sensed_row, output).unwrap();

        let pattern = pattern_builder.finish().unwrap();
        let mut ir = IslandIrBuilder::new(&pattern);

        let mode = ir.unknown_value(Some(input)).unwrap();
        let one = ir.constant_value(1.0).unwrap();
        let pull_up = ir.add_value(mode, one).unwrap();
        let pull_down = ir.sub_value(one, mode).unwrap();
        let sense_gain = ir.constant_value(2.0).unwrap();

        add_conductance(&mut ir, Some(high), Some(output), pull_up);
        add_conductance(&mut ir, Some(output), None, pull_down);

        let sense_slot = ir.pattern().slot(sensed_row, output).unwrap();
        ir.add_matrix(sense_slot, sense_gain, 1.0);

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

        assert!(compile_discrete_plan(&pattern, &ir, metadata).is_some());
    }

    #[test]
    fn electrical_output_load_disqualifies_driver() {
        let output = UnknownIndex::new(0);
        let high = UnknownIndex::new(1);
        let input = UnknownIndex::new(2);

        let mut pattern_builder = PatternBuilder::new(3).unwrap();

        request_conductance(&mut pattern_builder, Some(high), Some(output));
        request_conductance(&mut pattern_builder, Some(output), None);

        let pattern = pattern_builder.finish().unwrap();
        let mut ir = IslandIrBuilder::new(&pattern);

        let mode = ir.unknown_value(Some(input)).unwrap();
        let one = ir.constant_value(1.0).unwrap();
        let pull_up = ir.add_value(mode, one).unwrap();
        let pull_down = ir.sub_value(one, mode).unwrap();
        let load = ir.constant_value(0.5).unwrap();

        add_conductance(&mut ir, Some(high), Some(output), pull_up);
        add_conductance(&mut ir, Some(output), None, pull_down);
        add_conductance(&mut ir, Some(output), None, load);

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

        assert!(compile_discrete_plan(&pattern, &ir, metadata).is_none());
    }

    #[test]
    fn uncovered_iteration_matrix_source_becomes_barrier() {
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

        let mode = ir.unknown_value(Some(input)).unwrap();
        let one = ir.constant_value(1.0).unwrap();
        let pull_up = ir.add_value(mode, one).unwrap();
        let pull_down = ir.sub_value(one, mode).unwrap();
        let foreign = ir.unknown_value(Some(foreign_row)).unwrap();

        add_conductance(&mut ir, Some(high), Some(output), pull_up);
        add_conductance(&mut ir, Some(output), None, pull_down);

        let foreign_slot = ir.pattern().slot(foreign_row, foreign_row).unwrap();
        ir.add_matrix(foreign_slot, foreign, 1.0);

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
        let expected_index = ir
            .iteration_matrix_sources()
            .binary_search(&foreign)
            .unwrap();

        let plan = compile_discrete_plan(&pattern, &ir, metadata).unwrap();

        assert_eq!(plan.matrix_barriers().len(), 1);
        assert_eq!(plan.matrix_barriers()[0].source(), foreign);
        assert_eq!(
            plan.matrix_barriers()[0].factorized_source_index(),
            expected_index,
        );
    }

    #[test]
    fn rhs_sources_are_barriers() {
        let output = UnknownIndex::new(0);
        let high = UnknownIndex::new(1);
        let input = UnknownIndex::new(2);
        let rhs_row = UnknownIndex::new(3);

        let mut pattern_builder = PatternBuilder::new(4).unwrap();

        request_conductance(&mut pattern_builder, Some(high), Some(output));
        request_conductance(&mut pattern_builder, Some(output), None);

        let pattern = pattern_builder.finish().unwrap();
        let mut ir = IslandIrBuilder::new(&pattern);

        let mode = ir.unknown_value(Some(input)).unwrap();
        let one = ir.constant_value(1.0).unwrap();
        let pull_up = ir.add_value(mode, one).unwrap();
        let pull_down = ir.sub_value(one, mode).unwrap();

        add_conductance(&mut ir, Some(high), Some(output), pull_up);
        add_conductance(&mut ir, Some(output), None, pull_down);
        ir.add_rhs(rhs_row, mode, 1.0);

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

        assert_eq!(plan.rhs_barriers(), &[mode]);
    }

    #[test]
    fn duplicate_complementary_drivers_on_output_are_not_qualified() {
        let output = UnknownIndex::new(0);
        let high = UnknownIndex::new(1);
        let input = UnknownIndex::new(2);

        let mut pattern_builder = PatternBuilder::new(3).unwrap();

        request_conductance(&mut pattern_builder, Some(high), Some(output));
        request_conductance(&mut pattern_builder, Some(output), None);

        let pattern = pattern_builder.finish().unwrap();
        let mut ir = IslandIrBuilder::new(&pattern);

        let mode = ir.unknown_value(Some(input)).unwrap();
        let one = ir.constant_value(1.0).unwrap();
        let pull_up = ir.add_value(mode, one).unwrap();
        let pull_down = ir.sub_value(one, mode).unwrap();

        add_conductance(&mut ir, Some(high), Some(output), pull_up);
        add_conductance(&mut ir, Some(output), None, pull_down);

        let driver =
            BoundComplementaryDriver::new(mode, Some(output), Some(high), None, pull_up, pull_down);

        let metadata = BoundDiscreteMetadata::new(smallvec![mode], smallvec![driver, driver]);

        let ir = ir.finish().unwrap();

        assert!(compile_discrete_plan(&pattern, &ir, metadata).is_none());
    }

    #[test]
    fn output_rhs_contribution_disqualifies_driver() {
        let output = UnknownIndex::new(0);
        let high = UnknownIndex::new(1);
        let input = UnknownIndex::new(2);

        let mut pattern_builder = PatternBuilder::new(3).unwrap();

        request_conductance(&mut pattern_builder, Some(high), Some(output));
        request_conductance(&mut pattern_builder, Some(output), None);

        let pattern = pattern_builder.finish().unwrap();
        let mut ir = IslandIrBuilder::new(&pattern);

        let mode = ir.unknown_value(Some(input)).unwrap();
        let one = ir.constant_value(1.0).unwrap();
        let pull_up = ir.add_value(mode, one).unwrap();
        let pull_down = ir.sub_value(one, mode).unwrap();

        add_conductance(&mut ir, Some(high), Some(output), pull_up);
        add_conductance(&mut ir, Some(output), None, pull_down);
        ir.add_rhs(output, mode, 1.0);

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

        assert!(compile_discrete_plan(&pattern, &ir, metadata).is_none());
    }

    #[test]
    fn exactly_cancelled_foreign_output_terms_do_not_disqualify_driver() {
        let output = UnknownIndex::new(0);
        let high = UnknownIndex::new(1);
        let input = UnknownIndex::new(2);

        let mut pattern_builder = PatternBuilder::new(3).unwrap();

        request_conductance(&mut pattern_builder, Some(high), Some(output));
        request_conductance(&mut pattern_builder, Some(output), None);

        let pattern = pattern_builder.finish().unwrap();
        let mut ir = IslandIrBuilder::new(&pattern);

        let mode = ir.unknown_value(Some(input)).unwrap();
        let one = ir.constant_value(1.0).unwrap();
        let pull_up = ir.add_value(mode, one).unwrap();
        let pull_down = ir.sub_value(one, mode).unwrap();
        let foreign = ir.constant_value(0.25).unwrap();

        add_conductance(&mut ir, Some(high), Some(output), pull_up);
        add_conductance(&mut ir, Some(output), None, pull_down);

        let output_diagonal = ir.pattern().slot(output, output).unwrap();
        ir.add_matrix(output_diagonal, foreign, 1.0);
        ir.add_matrix(output_diagonal, foreign, -1.0);

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

        assert!(compile_discrete_plan(&pattern, &ir, metadata).is_some());
    }

    #[test]
    fn iteration_latch_disables_plan() {
        let output = UnknownIndex::new(0);
        let high = UnknownIndex::new(1);
        let input = UnknownIndex::new(2);

        let mut pattern_builder = PatternBuilder::new(3).unwrap();

        request_conductance(&mut pattern_builder, Some(high), Some(output));
        request_conductance(&mut pattern_builder, Some(output), None);

        let pattern = pattern_builder.finish().unwrap();
        let mut ir = IslandIrBuilder::new(&pattern);

        let mode = ir.unknown_value(Some(input)).unwrap();
        let initial = ir.constant_value(0.0).unwrap();
        let latch = ir.iteration_latch(initial).unwrap();

        ir.update_iteration_latch(latch, mode);

        let one = ir.constant_value(1.0).unwrap();
        let pull_up = ir.add_value(mode, one).unwrap();
        let pull_down = ir.sub_value(one, mode).unwrap();

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

        assert!(compile_discrete_plan(&pattern, &ir, metadata).is_none());
    }
}
