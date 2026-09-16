use crate::compile::unknown::{UnknownAllocationError, UnknownAllocator};
use crate::topology::NetId;
use hynergy_mna::pattern::UnknownIndex;
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
}
