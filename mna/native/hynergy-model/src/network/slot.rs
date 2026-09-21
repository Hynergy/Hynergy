use super::connection::ConnectionRef;
use smallvec::SmallVec;

#[derive(Debug, Clone, Default)]
pub struct WireSlot(SmallVec<[ConnectionRef; 2]>);

impl WireSlot {
    #[inline]
    pub(super) fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub(super) fn add_connection(&mut self, connection: ConnectionRef) {
        self.0.push(connection);
    }

    #[inline]
    pub(super) fn remove_connection(&mut self, connection: ConnectionRef) -> bool {
        let Some(index) = self.0.iter().position(|&item| item == connection) else {
            return false;
        };

        self.0.swap_remove(index);

        true
    }

    #[inline]
    pub(super) fn contains_connection(&self, connection: ConnectionRef) -> bool {
        self.0.contains(&connection)
    }

    #[inline]
    pub(super) fn connections(&self) -> &[ConnectionRef] {
        &self.0
    }
}
