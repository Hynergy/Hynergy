use super::connection::ConnectionRef;
use super::{Network, NetworkModelError};
use crate::device::definition::{DeviceId, TerminalId};

impl Network {
    pub fn attach_terminal(
        &mut self,
        wire: super::WireId,
        device: DeviceId,
        terminal: TerminalId,
    ) -> Result<(), NetworkModelError> {
        let wire_slot = Self::wire_mut(&mut self.wires, wire)?;
        let device_slot = Self::device_mut(&mut self.devices, device)?;
        let terminal_ref = Self::terminal_ref(device, terminal)?;

        debug_assert!(
            !wire_slot.contains_connection(terminal_ref),
            "wire already contains terminal while terminal reports otherwise"
        );

        device_slot
            .attach_terminal(terminal, ConnectionRef::from(wire))
            .map_err(Self::map_attach_error)?;
        wire_slot.add_connection(terminal_ref);
        Ok(())
    }

    pub fn detach_terminal(
        &mut self,
        wire: super::WireId,
        device: DeviceId,
        terminal: TerminalId,
    ) -> Result<(), NetworkModelError> {
        let wire_slot = Self::wire_mut(&mut self.wires, wire)?;
        let device_slot = Self::device_mut(&mut self.devices, device)?;
        let wire_ref = ConnectionRef::from(wire);
        let terminal_ref = Self::terminal_ref(device, terminal)?;
        let connection = Self::terminal_connection(device_slot, terminal)?;

        if connection != Some(wire_ref) || !wire_slot.contains_connection(terminal_ref) {
            return Err(NetworkModelError::NotConnected);
        }

        device_slot.detach_terminal(terminal);
        let removed = wire_slot.remove_connection(terminal_ref);
        debug_assert!(removed);
        Ok(())
    }
}
