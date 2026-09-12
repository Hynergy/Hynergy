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
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
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
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
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

pub(crate) use define_non_zero_id;
