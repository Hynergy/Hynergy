use std::num::{NonZeroU32, NonZeroU64};
use thiserror::Error;

pub mod circuits;
pub mod devices;
pub mod net;

macro_rules! define_id {
    ($($name:ident $(: $ty:ty)?),* $(,)?) => {
        $(
            define_id!(@one $name $(: $ty)?);
        )*
    };

    (@one $name:ident : $ty:ty) => {
        define_id!(@impl $name, $ty);
    };

    (@one $name:ident) => {
        define_id!(@impl $name, u32);
    };

    (@impl $name:ident, $ty:ty) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq,  Ord, PartialOrd, Hash)]
        pub struct $name($ty);

        impl $name {
            #[inline]
            pub const fn new(index: $ty) -> Self {
                Self(index)
            }

            #[inline]
            pub const fn id(self) -> u32 {
                self.0
            }

            #[inline]
            pub const fn index(self) -> usize {
                self.0 as usize
            }
        }

        impl From<u32> for $name {
            #[inline]
            fn from(index: u32) -> Self {
                Self::new(index as $ty)
            }
        }
    };
}

pub(crate) use define_id;

macro_rules! define_non_zero_id {
    ($($name:ident $(: $ty:ty)?),* $(,)?) => {
        $(
            define_non_zero_id!(@one $name $(: $ty)?);
        )*
    };

    (@one $name:ident : $ty:ty) => {
        define_non_zero_id!(@impl $name, $ty);
    };

    (@one $name:ident) => {
        define_non_zero_id!(@impl $name, std::num::NonZeroU32);
    };

    (@impl $name:ident, $ty:ty) => {
        #[repr(transparent)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq,  Ord, PartialOrd, Hash)]
        pub struct $name($ty);

        impl $name {
            #[inline]
            pub const fn new(id: $ty) -> Self {
                Self(id)
            }

            #[inline]
            pub const fn id(self) -> $ty {
                self.0
            }

            #[inline]
            pub const fn index(self) -> usize {
                (self.id().get() - 1) as usize
            }
        }

        impl From<$name> for $ty {
            #[inline]
            fn from(id: $name) -> Self {
                id.0
            }
        }
    };
}

use crate::devices::{DeviceId, TerminalId};
use crate::net::WireId;
pub(crate) use define_non_zero_id;

define_id!(NodeId: u32, ParameterId: u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectionType {
    Wire,
    Device,
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionRef(NonZeroU64);

impl ConnectionRef {
    const TYPE_BIT: u32 = 1 << 31;
    const PORT_MASK: u32 = Self::TYPE_BIT - 1;

    #[inline]
    pub fn new(id: NonZeroU32, port: u32, ty: ConnectionType) -> Option<Self> {
        if port > Self::PORT_MASK {
            return None;
        }

        let low = match ty {
            ConnectionType::Wire => port,
            ConnectionType::Device => port | Self::TYPE_BIT,
        };

        let packed = ((id.get() as u64) << 32) | low as u64;

        Some(Self(unsafe { NonZeroU64::new_unchecked(packed) }))
    }

    #[inline]
    pub fn new_unchecked(id: impl Into<NonZeroU32>, port: u32, ty: ConnectionType) -> Self {
        let low = match ty {
            ConnectionType::Wire => port,
            ConnectionType::Device => port | Self::TYPE_BIT,
        };

        let packed = ((id.into().get() as u64) << 32) | low as u64;

        Self(unsafe { NonZeroU64::new_unchecked(packed) })
    }

    #[inline]
    pub fn id(self) -> NonZeroU32 {
        unsafe { NonZeroU32::new_unchecked((self.0.get() >> 32) as u32) }
    }

    #[inline]
    pub fn index(self) -> usize {
        (self.0.get() >> 32) as usize - 1
    }

    #[inline]
    pub fn port(self) -> u32 {
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
        Self::new_unchecked(id, 0, ConnectionType::Wire)
    }
}

impl From<(DeviceId, TerminalId)> for ConnectionRef {
    #[inline]
    fn from(pair: (DeviceId, TerminalId)) -> Self {
        Self::new_unchecked(pair.0, pair.1.id(), ConnectionType::Device)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bound {
    pub value: f64,
    pub inclusive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ParameterConstraints {
    lower: Option<Bound>,
    upper: Option<Bound>,
    non_zero: bool,
    reciprocal_range: Option<(Option<Bound>, Option<Bound>)>,
}

#[derive(Debug, Error, Clone, Copy, Eq, PartialEq)]
pub enum ParameterConstraintError {
    #[error("parameter must be finite")]
    NonFinite,

    #[error("parameter must be non-zero")]
    ZeroNotAllowed,

    #[error("parameter is outside the allowed range")]
    OutOfRange,

    #[error("parameter reciprocal is outside the allowed range")]
    ReciprocalOutOfRange,
}

impl ParameterConstraints {
    pub fn new(
        lower: Option<Bound>,
        upper: Option<Bound>,
        non_zero: bool,
        reciprocal_range: Option<(Option<Bound>, Option<Bound>)>,
    ) -> Self {
        Self {
            lower,
            upper,
            non_zero,
            reciprocal_range,
        }
    }

    pub fn validate(&self, param: f64) -> Result<(), ParameterConstraintError> {
        if !param.is_finite() {
            return Err(ParameterConstraintError::NonFinite);
        }
        if self.non_zero && param == 0.0 {
            return Err(ParameterConstraintError::ZeroNotAllowed);
        }
        if !within_bounds(param, self.lower, self.upper) {
            return Err(ParameterConstraintError::OutOfRange);
        }

        if !self.reciprocal_range.is_none_or(|(lower, upper)| {
            let reciprocal = 1.0 / param;

            reciprocal.is_finite()
                && lower.is_none_or(|b| {
                    if b.inclusive {
                        param >= b.value
                    } else {
                        param > b.value
                    }
                })
                && upper.is_none_or(|b| {
                    if b.inclusive {
                        param <= b.value
                    } else {
                        param < b.value
                    }
                })
        }) {
            return Err(ParameterConstraintError::ReciprocalOutOfRange);
        }

        Ok(())
    }
}

#[inline]
fn within_bounds(value: f64, lower: Option<Bound>, upper: Option<Bound>) -> bool {
    lower.is_none_or(|b| {
        if b.inclusive {
            value >= b.value
        } else {
            value > b.value
        }
    }) && upper.is_none_or(|b| {
        if b.inclusive {
            value <= b.value
        } else {
            value < b.value
        }
    })
}
