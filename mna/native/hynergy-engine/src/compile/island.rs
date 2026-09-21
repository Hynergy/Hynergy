use crate::compile::definition::{
    CompiledDefinition, CompiledPartitionTemplate, DefinitionCompileError, DefinitionStateId,
};
use crate::compile::island_ir::{CompiledIslandIr, IslandIrBuildError, IslandIrBuilder};
use crate::compile::state::{BoundStateSlots, StateAllocationError};
use crate::compile::template::{BoundUnknowns, CompiledDefinitionTemplate, DefinitionLinkError};
use crate::compile::unknown::{UnknownAllocationError, UnknownAllocator};
use crate::topology::{DerivedTopology, IslandId, NetId};
use hynergy_ir::{InputSlot, StateSlot, ValueSlot};
use hynergy_mna::pattern::{MnaPattern, PatternBuilder, UnknownIndex};
use hynergy_model::device::definition::{DefinitionId, DefinitionObserverId, DeviceId, TerminalId};
use hynergy_model::device::registry::DefinitionRegistry;
use hynergy_model::network::Network;
use hynergy_model::parameter::ParameterId;
use smallvec::SmallVec;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct DeviceObserver {
    device: DeviceId,
    observer: DefinitionObserverId,
}

impl DeviceObserver {
    #[inline]
    pub(crate) const fn new(device: DeviceId, observer: DefinitionObserverId) -> Self {
        Self { device, observer }
    }

    #[inline]
    pub(crate) const fn device(self) -> DeviceId {
        self.device
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CompiledObserverOutput {
    observer: DeviceObserver,
    value: ValueSlot,
}

impl CompiledObserverOutput {
    #[inline]
    const fn new(observer: DeviceObserver, value: ValueSlot) -> Self {
        Self { observer, value }
    }

    #[inline]
    pub(crate) const fn observer(self) -> DeviceObserver {
        self.observer
    }

    #[inline]
    pub(crate) const fn value(self) -> ValueSlot {
        self.value
    }
}

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
    parameter_inputs: Box<[InputSlot]>,
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
    pub(crate) fn parameter_inputs(&self) -> &[InputSlot] {
        &self.parameter_inputs
    }
}

#[derive(Debug, Error, PartialEq)]
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
    states: IslandStateLayout,
    partition_inputs: Box<[CompiledPartitionInputs]>,
    observer_outputs: Box<[CompiledObserverOutput]>,
    #[cfg(test)]
    unknowns: IslandUnknownLayout,
}

#[derive(Debug)]
pub(crate) struct CompiledIslandParts {
    pub(crate) pattern: MnaPattern,
    pub(crate) ir: CompiledIslandIr,
    pub(crate) states: IslandStateLayout,
    pub(crate) partition_inputs: Box<[CompiledPartitionInputs]>,
    pub(crate) observer_outputs: Box<[CompiledObserverOutput]>,
    #[cfg(test)]
    pub(crate) unknowns: IslandUnknownLayout,
}

impl CompiledIsland {
    #[inline]
    pub(crate) fn into_parts(self) -> CompiledIslandParts {
        CompiledIslandParts {
            pattern: self.pattern,
            ir: self.ir,
            #[cfg(test)]
            unknowns: self.unknowns,
            states: self.states,
            partition_inputs: self.partition_inputs,
            observer_outputs: self.observer_outputs,
        }
    }
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

    #[inline]
    pub(crate) fn force_nonlinear_iteration_for_test(&mut self, affects_matrix: bool) {
        self.ir.force_nonlinear_iteration_for_test(affects_matrix);
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
    let mut observer_outputs = Vec::new();

    for (partition, spec) in bound_partitions.iter().zip(partitions) {
        let inputs =
            partition
                .template
                .bind(&partition.unknowns, &partition.states, &mut ir_builder)?;

        let (parameter_inputs, outputs) = inputs.into_parts();
        let definition_observers = spec.partition.definition_observers();

        debug_assert_eq!(
            definition_observers.len(),
            outputs.len(),
            "compiled observer IDs must match template outputs",
        );

        for (&observer, &value) in definition_observers.iter().zip(&outputs) {
            observer_outputs.push(CompiledObserverOutput::new(
                DeviceObserver::new(partition.device, observer),
                value,
            ));
        }

        partition_inputs.push(CompiledPartitionInputs {
            device: partition.device,

            definition_parameters: SmallVec::from_slice(spec.partition.definition_parameters()),

            parameter_inputs,
        });
    }

    observer_outputs.sort_unstable_by_key(|output| output.observer());

    debug_assert!(
        observer_outputs
            .windows(2)
            .all(|outputs| outputs[0].observer() != outputs[1].observer()),
        "device observer must have exactly one island output",
    );

    let ir = ir_builder.finish()?;

    Ok(CompiledIsland {
        pattern,
        ir,
        #[cfg(test)]
        unknowns: unknown_layout,
        states: state_layout,
        partition_inputs: partition_inputs.into_boxed_slice(),
        observer_outputs: observer_outputs.into_boxed_slice(),
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
    let device_view = network
        .device(device)
        .expect("island component device must exist");

    let connection = device_view
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

            let compiled = CompiledDefinition::compile(definitions, definition)?;

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

#[cfg(test)]
mod test {
    use crate::compile::definition::CompiledDefinition;
    use crate::compile::island::{
        IslandNode, IslandPartitionSpec, IslandUnknownLayout, bind_partition_unknowns,
        build_island_pattern, compile_island_parts, compile_topology_island,
    };
    use crate::compile::unknown::UnknownAllocator;
    use crate::topology::{DerivedTopology, DeviceComponent, NetId};
    use hynergy_mna::pattern::{PatternBuilder, UnknownIndex};
    use hynergy_model::device::definition::{
        DefinitionId, DeviceId, DevicePartitionId, PrimitiveElementKind, TerminalId,
    };
    use hynergy_model::device::registry::DefinitionRegistry;
    use hynergy_model::network::{Network, WireId};

    #[test]
    fn two_net_island_allocates_one_voltage_unknown() {
        let net_a = NetId::try_from(1).unwrap();

        let net_b = NetId::try_from(2).unwrap();

        let node_a = IslandNode::net(net_a);

        let node_b = IslandNode::net(net_b);

        let layout = IslandUnknownLayout::new(&[node_a, node_b]).unwrap();

        assert_eq!(layout.dimension(), 1,);

        let a = layout.node_unknown(node_a);

        let b = layout.node_unknown(node_b);

        assert!(matches!(
            (a, b),
            (
                None,
                Some(unknown)
            )
                | (
                    Some(unknown),
                    None
                )
                if unknown
                    == UnknownIndex::new(0)
        ));
    }

    #[test]
    fn conductance_partition_binds_terminal_nets_to_mna_unknowns() {
        let registry = DefinitionRegistry::new();

        let definition = registry
            .get(DefinitionId::from(PrimitiveElementKind::Conductance))
            .unwrap();

        let compiled = CompiledDefinition::compile(&registry, definition).unwrap();

        let partition = compiled.partition(DevicePartitionId::new(0)).unwrap();

        let net_a = NetId::try_from(1).unwrap();

        let net_b = NetId::try_from(2).unwrap();

        let node_a = IslandNode::net(net_a);

        let node_b = IslandNode::net(net_b);

        let layout = IslandUnknownLayout::new(&[node_a, node_b]).unwrap();

        let terminals = layout.bind_terminal_nodes(&[node_a, node_b]);

        assert_eq!(terminals.as_slice(), &[None, Some(UnknownIndex::new(0),),],);

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

        assert_eq!(pattern.dimension(), 1,);

        assert_eq!(pattern.nnz(), 1,);

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

        let compiled = CompiledDefinition::compile(&registry, definition).unwrap();

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

        assert_eq!(allocator.dimension(), 1,);

        let mut pattern = PatternBuilder::new(allocator.dimension()).unwrap();

        template.request_pattern(&bound, &mut pattern).unwrap();

        let pattern = pattern.finish().unwrap();

        assert_eq!(pattern.dimension(), 1,);

        assert_eq!(pattern.nnz(), 1,);

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

        let compiled = CompiledDefinition::compile(&registry, definition).unwrap();

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

        assert_eq!(allocator.dimension(), 2,);

        let mut pattern = PatternBuilder::new(allocator.dimension()).unwrap();

        template.request_pattern(&bound, &mut pattern).unwrap();

        let pattern = pattern.finish().unwrap();

        assert_eq!(pattern.dimension(), 2,);

        assert_eq!(pattern.nnz(), 2,);

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

        let compiled = CompiledDefinition::compile(&registry, definition).unwrap();

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

        assert_eq!(pattern.dimension(), 2,);

        assert_eq!(pattern.nnz(), 2,);
    }

    #[test]
    fn disconnected_terminals_are_distinct_island_nodes() {
        let device = DeviceId::try_from(1).unwrap();

        let positive = IslandNode::terminal(device, TerminalId::new(0));

        let negative = IslandNode::terminal(device, TerminalId::new(1));

        let layout = IslandUnknownLayout::new(&[positive, negative]).unwrap();

        assert_eq!(layout.dimension(), 1,);

        assert_eq!(layout.node_unknown(positive,), None,);

        assert_eq!(layout.node_unknown(negative,), Some(UnknownIndex::new(0),),);
    }

    #[test]
    fn conductance_parts_compile_into_island_ir() {
        let registry = DefinitionRegistry::new();

        let definition = registry
            .get(DefinitionId::from(PrimitiveElementKind::Conductance))
            .unwrap();

        let compiled_definition = CompiledDefinition::compile(&registry, definition).unwrap();

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

        assert_eq!(island.state_count(), 0,);
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
}
