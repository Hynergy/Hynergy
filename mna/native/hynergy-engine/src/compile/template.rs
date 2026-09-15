use crate::compile::UnknownRange;
use crate::compile::island_ir::IslandIrBuilder;
use crate::compile::state::StateRange;
use hynergy_ir::{InputSlot, ValueBuildError, ValueSlot};
use hynergy_mna::pattern::{PatternBuilder, PatternError, UnknownIndex};
use smallvec::SmallVec;
use thiserror::Error;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct LocalStateId(u32);

impl LocalStateId {
    #[inline]
    const fn new(index: u32) -> Self {
        Self(index)
    }

    #[inline]
    const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LocalState {
    id: LocalStateId,
    value: LocalValueId,
}

impl LocalState {
    #[inline]
    pub(crate) const fn value(self) -> LocalValueId {
        self.value
    }
}

#[derive(Debug)]
pub(crate) struct BoundUnknowns {
    values: SmallVec<[Option<UnknownIndex>; 4]>,
}

impl BoundUnknowns {
    #[inline]
    pub(crate) fn get(&self, local: LocalUnknownId) -> Option<UnknownIndex> {
        self.values[local.index()]
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct LocalUnknownId(u32);

impl LocalUnknownId {
    #[inline]
    const fn new(index: u32) -> Self {
        Self(index)
    }

    #[inline]
    const fn index(self) -> usize {
        self.0 as usize
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct LocalValueId(u32);

impl LocalValueId {
    #[inline]
    const fn new(index: u32) -> Self {
        Self(index)
    }

    #[inline]
    const fn index(self) -> usize {
        self.0 as usize
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct LocalParameterId(u32);

impl LocalParameterId {
    #[inline]
    const fn new(index: u32) -> Self {
        Self(index)
    }

    #[inline]
    const fn index(self) -> usize {
        self.0 as usize
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct LocalMatrixSlot(u32);

impl LocalMatrixSlot {
    #[inline]
    const fn new(index: u32) -> Self {
        Self(index)
    }

    #[inline]
    const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalUnknownKind {
    Voltage,
    BranchCurrent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalUnknownBinding {
    Terminal(u32),
    Allocated(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LocalUnknownInfo {
    kind: LocalUnknownKind,
    binding: LocalUnknownBinding,
}

#[derive(Debug, Clone, Copy)]
enum LocalValueNode {
    Parameter(LocalParameterId),
    Constant(f64),

    Timestep,
    State(LocalStateId),
    Unknown(LocalUnknownId),

    Add(LocalValueId, LocalValueId),
    Sub(LocalValueId, LocalValueId),
    Mul(LocalValueId, LocalValueId),
    Div(LocalValueId, LocalValueId),

    Neg(LocalValueId),
}

#[derive(Debug, Clone, Copy)]
struct LocalValueInfo {
    node: LocalValueNode,
    constant: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LocalMatrixEntry {
    row: LocalUnknownId,
    column: LocalUnknownId,
}

impl LocalMatrixEntry {
    #[inline]
    const fn new(row: LocalUnknownId, column: LocalUnknownId) -> Self {
        Self { row, column }
    }
}

#[derive(Debug, Clone, Copy)]
struct PendingMatrixTerm {
    row: LocalUnknownId,
    column: LocalUnknownId,

    source: LocalValueId,
    scale: f64,
}

#[derive(Debug, Clone, Copy)]
struct LocalMatrixTerm {
    destination: LocalMatrixSlot,

    source: LocalValueId,
    scale: f64,
}

#[derive(Debug, Clone, Copy)]
struct LocalRhsTerm {
    destination: LocalUnknownId,

    source: LocalValueId,
    scale: f64,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DefinitionTemplateBuildError {
    #[error("definition-template ID range is exhausted")]
    IdExhausted,

    #[error("persistent state {state} has more than one next-state producer")]
    DuplicateStateProducer { state: usize },

    #[error("persistent state {state} has no next-state producer")]
    MissingStateProducer { state: usize },
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DefinitionLinkError {
    #[error("expected {expected} terminal bindings, got {actual}")]
    WrongTerminalBindingCount { expected: usize, actual: usize },

    #[error("expected {expected} allocated unknowns, got {actual}")]
    WrongAllocatedUnknownCount { expected: usize, actual: usize },

    #[error("expected {expected} state bindings, got {actual}")]
    WrongStateBindingCount { expected: usize, actual: usize },

    #[error(transparent)]
    Pattern(#[from] PatternError),

    #[error(transparent)]
    Values(#[from] ValueBuildError),

    #[error("final MNA pattern is missing entry ({row:?}, {column:?})")]
    MissingPatternEntry {
        row: UnknownIndex,
        column: UnknownIndex,
    },
}

#[derive(Debug, Default)]
pub(crate) struct DefinitionTemplateBuilder {
    unknowns: Vec<LocalUnknownInfo>,
    unknown_values: Vec<Option<LocalValueId>>,

    values: Vec<LocalValueInfo>,
    parameter_count: usize,

    timestep_value: Option<LocalValueId>,

    state_writes: Vec<Option<LocalValueId>>,

    matrix_terms: Vec<PendingMatrixTerm>,
    rhs_terms: Vec<LocalRhsTerm>,

    terminal_count: usize,
    allocated_unknown_count: usize,
}

impl DefinitionTemplateBuilder {
    #[inline]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn timestep(&mut self) -> Result<LocalValueId, DefinitionTemplateBuildError> {
        if let Some(value) = self.timestep_value {
            return Ok(value);
        }

        let value = self.allocate_value(LocalValueInfo {
            node: LocalValueNode::Timestep,
            constant: None,
        })?;

        self.timestep_value = Some(value);

        Ok(value)
    }

    pub(crate) fn unknown_value(
        &mut self,
        unknown: LocalUnknownId,
    ) -> Result<LocalValueId, DefinitionTemplateBuildError> {
        debug_assert!(unknown.index() < self.unknowns.len());

        if let Some(value) = self.unknown_values[unknown.index()] {
            return Ok(value);
        }

        let value = self.allocate_value(LocalValueInfo {
            node: LocalValueNode::Unknown(unknown),
            constant: None,
        })?;

        self.unknown_values[unknown.index()] = Some(value);

        Ok(value)
    }

    pub(crate) fn state(&mut self) -> Result<LocalState, DefinitionTemplateBuildError> {
        let raw = u32::try_from(self.state_writes.len())
            .map_err(|_| DefinitionTemplateBuildError::IdExhausted)?;

        let id = LocalStateId::new(raw);

        let value = self.allocate_value(LocalValueInfo {
            node: LocalValueNode::State(id),
            constant: None,
        })?;

        self.state_writes.push(None);

        Ok(LocalState { id, value })
    }

    pub(crate) fn write_state(
        &mut self,
        state: LocalState,
        source: LocalValueId,
    ) -> Result<(), DefinitionTemplateBuildError> {
        debug_assert!(source.index() < self.values.len());

        let destination = &mut self.state_writes[state.id.index()];

        if destination.is_some() {
            return Err(DefinitionTemplateBuildError::DuplicateStateProducer {
                state: state.id.index(),
            });
        }

        *destination = Some(source);

        Ok(())
    }

    pub(crate) fn terminal_voltage(
        &mut self,
    ) -> Result<LocalUnknownId, DefinitionTemplateBuildError> {
        let terminal = u32::try_from(self.terminal_count)
            .map_err(|_| DefinitionTemplateBuildError::IdExhausted)?;

        self.terminal_count += 1;

        self.allocate_unknown(LocalUnknownInfo {
            kind: LocalUnknownKind::Voltage,
            binding: LocalUnknownBinding::Terminal(terminal),
        })
    }

    pub(crate) fn branch_current_unknown(
        &mut self,
    ) -> Result<LocalUnknownId, DefinitionTemplateBuildError> {
        let allocated = u32::try_from(self.allocated_unknown_count)
            .map_err(|_| DefinitionTemplateBuildError::IdExhausted)?;

        self.allocated_unknown_count += 1;

        self.allocate_unknown(LocalUnknownInfo {
            kind: LocalUnknownKind::BranchCurrent,
            binding: LocalUnknownBinding::Allocated(allocated),
        })
    }

    pub(crate) fn parameter(&mut self) -> Result<LocalValueId, DefinitionTemplateBuildError> {
        let parameter = u32::try_from(self.parameter_count)
            .map_err(|_| DefinitionTemplateBuildError::IdExhausted)?;

        self.parameter_count += 1;

        self.allocate_value(LocalValueInfo {
            node: LocalValueNode::Parameter(LocalParameterId::new(parameter)),
            constant: None,
        })
    }

    pub(crate) fn constant(
        &mut self,
        value: f64,
    ) -> Result<LocalValueId, DefinitionTemplateBuildError> {
        self.allocate_value(LocalValueInfo {
            node: LocalValueNode::Constant(value),
            constant: Some(value),
        })
    }

    pub(crate) fn add(
        &mut self,
        lhs: LocalValueId,
        rhs: LocalValueId,
    ) -> Result<LocalValueId, DefinitionTemplateBuildError> {
        self.binary(lhs, rhs, LocalBinaryOp::Add)
    }

    pub(crate) fn sub(
        &mut self,
        lhs: LocalValueId,
        rhs: LocalValueId,
    ) -> Result<LocalValueId, DefinitionTemplateBuildError> {
        self.binary(lhs, rhs, LocalBinaryOp::Sub)
    }

    pub(crate) fn mul(
        &mut self,
        lhs: LocalValueId,
        rhs: LocalValueId,
    ) -> Result<LocalValueId, DefinitionTemplateBuildError> {
        self.binary(lhs, rhs, LocalBinaryOp::Mul)
    }

    pub(crate) fn div(
        &mut self,
        lhs: LocalValueId,
        rhs: LocalValueId,
    ) -> Result<LocalValueId, DefinitionTemplateBuildError> {
        self.binary(lhs, rhs, LocalBinaryOp::Div)
    }

    pub(crate) fn neg(
        &mut self,
        operand: LocalValueId,
    ) -> Result<LocalValueId, DefinitionTemplateBuildError> {
        let info = self.values[operand.index()];

        if let Some(value) = info.constant {
            return self.constant(-value);
        }

        self.allocate_value(LocalValueInfo {
            node: LocalValueNode::Neg(operand),
            constant: None,
        })
    }

    #[inline]
    pub(crate) fn add_matrix(
        &mut self,
        row: LocalUnknownId,
        column: LocalUnknownId,
        source: LocalValueId,
        scale: f64,
    ) {
        debug_assert!(row.index() < self.unknowns.len());
        debug_assert!(column.index() < self.unknowns.len());
        debug_assert!(source.index() < self.values.len());

        if scale == 0.0 {
            return;
        }

        self.matrix_terms.push(PendingMatrixTerm {
            row,
            column,
            source,
            scale,
        });
    }

    #[inline]
    pub(crate) fn add_rhs(
        &mut self,
        destination: LocalUnknownId,
        source: LocalValueId,
        scale: f64,
    ) {
        debug_assert!(destination.index() < self.unknowns.len());
        debug_assert!(source.index() < self.values.len());

        if scale == 0.0 {
            return;
        }

        self.rhs_terms.push(LocalRhsTerm {
            destination,
            source,
            scale,
        });
    }

    pub(crate) fn finish(
        mut self,
    ) -> Result<CompiledDefinitionTemplate, DefinitionTemplateBuildError> {
        canonicalize_pending_matrix_terms(&mut self.matrix_terms);
        canonicalize_rhs_terms(&mut self.rhs_terms);

        let state_writes = std::mem::take(&mut self.state_writes)
            .into_iter()
            .enumerate()
            .map(|(state, source)| {
                source.ok_or(DefinitionTemplateBuildError::MissingStateProducer { state })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut coordinates = self
            .matrix_terms
            .iter()
            .map(|term| (term.column, term.row))
            .collect::<Vec<_>>();

        coordinates.sort_unstable();
        coordinates.dedup();

        let mut matrix_entries = Vec::with_capacity(coordinates.len());

        for &(column, row) in &coordinates {
            matrix_entries.push(LocalMatrixEntry::new(row, column));
        }

        let mut matrix_terms = Vec::with_capacity(self.matrix_terms.len());

        for term in self.matrix_terms {
            let key = (term.column, term.row);

            let index = coordinates
                .binary_search(&key)
                .expect("matrix coordinate was collected from term");

            let index =
                u32::try_from(index).map_err(|_| DefinitionTemplateBuildError::IdExhausted)?;

            matrix_terms.push(LocalMatrixTerm {
                destination: LocalMatrixSlot::new(index),
                source: term.source,
                scale: term.scale,
            });
        }

        Ok(CompiledDefinitionTemplate {
            unknowns: self.unknowns.into_boxed_slice(),
            values: self.values.into_boxed_slice(),

            matrix_entries: matrix_entries.into_boxed_slice(),
            matrix_terms: matrix_terms.into_boxed_slice(),
            rhs_terms: self.rhs_terms.into_boxed_slice(),

            state_writes: state_writes.into_boxed_slice(),

            parameter_count: self.parameter_count,
            terminal_count: self.terminal_count,
            allocated_unknown_count: self.allocated_unknown_count,
        })
    }

    fn allocate_unknown(
        &mut self,
        info: LocalUnknownInfo,
    ) -> Result<LocalUnknownId, DefinitionTemplateBuildError> {
        let index = u32::try_from(self.unknowns.len())
            .map_err(|_| DefinitionTemplateBuildError::IdExhausted)?;

        self.unknowns.push(info);
        self.unknown_values.push(None);

        Ok(LocalUnknownId::new(index))
    }

    fn allocate_value(
        &mut self,
        info: LocalValueInfo,
    ) -> Result<LocalValueId, DefinitionTemplateBuildError> {
        let index = u32::try_from(self.values.len())
            .map_err(|_| DefinitionTemplateBuildError::IdExhausted)?;

        self.values.push(info);

        Ok(LocalValueId::new(index))
    }

    fn binary(
        &mut self,
        lhs: LocalValueId,
        rhs: LocalValueId,
        operation: LocalBinaryOp,
    ) -> Result<LocalValueId, DefinitionTemplateBuildError> {
        let lhs_info = self.values[lhs.index()];
        let rhs_info = self.values[rhs.index()];

        if let (Some(lhs_value), Some(rhs_value)) = (lhs_info.constant, rhs_info.constant) {
            let value = match operation {
                LocalBinaryOp::Add => lhs_value + rhs_value,
                LocalBinaryOp::Sub => lhs_value - rhs_value,
                LocalBinaryOp::Mul => lhs_value * rhs_value,
                LocalBinaryOp::Div => lhs_value / rhs_value,
            };

            return self.constant(value);
        }

        let node = match operation {
            LocalBinaryOp::Add => LocalValueNode::Add(lhs, rhs),
            LocalBinaryOp::Sub => LocalValueNode::Sub(lhs, rhs),
            LocalBinaryOp::Mul => LocalValueNode::Mul(lhs, rhs),
            LocalBinaryOp::Div => LocalValueNode::Div(lhs, rhs),
        };

        self.allocate_value(LocalValueInfo {
            node,
            constant: None,
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum LocalBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug)]
pub(crate) struct CompiledDefinitionTemplate {
    unknowns: Box<[LocalUnknownInfo]>,
    values: Box<[LocalValueInfo]>,

    matrix_entries: Box<[LocalMatrixEntry]>,
    matrix_terms: Box<[LocalMatrixTerm]>,
    rhs_terms: Box<[LocalRhsTerm]>,

    state_writes: Box<[LocalValueId]>,

    parameter_count: usize,
    terminal_count: usize,
    allocated_unknown_count: usize,
}

impl CompiledDefinitionTemplate {
    pub(crate) fn request_pattern(
        &self,
        unknowns: &BoundUnknowns,
        pattern: &mut PatternBuilder,
    ) -> Result<(), DefinitionLinkError> {
        for entry in &self.matrix_entries {
            let Some(row) = unknowns.get(entry.row) else {
                continue;
            };

            let Some(column) = unknowns.get(entry.column) else {
                continue;
            };

            pattern.request(row, column)?;
        }

        Ok(())
    }

    pub(crate) fn bind(
        &self,
        unknowns: &BoundUnknowns,
        states: StateRange,
        ir: &mut IslandIrBuilder<'_>,
    ) -> Result<BoundDefinitionInputs, DefinitionLinkError> {
        if states.len() != self.state_count() {
            return Err(DefinitionLinkError::WrongStateBindingCount {
                expected: self.state_count(),
                actual: states.len(),
            });
        }

        let (values, parameters) = self.bind_values(unknowns, states, ir)?;

        for term in &self.matrix_terms {
            let entry = self.matrix_entries[term.destination.index()];

            let Some(row) = unknowns.get(entry.row) else {
                continue;
            };

            let Some(column) = unknowns.get(entry.column) else {
                continue;
            };

            let destination = ir
                .pattern()
                .slot(row, column)
                .ok_or(DefinitionLinkError::MissingPatternEntry { row, column })?;

            ir.add_matrix(destination, values[term.source.index()], term.scale);
        }

        for term in &self.rhs_terms {
            let Some(destination) = unknowns.get(term.destination) else {
                continue;
            };

            ir.add_rhs(destination, values[term.source.index()], term.scale);
        }

        for (index, &source) in self.state_writes.iter().enumerate() {
            let destination = states.get(index).expect("state range length was validated");

            ir.write_state(destination, values[source.index()]);
        }

        Ok(BoundDefinitionInputs { parameters })
    }

    fn bind_values(
        &self,
        unknowns: &BoundUnknowns,
        states: StateRange,
        ir: &mut IslandIrBuilder<'_>,
    ) -> Result<(Vec<ValueSlot>, Box<[InputSlot]>), DefinitionLinkError> {
        let mut values = Vec::with_capacity(self.values.len());

        let mut parameters = vec![None; self.parameter_count];

        for info in &self.values {
            let value = match info.node {
                LocalValueNode::Parameter(parameter) => {
                    let input = ir.parameter_input()?;

                    parameters[parameter.index()] = Some(input);

                    input.value()
                }

                LocalValueNode::Constant(value) => ir.constant_value(value)?,

                LocalValueNode::Timestep => ir.timestep_value()?,

                LocalValueNode::State(state) => {
                    let state = states
                        .get(state.index())
                        .expect("state range length was validated");

                    ir.state_value(state)?
                }

                LocalValueNode::Unknown(unknown) => ir.unknown_value(unknowns.get(unknown))?,

                LocalValueNode::Add(lhs, rhs) => {
                    ir.add_value(values[lhs.index()], values[rhs.index()])?
                }

                LocalValueNode::Sub(lhs, rhs) => {
                    ir.sub_value(values[lhs.index()], values[rhs.index()])?
                }

                LocalValueNode::Mul(lhs, rhs) => {
                    ir.mul_value(values[lhs.index()], values[rhs.index()])?
                }

                LocalValueNode::Div(lhs, rhs) => {
                    ir.div_value(values[lhs.index()], values[rhs.index()])?
                }

                LocalValueNode::Neg(operand) => ir.neg_value(values[operand.index()])?,
            };

            values.push(value);
        }

        let parameters = parameters
            .into_iter()
            .map(|parameter| parameter.expect("each template parameter has exactly one input node"))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Ok((values, parameters))
    }

    pub(crate) fn bind_unknowns(
        &self,
        terminals: &[Option<UnknownIndex>],
        allocated: UnknownRange,
    ) -> Result<BoundUnknowns, DefinitionLinkError> {
        if terminals.len() != self.terminal_count {
            return Err(DefinitionLinkError::WrongTerminalBindingCount {
                expected: self.terminal_count,
                actual: terminals.len(),
            });
        }

        if allocated.len() != self.allocated_unknown_count {
            return Err(DefinitionLinkError::WrongAllocatedUnknownCount {
                expected: self.allocated_unknown_count,
                actual: allocated.len(),
            });
        }

        let mut values = SmallVec::<[Option<UnknownIndex>; 4]>::with_capacity(self.unknowns.len());

        values.resize(self.unknowns.len(), None);

        for (local_index, info) in self.unknowns.iter().enumerate() {
            values[local_index] = match info.binding {
                LocalUnknownBinding::Terminal(terminal) => terminals[terminal as usize],

                LocalUnknownBinding::Allocated(index) => Some(
                    allocated
                        .get(index as usize)
                        .expect("allocated range length was validated"),
                ),
            };
        }

        Ok(BoundUnknowns { values })
    }

    #[inline]
    pub(crate) fn unknown_count(&self) -> usize {
        self.unknowns.len()
    }

    #[inline]
    pub(crate) fn matrix_entry_count(&self) -> usize {
        self.matrix_entries.len()
    }

    #[inline]
    pub(crate) fn parameter_count(&self) -> usize {
        self.parameter_count
    }

    #[inline]
    pub(crate) const fn terminal_count(&self) -> usize {
        self.terminal_count
    }

    #[inline]
    pub(crate) const fn allocated_unknown_count(&self) -> usize {
        self.allocated_unknown_count
    }

    #[inline]
    pub(crate) fn state_count(&self) -> usize {
        self.state_writes.len()
    }
}

#[derive(Debug)]
pub(crate) struct BoundDefinitionInputs {
    parameters: Box<[InputSlot]>,
}

impl BoundDefinitionInputs {
    #[inline]
    pub(crate) fn parameters(&self) -> &[InputSlot] {
        &self.parameters
    }

    #[inline]
    pub(crate) fn parameter(&self, index: usize) -> Option<InputSlot> {
        self.parameters.get(index).copied()
    }
}

fn canonicalize_pending_matrix_terms(terms: &mut Vec<PendingMatrixTerm>) {
    terms.retain(|term| term.scale != 0.0);

    terms.sort_unstable_by_key(|term| (term.column, term.row, term.source));

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

fn canonicalize_rhs_terms(terms: &mut Vec<LocalRhsTerm>) {
    terms.retain(|term| term.scale != 0.0);

    terms.sort_unstable_by_key(|term| (term.destination, term.source));

    if terms.len() < 2 {
        return;
    }

    let mut write = 0usize;

    for read in 1..terms.len() {
        if terms[write].destination == terms[read].destination
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::unknown::UnknownAllocator;

    #[test]
    fn folds_constant_local_expressions_once() {
        let mut builder = DefinitionTemplateBuilder::new();

        let four = builder.constant(4.0).unwrap();
        let two = builder.constant(2.0).unwrap();

        let value = builder.div(four, two).unwrap();

        assert_eq!(builder.values[value.index()].constant, Some(2.0),);
    }

    #[test]
    fn canonicalizes_duplicate_local_matrix_terms() {
        let mut builder = DefinitionTemplateBuilder::new();

        let node = builder.terminal_voltage().unwrap();
        let parameter = builder.parameter().unwrap();

        builder.add_matrix(node, node, parameter, 1.0);

        builder.add_matrix(node, node, parameter, 2.0);

        builder.add_matrix(node, node, parameter, -3.0);

        let template = builder.finish().unwrap();

        assert_eq!(template.matrix_entry_count(), 0);
    }

    #[test]
    fn allocated_unknown_is_bound_after_node_voltages() {
        let mut builder = DefinitionTemplateBuilder::new();

        let terminal = builder.terminal_voltage().unwrap();

        let branch = builder.branch_current_unknown().unwrap();

        let template = builder.finish().unwrap();

        assert_eq!(template.terminal_count(), 1);
        assert_eq!(template.allocated_unknown_count(), 1);

        let node = UnknownIndex::new(0);

        let mut allocator = UnknownAllocator::new(1).unwrap();

        let allocated = allocator
            .allocate(template.allocated_unknown_count())
            .unwrap();

        let bound = template.bind_unknowns(&[Some(node)], allocated).unwrap();

        assert_eq!(bound.get(terminal), Some(UnknownIndex::new(0)),);

        assert_eq!(bound.get(branch), Some(UnknownIndex::new(1)),);

        assert_eq!(allocator.dimension(), 2);
    }

    #[test]
    fn state_requires_exactly_one_next_state_producer() {
        let mut builder = DefinitionTemplateBuilder::new();

        let state = builder.state().unwrap();

        assert!(matches!(
            builder.finish(),
            Err(DefinitionTemplateBuildError::MissingStateProducer { state: 0 })
        ));

        let mut builder = DefinitionTemplateBuilder::new();

        let state = builder.state().unwrap();
        let one = builder.constant(1.0).unwrap();
        let two = builder.constant(2.0).unwrap();

        builder.write_state(state, one).unwrap();

        assert_eq!(
            builder.write_state(state, two),
            Err(DefinitionTemplateBuildError::DuplicateStateProducer { state: 0 }),
        );
    }

    #[test]
    fn state_timestep_and_solution_dependencies_bind_to_island_ir() {
        use crate::compile::{island_ir::IslandIrBuilder, state::StateAllocator};

        let mut builder = DefinitionTemplateBuilder::new();

        let terminal = builder.terminal_voltage().unwrap();
        let previous = builder.state().unwrap();
        let timestep = builder.timestep().unwrap();
        let voltage = builder.unknown_value(terminal).unwrap();
        let history_plus_dt = builder.add(previous.value(), timestep).unwrap();
        let next = builder.add(history_plus_dt, voltage).unwrap();

        builder.write_state(previous, next).unwrap();

        let template = builder.finish().unwrap();

        assert_eq!(template.state_count(), 1);

        let unknown = UnknownIndex::new(0);

        let mut unknown_allocator = UnknownAllocator::new(1).unwrap();

        let allocated_unknowns = unknown_allocator
            .allocate(template.allocated_unknown_count())
            .unwrap();

        let unknowns = template
            .bind_unknowns(&[Some(unknown)], allocated_unknowns)
            .unwrap();

        let mut state_allocator = StateAllocator::new();

        let states = state_allocator.allocate(template.state_count()).unwrap();
        let pattern = PatternBuilder::new(1).unwrap().finish().unwrap();

        let mut ir_builder = IslandIrBuilder::new(&pattern);

        template.bind(&unknowns, states, &mut ir_builder).unwrap();

        let ir = ir_builder.finish().unwrap();

        assert_eq!(ir.solution_inputs().len(), 1,);

        assert_eq!(ir.state_inputs().len(), 1,);

        let timestep_input = ir.timestep_input().unwrap();
        let state_input = ir.state_inputs()[0].1;
        let solution_input = ir.solution_inputs()[0].1;

        let mut workspace = ir.value_program().new_workspace();

        workspace.set_input(timestep_input, 0.5);
        workspace.set_input(state_input, 2.0);
        workspace.set_input(solution_input, 3.0);

        ir.value_program().execute_static(&mut workspace);

        ir.value_program().execute_tick(&mut workspace);

        ir.value_program().execute_iteration(&mut workspace);

        let mut next_state = [0.0];

        ir.state_transition()
            .execute(&mut next_state, workspace.values());

        assert_eq!(next_state, [5.5]);
    }
}
