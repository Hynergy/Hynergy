use crate::device::definition::{DeviceId, TerminalId};
use crate::network::WireId;
use std::num::{NonZeroU32, NonZeroU64};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConnectionType {
    Wire,
    Device,
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectionRef(NonZeroU64);

impl ConnectionRef {
    pub const TYPE_BIT: u32 = 1 << 31;
    const PORT_MASK: u32 = Self::TYPE_BIT - 1;

    #[inline]
    fn new(id: NonZeroU32, port: u32, ty: ConnectionType) -> Option<Self> {
        if port > Self::PORT_MASK {
            return None;
        }

        let low = match ty {
            ConnectionType::Wire => port,
            ConnectionType::Device => port | Self::TYPE_BIT,
        };
        let packed = ((id.get() as u64) << 32) | low as u64;

        Some(Self(NonZeroU64::new(packed).expect(
            "a non-zero entity ID produces a non-zero connection reference",
        )))
    }

    #[inline]
    pub(super) fn terminal(device: DeviceId, terminal: TerminalId) -> Option<Self> {
        Self::new(device.id(), terminal.id(), ConnectionType::Device)
    }

    #[inline]
    pub fn as_wire(self) -> Option<WireId> {
        (self.connection_type() == ConnectionType::Wire).then(|| {
            let id = NonZeroU32::new((self.0.get() >> 32) as u32)
                .expect("packed connection IDs are non-zero");
            WireId::from(id)
        })
    }

    #[inline]
    pub fn as_terminal(self) -> Option<(DeviceId, TerminalId)> {
        (self.connection_type() == ConnectionType::Device).then(|| {
            let id = NonZeroU32::new((self.0.get() >> 32) as u32)
                .expect("packed connection IDs are non-zero");
            (DeviceId::from(id), TerminalId::new(self.port()))
        })
    }

    #[inline]
    pub(super) fn index(self) -> usize {
        (self.0.get() >> 32) as usize - 1
    }

    #[inline]
    pub(super) fn port(self) -> u32 {
        self.0.get() as u32 & Self::PORT_MASK
    }

    #[inline]
    pub fn connection_type(self) -> ConnectionType {
        if self.0.get() as u32 & Self::TYPE_BIT != 0 {
            ConnectionType::Device
        } else {
            ConnectionType::Wire
        }
    }
}

impl From<WireId> for ConnectionRef {
    #[inline]
    fn from(id: WireId) -> Self {
        Self::new(id.into(), 0, ConnectionType::Wire).expect("a wire always uses a valid zero port")
    }
}

#[cfg(test)]
mod tests {
    use super::ConnectionRef;
    use crate::device::definition::{DeviceId, TerminalId};
    use crate::network::WireId;

    #[test]
    fn wire_connection_decodes_only_as_wire() {
        let wire = WireId::try_from(3).unwrap();
        let connection = ConnectionRef::from(wire);

        assert_eq!(connection.as_wire(), Some(wire));
        assert_eq!(connection.as_terminal(), None);
    }

    #[test]
    fn terminal_connection_decodes_only_as_terminal() {
        let device = DeviceId::try_from(2).unwrap();
        let terminal = TerminalId::new(7);
        let connection = ConnectionRef::terminal(device, terminal).unwrap();

        assert_eq!(connection.as_wire(), None);
        assert_eq!(connection.as_terminal(), Some((device, terminal)));
    }

    #[test]
    fn terminal_port_must_fit_packed_mask() {
        let device = DeviceId::try_from(1).unwrap();

        let largest_valid = TerminalId::new(ConnectionRef::TYPE_BIT - 1);

        let first_invalid = TerminalId::new(ConnectionRef::TYPE_BIT);

        assert!(ConnectionRef::terminal(device, largest_valid).is_some());

        assert!(ConnectionRef::terminal(device, first_invalid).is_none());
    }
}
