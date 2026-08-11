//!

use core::ops::Deref;

use crate::gpio::{Alternate, OpenDrain, Pin};
use crate::pac;
use crate::rcu::{Clocks, Enable, Rcu, Reset};
use crate::time::Hertz;

/// Smallest `CLKC` the hardware honours; below it SCL timing is undefined.
const CLKC_MIN_STANDARD: u32 = 4;
const CLKC_MIN_FAST: u32 = 1;

/// Slowest `pclk1` each mode can be driven from, per the manual.
const MIN_PCLK1_STANDARD_HZ: u32 = 2_000_000;
const MIN_PCLK1_FAST_HZ: u32 = 8_000_000;
const MIN_PCLK1_FAST_PLUS_HZ: u32 = 24_000_000;

/// Longest SCL rise time the I²C specification allows per mode, in nanoseconds
/// (NXP UM10204). Not a property of the controller — the line is pulled up by a
/// resistor, and `RISETIME` tells the peripheral how long that takes.
const RISE_TIME_STANDARD_NS: u32 = 1000;
const RISE_TIME_FAST_NS: u32 = 300;
const RISE_TIME_FAST_PLUS_NS: u32 = 120;

/// A peripheral that [`I2c`] can drive.
///
/// I2C0 and I2C1 share one register block layout, so this trait carries no
/// operations of its own: [`Deref`] hands out the registers and the driver is
/// written once, generically. The supertraits are what the constructor needs —
/// clock gating and reset.
pub trait Instance: Deref<Target = pac::i2c0::RegisterBlock> + Enable + Reset {}

impl Instance for pac::I2c0 {}
impl Instance for pac::I2c1 {}

/// Marks a pin usable as `SDA` for `I2C`, in the right alternate function.
pub trait SdaPin<I2C> {}
/// Marks a pin usable as `SCL` for `I2C`, in the right alternate function.
pub trait SclPin<I2C> {}

macro_rules! i2c_pins {
    ( $( $I2C:ty:
        SDA: [ $($sda_p:literal $sda_n:literal : $sda_af:literal),* $(,)? ]
        SCL: [ $($scl_p:literal $scl_n:literal : $scl_af:literal),* $(,)? ]
    ),* $(,)? ) => {
        $(
            $(impl SdaPin<$I2C> for Pin<$sda_p, $sda_n, Alternate<$sda_af, OpenDrain>> {})*
            $(impl SclPin<$I2C> for Pin<$scl_p, $scl_n, Alternate<$scl_af, OpenDrain>> {})*
        )*
    };
}

// PB10/PB11 at AF1 belong to a *different* I²C depending on the chip variant
// (datasheet Table 2-14 footnotes): I2C0 on GD32E230x4, I2C1 on GD32E230x8.
// They are therefore listed in the gated blocks, not here.
i2c_pins!(
    pac::I2c0:
        SDA: ['A' 10 : 4, 'B' 7 : 1, 'B' 9 : 1]
        SCL: ['A' 9 : 4, 'B' 6 : 1, 'B' 8 : 1],
);

// ---- (1) GD32E230x4 only: PB10/PB11 AF1 are I2C0 ----
#[cfg(feature = "gd32e230x4")]
i2c_pins!(
    pac::I2c0:
        SDA: ['B' 11 : 1]
        SCL: ['B' 10 : 1],
);

// ---- (3) GD32E230x8 only: I2C1 exists, and PB10/PB11 AF1 belong to it ----
#[cfg(feature = "gd32e230x8")]
i2c_pins!(
    pac::I2c1:
        SDA: ['A' 1 : 4, 'A' 12 : 5, 'B' 11 : 1, 'B' 14 : 5]
        SCL: ['A' 0 : 4, 'A' 11 : 5, 'B' 10 : 1, 'B' 13 : 5]
);

/// Writes the bus timing to `CTL1`, `CKCFG` and `RT`, leaving the peripheral enabled.
///
/// `I2CCLK` is `pclk1` in whole megahertz: the peripheral needs the absolute
/// input frequency, not just a divider, because `RISETIME` and the analog filter
/// are measured in real time. Everything else is derived from it and `mode`.
///
/// The peripheral is disabled for the duration — `CKCFG` and `I2CCLK` are only
/// taken while `I2CEN` is clear.
///
/// # Panics
///
/// If `pclk1` is below the minimum the mode needs (2 / 8 / 24 MHz), or if the
/// requested frequency is so high that `CLKC` comes out below what the hardware
/// honours. Both are properties of the firmware rather than of the bus, so they
/// either never fire or fire on the first run: the alternative is a bus quietly
/// running at a frequency nobody asked for.
fn apply_config<I2C: Instance>(i2c: &I2C, mode: I2cMode, pclk1: Hertz) {
    let pclk1 = pclk1.to_Hz();
    let pclk1_mhz = pclk1 / 1_000_000;
    // t_rise / T_pclk1 + 1, and `1 ns × pclk1` is exactly `pclk1` in MHz.
    let risetime = |t_rise_ns: u32| (pclk1_mhz * t_rise_ns / 1_000 + 1) as u8;

    i2c.ctl0().modify(|_, w| w.i2cen().disabled());
    i2c.ctl1()
        .modify(|_, w| unsafe { w.i2cclk().bits(pclk1_mhz as u8) });

    match mode {
        // Both halves of the period are `CLKC` cycles long, so the period is
        // twice that.
        I2cMode::Standard { frequency } => {
            assert!(
                pclk1 >= MIN_PCLK1_STANDARD_HZ,
                "I2C standard mode needs pclk1 of at least 2 MHz"
            );
            let clkc = pclk1 / (2 * frequency.to_Hz());
            assert!(
                clkc >= CLKC_MIN_STANDARD,
                "I2C frequency too high for this pclk1"
            );
            i2c.rt()
                .write(|w| w.risetime().bits(risetime(RISE_TIME_STANDARD_NS)));
            i2c.ckcfg()
                .write(|w| w.fast().standard().clkc().bits(clkc as u16));
        }
        I2cMode::Fast {
            frequency,
            duty_cycle,
        } => {
            assert!(
                pclk1 >= MIN_PCLK1_FAST_HZ,
                "I2C fast mode needs pclk1 of at least 8 MHz"
            );
            i2c.rt()
                .write(|w| w.risetime().bits(risetime(RISE_TIME_FAST_NS)));
            write_fast_ckcfg(i2c, pclk1, frequency, duty_cycle);
        }
        I2cMode::FastPlus {
            frequency,
            duty_cycle,
        } => {
            assert!(
                pclk1 >= MIN_PCLK1_FAST_PLUS_HZ,
                "I2C fast mode plus needs pclk1 of at least 24 MHz"
            );
            i2c.rt()
                .write(|w| w.risetime().bits(risetime(RISE_TIME_FAST_PLUS_NS)));
            write_fast_ckcfg(i2c, pclk1, frequency, duty_cycle);
            i2c.fmpcfg().write(|w| w.fmpen().set_bit());
        }
    }

    i2c.ctl0().modify(|_, w| w.i2cen().enabled());
}

/// Writes `CKCFG` for the two fast modes, which share their formulas.
///
/// The duty cycle splits the period into `2 + 1` or `16 + 9` parts of `CLKC`
/// cycles each, so the divider follows from how many parts there are.
fn write_fast_ckcfg<I2C: Instance>(i2c: &I2C, pclk1: u32, frequency: Hertz, duty_cycle: DutyCycle) {
    let parts = match duty_cycle {
        DutyCycle::Ratio2to1 => 3,
        DutyCycle::Ratio16to9 => 25,
    };
    let clkc = pclk1 / (parts * frequency.to_Hz());
    assert!(
        clkc >= CLKC_MIN_FAST,
        "I2C frequency too high for this pclk1"
    );
    i2c.ckcfg().write(|w| {
        let w = match duty_cycle {
            DutyCycle::Ratio2to1 => w.dtcy().duty2(),
            DutyCycle::Ratio16to9 => w.dtcy().duty16_9(),
        };
        w.fast().fast().clkc().bits(clkc as u16)
    });
}

/// Ratio of the low to the high half of an SCL period (`DTCY`).
///
/// Only fast and fast-mode-plus can shape the duty cycle; standard mode always
/// runs 1:1, which is why this is a field of those variants and not of
/// [`I2cMode`] itself.
#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[allow(missing_docs)]
pub enum DutyCycle {
    Ratio2to1,
    Ratio16to9,
}

/// Bus speed, and whatever else that speed implies.
///
/// Each variant carries its own minimum `pclk1` — 2 MHz for standard, 8 MHz for
/// fast, 24 MHz for fast-mode-plus — below which the peripheral cannot meet the
/// timings.
#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum I2cMode {
    /// Up to 100 kHz, duty cycle fixed at 1:1.
    Standard {
        /// SCL frequency.
        frequency: Hertz,
    },
    /// Up to 400 kHz.
    Fast {
        /// SCL frequency.
        frequency: Hertz,
        /// Shape of the SCL period.
        duty_cycle: DutyCycle,
    },
    /// Up to 1 MHz, and additionally enables the stronger line driver (`FMPCFG`).
    FastPlus {
        /// SCL frequency.
        frequency: Hertz,
        /// Shape of the SCL period.
        duty_cycle: DutyCycle,
    },
}

impl I2cMode {
    /// Standard mode at `frequency`.
    pub fn standard(frequency: Hertz) -> Self {
        Self::Standard { frequency }
    }
    /// Fast mode at `frequency`, with the given duty cycle.
    pub fn fast(frequency: Hertz, duty_cycle: DutyCycle) -> Self {
        Self::Fast {
            frequency,
            duty_cycle,
        }
    }
    /// Fast mode plus at `frequency`, with the given duty cycle.
    pub fn fast_plus(frequency: Hertz, duty_cycle: DutyCycle) -> Self {
        Self::FastPlus {
            frequency,
            duty_cycle,
        }
    }
}

/// A configured I²C master, owning the peripheral and its two pins.
///
/// Both lines are open-drain and pulled up externally; the peripheral only ever
/// drives them low. There is no chip select — the target is addressed by the
/// first byte of every transaction.
pub struct I2c<I2CX, SDA, SCL> {
    i2c: I2CX,
    sda_pin: SDA,
    scl_pin: SCL,
}

impl<I2CX, SDA, SCL> I2c<I2CX, SDA, SCL>
where
    I2CX: Instance,
    SDA: SdaPin<I2CX>,
    SCL: SclPin<I2CX>,
{
    /// Enables the peripheral's clock, resets it and applies `mode`.
    ///
    /// The pins must already be in the alternate function this I²C uses, and
    /// configured as open-drain; the bounds reject any other pin at compile
    /// time. They are moved in and handed back by [`release`](Self::release).
    ///
    /// `clocks` is needed because the peripheral is told `pclk1` itself, not
    /// merely a divider: `RISETIME` and the analog filter are measured in real
    /// time, so the absolute input frequency has to be written to `I2CCLK`.
    ///
    /// # Panics
    ///
    /// If `pclk1` is too slow for `mode` (2 MHz for standard, 8 for fast, 24 for
    /// fast mode plus), or if `mode`'s frequency is unreachable from it.
    pub fn new(
        rcu: &mut Rcu,
        i2c: I2CX,
        sda_pin: SDA,
        scl_pin: SCL,
        clocks: &Clocks,
        mode: I2cMode,
    ) -> Self {
        I2CX::enable(rcu);
        I2CX::reset(rcu);

        apply_config(&i2c, mode, clocks.pclk1());
        Self {
            i2c,
            sda_pin,
            scl_pin,
        }
    }
    /// Returns the peripheral and both pins.
    ///
    /// The clock is left enabled and no reset is performed — a later `new()`
    /// does both anyway.
    pub fn release(self) -> (I2CX, SDA, SCL) {
        (self.i2c, self.sda_pin, self.scl_pin)
    }
}
