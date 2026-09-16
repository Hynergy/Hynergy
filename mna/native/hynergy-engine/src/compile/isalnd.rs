use crate::compile::CompiledDefinitionTemplate;
use crate::compile::definition::DefinitionStateId;
use crate::compile::state::{BoundStateSlots, StateAllocationError, StateAllocator};
use crate::compile::template::{BoundUnknowns, DefinitionLinkError};
use crate::compile::unknown::{UnknownAllocationError, UnknownAllocator};
use crate::topology::NetId;
use hynergy_ir::StateSlot;
use hynergy_mna::pattern::{MnaPattern, PatternBuilder, UnknownIndex};
use hynergy_model::device::definition::DeviceId;
use smallvec::SmallVec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IslandUnknownLayout {
    nets: SmallVec<[NetId; 4]>,
}

impl IslandUnknownLayout {
    pub(crate) fn new(nets: &[NetId]) -> Result<Self, UnknownAllocationError> {
        let dimension = nets.len().saturating_sub(1);

        UnknownAllocator::new(dimension)?;

        #[cfg(debug_assertions)]
        {
            for (index, &net) in nets.iter().enumerate() {
                debug_assert!(
                    !nets[..index].contains(&net),
                    "island net list must not contain duplicates",
                );
            }
        }

        Ok(Self {
            nets: SmallVec::from_slice(nets),
        })
    }

    #[inline]
    pub(crate) fn dimension(&self) -> usize {
        self.nets.len().saturating_sub(1)
    }

    #[inline]
    pub(crate) fn net_unknown(&self, net: NetId) -> Option<UnknownIndex> {
        let position = self
            .nets
            .iter()
            .position(|&candidate| candidate == net)
            .expect("island net must belong to unknown layout");

        if position == 0 {
            return None;
        }

        Some(UnknownIndex::new(
            u32::try_from(position - 1).expect("island voltage index must fit UnknownIndex"),
        ))
    }

    #[inline]
    pub(crate) fn reference_net(&self) -> Option<NetId> {
        self.nets.first().copied()
    }

    pub(crate) fn bind_terminal_nets(&self, nets: &[NetId]) -> SmallVec<[Option<UnknownIndex>; 4]> {
        nets.iter().map(|&net| self.net_unknown(net)).collect()
    }
}

pub(crate) fn bind_partition_unknowns(
    layout: &IslandUnknownLayout,
    template: &CompiledDefinitionTemplate,
    terminal_nets: &[NetId],
    allocator: &mut UnknownAllocator,
) -> Result<BoundUnknowns, DefinitionLinkError> {
    let terminals = layout.bind_terminal_nets(terminal_nets);

    let allocated = allocator
        .allocate(template.allocated_unknown_count())
        .map_err(DefinitionLinkError::from)?;

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
                if self.states.len() >= StateAllocator::MAX_STATE_COUNT {
                    return Err(StateAllocationError::StateCountTooLarge {
                        requested: self.states.len() + 1,
                        max: StateAllocator::MAX_STATE_COUNT,
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::compile::definition::CompiledDefinition;
    use crate::topology::NetId;
    use hynergy_mna::pattern::{PatternBuilder, UnknownIndex};
    use hynergy_model::device::definition::{
        DefinitionId, DevicePartitionId, PrimitiveElementKind,
    };
    use hynergy_model::device::registry::DefinitionRegistry;

    #[test]
    fn two_net_island_allocates_one_voltage_unknown() {
        let net_a = NetId::try_from(1).unwrap();
        let net_b = NetId::try_from(2).unwrap();

        let layout = IslandUnknownLayout::new(&[net_a, net_b]).unwrap();

        assert_eq!(layout.dimension(), 1);

        let a = layout.net_unknown(net_a);
        let b = layout.net_unknown(net_b);

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

        let layout = IslandUnknownLayout::new(&[net_a, net_b]).unwrap();

        let terminals = layout.bind_terminal_nets(&[net_a, net_b]);

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

        let layout = IslandUnknownLayout::new(&[net_a, net_b]).unwrap();

        let mut allocator = UnknownAllocator::new(layout.dimension()).unwrap();

        let bound =
            bind_partition_unknowns(&layout, template, &[net_a, net_b], &mut allocator).unwrap();

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

        let layout = IslandUnknownLayout::new(&[net_a, net_b]).unwrap();

        let mut allocator = UnknownAllocator::new(layout.dimension()).unwrap();

        let bound =
            bind_partition_unknowns(&layout, template, &[net_a, net_b], &mut allocator).unwrap();

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

        let layout = IslandUnknownLayout::new(&[net_a, net_b]).unwrap();

        let mut allocator = UnknownAllocator::new(layout.dimension()).unwrap();

        let bound =
            bind_partition_unknowns(&layout, template, &[net_a, net_b], &mut allocator).unwrap();

        let pattern = build_island_pattern(allocator.dimension(), &[(template, &bound)]).unwrap();

        assert_eq!(pattern.dimension(), 2);
        assert_eq!(pattern.nnz(), 2);
    }
}
