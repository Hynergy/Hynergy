use hynergy_ir::{InputSlot, MatrixAdd, RhsAdd, ValueBuildError, ValueProgramBuilder, ValueSlot};
use hynergy_mna::pattern::{MnaPattern, PatternBuilder, PatternError, UnknownIndex};

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

#[derive(Debug, Clone, Copy)]
enum LocalValueNode {
    Parameter(LocalParameterId),
    Constant(f64),

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DefinitionTemplateBuildError {
    IdExhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DefinitionLinkError {
    WrongUnknownBindingCount {
        expected: usize,
        actual: usize,
    },
    RequiredUnknownBoundToReference {
        local_unknown: usize,
    },
    Pattern(PatternError),
    Values(ValueBuildError),
    MissingPatternEntry {
        row: UnknownIndex,
        column: UnknownIndex,
    },
}

impl From<PatternError> for DefinitionLinkError {
    #[inline]
    fn from(error: PatternError) -> Self {
        Self::Pattern(error)
    }
}

impl From<ValueBuildError> for DefinitionLinkError {
    #[inline]
    fn from(error: ValueBuildError) -> Self {
        Self::Values(error)
    }
}

#[derive(Debug, Default)]
pub(crate) struct DefinitionTemplateBuilder {
    unknowns: Vec<LocalUnknownKind>,

    values: Vec<LocalValueInfo>,
    parameter_count: usize,

    matrix_terms: Vec<PendingMatrixTerm>,
    rhs_terms: Vec<LocalRhsTerm>,
}

impl DefinitionTemplateBuilder {
    #[inline]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn voltage_unknown(
        &mut self,
    ) -> Result<LocalUnknownId, DefinitionTemplateBuildError> {
        self.allocate_unknown(LocalUnknownKind::Voltage)
    }

    pub(crate) fn branch_current_unknown(
        &mut self,
    ) -> Result<LocalUnknownId, DefinitionTemplateBuildError> {
        self.allocate_unknown(LocalUnknownKind::BranchCurrent)
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

            parameter_count: self.parameter_count,
        })
    }

    fn allocate_unknown(
        &mut self,
        kind: LocalUnknownKind,
    ) -> Result<LocalUnknownId, DefinitionTemplateBuildError> {
        let index = u32::try_from(self.unknowns.len())
            .map_err(|_| DefinitionTemplateBuildError::IdExhausted)?;

        self.unknowns.push(kind);

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
    unknowns: Box<[LocalUnknownKind]>,
    values: Box<[LocalValueInfo]>,
    matrix_entries: Box<[LocalMatrixEntry]>,
    matrix_terms: Box<[LocalMatrixTerm]>,
    rhs_terms: Box<[LocalRhsTerm]>,
    parameter_count: usize,
}

impl CompiledDefinitionTemplate {
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

    pub(crate) fn request_pattern(
        &self,
        unknowns: &[Option<UnknownIndex>],
        pattern: &mut PatternBuilder,
    ) -> Result<(), DefinitionLinkError> {
        self.validate_unknown_bindings(unknowns)?;

        for entry in &self.matrix_entries {
            let Some(row) = unknowns[entry.row.index()] else {
                continue;
            };

            let Some(column) = unknowns[entry.column.index()] else {
                continue;
            };

            pattern.request(row, column)?;
        }

        Ok(())
    }

    pub(crate) fn bind(
        &self,
        unknowns: &[Option<UnknownIndex>],
        pattern: &MnaPattern,
        value_builder: &mut ValueProgramBuilder,
        matrix_ops: &mut Vec<MatrixAdd>,
        rhs_ops: &mut Vec<RhsAdd>,
    ) -> Result<BoundDefinitionInputs, DefinitionLinkError> {
        self.validate_unknown_bindings(unknowns)?;

        let (values, parameters) = self.bind_values(value_builder)?;

        for term in &self.matrix_terms {
            let entry = self.matrix_entries[term.destination.index()];

            let Some(row) = unknowns[entry.row.index()] else {
                continue;
            };

            let Some(column) = unknowns[entry.column.index()] else {
                continue;
            };

            let destination = pattern
                .slot(row, column)
                .ok_or(DefinitionLinkError::MissingPatternEntry { row, column })?;

            matrix_ops.push(MatrixAdd::new(
                destination,
                values[term.source.index()],
                term.scale,
            ));
        }

        for term in &self.rhs_terms {
            let Some(destination) = unknowns[term.destination.index()] else {
                continue;
            };

            rhs_ops.push(RhsAdd::new(
                destination,
                values[term.source.index()],
                term.scale,
            ));
        }

        Ok(BoundDefinitionInputs { parameters })
    }

    fn bind_values(
        &self,
        builder: &mut ValueProgramBuilder,
    ) -> Result<(Vec<ValueSlot>, Box<[InputSlot]>), DefinitionLinkError> {
        let mut values = Vec::with_capacity(self.values.len());

        let mut parameters = vec![None; self.parameter_count];

        for info in &self.values {
            let value = match info.node {
                LocalValueNode::Parameter(parameter) => {
                    let input = builder.static_input()?;

                    parameters[parameter.index()] = Some(input);

                    input.value()
                }

                LocalValueNode::Constant(value) => builder.constant(value)?,

                LocalValueNode::Add(lhs, rhs) => {
                    builder.add(values[lhs.index()], values[rhs.index()])?
                }

                LocalValueNode::Sub(lhs, rhs) => {
                    builder.sub(values[lhs.index()], values[rhs.index()])?
                }

                LocalValueNode::Mul(lhs, rhs) => {
                    builder.mul(values[lhs.index()], values[rhs.index()])?
                }

                LocalValueNode::Div(lhs, rhs) => {
                    builder.div(values[lhs.index()], values[rhs.index()])?
                }

                LocalValueNode::Neg(operand) => builder.neg(values[operand.index()])?,
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

    fn validate_unknown_bindings(
        &self,
        unknowns: &[Option<UnknownIndex>],
    ) -> Result<(), DefinitionLinkError> {
        if unknowns.len() != self.unknown_count() {
            return Err(DefinitionLinkError::WrongUnknownBindingCount {
                expected: self.unknown_count(),
                actual: unknowns.len(),
            });
        }

        for (index, (&kind, binding)) in self.unknowns.iter().zip(unknowns).enumerate() {
            if kind == LocalUnknownKind::BranchCurrent && binding.is_none() {
                return Err(DefinitionLinkError::RequiredUnknownBoundToReference {
                    local_unknown: index,
                });
            }
        }

        Ok(())
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
    use hynergy_mna::pattern::PatternBuilder;

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

        let node = builder.voltage_unknown().unwrap();
        let parameter = builder.parameter().unwrap();

        builder.add_matrix(node, node, parameter, 1.0);

        builder.add_matrix(node, node, parameter, 2.0);

        builder.add_matrix(node, node, parameter, -3.0);

        let template = builder.finish().unwrap();

        assert_eq!(template.matrix_entry_count(), 0);
    }

    #[test]
    fn branch_current_cannot_bind_to_reference() {
        let mut builder = DefinitionTemplateBuilder::new();

        builder.branch_current_unknown().unwrap();

        let template = builder.finish().unwrap();

        let mut pattern = PatternBuilder::new(0).unwrap();

        assert_eq!(
            template.request_pattern(&[None], &mut pattern,),
            Err(DefinitionLinkError::RequiredUnknownBoundToReference { local_unknown: 0 },),
        );
    }
}
