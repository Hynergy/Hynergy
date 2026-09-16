use crate::compile::CompiledDefinitionTemplate;
use crate::compile::definition::{
    CompiledDefinition, CompiledPartitionTemplate, DefinitionCompileError, DefinitionStateId,
};
use crate::compile::island_ir::{CompiledIslandIr, IslandIrBuildError, IslandIrBuilder};
use crate::compile::state::{BoundStateSlots, StateAllocationError};
use crate::compile::template::{BoundDefinitionInputs, BoundUnknowns, DefinitionLinkError};
use crate::compile::unknown::{UnknownAllocationError, UnknownAllocator};
use crate::topology::{DerivedTopology, IslandId, NetId};
use hynergy_ir::{StateSlot, ValueWorkspace};
use hynergy_mna::pattern::{MnaPattern, PatternBuilder, UnknownIndex};
use hynergy_mna::system::{MnaError, MnaSystem};
use hynergy_model::device::definition::{DefinitionId, DeviceId, TerminalId};
use hynergy_model::device::registry::DefinitionRegistry;
use hynergy_model::network::Network;
use hynergy_model::parameter::ParameterId;
use smallvec::SmallVec;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IslandUnknownLayout {
    nodes: SmallVec<[IslandNode; 4]>,
}

impl IslandUnknownLayout {
    pub(crate) fn new(nodes: &[IslandNode]) -> Result<Self, UnknownAllocationError> {
        let dimension = nodes.len().saturating_sub(1);

        UnknownAllocator::new(dimension)?;

        #[cfg(debug_assertions)]
        {
            for (index, &node) in nodes.iter().enumerate() {
                debug_assert!(
                    !nodes[..index].contains(&node),
                    "island node list must not contain duplicates",
                );
            }
        }

        Ok(Self {
            nodes: SmallVec::from_slice(nodes),
        })
    }

    #[inline]
    pub(crate) fn dimension(&self) -> usize {
        self.nodes.len().saturating_sub(1)
    }

    #[inline]
    pub(crate) fn node_unknown(&self, node: IslandNode) -> Option<UnknownIndex> {
        let position = self
            .nodes
            .iter()
            .position(|&candidate| candidate == node)
            .expect("island node must belong to unknown layout");

        if position == 0 {
            return None;
        }

        Some(UnknownIndex::new(
            u32::try_from(position - 1).expect("island voltage index must fit UnknownIndex"),
        ))
    }

    pub(crate) fn bind_terminal_nodes(
        &self,
        nodes: &[IslandNode],
    ) -> SmallVec<[Option<UnknownIndex>; 4]> {
        nodes.iter().map(|&node| self.node_unknown(node)).collect()
    }
}

pub(crate) fn bind_partition_unknowns(
    layout: &IslandUnknownLayout,
    template: &CompiledDefinitionTemplate,
    terminal_nodes: &[IslandNode],
    allocator: &mut UnknownAllocator,
) -> Result<BoundUnknowns, DefinitionLinkError> {
    let terminals = layout.bind_terminal_nodes(terminal_nodes);

    let allocated = allocator.allocate(template.allocated_unknown_count())?;

    template.bind_unknowns(&terminals, allocated)
}

pub(crate) fn build_island_pattern(
    dimension: usize,
    partitions: &[(&CompiledDefinitionTemplate, &BoundUnknowns)],
) -> Result<MnaPattern, DefinitionLinkError> {
    let mut pattern = PatternBuilder::new(dimension)?;

    for &(template, unknowns) in partitions {
        template.request_pattern(unknowns, &mut pattern)?;
    }

    Ok(pattern.finish()?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct DeviceState {
    device: DeviceId,
    state: DefinitionStateId,
}

impl DeviceState {
    #[inline]
    pub(crate) const fn new(device: DeviceId, state: DefinitionStateId) -> Self {
        Self { device, state }
    }

    #[inline]
    pub(crate) const fn device(self) -> DeviceId {
        self.device
    }

    #[inline]
    pub(crate) const fn state(self) -> DefinitionStateId {
        self.state
    }
}

#[derive(Debug, Default)]
pub(crate) struct IslandStateLayout {
    states: SmallVec<[DeviceState; 2]>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct IslandPartitionSpec<'a> {
    device: DeviceId,
    partition: &'a CompiledPartitionTemplate,
    terminal_nodes: &'a [IslandNode],
}

impl<'a> IslandPartitionSpec<'a> {
    #[inline]
    pub(crate) const fn new(
        device: DeviceId,
        partition: &'a CompiledPartitionTemplate,
        terminal_nodes: &'a [IslandNode],
    ) -> Self {
        Self {
            device,
            partition,
            terminal_nodes,
        }
    }
}

#[derive(Debug)]
struct BoundIslandPartition<'a> {
    device: DeviceId,
    template: &'a CompiledDefinitionTemplate,
    unknowns: BoundUnknowns,
    states: BoundStateSlots,
}

#[derive(Debug)]
pub(crate) struct CompiledPartitionInputs {
    device: DeviceId,
    definition_parameters: SmallVec<[ParameterId; 1]>,
    inputs: BoundDefinitionInputs,
}

impl CompiledPartitionInputs {
    #[inline]
    pub(crate) const fn device(&self) -> DeviceId {
        self.device
    }

    #[inline]
    pub(crate) fn definition_parameters(&self) -> &[ParameterId] {
        &self.definition_parameters
    }

    #[inline]
    pub(crate) const fn inputs(&self) -> &BoundDefinitionInputs {
        &self.inputs
    }
}

#[derive(Debug, Error)]
pub(crate) enum IslandCompileError {
    #[error(transparent)]
    UnknownAllocation(#[from] UnknownAllocationError),

    #[error(transparent)]
    StateAllocation(#[from] StateAllocationError),

    #[error(transparent)]
    DefinitionLink(#[from] DefinitionLinkError),

    #[error(transparent)]
    Ir(#[from] IslandIrBuildError),

    #[error(transparent)]
    DefinitionCompile(#[from] DefinitionCompileError),
}

#[derive(Debug)]
pub(crate) struct CompiledIsland {
    pattern: MnaPattern,
    ir: CompiledIslandIr,
    unknowns: IslandUnknownLayout,
    states: IslandStateLayout,
    partition_inputs: Box<[CompiledPartitionInputs]>,
}

#[cfg(test)]
impl CompiledIsland {
    #[inline]
    pub(crate) const fn pattern(&self) -> &MnaPattern {
        &self.pattern
    }

    #[inline]
    pub(crate) const fn ir(&self) -> &CompiledIslandIr {
        &self.ir
    }

    #[inline]
    pub(crate) fn state_count(&self) -> usize {
        self.states.state_count()
    }

    #[inline]
    pub(crate) fn partition_inputs(&self) -> &[CompiledPartitionInputs] {
        &self.partition_inputs
    }
}

impl IslandStateLayout {
    #[inline]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn bind_partition_states(
        &mut self,
        device: DeviceId,
        definition_states: &[DefinitionStateId],
    ) -> Result<BoundStateSlots, StateAllocationError> {
        let mut slots = SmallVec::<[StateSlot; 4]>::with_capacity(definition_states.len());

        for &state in definition_states {
            let key = DeviceState::new(device, state);

            let slot = if let Some(index) =
                self.states.iter().position(|&candidate| candidate == key)
            {
                StateSlot::new(u32::try_from(index).expect("island state index must fit StateSlot"))
            } else {
                if self.states.len() >= crate::compile::state::MAX_STATE_COUNT {
                    return Err(StateAllocationError::StateCountTooLarge {
                        requested: self.states.len() + 1,
                        max: crate::compile::state::MAX_STATE_COUNT,
                    });
                }

                let slot = StateSlot::new(self.states.len() as u32);

                self.states.push(key);

                slot
            };

            slots.push(slot);
        }

        Ok(BoundStateSlots::new(slots))
    }

    #[inline]
    pub(crate) fn state_count(&self) -> usize {
        self.states.len()
    }

    #[inline]
    pub(crate) fn device_state(&self, slot: StateSlot) -> Option<DeviceState> {
        self.states.get(slot.index()).copied()
    }
}

pub(crate) fn compile_island_parts(
    nodes: &[IslandNode],
    partitions: &[IslandPartitionSpec<'_>],
) -> Result<CompiledIsland, IslandCompileError> {
    let unknown_layout = IslandUnknownLayout::new(nodes)?;

    let mut unknown_allocator = UnknownAllocator::new(unknown_layout.dimension())?;

    let mut state_layout = IslandStateLayout::new();

    let mut bound_partitions = Vec::with_capacity(partitions.len());

    for partition in partitions {
        let template = partition.partition.template();

        let unknowns = bind_partition_unknowns(
            &unknown_layout,
            template,
            partition.terminal_nodes,
            &mut unknown_allocator,
        )?;

        let states = state_layout
            .bind_partition_states(partition.device, partition.partition.definition_states())?;

        bound_partitions.push(BoundIslandPartition {
            device: partition.device,
            template,
            unknowns,
            states,
        });
    }

    let pattern_partitions = bound_partitions
        .iter()
        .map(|partition| (partition.template, &partition.unknowns))
        .collect::<Vec<_>>();

    let pattern = build_island_pattern(unknown_allocator.dimension(), &pattern_partitions)?;

    let mut ir_builder = IslandIrBuilder::new(&pattern);

    let mut partition_inputs = Vec::with_capacity(bound_partitions.len());

    for (partition, spec) in bound_partitions.iter().zip(partitions) {
        let inputs =
            partition
                .template
                .bind(&partition.unknowns, &partition.states, &mut ir_builder)?;

        partition_inputs.push(CompiledPartitionInputs {
            device: partition.device,

            definition_parameters: SmallVec::from_slice(spec.partition.definition_parameters()),

            inputs,
        });
    }

    let ir = ir_builder.finish()?;

    Ok(CompiledIsland {
        pattern,
        ir,
        unknowns: unknown_layout,
        states: state_layout,
        partition_inputs: partition_inputs.into_boxed_slice(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum IslandNode {
    Net(NetId),
    Terminal {
        device: DeviceId,
        terminal: TerminalId,
    },
}

impl IslandNode {
    #[inline]
    pub(crate) const fn net(net: NetId) -> Self {
        Self::Net(net)
    }

    #[inline]
    pub(crate) const fn terminal(device: DeviceId, terminal: TerminalId) -> Self {
        Self::Terminal { device, terminal }
    }
}

fn resolve_terminal_node(
    topology: &DerivedTopology,
    network: &Network,
    device: DeviceId,
    terminal: TerminalId,
) -> IslandNode {
    let device_slot = network
        .devices()
        .get(device.index())
        .and_then(Option::as_ref)
        .expect("island component device must exist");

    let connection = device_slot
        .terminals()
        .get(terminal.index())
        .copied()
        .expect("compiled definition terminal must exist");

    let Some(connection) = connection else {
        return IslandNode::terminal(device, terminal);
    };

    if let Some(wire) = connection.as_wire() {
        return IslandNode::net(topology.wire_net(wire));
    }

    let (other_device, other_terminal) = connection
        .as_terminal()
        .expect("network connection must be wire or terminal");

    let this = (device, terminal);
    let other = (other_device, other_terminal);

    let (canonical_device, canonical_terminal) = if this <= other { this } else { other };

    IslandNode::terminal(canonical_device, canonical_terminal)
}

pub(crate) fn compile_topology_island(
    definitions: &DefinitionRegistry,
    network: &Network,
    topology: &DerivedTopology,
    island_id: IslandId,
) -> Result<CompiledIsland, IslandCompileError> {
    let island = topology
        .island(island_id)
        .expect("compiled IslandId must reference a live island");

    let mut compiled_definitions = Vec::<(DefinitionId, CompiledDefinition)>::new();

    let mut compiled_definition_indices = Vec::with_capacity(island.components().len());

    let mut terminal_nodes = Vec::<Vec<IslandNode>>::with_capacity(island.components().len());

    let mut nodes = Vec::<IslandNode>::new();

    for &component in island.components() {
        let device = component.device();

        let definition_id = network
            .device_definition_id(device)
            .expect("island component device must exist");

        let compiled_index = if let Some(index) = compiled_definitions
            .iter()
            .position(|(candidate, _)| *candidate == definition_id)
        {
            index
        } else {
            let definition = definitions
                .get(definition_id)
                .expect("island definition must remain registered");

            let compiled = CompiledDefinition::compile(definition)?;

            compiled_definitions.push((definition_id, compiled));

            compiled_definitions.len() - 1
        };

        compiled_definition_indices.push(compiled_index);

        let partition = compiled_definitions[compiled_index]
            .1
            .partition(component.partition())
            .expect("component partition must exist in compiled definition");

        let mut component_nodes = Vec::with_capacity(partition.definition_terminals().len());

        for &terminal in partition.definition_terminals() {
            let node = resolve_terminal_node(topology, network, device, terminal);

            if !nodes.contains(&node) {
                nodes.push(node);
            }

            component_nodes.push(node);
        }

        terminal_nodes.push(component_nodes);
    }

    let mut partitions = Vec::with_capacity(island.components().len());

    for (index, &component) in island.components().iter().enumerate() {
        let compiled = &compiled_definitions[compiled_definition_indices[index]].1;

        let partition = compiled
            .partition(component.partition())
            .expect("component partition must exist in compiled definition");

        partitions.push(IslandPartitionSpec::new(
            component.device(),
            partition,
            &terminal_nodes[index],
        ));
    }

    compile_island_parts(&nodes, &partitions)
}

#[derive(Debug, Error)]
pub(crate) enum IslandRuntimeError {
    #[error("device {device:?} no longer exists")]
    MissingDevice { device: DeviceId },

    #[error("device {device:?} parameter {parameter:?} is not assigned")]
    MissingParameter {
        device: DeviceId,
        parameter: ParameterId,
    },

    #[error(transparent)]
    Mna(#[from] MnaError),

    #[error("state {state:?} for device {device:?} is not available")]
    MissingState {
        device: DeviceId,
        state: DefinitionStateId,
    },

    #[error("timestep must be finite and greater than zero")]
    InvalidTimestep,
}

#[derive(Debug)]
pub(crate) struct IslandRuntime {
    system: MnaSystem,
    ir: CompiledIslandIr,

    unknowns: IslandUnknownLayout,
    states: IslandStateLayout,
    partition_inputs: Box<[CompiledPartitionInputs]>,

    workspace: ValueWorkspace,
    solution: Box<[f64]>,

    solution_valid: bool,

    static_initialized: bool,
    matrix_dirty: bool,
    last_timestep: f64,

    #[cfg(test)]
    matrix_stamp_count: usize,
}

impl IslandRuntime {
    pub(crate) fn new(compiled: CompiledIsland) -> Result<Self, IslandRuntimeError> {
        let CompiledIsland {
            pattern,
            ir,
            unknowns,
            states,
            partition_inputs,
        } = compiled;

        let dimension = pattern.dimension();

        let workspace = ir.value_program().new_workspace();

        let system = MnaSystem::new(pattern)?;

        Ok(Self {
            system,
            ir,
            unknowns,
            states,
            partition_inputs,
            workspace,
            solution: vec![0.0; dimension].into_boxed_slice(),
            solution_valid: false,
            static_initialized: false,
            matrix_dirty: true,
            last_timestep: f64::NAN,
            #[cfg(test)]
            matrix_stamp_count: 0,
        })
    }

    fn solve_current_values(&mut self) -> Result<(), IslandRuntimeError> {
        if self.matrix_dirty {
            {
                let mut matrix = self.system.values_mut();

                matrix.clear();

                self.ir
                    .matrix_program()
                    .execute(&mut matrix, self.workspace.values());
            }

            self.system.factorize()?;

            self.matrix_dirty = false;

            #[cfg(test)]
            {
                self.matrix_stamp_count += 1;
            }
        }

        debug_assert!(
            self.system.is_factorized(),
            "clean island matrix must remain factorized",
        );

        self.ir
            .rhs_program()
            .execute(&mut self.solution, self.workspace.values());

        self.system.solve_in_place(&mut self.solution)?;

        for &(unknown, input) in self.ir.solution_inputs() {
            self.workspace
                .set_input(input, self.solution[unknown.index()]);
        }

        self.ir
            .value_program()
            .execute_iteration(&mut self.workspace);

        self.solution_valid = true;

        Ok(())
    }

    pub(crate) fn solve_tick<F>(
        &mut self,
        network: &Network,
        timestep: f64,
        mut old_state: F,
    ) -> Result<Vec<StagedStateWrite>, IslandRuntimeError>
    where
        F: FnMut(DeviceState) -> Option<f64>,
    {
        self.solution_valid = false;

        self.prepare_static(network, timestep)?;

        for &(slot, input) in self.ir.state_inputs() {
            let state = self
                .states
                .device_state(slot)
                .expect("compiled state input must have a physical state");

            let value = old_state(state).ok_or(IslandRuntimeError::MissingState {
                device: state.device(),
                state: state.state(),
            })?;

            self.workspace.set_input(input, value);
        }

        self.ir.value_program().execute_tick(&mut self.workspace);

        self.solve_current_values()?;

        let mut next_state = vec![0.0; self.states.state_count()];

        self.ir
            .state_transition()
            .execute(&mut next_state, self.workspace.values());

        let mut writes = Vec::with_capacity(self.ir.state_transition().len());

        for write in self.ir.state_transition().writes() {
            let slot = write.destination();

            let state = self
                .states
                .device_state(slot)
                .expect("compiled state write must have a physical state");

            writes.push(StagedStateWrite::new(state, next_state[slot.index()]));
        }

        Ok(writes)
    }

    fn load_parameters(&mut self, network: &Network) -> Result<bool, IslandRuntimeError> {
        let mut changed = false;

        for partition in &self.partition_inputs {
            let device = partition.device();

            let device_slot = network
                .devices()
                .get(device.index())
                .and_then(Option::as_ref)
                .ok_or(IslandRuntimeError::MissingDevice { device })?;

            let parameter_ids = partition.definition_parameters();

            let parameter_inputs = partition.inputs().parameters();

            debug_assert_eq!(parameter_ids.len(), parameter_inputs.len(),);

            for (&parameter, &input) in parameter_ids.iter().zip(parameter_inputs) {
                let value = device_slot
                    .parameters()
                    .get(parameter.index())
                    .copied()
                    .flatten()
                    .ok_or(IslandRuntimeError::MissingParameter { device, parameter })?;

                if self.workspace.value(input.value()) == value {
                    continue;
                }

                self.workspace.set_input(input, value);

                changed = true;
            }
        }

        Ok(changed)
    }

    fn prepare_static(
        &mut self,
        network: &Network,
        timestep: f64,
    ) -> Result<(), IslandRuntimeError> {
        let mut changed = self.load_parameters(network)?;

        if let Some(input) = self.ir.timestep_input() {
            if !timestep.is_finite() || timestep <= 0.0 {
                return Err(IslandRuntimeError::InvalidTimestep);
            }

            if self.last_timestep != timestep {
                self.workspace.set_input(input, timestep);

                self.last_timestep = timestep;
                changed = true;
            }
        }

        if !self.static_initialized || changed {
            self.ir.value_program().execute_static(&mut self.workspace);

            self.static_initialized = true;
            self.matrix_dirty = true;
        }

        Ok(())
    }

    pub(crate) fn node_voltage(&self, node: IslandNode) -> Option<f64> {
        if !self.solution_valid {
            return None;
        }

        Some(match self.unknowns.node_unknown(node) {
            None => 0.0,

            Some(unknown) => self.solution[unknown.index()],
        })
    }

    #[cfg(test)]
    #[inline]
    fn matrix_stamp_count(&self) -> usize {
        self.matrix_stamp_count
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct StagedStateWrite {
    state: DeviceState,
    value: f64,
}

impl StagedStateWrite {
    #[inline]
    pub(crate) const fn new(state: DeviceState, value: f64) -> Self {
        Self { state, value }
    }

    #[inline]
    pub(crate) const fn state(self) -> DeviceState {
        self.state
    }

    #[inline]
    pub(crate) const fn value(self) -> f64 {
        self.value
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::compile::definition::CompiledDefinition;
    use crate::topology::{DeviceComponent, NetId};
    use hynergy_mna::pattern::{PatternBuilder, UnknownIndex};
    use hynergy_model::device::definition::{
        DefinitionId, DevicePartitionId, PrimitiveElementKind,
    };
    use hynergy_model::device::registry::DefinitionRegistry;
    use hynergy_model::network::WireId;

    #[test]
    fn two_net_island_allocates_one_voltage_unknown() {
        let net_a = NetId::try_from(1).unwrap();
        let net_b = NetId::try_from(2).unwrap();

        let node_a = IslandNode::net(net_a);
        let node_b = IslandNode::net(net_b);

        let layout = IslandUnknownLayout::new(&[node_a, node_b]).unwrap();

        assert_eq!(layout.dimension(), 1);

        let a = layout.node_unknown(node_a);
        let b = layout.node_unknown(node_b);

        assert!(matches!(
            (a, b),
            (None, Some(unknown)) | (Some(unknown), None)
                if unknown == UnknownIndex::new(0)
        ));
    }

    #[test]
    fn conductance_partition_binds_terminal_nets_to_mna_unknowns() {
        let registry = DefinitionRegistry::new();

        let definition = registry
            .get(DefinitionId::from(PrimitiveElementKind::Conductance))
            .unwrap();

        let compiled = CompiledDefinition::compile(definition).unwrap();

        let partition = compiled.partition(DevicePartitionId::new(0)).unwrap();

        let net_a = NetId::try_from(1).unwrap();
        let net_b = NetId::try_from(2).unwrap();

        let node_a = IslandNode::net(net_a);
        let node_b = IslandNode::net(net_b);

        let layout = IslandUnknownLayout::new(&[node_a, node_b]).unwrap();

        let terminals = layout.bind_terminal_nodes(&[node_a, node_b]);

        assert_eq!(terminals.as_slice(), &[None, Some(UnknownIndex::new(0)),],);

        let template = partition.template();

        let mut unknown_allocator = UnknownAllocator::new(layout.dimension()).unwrap();

        let allocated = unknown_allocator
            .allocate(template.allocated_unknown_count())
            .unwrap();

        let bound = template.bind_unknowns(&terminals, allocated).unwrap();

        let mut pattern_builder = PatternBuilder::new(unknown_allocator.dimension()).unwrap();

        template
            .request_pattern(&bound, &mut pattern_builder)
            .unwrap();

        let pattern = pattern_builder.finish().unwrap();

        assert_eq!(pattern.dimension(), 1);
        assert_eq!(pattern.nnz(), 1);
        assert!(
            pattern
                .slot(UnknownIndex::new(0), UnknownIndex::new(0),)
                .is_some()
        );
    }

    #[test]
    fn conductance_partition_binds_all_unknowns() {
        let registry = DefinitionRegistry::new();

        let definition = registry
            .get(DefinitionId::from(PrimitiveElementKind::Conductance))
            .unwrap();

        let compiled = CompiledDefinition::compile(definition).unwrap();

        let partition = compiled.partition(DevicePartitionId::new(0)).unwrap();

        let template = partition.template();

        let net_a = NetId::try_from(1).unwrap();
        let net_b = NetId::try_from(2).unwrap();

        let node_a = IslandNode::net(net_a);
        let node_b = IslandNode::net(net_b);

        let layout = IslandUnknownLayout::new(&[node_a, node_b]).unwrap();

        let mut allocator = UnknownAllocator::new(layout.dimension()).unwrap();

        let bound =
            bind_partition_unknowns(&layout, template, &[node_a, node_b], &mut allocator).unwrap();

        assert_eq!(allocator.dimension(), 1);

        let mut pattern = PatternBuilder::new(allocator.dimension()).unwrap();

        template.request_pattern(&bound, &mut pattern).unwrap();

        let pattern = pattern.finish().unwrap();

        assert_eq!(pattern.dimension(), 1);
        assert_eq!(pattern.nnz(), 1);

        assert!(
            pattern
                .slot(UnknownIndex::new(0), UnknownIndex::new(0),)
                .is_some()
        );
    }

    #[test]
    fn voltage_source_auxiliary_follows_node_voltage() {
        let registry = DefinitionRegistry::new();

        let definition = registry
            .get(DefinitionId::from(PrimitiveElementKind::VoltageSource))
            .unwrap();

        let compiled = CompiledDefinition::compile(definition).unwrap();

        let template = compiled
            .partition(DevicePartitionId::new(0))
            .unwrap()
            .template();

        let net_a = NetId::try_from(1).unwrap();
        let net_b = NetId::try_from(2).unwrap();

        let node_a = IslandNode::net(net_a);
        let node_b = IslandNode::net(net_b);

        let layout = IslandUnknownLayout::new(&[node_a, node_b]).unwrap();

        let mut allocator = UnknownAllocator::new(layout.dimension()).unwrap();

        let bound =
            bind_partition_unknowns(&layout, template, &[node_a, node_b], &mut allocator).unwrap();

        assert_eq!(allocator.dimension(), 2);

        let mut pattern = PatternBuilder::new(allocator.dimension()).unwrap();

        template.request_pattern(&bound, &mut pattern).unwrap();

        let pattern = pattern.finish().unwrap();

        assert_eq!(pattern.dimension(), 2);
        assert_eq!(pattern.nnz(), 2);

        assert!(
            pattern
                .slot(UnknownIndex::new(0), UnknownIndex::new(1),)
                .is_some()
        );

        assert!(
            pattern
                .slot(UnknownIndex::new(1), UnknownIndex::new(0),)
                .is_some()
        );
    }

    #[test]
    fn island_pattern_uses_final_unknown_dimension() {
        let registry = DefinitionRegistry::new();

        let definition = registry
            .get(DefinitionId::from(PrimitiveElementKind::VoltageSource))
            .unwrap();

        let compiled = CompiledDefinition::compile(definition).unwrap();

        let template = compiled
            .partition(DevicePartitionId::new(0))
            .unwrap()
            .template();

        let net_a = NetId::try_from(1).unwrap();
        let net_b = NetId::try_from(2).unwrap();

        let node_a = IslandNode::net(net_a);
        let node_b = IslandNode::net(net_b);

        let layout = IslandUnknownLayout::new(&[node_a, node_b]).unwrap();

        let mut allocator = UnknownAllocator::new(layout.dimension()).unwrap();

        let bound =
            bind_partition_unknowns(&layout, template, &[node_a, node_b], &mut allocator).unwrap();

        let pattern = build_island_pattern(allocator.dimension(), &[(template, &bound)]).unwrap();

        assert_eq!(pattern.dimension(), 2);
        assert_eq!(pattern.nnz(), 2);
    }

    #[test]
    fn disconnected_terminals_are_distinct_island_nodes() {
        let device = DeviceId::try_from(1).unwrap();

        let positive = IslandNode::terminal(device, TerminalId::new(0));

        let negative = IslandNode::terminal(device, TerminalId::new(1));

        let layout = IslandUnknownLayout::new(&[positive, negative]).unwrap();

        assert_eq!(layout.dimension(), 1);

        assert_eq!(layout.node_unknown(positive), None,);

        assert_eq!(layout.node_unknown(negative), Some(UnknownIndex::new(0)),);
    }

    #[test]
    fn conductance_parts_compile_into_island_ir() {
        let registry = DefinitionRegistry::new();

        let definition = registry
            .get(DefinitionId::from(PrimitiveElementKind::Conductance))
            .unwrap();

        let compiled_definition = CompiledDefinition::compile(definition).unwrap();

        let partition = compiled_definition
            .partition(DevicePartitionId::new(0))
            .unwrap();

        let device = DeviceId::try_from(1).unwrap();

        let node_a = IslandNode::terminal(device, TerminalId::new(0));

        let node_b = IslandNode::terminal(device, TerminalId::new(1));

        let terminal_nodes = [node_a, node_b];

        let parts = [IslandPartitionSpec::new(device, partition, &terminal_nodes)];

        let island = compile_island_parts(&[node_a, node_b], &parts).unwrap();

        assert_eq!(island.pattern().dimension(), 1,);

        assert_eq!(island.pattern().nnz(), 1,);

        assert_eq!(island.ir().matrix_program().len(), 1,);

        assert_eq!(island.state_count(), 0);
    }

    #[test]
    fn topology_conductance_compiles_into_island() {
        let definitions = DefinitionRegistry::new();

        let mut network = Network::new();

        let wire_a = WireId::try_from(1).unwrap();

        let wire_b = WireId::try_from(2).unwrap();

        let device = DeviceId::try_from(1).unwrap();

        network.add_wire(wire_a).unwrap();

        network.add_wire(wire_b).unwrap();

        network
            .add_device(
                &definitions,
                device,
                DefinitionId::from(PrimitiveElementKind::Conductance),
            )
            .unwrap();

        network
            .attach_terminal(wire_a, device, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(wire_b, device, TerminalId::new(1))
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let component = DeviceComponent::new(device, DevicePartitionId::new(0));

        let island_id = topology.component_island(component);

        let island = compile_topology_island(&definitions, &network, &topology, island_id).unwrap();

        assert_eq!(island.pattern().dimension(), 1,);

        assert_eq!(island.pattern().nnz(), 1,);

        assert_eq!(island.ir().matrix_program().len(), 1,);

        assert_eq!(island.partition_inputs().len(), 1,);

        assert_eq!(island.partition_inputs()[0].device(), device,);

        assert_eq!(island.state_count(), 0,);
    }

    #[test]
    fn voltage_source_and_conductance_solve_node_voltage() {
        let definitions = DefinitionRegistry::new();

        let mut network = Network::new();

        let wire_negative = WireId::try_from(1).unwrap();

        let wire_positive = WireId::try_from(2).unwrap();

        let source = DeviceId::try_from(1).unwrap();

        let conductance = DeviceId::try_from(2).unwrap();

        network.add_wire(wire_negative).unwrap();

        network.add_wire(wire_positive).unwrap();

        network
            .add_device(
                &definitions,
                source,
                DefinitionId::from(PrimitiveElementKind::VoltageSource),
            )
            .unwrap();

        network
            .add_device(
                &definitions,
                conductance,
                DefinitionId::from(PrimitiveElementKind::Conductance),
            )
            .unwrap();

        network
            .attach_terminal(wire_positive, source, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(wire_negative, source, TerminalId::new(1))
            .unwrap();

        network
            .attach_terminal(wire_positive, conductance, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(wire_negative, conductance, TerminalId::new(1))
            .unwrap();

        network
            .set_device_parameter(&definitions, source, ParameterId::new(0), 5.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, conductance, ParameterId::new(0), 2.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island_id =
            topology.component_island(DeviceComponent::new(source, DevicePartitionId::new(0)));

        let positive = IslandNode::net(topology.wire_net(wire_positive));

        let negative = IslandNode::net(topology.wire_net(wire_negative));

        let compiled =
            compile_topology_island(&definitions, &network, &topology, island_id).unwrap();

        let mut runtime = IslandRuntime::new(compiled).unwrap();

        let writes = runtime.solve_tick(&network, 1.0, |_| None).unwrap();

        assert!(writes.is_empty());

        let positive_voltage = runtime.node_voltage(positive).unwrap();

        let negative_voltage = runtime.node_voltage(negative).unwrap();

        assert!((positive_voltage - negative_voltage - 5.0).abs() < 1.0e-12);
    }

    #[test]
    fn capacitor_tick_reads_old_state_and_stages_next_state() {
        let definitions = DefinitionRegistry::new();
        let mut network = Network::new();

        let wire_a = WireId::try_from(1).unwrap();
        let wire_b = WireId::try_from(2).unwrap();

        let conductance = DeviceId::try_from(1).unwrap();
        let capacitor = DeviceId::try_from(2).unwrap();
        let source = DeviceId::try_from(3).unwrap();

        network.add_wire(wire_a).unwrap();
        network.add_wire(wire_b).unwrap();

        network
            .add_device(
                &definitions,
                conductance,
                DefinitionId::from(PrimitiveElementKind::Conductance),
            )
            .unwrap();

        network
            .add_device(
                &definitions,
                capacitor,
                DefinitionId::from(PrimitiveElementKind::Capacitor),
            )
            .unwrap();

        network
            .add_device(
                &definitions,
                source,
                DefinitionId::from(PrimitiveElementKind::CurrentSource),
            )
            .unwrap();

        network
            .attach_terminal(wire_a, conductance, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(wire_b, conductance, TerminalId::new(1))
            .unwrap();

        network
            .attach_terminal(wire_a, capacitor, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(wire_b, capacitor, TerminalId::new(1))
            .unwrap();

        // Current flows from B to A.
        network
            .attach_terminal(wire_b, source, TerminalId::new(0))
            .unwrap();

        network
            .attach_terminal(wire_a, source, TerminalId::new(1))
            .unwrap();

        network
            .set_device_parameter(&definitions, conductance, ParameterId::new(0), 1.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, capacitor, ParameterId::new(0), 2.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, source, ParameterId::new(0), 2.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island_id =
            topology.component_island(DeviceComponent::new(capacitor, DevicePartitionId::new(0)));

        let node_a = IslandNode::net(topology.wire_net(wire_a));

        let node_b = IslandNode::net(topology.wire_net(wire_b));

        let compiled =
            compile_topology_island(&definitions, &network, &topology, island_id).unwrap();

        let mut runtime = IslandRuntime::new(compiled).unwrap();

        let capacitor_state = DeviceState::new(capacitor, DefinitionStateId::new(0));

        let writes = runtime
            .solve_tick(&network, 0.5, |state| {
                (state == capacitor_state).then_some(3.0)
            })
            .unwrap();

        let voltage = runtime.node_voltage(node_a).unwrap() - runtime.node_voltage(node_b).unwrap();

        assert!((voltage - 2.8).abs() < 1.0e-12);

        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].state(), capacitor_state,);
        assert!((writes[0].value() - 2.8).abs() < 1.0e-12);
    }

    #[test]
    fn unchanged_static_inputs_reuse_matrix_factorization() {
        let definitions = DefinitionRegistry::new();

        let mut network = Network::new();

        let negative = WireId::try_from(1).unwrap();

        let positive = WireId::try_from(2).unwrap();

        let source = DeviceId::try_from(1).unwrap();

        let conductance = DeviceId::try_from(2).unwrap();

        network.add_wire(negative).unwrap();

        network.add_wire(positive).unwrap();

        network
            .add_device(
                &definitions,
                source,
                PrimitiveElementKind::VoltageSource.into(),
            )
            .unwrap();

        network
            .add_device(
                &definitions,
                conductance,
                PrimitiveElementKind::Conductance.into(),
            )
            .unwrap();

        for device in [source, conductance] {
            network
                .attach_terminal(positive, device, TerminalId::new(0))
                .unwrap();

            network
                .attach_terminal(negative, device, TerminalId::new(1))
                .unwrap();
        }

        network
            .set_device_parameter(&definitions, source, ParameterId::new(0), 5.0)
            .unwrap();

        network
            .set_device_parameter(&definitions, conductance, ParameterId::new(0), 1.0)
            .unwrap();

        let topology = DerivedTopology::from_network(&network, &definitions);

        let island =
            topology.component_island(DeviceComponent::new(source, DevicePartitionId::new(0)));

        let compiled = compile_topology_island(&definitions, &network, &topology, island).unwrap();

        let mut runtime = IslandRuntime::new(compiled).unwrap();

        let writes = runtime.solve_tick(&network, 1.0, |_| None).unwrap();

        assert!(writes.is_empty());

        assert_eq!(runtime.matrix_stamp_count(), 1,);

        let writes = runtime.solve_tick(&network, 1.0, |_| None).unwrap();

        assert!(writes.is_empty());

        assert_eq!(runtime.matrix_stamp_count(), 1,);

        network
            .set_device_parameter(&definitions, conductance, ParameterId::new(0), 2.0)
            .unwrap();

        let writes = runtime.solve_tick(&network, 1.0, |_| None).unwrap();

        assert!(writes.is_empty());

        assert_eq!(runtime.matrix_stamp_count(), 2,);
    }
}
