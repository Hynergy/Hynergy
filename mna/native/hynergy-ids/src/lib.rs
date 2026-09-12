#[macro_export]
macro_rules! define_id {
    ($($name:ident $(: $ty:ty)?),* $(,)?) => {
        $(
            $crate::define_id!(@one $name $(: $ty)?);
        )*
    };

    (@one $name:ident : $ty:ty) => {
        $crate::define_id!(@impl $name, $ty);
    };

    (@one $name:ident) => {
        $crate::define_id!(@impl $name, u32);
    };

    (@impl $name:ident, $ty:ty) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
        pub struct $name($ty);

        impl Default for $name {
            fn default() -> Self {
                Self::new(0)
            }
        }

        impl $name {
            #[inline]
            pub const fn new(index: $ty) -> Self {
                Self(index)
            }

            #[inline]
            pub const fn id(self) -> $ty {
                self.0
            }

            #[inline]
            pub const fn index(self) -> usize {
                self.0 as usize
            }
        }

        impl From<$name> for $ty {
            #[inline]
            fn from(id: $name) -> $ty {
                id.0
            }
        }

        impl From<$ty> for $name {
            #[inline]
            fn from(id: $ty) -> Self {
                Self(id)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

#[macro_export]
macro_rules! define_non_zero_id {
    ($($name:ident $(: $ty:ident)?),* $(,)?) => {
        $(
            $crate::define_non_zero_id!(@one $name $(: $ty)?);
        )*
    };

    (@one $name:ident) => {
        $crate::define_non_zero_id!(@one $name : u32);
    };

    (@one $name:ident : u32) => {
        $crate::define_non_zero_id!(@impl $name, u32, std::num::NonZeroU32);
    };

    (@one $name:ident : u64) => {
        $crate::define_non_zero_id!(@impl $name, u64, std::num::NonZeroU64);
    };

    (@one $name:ident : usize) => {
        $crate::define_non_zero_id!(@impl $name, usize, std::num::NonZeroUsize);
    };

    (@one $name:ident : u16) => {
        $crate::define_non_zero_id!(@impl $name, u16, std::num::NonZeroU16);
    };

    (@one $name:ident : u8) => {
        $crate::define_non_zero_id!(@impl $name, u8, std::num::NonZeroU8);
    };

    (@impl $name:ident, $primitive:ty, $nonzero:ty) => {
        #[repr(transparent)]
        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
        )]
        pub struct $name($nonzero);

        impl $name {
            /// Creates an ID from its non-zero representation.
            #[inline]
            pub const fn new(id: $nonzero) -> Self {
                Self(id)
            }

            /// Returns the underlying non-zero value.
            #[inline]
            pub const fn id(self) -> $nonzero {
                self.0
            }

            /// Returns the underlying primitive integer.
            #[inline]
            pub const fn get(self) -> $primitive {
                self.0.get()
            }

            /// Returns the corresponding zero-based index.
            #[inline]
            pub const fn index(self) -> usize {
                (self.get() - 1) as usize
            }
        }

        impl From<$nonzero> for $name {
            #[inline]
            fn from(id: $nonzero) -> Self {
                Self::new(id)
            }
        }

        impl From<$name> for $nonzero {
            #[inline]
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl From<$name> for $primitive {
            #[inline]
            fn from(id: $name) -> Self {
                id.get()
            }
        }

        impl TryFrom<u32> for $name {
            type Error = std::num::TryFromIntError;

            #[inline]
            fn try_from(raw: $primitive) -> Result<Self, Self::Error> {
                Ok(Self(<$nonzero>::try_from(raw)?))
            }
        }

        impl std::fmt::Display for $name {
            #[inline]
            fn fmt(
                &self,
                f: &mut std::fmt::Formatter<'_>,
            ) -> std::fmt::Result {
                self.get().fmt(f)
            }
        }
    };
}
