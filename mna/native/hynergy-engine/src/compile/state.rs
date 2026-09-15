use hynergy_ir::StateSlot;
use thiserror::Error;

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StateAllocationError {
    #[error("requested state count {requested} exceeds maximum {max}")]
    StateCountTooLarge { requested: usize, max: usize },
}

#[derive(Debug, Default)]
pub(crate) struct StateAllocator {
    next: usize,
}

impl StateAllocator {
    pub(crate) const MAX_STATE_COUNT: usize = u32::MAX as usize;

    #[inline]
    pub(crate) const fn new() -> Self {
        Self { next: 0 }
    }

    pub(crate) fn allocate(&mut self, count: usize) -> Result<StateRange, StateAllocationError> {
        let end = self
            .next
            .checked_add(count)
            .ok_or(StateAllocationError::StateCountTooLarge {
                requested: usize::MAX,
                max: Self::MAX_STATE_COUNT,
            })?;

        if end > Self::MAX_STATE_COUNT {
            return Err(StateAllocationError::StateCountTooLarge {
                requested: end,
                max: Self::MAX_STATE_COUNT,
            });
        }

        let range = StateRange {
            start: self.next as u32,
            len: count as u32,
        };

        self.next = end;

        Ok(range)
    }

    #[inline]
    pub(crate) const fn state_count(&self) -> usize {
        self.next
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StateRange {
    start: u32,
    len: u32,
}

impl StateRange {
    #[inline]
    pub(crate) const fn len(self) -> usize {
        self.len as usize
    }

    #[inline]
    pub(crate) const fn is_empty(self) -> bool {
        self.len == 0
    }

    #[inline]
    pub(crate) fn get(self, index: usize) -> Option<StateSlot> {
        if index >= self.len() {
            return None;
        }

        Some(StateSlot::new(self.start + index as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_contiguous_state_ranges() {
        let mut allocator = StateAllocator::new();

        let first = allocator.allocate(2).unwrap();
        let second = allocator.allocate(1).unwrap();

        assert_eq!(first.get(0), Some(StateSlot::new(0)));
        assert_eq!(first.get(1), Some(StateSlot::new(1)));
        assert_eq!(first.get(2), None);

        assert_eq!(second.get(0), Some(StateSlot::new(2)));

        assert_eq!(allocator.state_count(), 3);
    }

    #[test]
    fn zero_length_allocation_does_not_advance() {
        let mut allocator = StateAllocator::new();

        let range = allocator.allocate(0).unwrap();

        assert!(range.is_empty());
        assert_eq!(allocator.state_count(), 0);
    }
}
