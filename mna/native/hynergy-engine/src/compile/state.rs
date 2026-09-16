use hynergy_ir::StateSlot;
use smallvec::SmallVec;
use thiserror::Error;

pub(crate) const MAX_STATE_COUNT: usize = u32::MAX as usize;

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StateAllocationError {
    #[error("requested state count {requested} exceeds maximum {max}")]
    StateCountTooLarge { requested: usize, max: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoundStateSlots {
    slots: SmallVec<[StateSlot; 4]>,
}

impl BoundStateSlots {
    #[inline]
    pub(crate) fn new(slots: SmallVec<[StateSlot; 4]>) -> Self {
        Self { slots }
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }

    #[inline]
    pub(crate) fn get(&self, index: usize) -> Option<StateSlot> {
        self.slots.get(index).copied()
    }
}
