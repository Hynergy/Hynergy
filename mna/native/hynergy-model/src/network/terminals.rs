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
        debug_assert!(wire_slot.remove_connection(terminal_ref));
        Ok(())
    }

    pub fn connect_terminals(
        &mut self,
        device_a: DeviceId,
        terminal_a: TerminalId,
        device_b: DeviceId,
        terminal_b: TerminalId,
    ) -> Result<(), NetworkModelError> {
        let a_index = device_a.index();
        let b_index = device_b.index();

        if a_index >= self.devices.len() {
            return Err(NetworkModelError::IdNotAssigned {
                ty: ConnectionType::Device,
                id: device_a.id(),
            });
        }
        if b_index >= self.devices.len() {
            return Err(NetworkModelError::IdNotAssigned {
                ty: ConnectionType::Device,
                id: device_b.id(),
            });
        }

        let a_ref = Self::terminal_ref(device_a, terminal_a)?;
        let b_ref = Self::terminal_ref(device_b, terminal_b)?;

        if a_index == b_index {
            let device =
                self.devices[a_index]
                    .as_mut()
                    .ok_or(NetworkModelError::IdNotAssigned {
                        ty: ConnectionType::Device,
                        id: device_a.id(),
                    })?;
            let a_connection = Self::terminal_connection(device, terminal_a)?;

            if terminal_a == terminal_b {
                if a_connection.is_some() {
                    return Err(NetworkModelError::TerminalAlreadyConnected);
                }

                device
                    .attach_terminal(terminal_a, a_ref)
                    .map_err(Self::map_attach_error)?;
                return Ok(());
            }

            let b_connection = Self::terminal_connection(device, terminal_b)?;
            if a_connection.is_some() || b_connection.is_some() {
                return Err(NetworkModelError::TerminalAlreadyConnected);
            }

            device
                .attach_terminal(terminal_a, b_ref)
                .expect("terminal was validated as free");
            device
                .attach_terminal(terminal_b, a_ref)
                .expect("terminal was validated as free");
            return Ok(());
        }

        let [a_slot, b_slot] =
            unsafe { self.devices.get_disjoint_unchecked_mut([a_index, b_index]) };
        let a_slot = a_slot.as_mut().ok_or(NetworkModelError::IdNotAssigned {
            ty: ConnectionType::Device,
            id: device_a.id(),
        })?;
        let b_slot = b_slot.as_mut().ok_or(NetworkModelError::IdNotAssigned {
            ty: ConnectionType::Device,
            id: device_b.id(),
        })?;
        let a_connection = Self::terminal_connection(a_slot, terminal_a)?;
        let b_connection = Self::terminal_connection(b_slot, terminal_b)?;

        if a_connection.is_some() || b_connection.is_some() {
            return Err(NetworkModelError::TerminalAlreadyConnected);
        }

        a_slot
            .attach_terminal(terminal_a, b_ref)
            .expect("terminal was validated as free");
        b_slot
            .attach_terminal(terminal_b, a_ref)
            .expect("terminal was validated as free");
        Ok(())
    }

    pub fn disconnect_terminals(
        &mut self,
        device_a: DeviceId,
        terminal_a: TerminalId,
        device_b: DeviceId,
        terminal_b: TerminalId,
    ) -> Result<(), NetworkModelError> {
        let a_index = device_a.index();
        let b_index = device_b.index();

        if a_index >= self.devices.len() {
            return Err(NetworkModelError::IdNotAssigned {
                ty: ConnectionType::Device,
                id: device_a.id(),
            });
        }
        if b_index >= self.devices.len() {
            return Err(NetworkModelError::IdNotAssigned {
                ty: ConnectionType::Device,
                id: device_b.id(),
            });
        }

        let a_ref = Self::terminal_ref(device_a, terminal_a)?;
        let b_ref = Self::terminal_ref(device_b, terminal_b)?;

        if a_index == b_index {
            let device =
                self.devices[a_index]
                    .as_mut()
                    .ok_or(NetworkModelError::IdNotAssigned {
                        ty: ConnectionType::Device,
                        id: device_a.id(),
                    })?;
            let a_connection = Self::terminal_connection(device, terminal_a)?;

            if terminal_a == terminal_b {
                if a_connection != Some(a_ref) {
                    return Err(NetworkModelError::NotConnected);
                }
                device.detach_terminal(terminal_a);
                return Ok(());
            }

            let b_connection = Self::terminal_connection(device, terminal_b)?;
            if a_connection != Some(b_ref) || b_connection != Some(a_ref) {
                return Err(NetworkModelError::NotConnected);
            }

            device.detach_terminal(terminal_a);
            device.detach_terminal(terminal_b);
            return Ok(());
        }

        let [a_slot, b_slot] =
            unsafe { self.devices.get_disjoint_unchecked_mut([a_index, b_index]) };
        let a_slot = a_slot.as_mut().ok_or(NetworkModelError::IdNotAssigned {
            ty: ConnectionType::Device,
            id: device_a.id(),
        })?;
        let b_slot = b_slot.as_mut().ok_or(NetworkModelError::IdNotAssigned {
            ty: ConnectionType::Device,
            id: device_b.id(),
        })?;
        let a_connection = Self::terminal_connection(a_slot, terminal_a)?;
        let b_connection = Self::terminal_connection(b_slot, terminal_b)?;

        if a_connection != Some(b_ref) || b_connection != Some(a_ref) {
            return Err(NetworkModelError::NotConnected);
        }

        a_slot.detach_terminal(terminal_a);
        b_slot.detach_terminal(terminal_b);
        Ok(())
    }
}
