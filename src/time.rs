#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Hertz(pub u32);
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct KiloHertz(pub u32);
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MegaHertz(pub u32);
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Bps(pub u32);
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MilliSeconds(pub u32);
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MicroSeconds(pub u32);

pub trait U32Ext {
    fn hz(self) -> Hertz;
    fn khz(self) -> KiloHertz;
    fn mhz(self) -> MegaHertz;
    fn bps(self) -> Bps;
    fn ms(self) -> MilliSeconds;
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
impl From<MilliSeconds> for Hertz {
    fn from(milliseconds: MilliSeconds) -> Self {
        Self(1_000 / milliseconds.0)
    }
}
impl From<MicroSeconds> for Hertz {
    fn from(microseconds: MicroSeconds) -> Self {
        Self(1_000_000 / microseconds.0)
    }
}

use core::ops;

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
