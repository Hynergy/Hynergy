use super::connection::ConnectionRef;
use super::{ConnectionType, Network, NetworkModelError};
use crate::device::definition::{DeviceId, TerminalId};

impl Network {
    pub fn attach_terminal(
        &mut self,
        wire: super::WireId,
        device: DeviceId,
        terminal: TerminalId,
    ) -> Result<(), NetworkModelError> {
        let wire_slot = self
            .wires
            .get(wire.index())
            .and_then(Option::as_ref)
            .ok_or(NetworkModelError::IdNotAssigned {
                ty: ConnectionType::Wire,
                id: wire.id(),
            })?;

        let connection = self.terminal_connection(device, terminal)?;
        let terminal_ref = Self::terminal_ref(device, terminal)?;

        debug_assert!(
            !wire_slot.contains_connection(terminal_ref),
            "wire already contains terminal while terminal reports otherwise"
        );

        if connection.is_some() {
            return Err(NetworkModelError::TerminalAlreadyConnected);
        }

        self.device_arena
            .attach_terminal(device, terminal, ConnectionRef::from(wire))?;

        Self::wire_mut(&mut self.wires, wire)?.add_connection(terminal_ref);

        Ok(())
    }

    pub fn detach_terminal(
        &mut self,
        wire: super::WireId,
        device: DeviceId,
        terminal: TerminalId,
    ) -> Result<(), NetworkModelError> {
        let wire_slot = self
            .wires
            .get(wire.index())
            .and_then(Option::as_ref)
            .ok_or(NetworkModelError::IdNotAssigned {
                ty: ConnectionType::Wire,
                id: wire.id(),
            })?;

        let wire_ref = ConnectionRef::from(wire);
        let terminal_ref = Self::terminal_ref(device, terminal)?;
        let connection = self.terminal_connection(device, terminal)?;

        if connection != Some(wire_ref) || !wire_slot.contains_connection(terminal_ref) {
            return Err(NetworkModelError::NotConnected);
        }

        let removed = self.device_arena.detach_terminal(device, terminal)?;

        debug_assert_eq!(removed, Some(wire_ref));

        let removed = Self::wire_mut(&mut self.wires, wire)?.remove_connection(terminal_ref);

        debug_assert!(removed);

        Ok(())
    }
}
