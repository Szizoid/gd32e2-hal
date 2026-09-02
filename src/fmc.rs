//! Flash memory controller.
//!
//! The wait states are not set from here: they have to be raised before the
//! system clock speeds up, so
//! [`UnfrozenRcu::freeze`](crate::rcu::UnfrozenRcu::freeze) borrows this type and
//! writes them itself.
//!
//! Erasing and programming need `CTL` unlocked, which happens for the body of
//! [`Fmc::with_unlocked`] and no longer. Option bytes are not covered.
//!
//! ```ignore
//! let mut fmc = dp.fmc.constrain();
//! let config = ClockConfig::default().sysclk(SysClk::Pll(PllFreq::Mhz48));
//! let mut rcu = dp.rcu.constrain().freeze(&mut fmc, config);
//!
//! fmc.with_unlocked(|f| {
//!     f.erase_page(Page::P63)?;
//!     f.program(Page::P63, 0, 0xDEAD_BEEF)
//! })?;
//! ```

use crate::pac;

/// Highest `hclk` each wait-state setting can be read at.
const WS0_MAX_HCLK: u32 = 24_000_000;
const WS1_MAX_HCLK: u32 = 48_000_000;
const WS2_MAX_HCLK: u32 = 72_000_000;
const UNLOCK_KEY1: u32 = 0x45670123;
const UNLOCK_KEY2: u32 = 0xCDEF89AB;

const BASE: u32 = 0x0800_0000;
const PAGE_SIZE: u32 = 0x400;
/// Bytes per programmed word, `PGW` being left at its reset width of 32 bits.
const WORD_SIZE: u32 = 4;

macro_rules! pages {
    ($($n:literal),* $(,)?) => {
        paste::paste! {
            /// An erasable 1 KB page of the main flash.
            ///
            /// The discriminant is the address the page starts at, so it goes
            /// into `ADDR` as it is. How many pages exist follows the flash of
            /// the part being built for: 16, 32 or 64.
            #[allow(missing_docs)]
            #[derive(Clone, Copy, PartialEq, Eq)]
            #[cfg_attr(feature = "defmt", derive(defmt::Format))]
            #[repr(u32)]
            pub enum Page {
                $([<P $n>] = BASE + $n * PAGE_SIZE),*
            }
        }
    };
}

#[cfg(chip_x4)]
#[rustfmt::skip]
pages!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15);

#[cfg(chip_x6)]
#[rustfmt::skip]
pages!(
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
    16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
);

#[cfg(chip_x8)]
#[rustfmt::skip]
pages!(
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
    16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
    32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47,
    48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63,
);

/// An FMC event that can raise an interrupt.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Event {
    /// An erase or a program operation finished (`ENDF`).
    End,
    /// An operation failed (`ERRIE`); which way is [`Fmc::take_error`].
    Error,
}

/// What an erase or a program operation failed on.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// The page is protected by the option bytes.
    WriteProtected,
    /// The address is not aligned to the programming width.
    ProgramAlignment,
    /// The cell was not erased before programming.
    Program,
}

/// The flash controller with `CTL` unlocked, borrowed for the body of
/// [`Fmc::with_unlocked`] and locked again when that call returns.
pub struct UnlockedFmc<'a> {
    fmc: &'a mut Fmc,
}

impl<'a> UnlockedFmc<'a> {
    fn lock(self) {
        self.fmc.fmc.ctl().modify(|_, w| w.lk().lock());
    }

    /// Erases one page, blocking until it is done.
    ///
    /// The whole page reads back as `0xFF`, so a page holding code or data still
    /// in use has to be picked by the caller, not by us — nothing here checks
    /// what is in it.
    pub fn erase_page(&mut self, page: Page) -> Result<(), Error> {
        self.fmc.fmc.ctl().modify(|_, w| w.per().page_erase());
        self.fmc.fmc.addr().write(|w| w.addr().bits(page as u32));
        self.fmc.fmc.ctl().modify(|_, w| w.start().start());
        let result = self.fmc.wait_busy();
        self.fmc.fmc.ctl().modify(|_, w| w.per().clear_bit());
        result
    }
    /// Erases the whole main flash, blocking until it is done.
    ///
    /// That includes the code running the call, so this only makes sense from
    /// SRAM or from a debugger.
    pub fn mass_erase(&mut self) -> Result<(), Error> {
        self.fmc.fmc.ctl().modify(|_, w| w.mer().mass_erase());
        self.fmc.fmc.ctl().modify(|_, w| w.start().start());
        let result = self.fmc.wait_busy();
        self.fmc.fmc.ctl().modify(|_, w| w.mer().clear_bit());
        result
    }
    /// Programs one 32-bit word, `index` counting words from the start of
    /// `page`, and blocks until it is done.
    ///
    /// Programming only clears bits, so the word has to be erased first: writing
    /// over anything but `0xFFFF_FFFF` returns [`Error::Program`].
    pub fn program(&mut self, page: Page, index: u8, word: u32) -> Result<(), Error> {
        self.fmc.fmc.ctl().modify(|_, w| w.pg().program());
        let addr = page as u32 + index as u32 * WORD_SIZE;
        // The write itself is the command: `PG` makes the FMC latch the address
        // and the data off the bus, so there is no `ADDR` and no `START` here.
        // The address is in the flash and a multiple of four by construction —
        // `Page` gives the base and 256 words is exactly what `u8` counts.
        unsafe {
            core::ptr::write_volatile(addr as *mut u32, word);
        };
        let result = self.fmc.wait_busy();
        self.fmc.fmc.ctl().modify(|_, w| w.pg().clear_bit());
        result
    }

    /// Raises an interrupt on `event`, which still has to be unmasked in the
    /// NVIC.
    ///
    /// `ENDIE` and `ERRIE` sit in `CTL`, which the lock covers whole, so this
    /// can only be done from inside [`Fmc::with_unlocked`]. The interrupt itself
    /// outlives the call: locking `CTL` again leaves both bits standing.
    pub fn listen(&mut self, event: Event) {
        self.fmc.fmc.ctl().modify(|_, w| match event {
            Event::End => w.endie().enabled(),
            Event::Error => w.errie().enabled(),
        });
    }
    /// Stops `event` from raising an interrupt.
    pub fn unlisten(&mut self, event: Event) {
        self.fmc.fmc.ctl().modify(|_, w| match event {
            Event::End => w.endie().disabled(),
            Event::Error => w.errie().disabled(),
        });
    }
}

/// Owns the flash memory controller.
pub struct Fmc {
    fmc: pac::Fmc,
}

impl Fmc {
    /// Blocks until the running operation ends, then clears `ENDF` and reports
    /// how it went.
    fn wait_busy(&mut self) -> Result<(), Error> {
        while self.fmc.stat().read().busy().is_active() {}
        self.clear_interrupt(Event::End);
        match self.take_error() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

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

    /// Returns the error the last operation ended with, clearing its flag.
    ///
    /// `ENDF` is left alone: it marks success and never stands together with an
    /// error.
    pub fn take_error(&mut self) -> Option<Error> {
        let stat = self.fmc.stat().read();
        if stat.wperr().is_error() {
            self.fmc.stat().write(|w| w.wperr().clear());
            Some(Error::WriteProtected)
        } else if stat.pgaerr().bit_is_set() {
            self.fmc.stat().write(|w| w.pgaerr().bit(true));
            Some(Error::ProgramAlignment)
        } else if stat.pgerr().is_error() {
            self.fmc.stat().write(|w| w.pgerr().clear());
            Some(Error::Program)
        } else {
            None
        }
    }

    /// Whether `event` currently raises an interrupt.
    ///
    /// Reading `CTL` is not covered by the lock, so a handler can ask without
    /// unlocking anything.
    pub fn is_listening(&self, event: Event) -> bool {
        let ctl = self.fmc.ctl().read();
        match event {
            Event::End => ctl.endie().is_enabled(),
            Event::Error => ctl.errie().is_enabled(),
        }
    }

    /// Clears the flag behind `event`, which is what stops it re-entering the
    /// handler.
    ///
    /// For [`Event::Error`] this drops every error flag at once; use
    /// [`take_error`](Self::take_error) instead to learn which one it was, it
    /// clears the flag as well.
    pub fn clear_interrupt(&mut self, event: Event) {
        self.fmc.stat().write(|w| match event {
            Event::End => w.endf().clear(),
            Event::Error => w.wperr().clear().pgaerr().bit(true).pgerr().clear(),
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
