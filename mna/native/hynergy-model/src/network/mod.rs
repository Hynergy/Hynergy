mod connection;
mod devices;
mod slot;
mod terminals;
mod wires;

pub use connection::ConnectionType;

use self::connection::ConnectionRef;
use self::slot::{AttachTerminalError, DeviceSlot, WireSlot};
use crate::device::definition::{DefinitionId, DeviceId, TerminalId};
use crate::ids::define_non_zero_id;
use crate::parameter::{ParameterConstraintError, ParameterId};
use std::num::NonZeroU32;
use thiserror::Error;

define_non_zero_id!(WireId);

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum NetworkModelError {
    #[error("provided id {id:#?} exceeds max bound {upper_bound:#?}")]
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

    #[error("parameter {parameter:#?} violates its constraints: {source}")]
    ParameterConstraint {
        parameter: ParameterId,
        #[source]
        source: ParameterConstraintError,
    },

    #[error("definition {definition:#?} is not registered")]
    UnknownDefinition { definition: DefinitionId },
}

#[derive(Debug, Default, Clone)]
pub struct Network {
    wires: Vec<Option<WireSlot>>,
    devices: Vec<Option<DeviceSlot>>,
}

impl Network {
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
    fn wire_mut(
        wires: &mut [Option<WireSlot>],
        id: WireId,
    ) -> Result<&mut WireSlot, NetworkModelError> {
        wires
            .get_mut(id.index())
            .and_then(Option::as_mut)
            .ok_or(NetworkModelError::IdNotAssigned {
                ty: ConnectionType::Wire,
                id: id.id(),
            })
    }

    #[inline]
    fn device_mut(
        devices: &mut [Option<DeviceSlot>],
        id: DeviceId,
    ) -> Result<&mut DeviceSlot, NetworkModelError> {
        devices.get_mut(id.index()).and_then(Option::as_mut).ok_or(
            NetworkModelError::IdNotAssigned {
                ty: ConnectionType::Device,
                id: id.id(),
            },
        )
    }

    #[inline]
    fn terminal_connection(
        device: &DeviceSlot,
        terminal: TerminalId,
    ) -> Result<Option<ConnectionRef>, NetworkModelError> {
        device
            .terminals()
            .get(terminal.index())
            .copied()
            .ok_or(NetworkModelError::InvalidTerminal)
    }

    #[inline]
    fn terminal_ref(
        device: DeviceId,
        terminal: TerminalId,
    ) -> Result<ConnectionRef, NetworkModelError> {
        ConnectionRef::terminal(device, terminal).ok_or(NetworkModelError::InvalidTerminal)
    }

    #[inline]
    fn map_attach_error(error: AttachTerminalError) -> NetworkModelError {
        match error {
            AttachTerminalError::AlreadyConnected => NetworkModelError::TerminalAlreadyConnected,
            AttachTerminalError::InvalidTerminal => NetworkModelError::InvalidTerminal,
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
}

#[cfg(test)]
mod tests {
    use super::{Network, NetworkModelError, connection::ConnectionRef};
    use crate::device::definition::{DefinitionId, DeviceId, PrimitiveElementKind, TerminalId};
    use crate::device::registry::DefinitionRegistry;
    use crate::parameter::{ParameterConstraintError, ParameterId};
    use std::num::NonZeroU32;

    fn device_id(raw: u32) -> DeviceId {
        DeviceId::new(NonZeroU32::new(raw).unwrap())
    }

    #[test]
    fn terminal_connection_rejects_ports_above_the_packed_mask() {
        let device = device_id(1);
        let terminal = TerminalId::new(ConnectionRef::TYPE_BIT);

        assert!(ConnectionRef::terminal(device, terminal).is_none());
    }

    #[test]
    fn device_parameter_validation_preserves_previous_value() {
        let definitions = DefinitionRegistry::new();
        let mut model = Network::new();
        let device = device_id(1);
        let definition = DefinitionId::from(PrimitiveElementKind::Admittance);

        model.add_device(&definitions, device, definition).unwrap();
        assert_eq!(model.devices[0].as_ref().unwrap().parameters(), &[None]);

        model
            .set_device_parameter(&definitions, device, ParameterId::new(0), 1.0)
            .unwrap();
        assert_eq!(
            model.devices[0].as_ref().unwrap().parameters(),
            &[Some(1.0)]
        );

        for (value, error) in [
            (0.0, ParameterConstraintError::OutOfRange),
            (-1.0, ParameterConstraintError::OutOfRange),
            (f64::NAN, ParameterConstraintError::NonFinite),
        ] {
            assert_eq!(
                model.set_device_parameter(&definitions, device, ParameterId::new(0), value),
                Err(NetworkModelError::ParameterConstraint {
                    parameter: ParameterId::new(0),
                    source: error,
                })
            );
            assert_eq!(
                model.devices[0].as_ref().unwrap().parameters(),
                &[Some(1.0)]
            );
        }
    }
}
