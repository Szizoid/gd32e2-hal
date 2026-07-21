//! Typed units for frequencies and durations.
//!
//! These are newtypes over `u32`, so they cost nothing at runtime but keep a bare
//! number from being passed where a frequency is expected. Values are wrapped at
//! API boundaries and unwrapped (`.0`) for internal arithmetic.
//!
//! ```ignore
//! use gd32e2_hal::time::U32Ext;
//! let baud = 115_200.bps();
//! let sysclk = 48.mhz();
//! ```

use core::ops;

/// Frequency in hertz.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Hertz(pub u32);
/// Frequency in kilohertz.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct KiloHertz(pub u32);
/// Frequency in megahertz.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MegaHertz(pub u32);
/// Bit rate in bits per second.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Bps(pub u32);
/// Duration in milliseconds.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MilliSeconds(pub u32);
/// Duration in microseconds.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MicroSeconds(pub u32);

/// Extension trait attaching unit suffixes to plain integers.
pub trait U32Ext {
    /// Wraps as [`Hertz`].
    fn hz(self) -> Hertz;
    /// Wraps as [`KiloHertz`].
    fn khz(self) -> KiloHertz;
    /// Wraps as [`MegaHertz`].
    fn mhz(self) -> MegaHertz;
    /// Wraps as [`Bps`].
    fn bps(self) -> Bps;
    /// Wraps as [`MilliSeconds`].
    fn ms(self) -> MilliSeconds;
    /// Wraps as [`MicroSeconds`].
    fn mcs(self) -> MicroSeconds;
}

impl U32Ext for u32 {
    fn hz(self) -> Hertz {
        Hertz(self)
    }
    fn khz(self) -> KiloHertz {
        KiloHertz(self)
    }
    fn mhz(self) -> MegaHertz {
        MegaHertz(self)
    }
    fn bps(self) -> Bps {
        Bps(self)
    }
    fn ms(self) -> MilliSeconds {
        MilliSeconds(self)
    }
    fn mcs(self) -> MicroSeconds {
        MicroSeconds(self)
    }
}

impl From<KiloHertz> for Hertz {
    fn from(val: KiloHertz) -> Self {
        Self(val.0 * 1_000)
    }
}
impl From<MegaHertz> for Hertz {
    fn from(val: MegaHertz) -> Self {
        Self(val.0 * 1_000_000)
    }
}
impl From<MegaHertz> for KiloHertz {
    fn from(val: MegaHertz) -> Self {
        Self(val.0 * 1_000)
    }
}
/// Period to frequency — a reciprocal, not a change of unit.
impl From<MilliSeconds> for Hertz {
    fn from(milliseconds: MilliSeconds) -> Self {
        Self(1_000 / milliseconds.0)
    }
}
/// Period to frequency — a reciprocal, not a change of unit.
impl From<MicroSeconds> for Hertz {
    fn from(microseconds: MicroSeconds) -> Self {
        Self(1_000_000 / microseconds.0)
    }
}

macro_rules! impl_arithmetic {
    ($wrapper:ty, $wrapped:ty) => {
        impl ops::Mul<$wrapped> for $wrapper {
            type Output = Self;
            fn mul(self, rhs: $wrapped) -> Self {
                Self(self.0 * rhs)
            }
        }
        impl ops::MulAssign<$wrapped> for $wrapper {
            fn mul_assign(&mut self, rhs: $wrapped) {
                self.0 *= rhs;
            }
        }
        impl ops::Div<$wrapped> for $wrapper {
            type Output = Self;
            fn div(self, rhs: $wrapped) -> Self {
                Self(self.0 / rhs)
            }
        }
        impl ops::Div<$wrapper> for $wrapper {
            type Output = $wrapped;
            fn div(self, rhs: $wrapper) -> $wrapped {
                self.0 / rhs.0
            }
        }
        impl ops::DivAssign<$wrapped> for $wrapper {
            fn div_assign(&mut self, rhs: $wrapped) {
                self.0 /= rhs;
            }
        }
    };
}

impl_arithmetic!(Hertz, u32);
impl_arithmetic!(KiloHertz, u32);
impl_arithmetic!(MegaHertz, u32);
impl_arithmetic!(Bps, u32);
