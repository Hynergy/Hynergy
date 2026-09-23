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

    #[cfg(test)]
    #[inline]
    pub(crate) fn modes(&self) -> &[ValueSlot] {
        &self.modes
    }

    #[cfg(test)]
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
    dependent_offsets: Box<[u32]>,
    dependents: Box<[u32]>,
    stability_dependent_offsets: Box<[u32]>,
    stability_dependents: Box<[u32]>,
    conservative_seed_drivers: Box<[u32]>,
    matrix_barriers: Box<[MatrixBarrierSource]>,
    rhs_barriers: Box<[ValueSlot]>,
}

impl CompiledDiscretePlan {
    #[inline]
    pub(crate) fn drivers(&self) -> &[QualifiedComplementaryDriver] {
        &self.drivers
    }

    #[inline]
    pub(crate) fn dependents_for(&self, producer: usize) -> &[u32] {
        let start = self.dependent_offsets[producer] as usize;
        let end = self.dependent_offsets[producer + 1] as usize;

        &self.dependents[start..end]
    }

    #[inline]
    pub(crate) fn stability_dependents_for(&self, stability_index: usize) -> &[u32] {
        let start = self.stability_dependent_offsets[stability_index] as usize;
        let end = self.stability_dependent_offsets[stability_index + 1] as usize;

        &self.stability_dependents[start..end]
    }

    #[inline]
    pub(crate) fn conservative_seed_drivers(&self) -> &[u32] {
        &self.conservative_seed_drivers
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

    let (dependent_offsets, dependents) = compile_driver_dependencies(ir, &qualified);
    let (stability_dependent_offsets, stability_dependents, conservative_seed_drivers) =
        compile_initial_frontier_dependencies(ir, &qualified);

    debug_assert_eq!(dependent_offsets.len(), qualified.len() + 1);
    debug_assert_eq!(
        stability_dependent_offsets.len(),
        ir.iteration_stability_values().len() + 1,
    );

    Some(Box::new(CompiledDiscretePlan {
        drivers: qualified.into_boxed_slice(),
        dependent_offsets,
        dependents,
        stability_dependent_offsets,
        stability_dependents,
        conservative_seed_drivers,
        matrix_barriers: matrix_barriers.into_boxed_slice(),
        rhs_barriers: rhs_barriers.into_boxed_slice(),
    }))
}

fn compile_driver_dependencies(
    ir: &CompiledIslandIr,
    drivers: &[QualifiedComplementaryDriver],
) -> (Box<[u32]>, Box<[u32]>) {
    let value_count = ir.value_program().value_count();

    let mut value_dependencies = vec![SmallVec::<[ValueSlot; 2]>::new(); value_count];

    ir.value_program()
        .visit_iteration_dependencies(|destination, source| {
            value_dependencies[destination.index()].push(source);
        });

    let mut producers_by_output = drivers
        .iter()
        .enumerate()
        .map(|(index, driver)| {
            (
                driver.output(),
                u32::try_from(index).expect("discrete driver index must fit u32"),
            )
        })
        .collect::<Vec<_>>();

    producers_by_output.sort_unstable_by_key(|&(output, _)| output);

    let mut producer_by_solution_value = vec![None; value_count];

    for &(unknown, input) in ir.solution_inputs() {
        if let Ok(position) =
            producers_by_output.binary_search_by_key(&unknown, |&(output, _)| output)
        {
            producer_by_solution_value[input.value().index()] =
                Some(producers_by_output[position].1);
        }
    }

    let mut adjacency = vec![SmallVec::<[u32; 4]>::new(); drivers.len()];
    let mut stack = Vec::<ValueSlot>::new();
    let mut visited = vec![0u32; value_count];
    let mut generation = 0u32;

    for (dependent, &driver) in drivers.iter().enumerate() {
        let dependent = u32::try_from(dependent).expect("discrete driver index must fit u32");

        for rail in [driver.high_rail(), driver.low_rail()]
            .into_iter()
            .flatten()
        {
            if let Ok(position) =
                producers_by_output.binary_search_by_key(&rail, |&(output, _)| output)
            {
                adjacency[producers_by_output[position].1 as usize].push(dependent);
            }
        }

        generation = generation.wrapping_add(1);

        if generation == 0 {
            visited.fill(0);
            generation = 1;
        }

        stack.clear();
        stack.push(driver.pull_up());
        stack.push(driver.pull_down());

        while let Some(value) = stack.pop() {
            if visited[value.index()] == generation {
                continue;
            }

            visited[value.index()] = generation;

            if let Some(producer) = producer_by_solution_value[value.index()] {
                adjacency[producer as usize].push(dependent);
                continue;
            }

            stack.extend_from_slice(&value_dependencies[value.index()]);
        }
    }

    let mut dependent_offsets = Vec::with_capacity(drivers.len() + 1);
    let mut dependents = Vec::<u32>::new();

    dependent_offsets.push(0);

    for edges in &mut adjacency {
        edges.sort_unstable();
        edges.dedup();
        dependents.extend_from_slice(edges);

        dependent_offsets.push(
            u32::try_from(dependents.len()).expect("discrete dependency edge count must fit u32"),
        );
    }

    (
        dependent_offsets.into_boxed_slice(),
        dependents.into_boxed_slice(),
    )
}

fn compile_initial_frontier_dependencies(
    ir: &CompiledIslandIr,
    drivers: &[QualifiedComplementaryDriver],
) -> (Box<[u32]>, Box<[u32]>, Box<[u32]>) {
    let value_count = ir.value_program().value_count();
    let stability_values = ir.iteration_stability_values();

    let mut value_dependencies = vec![SmallVec::<[ValueSlot; 2]>::new(); value_count];

    ir.value_program()
        .visit_iteration_dependencies(|destination, source| {
            value_dependencies[destination.index()].push(source);
        });

    let mut solution_values = vec![false; value_count];

    for &(_, input) in ir.solution_inputs() {
        solution_values[input.value().index()] = true;
    }

    let mut stability_adjacency = vec![Vec::<u32>::new(); stability_values.len()];
    let mut conservative_seed_drivers = Vec::<u32>::new();
    let mut stack = Vec::<ValueSlot>::new();
    let mut visited = vec![0u32; value_count];
    let mut generation = 0u32;

    for (dependent, &driver) in drivers.iter().enumerate() {
        let dependent = u32::try_from(dependent).expect("discrete driver index must fit u32");

        generation = generation.wrapping_add(1);

        if generation == 0 {
            visited.fill(0);
            generation = 1;
        }

        stack.clear();
        stack.push(driver.pull_up());
        stack.push(driver.pull_down());

        let mut conservative = false;

        while let Some(value) = stack.pop() {
            if visited[value.index()] == generation {
                continue;
            }

            visited[value.index()] = generation;

            if let Ok(stability_index) = stability_values.binary_search(&value) {
                stability_adjacency[stability_index].push(dependent);
                continue;
            }

            if solution_values[value.index()] {
                conservative = true;
                continue;
            }

            stack.extend_from_slice(&value_dependencies[value.index()]);
        }

        if conservative {
            conservative_seed_drivers.push(dependent);
        }
    }

    let mut stability_dependent_offsets = Vec::with_capacity(stability_adjacency.len() + 1);
    let mut stability_dependents = Vec::<u32>::new();

    stability_dependent_offsets.push(0);

    for edges in &mut stability_adjacency {
        edges.sort_unstable();
        edges.dedup();
        stability_dependents.extend_from_slice(edges);

        stability_dependent_offsets.push(
            u32::try_from(stability_dependents.len())
                .expect("discrete stability dependency edge count must fit u32"),
        );
    }

    conservative_seed_drivers.sort_unstable();
    conservative_seed_drivers.dedup();

    (
        stability_dependent_offsets.into_boxed_slice(),
        stability_dependents.into_boxed_slice(),
        conservative_seed_drivers.into_boxed_slice(),
    )
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
    use hynergy_ir::StateSlot;
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

    fn test_driver(
        output: UnknownIndex,
        high_rail: Option<UnknownIndex>,
        low_rail: Option<UnknownIndex>,
        pull_up: ValueSlot,
        pull_down: ValueSlot,
    ) -> QualifiedComplementaryDriver {
        QualifiedComplementaryDriver {
            output,
            high_rail,
            low_rail,
            pull_up,
            pull_down,
        }
    }

    fn graph_edges<'a>(offsets: &[u32], dependents: &'a [u32], producer: usize) -> &'a [u32] {
        let start = offsets[producer] as usize;
        let end = offsets[producer + 1] as usize;

        &dependents[start..end]
    }

    #[test]
    fn dependency_graph_tracks_multilevel_pull_dependency() {
        let output_a = UnknownIndex::new(0);
        let output_b = UnknownIndex::new(1);
        let input = UnknownIndex::new(2);

        let pattern = PatternBuilder::new(3).unwrap().finish().unwrap();
        let mut ir = IslandIrBuilder::new(&pattern);

        let one = ir.constant_value(1.0).unwrap();
        let threshold = ir.constant_value(2.5).unwrap();
        let input_value = ir.unknown_value(Some(input)).unwrap();
        let output_a_value = ir.unknown_value(Some(output_a)).unwrap();

        let mode_a = ir.less_equal_value(threshold, input_value).unwrap();
        let intermediate = ir.add_value(output_a_value, one).unwrap();
        let mode_b = ir.less_equal_value(threshold, intermediate).unwrap();

        let pull_down_a = ir.sub_value(one, mode_a).unwrap();
        let pull_down_b = ir.sub_value(one, mode_b).unwrap();

        let ir = ir.finish().unwrap();
        let drivers = [
            test_driver(output_a, None, None, mode_a, pull_down_a),
            test_driver(output_b, None, None, mode_b, pull_down_b),
        ];

        let (offsets, dependents) = compile_driver_dependencies(&ir, &drivers);

        assert_eq!(graph_edges(&offsets, &dependents, 0), &[1]);
        assert!(graph_edges(&offsets, &dependents, 1).is_empty());
    }

    #[test]
    fn dependency_graph_deduplicates_multiple_paths_to_same_driver() {
        let output_a = UnknownIndex::new(0);
        let output_b = UnknownIndex::new(1);

        let pattern = PatternBuilder::new(2).unwrap().finish().unwrap();
        let mut ir = IslandIrBuilder::new(&pattern);

        let one = ir.constant_value(1.0).unwrap();
        let threshold = ir.constant_value(2.5).unwrap();
        let output_a_value = ir.unknown_value(Some(output_a)).unwrap();

        let mode_b = ir.less_equal_value(threshold, output_a_value).unwrap();
        let pull_down_b = ir.sub_value(one, mode_b).unwrap();

        let ir = ir.finish().unwrap();
        let drivers = [
            test_driver(output_a, None, None, one, one),
            test_driver(output_b, None, None, mode_b, pull_down_b),
        ];

        let (offsets, dependents) = compile_driver_dependencies(&ir, &drivers);

        assert_eq!(graph_edges(&offsets, &dependents, 0), &[1]);
    }

    #[test]
    fn dependency_graph_ignores_static_and_tick_only_pull_branch() {
        let output_a = UnknownIndex::new(0);
        let output_b = UnknownIndex::new(1);
        let output_c = UnknownIndex::new(2);

        let pattern = PatternBuilder::new(3).unwrap().finish().unwrap();
        let mut ir = IslandIrBuilder::new(&pattern);

        let one = ir.constant_value(1.0).unwrap();
        let threshold = ir.constant_value(2.5).unwrap();
        let output_a_value = ir.unknown_value(Some(output_a)).unwrap();

        let mode_b = ir.less_equal_value(threshold, output_a_value).unwrap();
        let pull_down_b = ir.sub_value(one, mode_b).unwrap();

        let parameter = ir.parameter_input(false).unwrap().value();
        let state = ir.state_value(StateSlot::new(0)).unwrap();
        let unrelated_pull = ir.add_value(parameter, state).unwrap();
        let unrelated_pull_down = ir.add_value(unrelated_pull, one).unwrap();

        let ir = ir.finish().unwrap();
        let drivers = [
            test_driver(output_a, None, None, one, one),
            test_driver(output_b, None, None, mode_b, pull_down_b),
            test_driver(output_c, None, None, unrelated_pull, unrelated_pull_down),
        ];

        let (offsets, dependents) = compile_driver_dependencies(&ir, &drivers);

        assert_eq!(graph_edges(&offsets, &dependents, 0), &[1]);
        assert!(graph_edges(&offsets, &dependents, 1).is_empty());
        assert!(graph_edges(&offsets, &dependents, 2).is_empty());
    }

    #[test]
    fn dependency_graph_keeps_self_and_cycle_edges() {
        let output_a = UnknownIndex::new(0);
        let output_b = UnknownIndex::new(1);

        let pattern = PatternBuilder::new(2).unwrap().finish().unwrap();
        let mut ir = IslandIrBuilder::new(&pattern);

        let one = ir.constant_value(1.0).unwrap();
        let output_a_value = ir.unknown_value(Some(output_a)).unwrap();
        let output_b_value = ir.unknown_value(Some(output_b)).unwrap();

        let pull_a = ir.add_value(output_a_value, output_b_value).unwrap();
        let pull_b = output_a_value;

        let ir = ir.finish().unwrap();
        let drivers = [
            test_driver(output_a, None, None, pull_a, one),
            test_driver(output_b, None, None, pull_b, one),
        ];

        let (offsets, dependents) = compile_driver_dependencies(&ir, &drivers);

        assert_eq!(graph_edges(&offsets, &dependents, 0), &[0, 1]);
        assert_eq!(graph_edges(&offsets, &dependents, 1), &[0]);
    }

    #[test]
    fn dependency_graph_tracks_direct_rail_dependency() {
        let output_a = UnknownIndex::new(0);
        let output_b = UnknownIndex::new(1);

        let pattern = PatternBuilder::new(2).unwrap().finish().unwrap();
        let mut ir = IslandIrBuilder::new(&pattern);

        let one = ir.constant_value(1.0).unwrap();
        let ir = ir.finish().unwrap();

        let drivers = [
            test_driver(output_a, None, None, one, one),
            test_driver(output_b, Some(output_a), None, one, one),
        ];

        let (offsets, dependents) = compile_driver_dependencies(&ir, &drivers);

        assert_eq!(graph_edges(&offsets, &dependents, 0), &[1]);
        assert!(graph_edges(&offsets, &dependents, 1).is_empty());
    }

    #[test]
    fn initial_frontier_maps_stability_values_to_affected_drivers() {
        let output_a = UnknownIndex::new(0);
        let output_b = UnknownIndex::new(1);
        let input = UnknownIndex::new(2);

        let pattern = PatternBuilder::new(3).unwrap().finish().unwrap();
        let mut ir = IslandIrBuilder::new(&pattern);

        let one = ir.constant_value(1.0).unwrap();
        let threshold = ir.constant_value(2.5).unwrap();
        let input_value = ir.unknown_value(Some(input)).unwrap();
        let output_a_value = ir.unknown_value(Some(output_a)).unwrap();

        let mode_a = ir.less_equal_value(threshold, input_value).unwrap();
        let mode_b = ir.less_equal_value(threshold, output_a_value).unwrap();
        let pull_down_a = ir.sub_value(one, mode_a).unwrap();
        let pull_down_b = ir.sub_value(one, mode_b).unwrap();

        ir.require_iteration_stability(mode_a);
        ir.require_iteration_stability(mode_b);

        let ir = ir.finish().unwrap();
        let drivers = [
            test_driver(output_a, None, None, mode_a, pull_down_a),
            test_driver(output_b, None, None, mode_b, pull_down_b),
        ];

        let (offsets, dependents, conservative) =
            compile_initial_frontier_dependencies(&ir, &drivers);

        let mode_a_index = ir
            .iteration_stability_values()
            .binary_search(&mode_a)
            .unwrap();
        let mode_b_index = ir
            .iteration_stability_values()
            .binary_search(&mode_b)
            .unwrap();

        assert_eq!(graph_edges(&offsets, &dependents, mode_a_index), &[0]);
        assert_eq!(graph_edges(&offsets, &dependents, mode_b_index), &[1]);
        assert!(conservative.is_empty());
    }

    #[test]
    fn initial_frontier_conservatively_seeds_raw_solution_pull_dependencies() {
        let output_a = UnknownIndex::new(0);
        let output_b = UnknownIndex::new(1);
        let input = UnknownIndex::new(2);

        let pattern = PatternBuilder::new(3).unwrap().finish().unwrap();
        let mut ir = IslandIrBuilder::new(&pattern);

        let one = ir.constant_value(1.0).unwrap();
        let threshold = ir.constant_value(2.5).unwrap();
        let input_value = ir.unknown_value(Some(input)).unwrap();

        let mode_a = ir.less_equal_value(threshold, input_value).unwrap();
        let pull_down_a = ir.sub_value(one, mode_a).unwrap();
        let raw_pull_down = ir.add_value(input_value, one).unwrap();

        ir.require_iteration_stability(mode_a);

        let ir = ir.finish().unwrap();
        let drivers = [
            test_driver(output_a, None, None, mode_a, pull_down_a),
            test_driver(output_b, None, None, input_value, raw_pull_down),
        ];

        let (offsets, dependents, conservative) =
            compile_initial_frontier_dependencies(&ir, &drivers);

        let mode_a_index = ir
            .iteration_stability_values()
            .binary_search(&mode_a)
            .unwrap();

        assert_eq!(graph_edges(&offsets, &dependents, mode_a_index), &[0]);
        assert_eq!(conservative.as_ref(), &[1]);
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
        assert!(plan.dependents_for(0).is_empty());
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
