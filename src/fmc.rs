//! Flash memory controller.
//!
//! Only the flash wait states are covered, and they are not set from here: they
//! have to be raised before the system clock speeds up, so
//! [`UnfrozenRcu::freeze`](crate::rcu::UnfrozenRcu::freeze) borrows this type and
//! writes them itself. Programming and erasing are not implemented.
//!
//! ```ignore
//! let mut fmc = dp.fmc.constrain();
//! let config = ClockConfig::default().sysclk(SysClk::Pll(PllFreq::Mhz48));
//! let mut rcu = dp.rcu.constrain().freeze(&mut fmc, config);
//! ```

use crate::pac;

/// Highest `hclk` each wait-state setting can be read at.
const WS0_MAX_HCLK: u32 = 24_000_000;
const WS1_MAX_HCLK: u32 = 48_000_000;
const WS2_MAX_HCLK: u32 = 72_000_000;
const UNLOCK_KEY1: u32 = 0x45670123;
const UNLOCK_KEY2: u32 = 0xCDEF89AB;

/// The flash controller with `CTL` unlocked, borrowed for the body of
/// [`Fmc::with_unlocked`] and locked again when that call returns.
pub struct UnlockedFmc<'a> {
    fmc: &'a mut Fmc,
}

impl<'a> UnlockedFmc<'a> {
    fn lock(self) {
        self.fmc.fmc.ctl().modify(|_, w| w.lk().lock());
    }
}

/// Owns the flash memory controller.
pub struct Fmc {
    fmc: pac::Fmc,
}

impl Fmc {
    fn unlock(&mut self) -> UnlockedFmc<'_> {
        self.fmc.key().write(|w| w.key().bits(UNLOCK_KEY1));
        self.fmc.key().write(|w| w.key().bits(UNLOCK_KEY2));
        UnlockedFmc { fmc: self }
    }

    /// Unlocks `CTL`, runs `f`, locks it again, and returns what `f` returned.
    ///
    /// The unlocked handle cannot outlive the call, so the flash is never left
    /// writable.
    pub fn with_unlocked<R>(&mut self, f: impl FnOnce(&mut UnlockedFmc) -> R) -> R {
        let mut unlocked = self.unlock();
        let result = f(&mut unlocked);
        unlocked.lock();
        result
    }

    /// Returns the peripheral.
    pub fn release(self) -> pac::Fmc {
        self.fmc
    }

    /// Sets the wait states `hclk` can be read at, `hclk` given in Hz.
    ///
    /// Must run before the system clock rises and after it falls, so the flash
    /// is never read faster than it responds.
    pub(crate) fn set_ws(&mut self, hclk: u32) {
        self.fmc.ws().modify(|_, w| {
            if hclk <= WS0_MAX_HCLK {
                w.wscnt().ws0()
            } else if hclk <= WS1_MAX_HCLK {
                w.wscnt().ws1()
            } else if hclk <= WS2_MAX_HCLK {
                w.wscnt().ws2()
            } else {
                unreachable!()
            }
        });
    }
}

/// Entry point on the raw peripheral, mirroring [`GpioExt`](crate::gpio::GpioExt).
pub trait FmcExt {
    /// Takes the peripheral.
    ///
    /// Nothing is written and no clock is gated — the flash controller is always
    /// clocked.
    fn constrain(self) -> Fmc;
}

impl FmcExt for pac::Fmc {
    fn constrain(self) -> Fmc {
        Fmc { fmc: self }
    }
}
