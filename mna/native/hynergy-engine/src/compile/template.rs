use crate::compile::discrete::{BoundComplementaryDriver, BoundDiscreteMetadata};
use crate::compile::island_ir::IslandIrBuilder;
use crate::compile::state::BoundStateSlots;
use crate::compile::unknown::{UnknownAllocationError, UnknownRange};
use hynergy_ids::{define_id, define_non_zero_id};
use hynergy_ir::{InputSlot, ValueBuildError, ValueSlot};
use hynergy_mna::pattern::{PatternBuilder, PatternError, UnknownIndex};
use smallvec::SmallVec;
use thiserror::Error;

define_id!(
    LocalStateId: u32,
    LocalUnknownId: u32,
    LocalParameterId: u32,
    LocalMatrixSlot: u32,
    LocalOutputId: u32,
);
define_non_zero_id!(LocalValueId: u32);

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
    LessEqual(LocalValueId, LocalValueId),

    IterationLatch(LocalValueId),

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
pub(crate) struct LocalIterationLatch {
    value: LocalValueId,
}

impl LocalIterationLatch {
    #[inline]
    pub(crate) const fn value(self) -> LocalValueId {
        self.value
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LocalDiscreteMode {
    value: LocalValueId,
}

impl LocalDiscreteMode {
    #[inline]
    pub(crate) const fn value(self) -> LocalValueId {
        self.value
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LocalComplementaryDriver {
    mode: LocalDiscreteMode,
    output: LocalUnknownId,
    high_rail: LocalUnknownId,
    low_rail: LocalUnknownId,
    pull_up: LocalValueId,
    pull_down: LocalValueId,
}

impl LocalComplementaryDriver {
    #[inline]
    pub(crate) const fn mode(self) -> LocalDiscreteMode {
        self.mode
    }

    #[inline]
    pub(crate) const fn output(self) -> LocalUnknownId {
        self.output
    }

    #[inline]
    pub(crate) const fn high_rail(self) -> LocalUnknownId {
        self.high_rail
    }

    #[inline]
    pub(crate) const fn low_rail(self) -> LocalUnknownId {
        self.low_rail
    }

    #[inline]
    pub(crate) const fn pull_up(self) -> LocalValueId {
        self.pull_up
    }

    #[inline]
    pub(crate) const fn pull_down(self) -> LocalValueId {
        self.pull_down
    }
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

    #[error(transparent)]
    UnknownAllocation(#[from] UnknownAllocationError),
}

#[derive(Debug, Default)]
pub(crate) struct DefinitionTemplateBuilder {
    unknowns: Vec<LocalUnknownInfo>,
    unknown_values: Vec<Option<LocalValueId>>,
    allocated_unknown_count: usize,

    values: Vec<LocalValueInfo>,
    terminal_count: usize,
    parameter_count: usize,

    timestep_value: Option<LocalValueId>,
    state_writes: Vec<Option<LocalValueId>>,
    state_requires_write: SmallVec<[u64; 2]>,

    matrix_terms: Vec<PendingMatrixTerm>,
    rhs_terms: Vec<LocalRhsTerm>,

    iteration_stability_values: Vec<LocalValueId>,
    iteration_latches: Vec<(LocalValueId, Option<LocalValueId>)>,

    discrete_modes: Vec<LocalValueId>,
    complementary_drivers: Vec<LocalComplementaryDriver>,

    outputs: Vec<LocalValueId>,
}

impl DefinitionTemplateBuilder {
    pub(crate) fn output(
        &mut self,
        value: LocalValueId,
    ) -> Result<LocalOutputId, DefinitionTemplateBuildError> {
        debug_assert!(value.index() < self.values.len());

        let raw = u32::try_from(self.outputs.len())
            .map_err(|_| DefinitionTemplateBuildError::IdExhausted)?;

        let output = LocalOutputId::new(raw);

        self.outputs.push(value);

        Ok(output)
    }

    pub(crate) fn iteration_latch(
        &mut self,
        initial: LocalValueId,
    ) -> Result<LocalIterationLatch, DefinitionTemplateBuildError> {
        debug_assert!(initial.index() < self.values.len());

        let value = self.allocate_value(LocalValueInfo {
            node: LocalValueNode::IterationLatch(initial),
            constant: None,
        })?;

        self.iteration_latches.push((value, None));

        Ok(LocalIterationLatch { value })
    }

    pub(crate) fn update_iteration_latch(
        &mut self,
        latch: LocalIterationLatch,
        update: LocalValueId,
    ) {
        debug_assert!(update.index() < self.values.len());

        let (_, destination) = self
            .iteration_latches
            .iter_mut()
            .find(|(value, _)| *value == latch.value)
            .expect("iteration latch must belong to this template");

        assert!(
            destination.replace(update).is_none(),
            "iteration latch must have exactly one update",
        );
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
        self.allocate_read_state(true)
    }

    pub(crate) fn read_state(&mut self) -> Result<LocalState, DefinitionTemplateBuildError> {
        self.allocate_read_state(false)
    }

    fn allocate_read_state(
        &mut self,
        requires_write: bool,
    ) -> Result<LocalState, DefinitionTemplateBuildError> {
        let id = self.allocate_state_slot(requires_write)?;

        let value = self.allocate_value(LocalValueInfo {
            node: LocalValueNode::State(id),
            constant: None,
        })?;

        Ok(LocalState { id, value })
    }

    pub(crate) fn write_only_state(
        &mut self,
    ) -> Result<LocalStateId, DefinitionTemplateBuildError> {
        self.allocate_state_slot(true)
    }

    pub(crate) fn write_state(
        &mut self,
        state: LocalState,
        source: LocalValueId,
    ) -> Result<(), DefinitionTemplateBuildError> {
        self.write_state_id(state.id, source)
    }

    pub(crate) fn write_state_id(
        &mut self,
        state: LocalStateId,
        source: LocalValueId,
    ) -> Result<(), DefinitionTemplateBuildError> {
        debug_assert!(source.index() < self.values.len());

        let destination = &mut self.state_writes[state.index()];

        if destination.is_some() {
            return Err(DefinitionTemplateBuildError::DuplicateStateProducer {
                state: state.index(),
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

    pub(crate) fn allocated_voltage_unknown(
        &mut self,
    ) -> Result<LocalUnknownId, DefinitionTemplateBuildError> {
        self.allocated_unknown(LocalUnknownKind::Voltage)
    }

    pub(crate) fn branch_current_unknown(
        &mut self,
    ) -> Result<LocalUnknownId, DefinitionTemplateBuildError> {
        self.allocated_unknown(LocalUnknownKind::BranchCurrent)
    }

    fn allocated_unknown(
        &mut self,
        kind: LocalUnknownKind,
    ) -> Result<LocalUnknownId, DefinitionTemplateBuildError> {
        let allocated = u32::try_from(self.allocated_unknown_count)
            .map_err(|_| DefinitionTemplateBuildError::IdExhausted)?;

        self.allocated_unknown_count += 1;

        self.allocate_unknown(LocalUnknownInfo {
            kind,
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

    pub(crate) fn less_equal(
        &mut self,
        lhs: LocalValueId,
        rhs: LocalValueId,
    ) -> Result<LocalValueId, DefinitionTemplateBuildError> {
        self.binary(lhs, rhs, LocalBinaryOp::LessEqual)
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

        let (matrix_parameter_dependencies, timestep_affects_matrix) =
            matrix_static_dependencies(&self.values, &self.matrix_terms, self.parameter_count);

        let pending_state_writes = std::mem::take(&mut self.state_writes);
        let state_count = pending_state_writes.len();
        let mut state_writes = Vec::new();

        for (state, source) in pending_state_writes.into_iter().enumerate() {
            match source {
                Some(source) => {
                    let state_id = LocalStateId::new(
                        u32::try_from(state).expect("state index must fit LocalStateId"),
                    );

                    state_writes.push((state_id, source));
                }

                None if self.state_requires_write(state) => {
                    return Err(DefinitionTemplateBuildError::MissingStateProducer { state });
                }

                None => {}
            }
        }

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

        self.iteration_stability_values
            .sort_unstable_by_key(|value| value.index());

        self.iteration_stability_values
            .dedup_by_key(|value| value.index());

        self.discrete_modes
            .sort_unstable_by_key(|value| value.index());

        self.discrete_modes.dedup_by_key(|value| value.index());

        let iteration_latches = self
            .iteration_latches
            .into_iter()
            .map(|(latch, update)| (latch, update.expect("iteration latch must have an update")))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Ok(CompiledDefinitionTemplate {
            unknowns: self.unknowns.into_boxed_slice(),
            values: self.values.into_boxed_slice(),

            matrix_entries: matrix_entries.into_boxed_slice(),
            matrix_terms: matrix_terms.into_boxed_slice(),
            rhs_terms: self.rhs_terms.into_boxed_slice(),

            state_writes: state_writes.into_boxed_slice(),
            state_count,

            matrix_parameter_dependencies,
            timestep_affects_matrix,

            parameter_count: self.parameter_count,
            terminal_count: self.terminal_count,
            allocated_unknown_count: self.allocated_unknown_count,

            iteration_stability_values: self.iteration_stability_values.into_boxed_slice(),
            iteration_latches,

            discrete_modes: self.discrete_modes.into_boxed_slice(),
            complementary_drivers: self.complementary_drivers.into_boxed_slice(),

            outputs: self.outputs.into_boxed_slice(),
        })
    }

    #[inline]
    pub(crate) fn output_count(&self) -> usize {
        self.outputs.len()
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
        let raw = u32::try_from(self.values.len())
            .ok()
            .and_then(|index| index.checked_add(1))
            .ok_or(DefinitionTemplateBuildError::IdExhausted)?;

        let id =
            LocalValueId::try_from(raw).map_err(|_| DefinitionTemplateBuildError::IdExhausted)?;

        self.values.push(info);

        Ok(id)
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

                LocalBinaryOp::LessEqual => {
                    if lhs_value <= rhs_value {
                        1.0
                    } else {
                        0.0
                    }
                }
            };

            return self.constant(value);
        }

        let node = match operation {
            LocalBinaryOp::Add => LocalValueNode::Add(lhs, rhs),
            LocalBinaryOp::Sub => LocalValueNode::Sub(lhs, rhs),
            LocalBinaryOp::Mul => LocalValueNode::Mul(lhs, rhs),
            LocalBinaryOp::Div => LocalValueNode::Div(lhs, rhs),
            LocalBinaryOp::LessEqual => LocalValueNode::LessEqual(lhs, rhs),
        };

        self.allocate_value(LocalValueInfo {
            node,
            constant: None,
        })
    }

    fn set_state_requires_write(&mut self, index: usize, requires_write: bool) {
        if !requires_write {
            return;
        }

        let word = index / 64;
        let bit = index % 64;

        while self.state_requires_write.len() <= word {
            self.state_requires_write.push(0);
        }

        self.state_requires_write[word] |= 1u64 << bit;
    }

    #[inline]
    fn state_requires_write(&self, index: usize) -> bool {
        let word = index / 64;
        let bit = index % 64;

        self.state_requires_write
            .get(word)
            .is_some_and(|word| word & (1u64 << bit) != 0)
    }

    #[inline]
    pub(crate) fn allocate_state_slot(
        &mut self,
        requires_write: bool,
    ) -> Result<LocalStateId, DefinitionTemplateBuildError> {
        let index = self.state_writes.len();

        let raw = u32::try_from(index).map_err(|_| DefinitionTemplateBuildError::IdExhausted)?;

        let id = LocalStateId::new(raw);

        self.state_writes.push(None);
        self.set_state_requires_write(index, requires_write);

        Ok(id)
    }

    #[inline]
    pub(crate) fn discrete_mode(&mut self, value: LocalValueId) -> LocalDiscreteMode {
        debug_assert!(value.index() < self.values.len());

        self.require_iteration_stability(value);
        self.discrete_modes.push(value);

        LocalDiscreteMode { value }
    }

    #[inline]
    pub(crate) fn register_complementary_driver(
        &mut self,
        mode: LocalDiscreteMode,
        output: LocalUnknownId,
        high_rail: LocalUnknownId,
        low_rail: LocalUnknownId,
        pull_up: LocalValueId,
        pull_down: LocalValueId,
    ) {
        debug_assert!(mode.value.index() < self.values.len());
        debug_assert!(output.index() < self.unknowns.len());
        debug_assert!(high_rail.index() < self.unknowns.len());
        debug_assert!(low_rail.index() < self.unknowns.len());
        debug_assert!(pull_up.index() < self.values.len());
        debug_assert!(pull_down.index() < self.values.len());
        debug_assert!(
            self.discrete_modes.contains(&mode.value),
            "complementary driver mode must be registered as a discrete mode",
        );

        self.complementary_drivers.push(LocalComplementaryDriver {
            mode,
            output,
            high_rail,
            low_rail,
            pull_up,
            pull_down,
        });
    }

    #[inline]
    pub(crate) fn require_iteration_stability(&mut self, value: LocalValueId) {
        debug_assert!(value.index() < self.values.len());

        self.iteration_stability_values.push(value);
    }
}

#[derive(Debug, Clone, Copy)]
enum LocalBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    LessEqual,
}

#[derive(Debug)]
pub(crate) struct CompiledDefinitionTemplate {
    unknowns: Box<[LocalUnknownInfo]>,
    values: Box<[LocalValueInfo]>,
    matrix_entries: Box<[LocalMatrixEntry]>,
    matrix_terms: Box<[LocalMatrixTerm]>,
    rhs_terms: Box<[LocalRhsTerm]>,
    state_writes: Box<[(LocalStateId, LocalValueId)]>,
    state_count: usize,
    matrix_parameter_dependencies: SmallVec<[u64; 1]>,
    timestep_affects_matrix: bool,
    parameter_count: usize,
    terminal_count: usize,
    allocated_unknown_count: usize,
    iteration_stability_values: Box<[LocalValueId]>,
    iteration_latches: Box<[(LocalValueId, LocalValueId)]>,
    discrete_modes: Box<[LocalValueId]>,
    complementary_drivers: Box<[LocalComplementaryDriver]>,
    outputs: Box<[LocalValueId]>,
}

impl CompiledDefinitionTemplate {
    pub(crate) fn instantiate_into(
        &self,
        builder: &mut DefinitionTemplateBuilder,
        terminals: &[LocalUnknownId],
        parameters: &[LocalValueId],
        states: &[LocalStateId],
    ) -> Result<SmallVec<[LocalValueId; 4]>, DefinitionTemplateBuildError> {
        assert_eq!(
            terminals.len(),
            self.terminal_count,
            "template instantiation terminal count mismatch",
        );

        assert_eq!(
            parameters.len(),
            self.parameter_count,
            "template instantiation parameter count mismatch",
        );

        assert_eq!(
            states.len(),
            self.state_count,
            "template instantiation state count mismatch",
        );

        let mut unknowns = Vec::with_capacity(self.unknowns.len());

        for info in &self.unknowns {
            let unknown = match info.binding {
                LocalUnknownBinding::Terminal(index) => terminals[index as usize],

                LocalUnknownBinding::Allocated(_) => match info.kind {
                    LocalUnknownKind::Voltage => builder.allocated_voltage_unknown()?,

                    LocalUnknownKind::BranchCurrent => builder.branch_current_unknown()?,
                },
            };

            unknowns.push(unknown);
        }

        let mut values = Vec::with_capacity(self.values.len());

        for info in &self.values {
            let value = match info.node {
                LocalValueNode::Parameter(parameter) => parameters[parameter.index()],

                LocalValueNode::Constant(value) => builder.constant(value)?,

                LocalValueNode::Timestep => builder.timestep()?,

                LocalValueNode::State(state) => {
                    let state = states[state.index()];

                    builder.allocate_value(LocalValueInfo {
                        node: LocalValueNode::State(state),
                        constant: None,
                    })?
                }

                LocalValueNode::Unknown(unknown) => {
                    builder.unknown_value(unknowns[unknown.index()])?
                }

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

                LocalValueNode::LessEqual(lhs, rhs) => {
                    builder.less_equal(values[lhs.index()], values[rhs.index()])?
                }

                LocalValueNode::IterationLatch(initial) => {
                    builder.iteration_latch(values[initial.index()])?.value()
                }

                LocalValueNode::Neg(operand) => builder.neg(values[operand.index()])?,
            };

            values.push(value);
        }

        for &(latch, update) in &self.iteration_latches {
            builder.update_iteration_latch(
                LocalIterationLatch {
                    value: values[latch.index()],
                },
                values[update.index()],
            );
        }

        for &value in &self.iteration_stability_values {
            builder.require_iteration_stability(values[value.index()]);
        }

        for &mode in &self.discrete_modes {
            builder.discrete_mode(values[mode.index()]);
        }

        for &driver in &self.complementary_drivers {
            builder.register_complementary_driver(
                LocalDiscreteMode {
                    value: values[driver.mode.value.index()],
                },
                unknowns[driver.output.index()],
                unknowns[driver.high_rail.index()],
                unknowns[driver.low_rail.index()],
                values[driver.pull_up.index()],
                values[driver.pull_down.index()],
            );
        }

        for term in &self.matrix_terms {
            let entry = self.matrix_entries[term.destination.index()];

            builder.add_matrix(
                unknowns[entry.row.index()],
                unknowns[entry.column.index()],
                values[term.source.index()],
                term.scale,
            );
        }

        for term in &self.rhs_terms {
            builder.add_rhs(
                unknowns[term.destination.index()],
                values[term.source.index()],
                term.scale,
            );
        }

        for &(state, source) in &self.state_writes {
            builder.write_state_id(states[state.index()], values[source.index()])?;
        }

        let outputs = self
            .outputs
            .iter()
            .map(|output| values[output.index()])
            .collect::<SmallVec<[LocalValueId; 4]>>();

        Ok(outputs)
    }

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
        states: &BoundStateSlots,
        ir: &mut IslandIrBuilder<'_>,
    ) -> Result<BoundDefinitionInputs, DefinitionLinkError> {
        if states.len() != self.state_count() {
            return Err(DefinitionLinkError::WrongStateBindingCount {
                expected: self.state_count(),
                actual: states.len(),
            });
        }

        let (values, parameters) = self.bind_values(unknowns, states, ir)?;
        let discrete = self.bind_discrete_metadata(unknowns, &values);

        for &(latch, update) in &self.iteration_latches {
            ir.update_iteration_latch(values[latch.index()], values[update.index()]);
        }

        for &value in &self.iteration_stability_values {
            ir.require_iteration_stability(values[value.index()]);
        }

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

        for &(state, source) in &self.state_writes {
            let destination = states
                .get(state.index())
                .expect("state binding count was validated");

            ir.write_state(destination, values[source.index()]);
        }

        let outputs = self
            .outputs
            .iter()
            .map(|output| values[output.index()])
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Ok(BoundDefinitionInputs {
            parameters,
            outputs,
            discrete,
        })
    }

    fn bind_discrete_metadata(
        &self,
        unknowns: &BoundUnknowns,
        values: &[ValueSlot],
    ) -> BoundDiscreteMetadata {
        let mut modes = SmallVec::<[ValueSlot; 2]>::with_capacity(self.discrete_modes.len());

        for &mode in &self.discrete_modes {
            modes.push(values[mode.index()]);
        }

        let mut complementary_drivers = SmallVec::<[BoundComplementaryDriver; 1]>::with_capacity(
            self.complementary_drivers.len(),
        );

        for &driver in &self.complementary_drivers {
            complementary_drivers.push(BoundComplementaryDriver::new(
                values[driver.mode().value().index()],
                unknowns.get(driver.output()),
                unknowns.get(driver.high_rail()),
                unknowns.get(driver.low_rail()),
                values[driver.pull_up().index()],
                values[driver.pull_down().index()],
            ));
        }

        BoundDiscreteMetadata::new(modes, complementary_drivers)
    }

    fn bind_values(
        &self,
        unknowns: &BoundUnknowns,
        states: &BoundStateSlots,
        ir: &mut IslandIrBuilder<'_>,
    ) -> Result<(Vec<ValueSlot>, Box<[InputSlot]>), DefinitionLinkError> {
        let mut values = Vec::with_capacity(self.values.len());

        let mut parameters = vec![None; self.parameter_count];

        for info in &self.values {
            let value = match info.node {
                LocalValueNode::Parameter(parameter) => {
                    let input = ir.parameter_input(self.parameter_affects_matrix(parameter))?;

                    parameters[parameter.index()] = Some(input);

                    input.value()
                }

                LocalValueNode::Constant(value) => ir.constant_value(value)?,

                LocalValueNode::Timestep => ir.timestep_value(self.timestep_affects_matrix)?,

                LocalValueNode::State(state) => {
                    let state = states
                        .get(state.index())
                        .expect("state binding count was validated");

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

                LocalValueNode::LessEqual(lhs, rhs) => {
                    ir.less_equal_value(values[lhs.index()], values[rhs.index()])?
                }

                LocalValueNode::IterationLatch(initial) => {
                    ir.iteration_latch(values[initial.index()])?
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
    fn parameter_affects_matrix(&self, parameter: LocalParameterId) -> bool {
        let index = parameter.index();

        let word = index / 64;
        let bit = index % 64;

        self.matrix_parameter_dependencies
            .get(word)
            .is_some_and(|word| word & (1u64 << bit) != 0)
    }

    #[cfg(test)]
    #[inline]
    pub(crate) fn matrix_entry_count(&self) -> usize {
        self.matrix_entries.len()
    }

    #[cfg(test)]
    #[inline]
    pub(crate) fn parameter_count(&self) -> usize {
        self.parameter_count
    }

    #[cfg(test)]
    #[inline]
    pub(crate) const fn terminal_count(&self) -> usize {
        self.terminal_count
    }

    #[inline]
    pub(crate) const fn allocated_unknown_count(&self) -> usize {
        self.allocated_unknown_count
    }

    #[inline]
    pub(crate) const fn state_count(&self) -> usize {
        self.state_count
    }

    #[inline]
    pub(crate) fn discrete_modes(&self) -> &[LocalValueId] {
        &self.discrete_modes
    }

    #[inline]
    pub(crate) fn complementary_drivers(&self) -> &[LocalComplementaryDriver] {
        &self.complementary_drivers
    }

    #[inline]
    pub(crate) fn output_count(&self) -> usize {
        self.outputs.len()
    }
}

fn matrix_static_dependencies(
    values: &[LocalValueInfo],
    matrix_terms: &[PendingMatrixTerm],
    parameter_count: usize,
) -> (SmallVec<[u64; 1]>, bool) {
    let word_count = parameter_count / 64 + usize::from(!parameter_count.is_multiple_of(64));
    let mut parameters = SmallVec::<[u64; 1]>::new();

    parameters.resize(word_count, 0);

    let mut timestep = false;
    let mut visited = vec![false; values.len()];

    let mut pending = matrix_terms
        .iter()
        .map(|term| term.source)
        .collect::<Vec<_>>();

    while let Some(value) = pending.pop() {
        let index = value.index();

        if visited[index] {
            continue;
        }

        visited[index] = true;

        match values[index].node {
            LocalValueNode::Parameter(parameter) => {
                let index = parameter.index();
                let word = index / 64;
                let bit = index % 64;

                parameters[word] |= 1u64 << bit;
            }
            LocalValueNode::Timestep => {
                timestep = true;
            }
            LocalValueNode::Add(lhs, rhs)
            | LocalValueNode::Sub(lhs, rhs)
            | LocalValueNode::Mul(lhs, rhs)
            | LocalValueNode::Div(lhs, rhs)
            | LocalValueNode::LessEqual(lhs, rhs) => {
                pending.push(lhs);
                pending.push(rhs);
            }
            LocalValueNode::IterationLatch(initial) => {
                pending.push(initial);
            }
            LocalValueNode::Neg(operand) => {
                pending.push(operand);
            }
            LocalValueNode::Constant(_) | LocalValueNode::State(_) | LocalValueNode::Unknown(_) => {
            }
        }
    }

    (parameters, timestep)
}

#[derive(Debug)]
pub(crate) struct BoundDefinitionInputs {
    parameters: Box<[InputSlot]>,
    outputs: Box<[ValueSlot]>,
    discrete: BoundDiscreteMetadata,
}

impl BoundDefinitionInputs {
    pub(crate) fn into_parts(self) -> (Box<[InputSlot]>, Box<[ValueSlot]>, BoundDiscreteMetadata) {
        (self.parameters, self.outputs, self.discrete)
    }

    #[cfg(test)]
    #[inline]
    pub(crate) fn parameter(&self, index: usize) -> Option<InputSlot> {
        self.parameters.get(index).copied()
    }

    #[cfg(test)]
    #[inline]
    pub(crate) fn output(&self, index: usize) -> Option<ValueSlot> {
        self.outputs.get(index).copied()
    }

    #[cfg(test)]
    #[inline]
    pub(crate) const fn discrete(&self) -> &BoundDiscreteMetadata {
        &self.discrete
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
    use crate::compile::state::BoundStateSlots;
    use crate::compile::unknown::UnknownAllocator;
    use hynergy_ir::StateSlot;

    fn state_slots(indices: &[u32]) -> BoundStateSlots {
        BoundStateSlots::new(indices.iter().copied().map(StateSlot::new).collect())
    }

    #[test]
    fn folds_constant_local_expressions_once() {
        let mut builder = DefinitionTemplateBuilder::default();

        let four = builder.constant(4.0).unwrap();
        let two = builder.constant(2.0).unwrap();

        let value = builder.div(four, two).unwrap();

        assert_eq!(builder.values[value.index()].constant, Some(2.0),);
    }

    #[test]
    fn canonicalizes_duplicate_local_matrix_terms() {
        let mut builder = DefinitionTemplateBuilder::default();

        let node = builder.terminal_voltage().unwrap();
        let parameter = builder.parameter().unwrap();

        builder.add_matrix(node, node, parameter, 1.0);

        builder.add_matrix(node, node, parameter, 2.0);

        builder.add_matrix(node, node, parameter, -3.0);

        let template = builder.finish().unwrap();

        assert_eq!(template.matrix_entry_count(), 0);
    }

    #[test]
    fn discrete_mode_is_stable_and_deduplicated() {
        let mut builder = DefinitionTemplateBuilder::default();

        let input = builder.terminal_voltage().unwrap();
        let value = builder.unknown_value(input).unwrap();

        builder.discrete_mode(value);
        builder.discrete_mode(value);

        let template = builder.finish().unwrap();

        assert_eq!(template.discrete_modes(), &[value]);
        assert_eq!(template.iteration_stability_values.as_ref(), &[value]);
    }

    #[test]
    fn complementary_driver_preserves_exact_local_references() {
        let mut builder = DefinitionTemplateBuilder::default();

        let output = builder.terminal_voltage().unwrap();
        let high_rail = builder.terminal_voltage().unwrap();
        let low_rail = builder.terminal_voltage().unwrap();
        let input = builder.terminal_voltage().unwrap();

        let mode_value = builder.unknown_value(input).unwrap();
        let mode = builder.discrete_mode(mode_value);
        let pull_up = builder.parameter().unwrap();
        let pull_down = builder.parameter().unwrap();

        builder
            .register_complementary_driver(mode, output, high_rail, low_rail, pull_up, pull_down);

        let template = builder.finish().unwrap();
        let driver = template.complementary_drivers()[0];

        assert_eq!(driver.mode().value(), mode_value);
        assert_eq!(driver.output(), output);
        assert_eq!(driver.high_rail(), high_rail);
        assert_eq!(driver.low_rail(), low_rail);
        assert_eq!(driver.pull_up(), pull_up);
        assert_eq!(driver.pull_down(), pull_down);
    }

    #[test]
    fn compiled_template_instantiation_preserves_discrete_driver_metadata() {
        let mut child = DefinitionTemplateBuilder::default();

        let child_output = child.terminal_voltage().unwrap();
        let child_high_rail = child.terminal_voltage().unwrap();
        let child_low_rail = child.terminal_voltage().unwrap();
        let child_input = child.terminal_voltage().unwrap();

        let child_mode_value = child.unknown_value(child_input).unwrap();
        let child_mode = child.discrete_mode(child_mode_value);
        let child_pull_up = child.parameter().unwrap();
        let child_pull_down = child.parameter().unwrap();

        child.register_complementary_driver(
            child_mode,
            child_output,
            child_high_rail,
            child_low_rail,
            child_pull_up,
            child_pull_down,
        );

        let child = child.finish().unwrap();

        let mut parent = DefinitionTemplateBuilder::default();

        let output = parent.terminal_voltage().unwrap();
        let high_rail = parent.terminal_voltage().unwrap();
        let low_rail = parent.terminal_voltage().unwrap();
        let input = parent.terminal_voltage().unwrap();
        let pull_up = parent.parameter().unwrap();
        let pull_down = parent.parameter().unwrap();

        child
            .instantiate_into(
                &mut parent,
                &[output, high_rail, low_rail, input],
                &[pull_up, pull_down],
                &[],
            )
            .unwrap();

        let parent = parent.finish().unwrap();

        assert_eq!(parent.discrete_modes().len(), 1);
        assert_eq!(parent.complementary_drivers().len(), 1);

        let driver = parent.complementary_drivers()[0];

        assert_eq!(driver.mode().value(), parent.discrete_modes()[0]);
        assert_eq!(driver.output(), output);
        assert_eq!(driver.high_rail(), high_rail);
        assert_eq!(driver.low_rail(), low_rail);
        assert_eq!(driver.pull_up(), pull_up);
        assert_eq!(driver.pull_down(), pull_down);
    }

    #[test]
    fn bind_maps_discrete_metadata_to_island_values_and_unknowns() {
        let mut builder = DefinitionTemplateBuilder::default();

        let output = builder.terminal_voltage().unwrap();
        let high_rail = builder.terminal_voltage().unwrap();
        let low_rail = builder.terminal_voltage().unwrap();
        let input = builder.terminal_voltage().unwrap();

        let mode_value = builder.unknown_value(input).unwrap();
        let mode = builder.discrete_mode(mode_value);
        let pull_up = builder.parameter().unwrap();
        let pull_down = builder.parameter().unwrap();

        builder
            .register_complementary_driver(mode, output, high_rail, low_rail, pull_up, pull_down);

        let template = builder.finish().unwrap();

        let output_unknown = UnknownIndex::new(10);
        let high_rail_unknown = UnknownIndex::new(4);
        let low_rail_unknown = UnknownIndex::new(1);
        let input_unknown = UnknownIndex::new(7);

        let mut unknown_allocator = UnknownAllocator::new(12).unwrap();
        let allocated = unknown_allocator
            .allocate(template.allocated_unknown_count())
            .unwrap();

        let unknowns = template
            .bind_unknowns(
                &[
                    Some(output_unknown),
                    Some(high_rail_unknown),
                    Some(low_rail_unknown),
                    Some(input_unknown),
                ],
                allocated,
            )
            .unwrap();

        let pattern = PatternBuilder::new(12).unwrap().finish().unwrap();
        let states = state_slots(&[]);
        let mut ir_builder = IslandIrBuilder::new(&pattern);

        let inputs = template.bind(&unknowns, &states, &mut ir_builder).unwrap();

        let discrete = inputs.discrete();

        assert_eq!(discrete.modes().len(), 1);
        assert_eq!(discrete.complementary_drivers().len(), 1);

        let driver = discrete.complementary_drivers()[0];

        assert_eq!(driver.mode(), discrete.modes()[0]);
        assert_eq!(driver.output(), Some(output_unknown));
        assert_eq!(driver.high_rail(), Some(high_rail_unknown));
        assert_eq!(driver.low_rail(), Some(low_rail_unknown));
        assert_eq!(driver.pull_up(), inputs.parameter(0).unwrap().value());
        assert_eq!(driver.pull_down(), inputs.parameter(1).unwrap().value());
    }

    #[test]
    fn bind_preserves_reference_rails_in_discrete_metadata() {
        let mut builder = DefinitionTemplateBuilder::default();

        let output = builder.terminal_voltage().unwrap();
        let high_rail = builder.terminal_voltage().unwrap();
        let low_rail = builder.terminal_voltage().unwrap();
        let input = builder.terminal_voltage().unwrap();

        let mode_value = builder.unknown_value(input).unwrap();
        let mode = builder.discrete_mode(mode_value);
        let pull_up = builder.parameter().unwrap();
        let pull_down = builder.parameter().unwrap();

        builder
            .register_complementary_driver(mode, output, high_rail, low_rail, pull_up, pull_down);

        let template = builder.finish().unwrap();

        let output_unknown = UnknownIndex::new(0);
        let low_rail_unknown = UnknownIndex::new(1);
        let input_unknown = UnknownIndex::new(2);

        let mut unknown_allocator = UnknownAllocator::new(3).unwrap();
        let allocated = unknown_allocator
            .allocate(template.allocated_unknown_count())
            .unwrap();

        let unknowns = template
            .bind_unknowns(
                &[
                    Some(output_unknown),
                    None,
                    Some(low_rail_unknown),
                    Some(input_unknown),
                ],
                allocated,
            )
            .unwrap();

        let pattern = PatternBuilder::new(3).unwrap().finish().unwrap();
        let states = state_slots(&[]);
        let mut ir_builder = IslandIrBuilder::new(&pattern);

        let inputs = template.bind(&unknowns, &states, &mut ir_builder).unwrap();

        let driver = inputs.discrete().complementary_drivers()[0];

        assert_eq!(driver.output(), Some(output_unknown));
        assert_eq!(driver.high_rail(), None);
        assert_eq!(driver.low_rail(), Some(low_rail_unknown));
    }

    #[test]
    fn state_requires_exactly_one_next_state_producer() {
        let mut builder = DefinitionTemplateBuilder::default();

        let _state = builder.state().unwrap();

        assert!(matches!(
            builder.finish(),
            Err(DefinitionTemplateBuildError::MissingStateProducer { state: 0 })
        ));

        let mut builder = DefinitionTemplateBuilder::default();

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
        use crate::compile::island_ir::IslandIrBuilder;

        let mut builder = DefinitionTemplateBuilder::default();

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

        let states = state_slots(&[0]);
        let pattern = PatternBuilder::new(1).unwrap().finish().unwrap();

        let mut ir_builder = IslandIrBuilder::new(&pattern);

        template.bind(&unknowns, &states, &mut ir_builder).unwrap();

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

    #[test]
    fn state_binding_can_use_non_contiguous_island_slot() {
        let mut builder = DefinitionTemplateBuilder::default();

        let state = builder.state().unwrap();

        builder.write_state(state, state.value()).unwrap();

        let template = builder.finish().unwrap();

        let mut unknown_allocator = UnknownAllocator::new(0).unwrap();
        let allocated = unknown_allocator.allocate(0).unwrap();

        let unknowns = template.bind_unknowns(&[], allocated).unwrap();

        let pattern = PatternBuilder::new(0).unwrap().finish().unwrap();

        let states = BoundStateSlots::new(SmallVec::from_slice(&[StateSlot::new(7)]));

        let mut ir_builder = IslandIrBuilder::new(&pattern);

        template.bind(&unknowns, &states, &mut ir_builder).unwrap();

        let ir = ir_builder.finish().unwrap();

        assert_eq!(ir.state_inputs().len(), 1);
        assert_eq!(ir.state_inputs()[0].0, StateSlot::new(7),);

        let state_input = ir.state_inputs()[0].1;

        let mut workspace = ir.value_program().new_workspace();

        workspace.set_input(state_input, 3.0);

        ir.value_program().execute_tick(&mut workspace);

        let mut next_state = [0.0; 8];

        ir.state_transition()
            .execute(&mut next_state, workspace.values());

        assert_eq!(next_state[7], 3.0);
    }

    #[test]
    fn matrix_dependency_follows_parameter_value_graph() {
        let mut builder = DefinitionTemplateBuilder::default();

        let terminal = builder.terminal_voltage().unwrap();

        let matrix_parameter = builder.parameter().unwrap();

        let rhs_parameter = builder.parameter().unwrap();

        let two = builder.constant(2.0).unwrap();

        let matrix_value = builder.mul(matrix_parameter, two).unwrap();

        builder.add_matrix(terminal, terminal, matrix_value, 1.0);

        builder.add_rhs(terminal, rhs_parameter, 1.0);

        let template = builder.finish().unwrap();

        let mut unknown_allocator = UnknownAllocator::new(1).unwrap();

        let unknowns = template
            .bind_unknowns(
                &[Some(UnknownIndex::new(0))],
                unknown_allocator.allocate(0).unwrap(),
            )
            .unwrap();

        let mut pattern_builder = PatternBuilder::new(1).unwrap();

        template
            .request_pattern(&unknowns, &mut pattern_builder)
            .unwrap();

        let pattern = pattern_builder.finish().unwrap();

        let states = BoundStateSlots::new(SmallVec::new());

        let mut ir_builder = IslandIrBuilder::new(&pattern);

        let inputs = template.bind(&unknowns, &states, &mut ir_builder).unwrap();

        let ir = ir_builder.finish().unwrap();

        assert!(ir.static_input_affects_matrix(inputs.parameter(0).unwrap(),));

        assert!(!ir.static_input_affects_matrix(inputs.parameter(1).unwrap(),));
    }

    #[test]
    fn matrix_dependency_follows_timestep_value_graph() {
        let mut builder = DefinitionTemplateBuilder::default();

        let terminal = builder.terminal_voltage().unwrap();

        let timestep = builder.timestep().unwrap();

        builder.add_matrix(terminal, terminal, timestep, 1.0);

        let template = builder.finish().unwrap();

        let mut unknown_allocator = UnknownAllocator::new(1).unwrap();

        let unknowns = template
            .bind_unknowns(
                &[Some(UnknownIndex::new(0))],
                unknown_allocator.allocate(0).unwrap(),
            )
            .unwrap();

        let mut pattern_builder = PatternBuilder::new(1).unwrap();

        template
            .request_pattern(&unknowns, &mut pattern_builder)
            .unwrap();

        let pattern = pattern_builder.finish().unwrap();

        let states = BoundStateSlots::new(SmallVec::new());

        let mut ir_builder = IslandIrBuilder::new(&pattern);

        template.bind(&unknowns, &states, &mut ir_builder).unwrap();

        let ir = ir_builder.finish().unwrap();

        assert!(ir.static_input_affects_matrix(ir.timestep_input().unwrap(),));
    }

    #[test]
    fn matrix_dependency_follows_comparison_operands() {
        let mut builder = DefinitionTemplateBuilder::default();

        let terminal = builder.terminal_voltage().unwrap();

        let lhs = builder.parameter().unwrap();
        let rhs = builder.parameter().unwrap();

        let comparison = builder.less_equal(lhs, rhs).unwrap();

        builder.add_matrix(terminal, terminal, comparison, 1.0);

        let template = builder.finish().unwrap();

        let mut unknown_allocator = UnknownAllocator::new(1).unwrap();

        let unknowns = template
            .bind_unknowns(
                &[Some(UnknownIndex::new(0))],
                unknown_allocator.allocate(0).unwrap(),
            )
            .unwrap();

        let mut pattern_builder = PatternBuilder::new(1).unwrap();

        template
            .request_pattern(&unknowns, &mut pattern_builder)
            .unwrap();

        let pattern = pattern_builder.finish().unwrap();
        let states = BoundStateSlots::new(SmallVec::new());

        let mut ir_builder = IslandIrBuilder::new(&pattern);

        let inputs = template.bind(&unknowns, &states, &mut ir_builder).unwrap();
        let ir = ir_builder.finish().unwrap();

        assert!(ir.static_input_affects_matrix(inputs.parameter(0).unwrap(),),);
        assert!(ir.static_input_affects_matrix(inputs.parameter(1).unwrap(),),);
    }

    #[test]
    fn allocated_voltage_and_branch_unknowns_bind_in_allocation_order() {
        let mut builder = DefinitionTemplateBuilder::default();

        let terminal = builder.terminal_voltage().unwrap();
        let internal = builder.allocated_voltage_unknown().unwrap();
        let branch = builder.branch_current_unknown().unwrap();

        let template = builder.finish().unwrap();

        assert_eq!(template.terminal_count(), 1);
        assert_eq!(template.allocated_unknown_count(), 2);

        let terminal_unknown = UnknownIndex::new(0);

        let mut allocator = UnknownAllocator::new(1).unwrap();

        let allocated = allocator
            .allocate(template.allocated_unknown_count())
            .unwrap();

        let bound = template
            .bind_unknowns(&[Some(terminal_unknown)], allocated)
            .unwrap();

        assert_eq!(bound.get(terminal), Some(UnknownIndex::new(0)),);

        assert_eq!(bound.get(internal), Some(UnknownIndex::new(1)),);

        assert_eq!(bound.get(branch), Some(UnknownIndex::new(2)),);

        assert_eq!(allocator.dimension(), 3);
    }

    #[test]
    fn instantiation_returns_remapped_outputs() {
        let mut child = DefinitionTemplateBuilder::default();

        let positive = child.terminal_voltage().unwrap();
        let negative = child.terminal_voltage().unwrap();

        let positive_value = child.unknown_value(positive).unwrap();
        let negative_value = child.unknown_value(negative).unwrap();

        let voltage = child.sub(positive_value, negative_value).unwrap();

        child.output(voltage).unwrap();

        let child = child.finish().unwrap();

        let mut parent = DefinitionTemplateBuilder::default();

        let parent_positive = parent.terminal_voltage().unwrap();
        let parent_negative = parent.terminal_voltage().unwrap();

        let outputs = child
            .instantiate_into(&mut parent, &[parent_positive, parent_negative], &[], &[])
            .unwrap();

        assert_eq!(outputs.len(), 1);

        parent.output(outputs[0]).unwrap();

        let _parent = parent.finish().unwrap();
    }

    #[test]
    fn compiled_template_instantiates_into_parent_builder() {
        let mut child = DefinitionTemplateBuilder::default();

        let child_a = child.terminal_voltage().unwrap();
        let _child_b = child.terminal_voltage().unwrap();

        let internal = child.allocated_voltage_unknown().unwrap();
        let branch = child.branch_current_unknown().unwrap();

        let parameter = child.parameter().unwrap();

        child.add_matrix(child_a, internal, parameter, 2.0);
        child.add_rhs(branch, parameter, -3.0);

        let child = child.finish().unwrap();

        let mut parent = DefinitionTemplateBuilder::default();

        let parent_a = parent.terminal_voltage().unwrap();
        let parent_b = parent.terminal_voltage().unwrap();
        let parent_parameter = parent.parameter().unwrap();

        child
            .instantiate_into(&mut parent, &[parent_a, parent_b], &[parent_parameter], &[])
            .unwrap();

        assert_eq!(parent.terminal_count, 2);
        assert_eq!(parent.parameter_count, 1);
        assert_eq!(parent.allocated_unknown_count, 2);

        assert_eq!(parent.unknowns[2].kind, LocalUnknownKind::Voltage,);

        assert_eq!(parent.unknowns[3].kind, LocalUnknownKind::BranchCurrent,);

        assert_eq!(parent.matrix_terms.len(), 1);
        assert_eq!(parent.rhs_terms.len(), 1);

        let parent = parent.finish().unwrap();

        assert!(parent.parameter_affects_matrix(LocalParameterId::new(0),));
    }

    #[test]
    fn compiled_template_instantiation_preserves_iteration_semantics() {
        let mut child = DefinitionTemplateBuilder::default();

        let control = child.terminal_voltage().unwrap();
        let initial = child.parameter().unwrap();

        let latch = child.iteration_latch(initial).unwrap();

        let control_value = child.unknown_value(control).unwrap();

        let update = child.less_equal(latch.value(), control_value).unwrap();

        child.update_iteration_latch(latch, update);
        child.require_iteration_stability(update);

        let child = child.finish().unwrap();

        let mut parent = DefinitionTemplateBuilder::default();

        let parent_control = parent.terminal_voltage().unwrap();

        let parent_initial = parent.parameter().unwrap();

        child
            .instantiate_into(&mut parent, &[parent_control], &[parent_initial], &[])
            .unwrap();

        let parent = parent.finish().unwrap();

        assert_eq!(parent.iteration_latches.len(), 1);
        assert_eq!(parent.iteration_stability_values.len(), 1,);
    }

    #[test]
    fn compiled_template_instantiation_maps_state_reads_and_writes() {
        let mut child = DefinitionTemplateBuilder::default();

        let terminal = child.terminal_voltage().unwrap();

        let state = child.state().unwrap();

        let terminal_value = child.unknown_value(terminal).unwrap();

        let next = child.add(state.value(), terminal_value).unwrap();

        child.write_state(state, next).unwrap();

        let child = child.finish().unwrap();

        let mut parent = DefinitionTemplateBuilder::default();

        let parent_terminal = parent.terminal_voltage().unwrap();

        let parent_state = parent.allocate_state_slot(true).unwrap();

        child
            .instantiate_into(&mut parent, &[parent_terminal], &[], &[parent_state])
            .unwrap();

        let parent = parent.finish().unwrap();

        assert_eq!(parent.state_count(), 1);
        assert_eq!(parent.state_writes.len(), 1);
        assert_eq!(parent.state_writes[0].0, parent_state);
    }

    #[test]
    fn exported_value_binds_to_island_value() {
        let mut builder = DefinitionTemplateBuilder::default();

        let terminal = builder.terminal_voltage().unwrap();
        let voltage = builder.unknown_value(terminal).unwrap();

        let output = builder.output(voltage).unwrap();

        assert_eq!(output, LocalOutputId::new(0));

        let template = builder.finish().unwrap();

        assert_eq!(template.output_count(), 1);

        let mut unknown_allocator = UnknownAllocator::new(1).unwrap();

        let unknowns = template
            .bind_unknowns(
                &[Some(UnknownIndex::new(0))],
                unknown_allocator.allocate(0).unwrap(),
            )
            .unwrap();

        let states = BoundStateSlots::new(SmallVec::new());
        let pattern = PatternBuilder::new(1).unwrap().finish().unwrap();

        let mut ir_builder = IslandIrBuilder::new(&pattern);

        let bindings = template.bind(&unknowns, &states, &mut ir_builder).unwrap();
        let output = bindings.output(0).unwrap();
        let ir = ir_builder.finish().unwrap();

        assert_eq!(ir.solution_inputs().len(), 1);

        let solution_input = ir.solution_inputs()[0].1;

        let mut workspace = ir.value_program().new_workspace();

        workspace.set_input(solution_input, 7.5);

        ir.value_program().execute_iteration(&mut workspace);

        assert_eq!(workspace.value(output), 7.5);
    }
}
