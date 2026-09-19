use std::fmt::Debug;
use std::num::NonZeroU32;

pub(super) trait DenseId: Copy + Eq + Debug {
    fn from_slot(slot: usize) -> Self;
    fn slot(self) -> usize;
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct DenseIdStore<I, T>
where
    I: DenseId,
{
    slots: Vec<Option<NonZeroU32>>,
    dense_ids: Vec<I>,
    values: Vec<T>,
}

impl<I, T> Default for DenseIdStore<I, T>
where
    I: DenseId,
{
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            dense_ids: Vec::new(),
            values: Vec::new(),
        }
    }
}

impl<I, T> DenseIdStore<I, T>
where
    I: DenseId,
{
    #[inline]
    #[allow(dead_code)]
    pub(super) fn len(&self) -> usize {
        self.values.len()
    }

    #[inline]
    pub(super) fn slot_count(&self) -> usize {
        self.slots.len()
    }

    #[inline]
    pub(super) fn insert(&mut self, value: T) -> I {
        let id = I::from_slot(self.slots.len());
        let dense_position = self
            .values
            .len()
            .checked_add(1)
            .and_then(|position| u32::try_from(position).ok())
            .and_then(NonZeroU32::new)
            .expect("dense topology store exceeded the u32 position range");

        self.slots.push(Some(dense_position));
        self.dense_ids.push(id);
        self.values.push(value);
        id
    }

    #[inline]
    pub(super) fn get(&self, id: I) -> Option<&T> {
        let dense = self.dense_index(id)?;
        self.values.get(dense)
    }

    #[inline]
    pub(super) fn get_mut(&mut self, id: I) -> Option<&mut T> {
        let dense = self.dense_index(id)?;
        self.values.get_mut(dense)
    }

    #[inline]
    pub(super) fn remove(&mut self, id: I) -> Option<T> {
        let dense = self.dense_index(id)?;
        self.slots[id.slot()] = None;

        let removed_id = self.dense_ids.swap_remove(dense);
        debug_assert_eq!(removed_id, id);
        let removed_value = self.values.swap_remove(dense);

        if dense < self.values.len() {
            let moved_id = self.dense_ids[dense];
            self.slots[moved_id.slot()] = Some(Self::encode_dense_index(dense));
        }

        Some(removed_value)
    }

    #[inline]
    pub(super) fn iter(&self) -> impl ExactSizeIterator<Item = (I, &T)> {
        self.dense_ids.iter().copied().zip(self.values.iter())
    }

    #[inline]
    fn dense_index(&self, id: I) -> Option<usize> {
        self.slots
            .get(id.slot())
            .copied()
            .flatten()
            .map(|position| position.get() as usize - 1)
    }

    #[inline]
    fn encode_dense_index(index: usize) -> NonZeroU32 {
        index
            .checked_add(1)
            .and_then(|position| u32::try_from(position).ok())
            .and_then(NonZeroU32::new)
            .expect("dense topology store exceeded the u32 position range")
    }
}

#[cfg(test)]
mod tests {
    use super::{DenseId, DenseIdStore};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct TestId(u32);

    impl DenseId for TestId {
        fn from_slot(slot: usize) -> Self {
            Self(u32::try_from(slot + 1).unwrap())
        }

        fn slot(self) -> usize {
            self.0 as usize - 1
        }
    }

    #[test]
    fn inserted_ids_resolve_to_dense_values() {
        let mut store: DenseIdStore<TestId, &str> = DenseIdStore::default();

        let a = store.insert("a");
        let b = store.insert("b");
        let c = store.insert("c");

        assert_eq!(store.get(a), Some(&"a"));
        assert_eq!(store.get(b), Some(&"b"));
        assert_eq!(store.get(c), Some(&"c"));
    }

    #[test]
    fn removing_a_middle_value_does_not_change_the_moved_values_id() {
        let mut store: DenseIdStore<TestId, &str> = DenseIdStore::default();

        let a = store.insert("a");
        let b = store.insert("b");
        let c = store.insert("c");

        assert_eq!(store.remove(b), Some("b"));

        assert_eq!(store.get(a), Some(&"a"));
        assert_eq!(store.get(b), None);
        assert_eq!(store.get(c), Some(&"c"));
    }

    #[test]
    fn retired_ids_are_not_reused() {
        let mut store: DenseIdStore<TestId, usize> = DenseIdStore::default();

        let first = store.insert(10);
        let retired = store.insert(20);
        assert_eq!(store.remove(retired), Some(20));
        let next = store.insert(30);

        assert_ne!(retired, next);
        assert_eq!(first, TestId(1));
        assert_eq!(retired, TestId(2));
        assert_eq!(next, TestId(3));
    }
}
