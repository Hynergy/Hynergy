use hynergy_mna::pattern::{PatternBuilder, UnknownIndex};
use thiserror::Error;

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnknownAllocationError {
    #[error("requested MNA dimension {requested} exceeds maximum {max}")]
    DimensionTooLarge { requested: usize, max: usize },
}

#[derive(Debug)]
pub(crate) struct UnknownAllocator {
    next: usize,
}

impl UnknownAllocator {
    pub(crate) fn new(node_voltage_count: usize) -> Result<Self, UnknownAllocationError> {
        if node_voltage_count > PatternBuilder::MAX_DIMENSION {
            return Err(UnknownAllocationError::DimensionTooLarge {
                requested: node_voltage_count,
                max: PatternBuilder::MAX_DIMENSION,
            });
        }

        Ok(Self {
            next: node_voltage_count,
        })
    }

    pub(crate) fn allocate(
        &mut self,
        count: usize,
    ) -> Result<UnknownRange, UnknownAllocationError> {
        let end =
            self.next
                .checked_add(count)
                .ok_or(UnknownAllocationError::DimensionTooLarge {
                    requested: usize::MAX,
                    max: PatternBuilder::MAX_DIMENSION,
                })?;

        if end > PatternBuilder::MAX_DIMENSION {
            return Err(UnknownAllocationError::DimensionTooLarge {
                requested: end,
                max: PatternBuilder::MAX_DIMENSION,
            });
        }

        let start = self.next;
        self.next = end;

        Ok(UnknownRange {
            start: start as u32,
            len: count as u32,
        })
    }

    #[inline]
    pub(crate) const fn dimension(&self) -> usize {
        self.next
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnknownRange {
    start: u32,
    len: u32,
}

impl UnknownRange {
    #[inline]
    pub(crate) const fn len(self) -> usize {
        self.len as usize
    }

    #[inline]
    pub(crate) const fn is_empty(self) -> bool {
        self.len == 0
    }

    #[inline]
    pub(crate) fn get(self, index: usize) -> Option<UnknownIndex> {
        if index >= self.len() {
            return None;
        }

        Some(UnknownIndex::new(self.start + index as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auxiliaries_follow_node_voltages() {
        let mut allocator = UnknownAllocator::new(3).unwrap();

        let first = allocator.allocate(2).unwrap();
        let second = allocator.allocate(1).unwrap();

        assert_eq!(first.get(0), Some(UnknownIndex::new(3)));
        assert_eq!(first.get(1), Some(UnknownIndex::new(4)));
        assert_eq!(first.get(2), None);

        assert_eq!(second.get(0), Some(UnknownIndex::new(5)));

        assert_eq!(allocator.dimension(), 6);
    }

    #[test]
    fn zero_length_allocation_does_not_advance_dimension() {
        let mut allocator = UnknownAllocator::new(2).unwrap();

        let range = allocator.allocate(0).unwrap();

        assert!(range.is_empty());
        assert_eq!(allocator.dimension(), 2);
    }
}
