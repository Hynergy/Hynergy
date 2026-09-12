use crate::device::definition::{DeviceId, TerminalId};
use crate::network::WireId;
use std::num::{NonZeroU32, NonZeroU64};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectionType {
    Wire,
    Device,
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct ConnectionRef(NonZeroU64);

impl ConnectionRef {
    pub(super) const TYPE_BIT: u32 = 1 << 31;
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
    pub(super) fn index(self) -> usize {
        (self.0.get() >> 32) as usize - 1
    }

    #[inline]
    pub(super) fn port(self) -> u32 {
        self.0.get() as u32 & Self::PORT_MASK
    }

    #[inline]
    pub(super) fn connection_type(self) -> ConnectionType {
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
