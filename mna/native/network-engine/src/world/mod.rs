use network_model::devices::registry::DefinitionRegistry;
use network_model::devices::{AttachTerminalError, DefinitionId, DeviceId, DeviceSlot, TerminalId};
use network_model::net::{WireId, WireSlot};
use network_model::{ConnectionRef, ConnectionType, ParameterId};
use std::num::NonZeroU32;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum WorldError {
    #[error(
        "provided id {id:#?} exceeds \
         max bound {upper_bound:#?}"
    )]
    IdOutOfBound { id: NonZeroU32, upper_bound: usize },

    #[error("id {id:#?} is already assigned")]
    IdAlreadyAssigned { id: NonZeroU32 },

    #[error("no {ty:#?} with assigned id {id:#?}")]
    IdNotAssigned { ty: ConnectionType, id: NonZeroU32 },

    #[error("cannot connect wire to itself")]
    WireConnectToSelf,

    #[error("elements are already connected")]
    AlreadyConnected,

    #[error("elements are not connected")]
    NotConnected,

    #[error("terminal is already connected")]
    TerminalAlreadyConnected,

    #[error("invalid terminal")]
    InvalidTerminal,

    #[error("invalid parameter {parameter:#?}")]
    InvalidParameter { parameter: ParameterId },

    #[error("definition {definition:#?} is not registered")]
    UnknownDefinition { definition: DefinitionId },
}

#[derive(Debug, Clone)]
pub struct World {
    wires: Vec<Option<WireSlot>>,
    devices: Vec<Option<DeviceSlot>>,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    pub fn new() -> Self {
        Self {
            wires: Vec::new(),
            devices: Vec::new(),
        }
    }

    pub fn with_capacity(wires: usize, devices: usize) -> Self {
        Self {
            wires: Vec::with_capacity(wires),
            devices: Vec::with_capacity(devices),
        }
    }

    #[inline]
    fn wire_mut(wires: &mut [Option<WireSlot>], id: WireId) -> Result<&mut WireSlot, WorldError> {
        wires
            .get_mut(id.index())
            .and_then(Option::as_mut)
            .ok_or(WorldError::IdNotAssigned {
                ty: ConnectionType::Wire,
                id: id.id(),
            })
    }

    #[inline]
    fn device_mut(
        devices: &mut [Option<DeviceSlot>],
        id: DeviceId,
    ) -> Result<&mut DeviceSlot, WorldError> {
        devices
            .get_mut(id.index())
            .and_then(Option::as_mut)
            .ok_or(WorldError::IdNotAssigned {
                ty: ConnectionType::Device,
                id: id.id(),
            })
    }

    #[inline]
    fn terminal_connection(
        device: &DeviceSlot,
        terminal: TerminalId,
    ) -> Result<Option<ConnectionRef>, WorldError> {
        device
            .terminals()
            .get(terminal.index())
            .copied()
            .ok_or(WorldError::InvalidTerminal)
    }

    #[inline]
    fn map_attach_error(error: AttachTerminalError) -> WorldError {
        match error {
            AttachTerminalError::AlreadyConnected => WorldError::TerminalAlreadyConnected,

            AttachTerminalError::InvalidTerminal => WorldError::InvalidTerminal,
        }
    }

    #[inline]
    fn unlink_one_way(&mut self, endpoint: ConnectionRef, peer: ConnectionRef) {
        match endpoint.connection_type() {
            ConnectionType::Wire => {
                let removed = self.wires[endpoint.index()]
                    .as_mut()
                    .expect("stored wire connection should reference a valid wire")
                    .remove_connection(peer);

                debug_assert!(removed, "bidirectional wire connection invariant violated");
            }

            ConnectionType::Device => {
                let terminal = TerminalId::from(endpoint.port());

                let device = self.devices[endpoint.index()]
                    .as_mut()
                    .expect("stored terminal connection should reference a valid device");

                let current = Self::terminal_connection(device, terminal)
                    .expect("stored terminal connection should reference a valid terminal");

                debug_assert_eq!(
                    current,
                    Some(peer),
                    "bidirectional terminal connection invariant violated"
                );

                device.detach_terminal(terminal);
            }
        }
    }

    pub fn add_wire(&mut self, id: WireId) -> Result<(), WorldError> {
        let index = id.index();
        let len = self.wires.len();

        if index > len {
            return Err(WorldError::IdOutOfBound {
                id: id.id(),
                upper_bound: len,
            });
        }

        if index == len {
            self.wires.push(Some(WireSlot::new()));
            return Ok(());
        }

        let slot = &mut self.wires[index];

        if slot.is_some() {
            return Err(WorldError::IdAlreadyAssigned { id: id.id() });
        }

        *slot = Some(WireSlot::new());

        Ok(())
    }

    pub fn remove_wire(&mut self, id: WireId) -> Result<(), WorldError> {
        let index = id.index();
        let len = self.wires.len();

        if index >= len {
            return Err(WorldError::IdOutOfBound {
                id: id.id(),
                upper_bound: len,
            });
        }

        let slot = self.wires[index].take().ok_or(WorldError::IdNotAssigned {
            ty: ConnectionType::Wire,
            id: id.id(),
        })?;

        let removed_ref = ConnectionRef::from(id);

        for &connection in slot.connections() {
            self.unlink_one_way(connection, removed_ref);
        }

        Ok(())
    }

    pub fn add_device(
        &mut self,
        definition_registry: &DefinitionRegistry,
        id: DeviceId,
        definition_id: DefinitionId,
    ) -> Result<(), WorldError> {
        let index = id.index();
        let len = self.devices.len();

        if index > len {
            return Err(WorldError::IdOutOfBound {
                id: id.id(),
                upper_bound: len,
            });
        }

        if index < len && self.devices[index].is_some() {
            return Err(WorldError::IdAlreadyAssigned { id: id.id() });
        }

        let definition =
            definition_registry
                .get(definition_id)
                .ok_or(WorldError::UnknownDefinition {
                    definition: definition_id,
                })?;

        let slot = Some(DeviceSlot::new(definition_id, definition));

        if index == len {
            self.devices.push(slot);
        } else {
            self.devices[index] = slot;
        }

        Ok(())
    }

    pub fn remove_device(&mut self, id: DeviceId) -> Result<(), WorldError> {
        let index = id.index();
        let len = self.devices.len();

        if index >= len {
            return Err(WorldError::IdOutOfBound {
                id: id.id(),
                upper_bound: len,
            });
        }

        let slot = self.devices[index]
            .take()
            .ok_or(WorldError::IdNotAssigned {
                ty: ConnectionType::Device,
                id: id.id(),
            })?;

        for (terminal_index, connection) in slot.terminals().iter().enumerate() {
            let Some(connection) = *connection else {
                continue;
            };

            if connection.connection_type() == ConnectionType::Device && connection.index() == index
            {
                continue;
            }

            let terminal = TerminalId::from(terminal_index as u32);

            let removed_ref = ConnectionRef::from((id, terminal));

            self.unlink_one_way(connection, removed_ref);
        }

        Ok(())
    }

    pub fn connect_wires(&mut self, wire_a: WireId, wire_b: WireId) -> Result<(), WorldError> {
        if wire_a == wire_b {
            return Err(WorldError::WireConnectToSelf);
        }

        let a_index = wire_a.index();
        let b_index = wire_b.index();

        if a_index >= self.wires.len() {
            return Err(WorldError::IdNotAssigned {
                ty: ConnectionType::Wire,
                id: wire_a.id(),
            });
        }

        if b_index >= self.wires.len() {
            return Err(WorldError::IdNotAssigned {
                ty: ConnectionType::Wire,
                id: wire_b.id(),
            });
        }

        debug_assert_ne!(a_index, b_index);

        let [a_slot, b_slot] = unsafe { self.wires.get_disjoint_unchecked_mut([a_index, b_index]) };

        let a_slot = a_slot.as_mut().ok_or(WorldError::IdNotAssigned {
            ty: ConnectionType::Wire,
            id: wire_a.id(),
        })?;

        let b_slot = b_slot.as_mut().ok_or(WorldError::IdNotAssigned {
            ty: ConnectionType::Wire,
            id: wire_b.id(),
        })?;

        let a_to_b = ConnectionRef::from(wire_b);
        let b_to_a = ConnectionRef::from(wire_a);

        let a_connected = a_slot.contains_connection(a_to_b);

        let b_connected = b_slot.contains_connection(b_to_a);

        debug_assert_eq!(
            a_connected, b_connected,
            "bidirectional wire connection invariant violated"
        );

        if a_connected || b_connected {
            return Err(WorldError::AlreadyConnected);
        }

        a_slot.add_connection(a_to_b);
        b_slot.add_connection(b_to_a);

        Ok(())
    }

    pub fn disconnect_wires(&mut self, wire_a: WireId, wire_b: WireId) -> Result<(), WorldError> {
        if wire_a == wire_b {
            return Err(WorldError::WireConnectToSelf);
        }

        let a_index = wire_a.index();
        let b_index = wire_b.index();

        if a_index >= self.wires.len() {
            return Err(WorldError::IdNotAssigned {
                ty: ConnectionType::Wire,
                id: wire_a.id(),
            });
        }

        if b_index >= self.wires.len() {
            return Err(WorldError::IdNotAssigned {
                ty: ConnectionType::Wire,
                id: wire_b.id(),
            });
        }

        debug_assert_ne!(a_index, b_index);

        let [a_slot, b_slot] = unsafe { self.wires.get_disjoint_unchecked_mut([a_index, b_index]) };

        let a_slot = a_slot.as_mut().ok_or(WorldError::IdNotAssigned {
            ty: ConnectionType::Wire,
            id: wire_a.id(),
        })?;

        let b_slot = b_slot.as_mut().ok_or(WorldError::IdNotAssigned {
            ty: ConnectionType::Wire,
            id: wire_b.id(),
        })?;

        let a_to_b = ConnectionRef::from(wire_b);
        let b_to_a = ConnectionRef::from(wire_a);

        let a_connected = a_slot.contains_connection(a_to_b);

        let b_connected = b_slot.contains_connection(b_to_a);

        debug_assert_eq!(
            a_connected, b_connected,
            "bidirectional wire connection invariant violated"
        );

        if !a_connected || !b_connected {
            return Err(WorldError::NotConnected);
        }

        debug_assert!(a_slot.remove_connection(a_to_b));

        debug_assert!(b_slot.remove_connection(b_to_a));

        Ok(())
    }

    pub fn attach_terminal(
        &mut self,
        wire: WireId,
        device: DeviceId,
        terminal: TerminalId,
    ) -> Result<(), WorldError> {
        let wire_slot = Self::wire_mut(&mut self.wires, wire)?;

        let device_slot = Self::device_mut(&mut self.devices, device)?;

        let terminal_ref = ConnectionRef::from((device, terminal));

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
        wire: WireId,
        device: DeviceId,
        terminal: TerminalId,
    ) -> Result<(), WorldError> {
        let wire_slot = Self::wire_mut(&mut self.wires, wire)?;

        let device_slot = Self::device_mut(&mut self.devices, device)?;

        let wire_ref = ConnectionRef::from(wire);
        let terminal_ref = ConnectionRef::from((device, terminal));

        let connection = Self::terminal_connection(device_slot, terminal)?;

        if connection != Some(wire_ref) || !wire_slot.contains_connection(terminal_ref) {
            return Err(WorldError::NotConnected);
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
    ) -> Result<(), WorldError> {
        let a_index = device_a.index();
        let b_index = device_b.index();

        if a_index >= self.devices.len() {
            return Err(WorldError::IdNotAssigned {
                ty: ConnectionType::Device,
                id: device_a.id(),
            });
        }

        if b_index >= self.devices.len() {
            return Err(WorldError::IdNotAssigned {
                ty: ConnectionType::Device,
                id: device_b.id(),
            });
        }

        let a_ref = ConnectionRef::from((device_a, terminal_a));

        let b_ref = ConnectionRef::from((device_b, terminal_b));

        if a_index == b_index {
            let device = self.devices[a_index]
                .as_mut()
                .ok_or(WorldError::IdNotAssigned {
                    ty: ConnectionType::Device,
                    id: device_a.id(),
                })?;

            let a_connection = Self::terminal_connection(device, terminal_a)?;

            if terminal_a == terminal_b {
                if a_connection.is_some() {
                    return Err(WorldError::TerminalAlreadyConnected);
                }

                device
                    .attach_terminal(terminal_a, a_ref)
                    .map_err(Self::map_attach_error)?;

                return Ok(());
            }

            let b_connection = Self::terminal_connection(device, terminal_b)?;

            if a_connection.is_some() || b_connection.is_some() {
                return Err(WorldError::TerminalAlreadyConnected);
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

        let a_slot = a_slot.as_mut().ok_or(WorldError::IdNotAssigned {
            ty: ConnectionType::Device,
            id: device_a.id(),
        })?;

        let b_slot = b_slot.as_mut().ok_or(WorldError::IdNotAssigned {
            ty: ConnectionType::Device,
            id: device_b.id(),
        })?;

        let a_connection = Self::terminal_connection(a_slot, terminal_a)?;

        let b_connection = Self::terminal_connection(b_slot, terminal_b)?;

        if a_connection.is_some() || b_connection.is_some() {
            return Err(WorldError::TerminalAlreadyConnected);
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
    ) -> Result<(), WorldError> {
        let a_index = device_a.index();
        let b_index = device_b.index();

        if a_index >= self.devices.len() {
            return Err(WorldError::IdNotAssigned {
                ty: ConnectionType::Device,
                id: device_a.id(),
            });
        }

        if b_index >= self.devices.len() {
            return Err(WorldError::IdNotAssigned {
                ty: ConnectionType::Device,
                id: device_b.id(),
            });
        }

        let a_ref = ConnectionRef::from((device_a, terminal_a));

        let b_ref = ConnectionRef::from((device_b, terminal_b));

        if a_index == b_index {
            let device = self.devices[a_index]
                .as_mut()
                .ok_or(WorldError::IdNotAssigned {
                    ty: ConnectionType::Device,
                    id: device_a.id(),
                })?;

            let a_connection = Self::terminal_connection(device, terminal_a)?;

            if terminal_a == terminal_b {
                if a_connection != Some(a_ref) {
                    return Err(WorldError::NotConnected);
                }

                device.detach_terminal(terminal_a);

                return Ok(());
            }

            let b_connection = Self::terminal_connection(device, terminal_b)?;

            if a_connection != Some(b_ref) || b_connection != Some(a_ref) {
                return Err(WorldError::NotConnected);
            }

            device.detach_terminal(terminal_a);
            device.detach_terminal(terminal_b);

            return Ok(());
        }

        let [a_slot, b_slot] =
            unsafe { self.devices.get_disjoint_unchecked_mut([a_index, b_index]) };

        let a_slot = a_slot.as_mut().ok_or(WorldError::IdNotAssigned {
            ty: ConnectionType::Device,
            id: device_a.id(),
        })?;

        let b_slot = b_slot.as_mut().ok_or(WorldError::IdNotAssigned {
            ty: ConnectionType::Device,
            id: device_b.id(),
        })?;

        let a_connection = Self::terminal_connection(a_slot, terminal_a)?;

        let b_connection = Self::terminal_connection(b_slot, terminal_b)?;

        if a_connection != Some(b_ref) || b_connection != Some(a_ref) {
            return Err(WorldError::NotConnected);
        }

        a_slot.detach_terminal(terminal_a);
        b_slot.detach_terminal(terminal_b);

        Ok(())
    }

    pub fn set_device_parameter(
        &mut self,
        device: DeviceId,
        parameter: ParameterId,
        value: f64,
    ) -> Result<(), WorldError> {
        let slot = Self::device_mut(&mut self.devices, device)?;

        if parameter.index() >= slot.parameters().len() {
            return Err(WorldError::InvalidParameter { parameter });
        }

        slot.set_parameter(parameter, value);

        Ok(())
    }
}
