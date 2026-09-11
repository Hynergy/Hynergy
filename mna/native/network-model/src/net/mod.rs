use crate::{ConnectionRef, define_non_zero_id};
use smallvec::SmallVec;

define_non_zero_id!(WireId);

#[derive(Debug, Clone, Default)]
pub struct WireSlot(SmallVec<[ConnectionRef; 2]>);

impl WireSlot {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self(SmallVec::with_capacity(capacity))
    }

    #[inline]
    pub fn add_connection(&mut self, conn: ConnectionRef) {
        self.0.push(conn);
    }

    #[inline]
    pub fn remove_connection(&mut self, conn: ConnectionRef) -> bool {
        let Some(index) = self.0.iter().position(|&c| c == conn) else {
            return false;
        };

        self.0.swap_remove(index);
        true
    }

    #[inline]
    pub fn contains_connection(&self, conn: ConnectionRef) -> bool {
        self.0.contains(&conn)
    }

    #[inline]
    pub fn connections(&self) -> &[ConnectionRef] {
        &self.0
    }
}
