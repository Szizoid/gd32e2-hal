//! General-purpose I/O.
//!
//! A pin's port, number and mode all live in its type, so the compiler rejects
//! whatever the current mode doesn't support: an [`Input`] pin has no
//! `set_high`, and an alternate function the pin doesn't have won't compile.
//!
//! Pins are handed out by [`GpioExt::split`], which consumes the port and
//! enables its clock, so a pin can neither be obtained twice nor used unclocked.
//! Modes are changed with the `into_*` methods, each returning a new type.
//!
//! ```ignore
//! let parts = dp.gpioa.split(&mut rcu);
//! let mut led = parts.pa5.into_output();
//! led.set_high().unwrap();
//! let tx = parts.pa9.into_alternate::<1>();
//! ```
//!
//! Two modes are special. `PA13`/`PA14` start as [`Debugger`], since after reset
//! they really are wired to SWD; leaving that goes through the separate
//! `activate_into_*()` family. [`Pin::lock`] freezes a configuration until the
//! next chip reset, which the type reflects as [`Locked`] — with no way back,
//! since the hardware has none either.

use core::convert::Infallible;
use core::marker::PhantomData;

use embedded_hal::digital::{ErrorType, InputPin, OutputPin, StatefulOutputPin};

use crate::pac;
use crate::rcu::Rcu;

const CTL_INPUT: u32 = 0b00;
const CTL_OUTPUT: u32 = 0b01;
const CTL_AF: u32 = 0b10;
const CTL_ANALOG: u32 = 0b11;

const OMODE_PUSH_PULL: u32 = 0b0;
const OMODE_OPEN_DRAIN: u32 = 0b1;

/// Mode: digital input.
pub struct Input;
/// Output type: driven both high and low.
pub struct PushPull;
/// Output type: driven low, released high — needs a pull-up to reach a high level.
pub struct OpenDrain;
/// Mode: digital output, of type `OTYPE` ([`PushPull`] or [`OpenDrain`]).
pub struct Output<OTYPE> {
    _otype: PhantomData<OTYPE>,
}
/// Mode: analog, the input mode the ADC requires.
pub struct Analog;
/// Mode: alternate function `AF`, routing the pin to a peripheral, driven as
/// `OTYPE` ([`PushPull`] or [`OpenDrain`]).
///
/// The output type is part of the mode because some peripherals accept only one:
/// I²C needs open-drain lines, and a push-pull pin there fights whoever pulls the
/// line low. Drivers bind to the type they need. Defaults to [`PushPull`].
pub struct Alternate<const AF: u8, OTYPE = PushPull> {
    _otype: PhantomData<OTYPE>,
}
/// Mode: serial-wire debug, the reset state of `PA13`/`PA14`.
///
/// Deliberately not [`Input`]: those pins are genuinely driving SWD out of reset.
/// The mode is not [`Active`], so the ordinary `into_*` do not reach it — leaving
/// it goes through [`Pin::activate`] or one of its `activate_into_*` siblings.
pub struct Debugger;
/// Mode: configuration frozen until the next chip reset, wrapping the mode it
/// was locked in.
///
/// Reading and writing the pin still work exactly as they did before locking;
/// only reconfiguration is barred.
pub struct Locked<MODE> {
    _mode: PhantomData<MODE>,
}

/// A single pin: `P` is the port (`'A'`, `'B'` or `'F'`), `N` the pin number.
///
/// Zero-sized — the identity lives entirely in the type, so passing a pin around
/// costs nothing at runtime.
pub struct Pin<const P: char, const N: u8, MODE> {
    _mode: PhantomData<MODE>,
}

/// Which port an [`ErasedPin`] came from.
///
/// Only the ports this package bonds — a pin of any other port cannot be
/// constructed, so the enum has nothing to say about them.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[allow(missing_docs)]
pub enum Port {
    A,
    B,
    F,
}

impl Port {
    /// Converts the type-level port letter into its runtime form.
    ///
    /// `const` so that erasing a pin costs nothing: the letter is a constant at
    /// every call site, and the match folds away with it.
    const fn from_char(port: char) -> Self {
        match port {
            'A' => Port::A,
            'B' => Port::B,
            'F' => Port::F,
            _ => unreachable!(),
        }
    }
}

/// A pin whose identity has moved into runtime values, keeping its mode.
///
/// Port and number become fields, so pins of different ports share a type and fit
/// in one array. `MODE` stays a type parameter — an `ErasedPin<Input>` still has
/// no `set_high`. One-way: the checks that need the exact pin have already run by
/// the time it is erased.
pub struct ErasedPin<MODE> {
    port: Port,
    number: u8,
    _mode: PhantomData<MODE>,
}

/// Internal pull resistor.
#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Pull {
    /// No pull resistor.
    Floating = 0b00,
    /// Pulled up to the supply.
    Up = 0b01,
    /// Pulled down to ground.
    Down = 0b10,
}

/// Output slew rate. Slower edges radiate less; faster ones are needed for
/// high-speed peripherals.
#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[allow(missing_docs)]
pub enum Speed {
    Mhz2 = 0b00,
    Mhz10 = 0b01,
    Mhz50 = 0b11,
}

/// Marks `AF` as a valid alternate function number for this pin.
///
/// Populated from the datasheet's pin table, so [`Pin::into_alternate`] rejects
/// a number the pin doesn't have. Which numbers are valid depends on the chip
/// variant feature.
pub trait ValidAf<const AF: u8> {}

macro_rules! pin_af {
    ( $( $p:literal $n:literal => [ $($af:literal),* $(,)? ] ),* $(,)? ) => {
        $( $( impl<MODE> ValidAf<$af> for Pin<$p, $n, MODE> {} )* )*
    };
}

// AF map from datasheet Table 2-13/2-14 (die-level), whose footnotes mark
// functions present on some variants only:
//   (1) GD32E230x4 only          -> cfg `chip_x4`
//   (2) GD32E230x8/6             -> cfg `chip_x6` or `chip_x8`
//   (3) GD32E230x8 only          -> cfg `chip_x8`
// The cfgs come from build.rs, which derives them from the part's flash code, so
// a gate names the die rather than each of the parts sharing it.
// AF numbers that exist on every variant live in the common block below, even
// where the function behind them differs (PA2 AF1 is USART0_TX on x4, USART1_TX
// on x8) — `ValidAf` gates the number alone. Which peripheral a pin belongs to is
// decided by `usart_pins!` / `spi_pins!` / `i2c_pins!`, gated the same way.
pin_af! {
    // ---- Port A ----
    'A' 0  => [1, 7],                // 1:USART0_CTS(1)/USART1_CTS(2) 7:CMP_OUT
    'A' 1  => [0, 1],                // 0:EVENTOUT 1:USART0_RTS/DE(1) | USART1_RTS/DE(2)
    'A' 2  => [1],                   // 1:USART0_TX(1) | USART1_TX(2)
    'A' 3  => [1],                   // 1:USART0_RX(1) | USART1_RX(2)
    'A' 4  => [0, 1, 4],             // 0:SPI0_NSS/I2S0_WS 1:USART0_CK(1)|USART1_CK(2) 4:TIMER13_CH0
    'A' 5  => [0],                   // 0:SPI0_SCK/I2S0_CK
    'A' 6  => [0, 1, 2, 5, 6, 7],    // 0:SPI0_MISO 1:TIMER2_CH0 2:TIMER0_BRKIN 5:TIMER15_CH0 6:EVENTOUT 7:CMP_OUT
    'A' 7  => [0, 1, 2, 4, 5, 6],    // 0:SPI0_MOSI 1:TIMER2_CH1 2:TIMER0_CH0_ON 4:TIMER13_CH0 5:TIMER16_CH0 6:EVENTOUT
    'A' 8  => [0, 1, 2, 3],          // 0:CK_OUT 1:USART0_CK 2:TIMER0_CH0 3:EVENTOUT
    'A' 9  => [1, 2, 4, 5],          // 1:USART0_TX 2:TIMER0_CH1 4:I2C0_SCL 5:CK_OUT
    'A' 10 => [0, 1, 2, 4],          // 0:TIMER16_BRKIN 1:USART0_RX 2:TIMER0_CH2 4:I2C0_SDA
    'A' 11 => [0, 1, 2, 4, 7],       // 0:EVENTOUT 1:USART0_CTS 2:TIMER0_CH3 4:I2C0_SMBA 7:CMP_OUT
    'A' 12 => [0, 1, 2, 4],          // 0:EVENTOUT 1:USART0_RTS/DE 2:TIMER0_ETI 4:I2C0_TXFRAME
    'A' 13 => [0, 1],                // 0:SWDIO 1:IFRP_OUT
    'A' 14 => [0, 1],                // 0:SWCLK 1:USART0_TX(1) | USART1_TX(2)
    'A' 15 => [0, 1, 3],             // 0:SPI0_NSS/I2S0_WS 1:USART0_RX(1)|USART1_RX(2) 3:EVENTOUT
    // ---- Port B ----
    'B' 0  => [0, 1, 2],             // 0:EVENTOUT 1:TIMER2_CH2 2:TIMER0_CH1_ON
    'B' 1  => [1, 2, 3],             // 1:TIMER2_CH3 2:TIMER13_CH0 3:TIMER0_CH2_ON
    'B' 2  => [1],                   // 1:TIMER2_ETI
    'B' 3  => [0, 1],                // 0:SPI0_SCK/I2S0_CK 1:EVENTOUT
    'B' 4  => [0, 1, 2, 4, 6],       // 0:SPI0_MISO 1:TIMER2_CH0 2:EVENTOUT 4:I2C0_TXFRAME 6:TIMER16_BRKIN
    'B' 5  => [0, 1, 2, 3],          // 0:SPI0_MOSI 1:TIMER2_CH1 2:I2C0_SMBA 3:TIMER15_BRKIN
    'B' 6  => [0, 1, 2],             // 0:USART0_TX 1:I2C0_SCL 2:TIMER15_CH0_ON
    'B' 7  => [0, 1, 2],             // 0:USART0_RX 1:I2C0_SDA 2:TIMER16_CH0_ON
    'B' 8  => [1, 2],                // 1:I2C0_SCL 2:TIMER15_CH0
    'B' 9  => [0, 1, 2, 3, 5],       // 0:IFRP_OUT 1:I2C0_SDA 2:TIMER16_CH0 3:EVENTOUT 5:I2S0_MCK
    'B' 11 => [0],                   // 0:EVENTOUT
    'B' 12 => [1, 2],                // 1:EVENTOUT 2:TIMER0_BRKIN
    'B' 13 => [2],                   // 2:TIMER0_CH0_ON
    'B' 14 => [2],                   // 2:TIMER0_CH1_ON
    'B' 15 => [2],                   // 2:TIMER0_CH2_ON
    // NB: 'B' 10 has no variant-independent AF — every one of its functions is
    // footnoted, so it appears only in the gated blocks below.
}

// ---- (1) GD32E230x4 only ----
#[cfg(chip_x4)]
pin_af! {
    'B' 10 => [1],                   // 1:I2C0_SCL
    'B' 11 => [1],                   // 1:I2C0_SDA
    'B' 12 => [0],                   // 0:SPI0_NSS
    'B' 13 => [0],                   // 0:SPI0_SCK
    'B' 14 => [0],                   // 0:SPI0_MISO
    'B' 15 => [0],                   // 0:SPI0_MOSI
}

// ---- (2) GD32E230x8/6 ----
#[cfg(any(chip_x6, chip_x8))]
pin_af! {
    'A' 8  => [4],                   // 4:USART1_TX
    'B' 0  => [4],                   // 4:USART1_RX
}

// ---- (3) GD32E230x8 only ----
#[cfg(chip_x8)]
pin_af! {
    'A' 0  => [4],                   // 4:I2C1_SCL
    'A' 1  => [4, 5],                // 4:I2C1_SDA 5:TIMER14_CH0_ON
    'A' 2  => [0],                   // 0:TIMER14_CH0
    'A' 3  => [0],                   // 0:TIMER14_CH1
    'A' 4  => [6],                   // 6:SPI1_NSS
    'A' 9  => [0],                   // 0:TIMER14_BRKIN
    'A' 11 => [5, 6],                // 5:I2C1_SCL 6:SPI1_IO2
    'A' 12 => [5, 6],                // 5:I2C1_SDA 6:SPI1_IO3
    'A' 13 => [6],                   // 6:SPI1_MISO
    'A' 14 => [6],                   // 6:SPI1_MOSI
    'A' 15 => [6],                   // 6:SPI1_NSS
    'B' 1  => [6],                   // 6:SPI1_SCK
    'B' 9  => [7],                   // 7:SPI1_NSS
    'B' 10 => [1, 6, 7],             // 1:I2C1_SCL 6:SPI1_IO2 7:SPI1_SCK
    'B' 11 => [1, 6],                // 1:I2C1_SDA 6:SPI1_IO3
    'B' 12 => [0, 4],                // 0:SPI1_NSS 4:I2C1_SMBA
    'B' 13 => [0, 1, 5],             // 0:SPI1_SCK 1:I2C1_TXFRAME 5:I2C1_SCL
    'B' 14 => [0, 1, 5],             // 0:SPI1_MISO 1:TIMER14_CH0 5:I2C1_SDA
    'B' 15 => [0, 1, 3],             // 0:SPI1_MOSI 1:TIMER14_CH1 3:TIMER14_CH0_ON
}

/// Marks a mode whose pin may be reconfigured.
///
/// [`Debugger`] and [`Locked`] deliberately don't implement it, which is what
/// makes the `into_*` methods and the setters unavailable on them.
pub trait Active {}

impl Active for Input {}
impl Active for Analog {}
impl<const AF: u8, OTYPE> Active for Alternate<AF, OTYPE> {}
impl<OTYPE> Active for Output<OTYPE> {}

/// Marks a pin on a port that has a `LOCK` register, gating [`Pin::lock`].
///
/// Ports A and B have one; port F does not.
pub trait HasLock {}
impl<const N: u8, MODE> HasLock for Pin<'A', N, MODE> {}
impl<const N: u8, MODE> HasLock for Pin<'B', N, MODE> {}

/// Ways out of [`Debugger`], each giving up debug access on this pin.
///
/// Every one of them writes the mode itself rather than merely relabelling the
/// type, so the pin never sits in a state its type misdescribes. The `activate_`
/// prefix is the whole point: an SWD pin cannot be reconfigured by accident
/// through the ordinary `into_*`, which [`Debugger`] does not reach.
///
/// Losing SWD is not a memory-safety matter, so none of this is `unsafe` — the
/// cost is a board that stops answering the probe until the next reset, which
/// the name is there to announce.
impl<const P: char, const N: u8> Pin<P, N, Debugger> {
    /// Releases the pin as a digital input.
    pub fn activate_into_input(self) -> Pin<P, N, Input> {
        self.set_mode(CTL_INPUT);
        Pin { _mode: PhantomData }
    }
    /// Releases the pin as an input; shorthand for
    /// [`activate_into_input`](Self::activate_into_input).
    pub fn activate(self) -> Pin<P, N, Input> {
        self.activate_into_input()
    }
    /// Releases the pin as a push-pull output.
    pub fn activate_into_push_pull_output(self) -> Pin<P, N, Output<PushPull>> {
        self.set_mode(CTL_OUTPUT);
        self.set_omode(OMODE_PUSH_PULL);
        Pin { _mode: PhantomData }
    }
    /// Releases the pin as an open-drain output.
    pub fn activate_into_open_drain_output(self) -> Pin<P, N, Output<OpenDrain>> {
        self.set_mode(CTL_OUTPUT);
        self.set_omode(OMODE_OPEN_DRAIN);
        Pin { _mode: PhantomData }
    }
    /// Releases the pin as an output; shorthand for the push-pull variant.
    pub fn activate_into_output(self) -> Pin<P, N, Output<PushPull>> {
        self.activate_into_push_pull_output()
    }
    /// Releases the pin as an analog input, as required by the ADC.
    pub fn activate_into_analog(self) -> Pin<P, N, Analog> {
        self.set_mode(CTL_ANALOG);
        Pin { _mode: PhantomData }
    }
    /// Releases the pin to a peripheral through alternate function `AF`,
    /// driven push-pull.
    ///
    /// Gated by [`ValidAf`] exactly as
    /// [`into_alternate`](Pin::into_alternate) is.
    pub fn activate_into_alternate<const AF: u8>(self) -> Pin<P, N, Alternate<AF>>
    where
        Self: ValidAf<AF>,
    {
        self.set_alternate(AF as u32, OMODE_PUSH_PULL);
        Pin { _mode: PhantomData }
    }
    /// Same, but leaves the pin open-drain, as I²C requires.
    pub fn activate_into_alternate_open_drain<const AF: u8>(
        self,
    ) -> Pin<P, N, Alternate<AF, OpenDrain>>
    where
        Self: ValidAf<AF>,
    {
        self.set_alternate(AF as u32, OMODE_OPEN_DRAIN);
        Pin { _mode: PhantomData }
    }
}

// State access takes the identity as arguments so `Pin` and `ErasedPin` share one
// body apiece — the first passes constants, the second its fields. Configuration
// helpers stay methods on `Pin`; an erased pin has no use for them.
fn reg(port: Port) -> &'static pac::gpioa::RegisterBlock {
    let ptr = match port {
        Port::A => pac::Gpioa::ptr(),
        Port::B => pac::Gpiob::ptr() as *const _,
        Port::F => pac::Gpiof::ptr() as *const _, // AFSEL0/1 and LOCK registers are unavailable
    };
    unsafe { &*ptr }
}

fn read_pin(port: Port, number: u8) -> bool {
    let bits = reg(port).istat().read().bits();
    ((bits >> number) & 0b1) == 0b1
}

fn read_octl(port: Port, number: u8) -> bool {
    let bits = reg(port).octl().read().bits();
    ((bits >> number) & 0b1) == 0b1
}

fn set_bop(port: Port, number: u8) {
    reg(port).bop().write(|w| unsafe { w.bits(1 << number) });
}

fn set_bc(port: Port, number: u8) {
    reg(port).bc().write(|w| unsafe { w.bits(1 << number) });
}

fn set_tg(port: Port, number: u8) {
    reg(port).tg().write(|w| unsafe { w.bits(1 << number) });
}

impl<const P: char, const N: u8, MODE> Pin<P, N, MODE> {
    fn reg(&self) -> &'static pac::gpioa::RegisterBlock {
        reg(Port::from_char(P))
    }

    fn read_pin(&self) -> bool {
        read_pin(Port::from_char(P), N)
    }

    fn read_octl(&self) -> bool {
        read_octl(Port::from_char(P), N)
    }
    fn set_bop(&self) {
        set_bop(Port::from_char(P), N);
    }

    fn set_bc(&self) {
        set_bc(Port::from_char(P), N);
    }

    fn set_tg(&self) {
        set_tg(Port::from_char(P), N);
    }
}

// The register writers sit in the unbounded block, not with the `into_*` that
// call them: `Debugger` is not `Active`, yet leaving that mode is itself a mode
// write, and the bounded block is invisible from outside it.
impl<const P: char, const N: u8, MODE> Pin<P, N, MODE> {
    fn set_mode(&self, mode: u32) {
        let offset = N * 2;
        self.reg()
            .ctl()
            .modify(|r, w| unsafe { w.bits((r.bits() & !(0b11 << offset)) | (mode << offset)) });
    }
    fn set_af(&self, af: u32) {
        let is_afsel0 = N < 8;
        let offset = (N % 8) * 4;
        if is_afsel0 {
            self.reg().afsel0().modify(|r, w| unsafe {
                w.bits((r.bits() & !(0b1111 << offset)) | (af << offset))
            });
        } else {
            self.reg().afsel1().modify(|r, w| unsafe {
                w.bits((r.bits() & !(0b1111 << offset)) | (af << offset))
            });
        }
    }
    fn set_pud(&self, bits: u32) {
        let offset = N * 2;
        self.reg()
            .pud()
            .modify(|r, w| unsafe { w.bits((r.bits() & !(0b11 << offset)) | (bits << offset)) });
    }
    fn set_ospd(&self, bits: u32) {
        let offset = N * 2;
        self.reg()
            .ospd()
            .modify(|r, w| unsafe { w.bits((r.bits() & !(0b11 << offset)) | (bits << offset)) });
    }
    fn set_omode(&self, bits: u32) {
        let offset = N;
        self.reg()
            .omode()
            .modify(|r, w| unsafe { w.bits((r.bits() & !(0b1 << offset)) | (bits << offset)) });
    }
    /// Routes the pin to `af` and drives it as `omode`, the two halves of every
    /// `*_alternate*` method — the output type is part of the resulting mode, so
    /// it is written here rather than left at whatever the previous mode used.
    fn set_alternate(&self, af: u32, omode: u32) {
        self.set_mode(CTL_AF);
        self.set_af(af);
        self.set_omode(omode);
    }
    fn set_lk(&self, lkk: bool) {
        self.reg().lock().modify(|_, w| {
            let w = match N {
                0 => w.lk0().locked(),
                1 => w.lk1().locked(),
                2 => w.lk2().locked(),
                3 => w.lk3().locked(),
                4 => w.lk4().locked(),
                5 => w.lk5().locked(),
                6 => w.lk6().locked(),
                7 => w.lk7().locked(),
                8 => w.lk8().locked(),
                9 => w.lk9().locked(),
                10 => w.lk10().locked(),
                11 => w.lk11().locked(),
                12 => w.lk12().locked(),
                13 => w.lk13().locked(),
                14 => w.lk14().locked(),
                15 => w.lk15().locked(),
                _ => unreachable!(),
            };
            if lkk {
                w.lkk().active()
            } else {
                w.lkk().not_active()
            }
        });
    }
}

impl<const P: char, const N: u8, MODE> Pin<P, N, MODE>
where
    MODE: Active,
{
    /// Reconfigures the pin as a digital input.
    pub fn into_input(self) -> Pin<P, N, Input> {
        self.set_mode(CTL_INPUT);
        Pin { _mode: PhantomData }
    }
    /// Reconfigures the pin as a push-pull output.
    pub fn into_push_pull_output(self) -> Pin<P, N, Output<PushPull>> {
        self.set_mode(CTL_OUTPUT);
        self.set_omode(OMODE_PUSH_PULL);
        Pin { _mode: PhantomData }
    }
    /// Reconfigures the pin as an open-drain output.
    ///
    /// The pin can also be read back in this mode, which shared buses such as
    /// I²C rely on.
    pub fn into_open_drain_output(self) -> Pin<P, N, Output<OpenDrain>> {
        self.set_mode(CTL_OUTPUT);
        self.set_omode(OMODE_OPEN_DRAIN);
        Pin { _mode: PhantomData }
    }
    /// Reconfigures the pin as an output; shorthand for the push-pull variant.
    pub fn into_output(self) -> Pin<P, N, Output<PushPull>> {
        self.into_push_pull_output()
    }
    /// Reconfigures the pin as an analog input, as required by the ADC.
    pub fn into_analog(self) -> Pin<P, N, Analog> {
        self.set_mode(CTL_ANALOG);
        Pin { _mode: PhantomData }
    }
    /// Routes the pin to a peripheral through alternate function `AF`, driven
    /// push-pull.
    ///
    /// Only numbers this pin has will compile — see [`ValidAf`] — and the number
    /// stays in the returned type, so a driver can demand the exact function.
    pub fn into_alternate<const AF: u8>(self) -> Pin<P, N, Alternate<AF>>
    where
        Self: ValidAf<AF>,
    {
        self.set_alternate(AF as u32, OMODE_PUSH_PULL);
        Pin { _mode: PhantomData }
    }
    /// Same, but leaves the pin open-drain: [`I2c`](crate::i2c::I2c) accepts only
    /// pins that went through here.
    pub fn into_alternate_open_drain<const AF: u8>(self) -> Pin<P, N, Alternate<AF, OpenDrain>>
    where
        Self: ValidAf<AF>,
    {
        self.set_alternate(AF as u32, OMODE_OPEN_DRAIN);
        Pin { _mode: PhantomData }
    }
    /// Freezes the pin's configuration until the next chip reset.
    ///
    /// Mode, pull, output type, speed and alternate function stop responding to
    /// writes, with no way back in hardware or in the type: a [`Locked`] pin is
    /// still read and written, never reconfigured. Ports with a `LOCK` register
    /// only.
    pub fn lock(self) -> Pin<P, N, Locked<MODE>>
    where
        Self: HasLock,
    {
        // LKK write sequence from the manual: 1 -> 0 -> 1.
        self.set_lk(true);
        self.set_lk(false);
        self.set_lk(true);
        for _ in 0..2 {
            self.reg().lock().read();
        }
        Pin { _mode: PhantomData }
    }
    /// Moves the pin's identity into runtime values, keeping its mode.
    ///
    /// Trades knowing which pin this is for a type shared with every other erased
    /// pin, so they can be collected into an array. Nothing is written; the
    /// configuration stands. One way only — recovering `Pin<'A', 5, _>` would be
    /// a runtime check returning `Option`, which buys nothing.
    pub fn erase(self) -> ErasedPin<MODE> {
        ErasedPin {
            port: Port::from_char(P),
            number: N,
            _mode: PhantomData,
        }
    }

    /// Selects the internal pull resistor.
    pub fn set_pull(&self, p: Pull) {
        self.set_pud(p as u32);
    }
    /// Selects the output slew rate.
    pub fn set_speed(&self, s: Speed) {
        self.set_ospd(s as u32);
    }
}

impl<const P: char, const N: u8, OTYPE> Pin<P, N, Output<OTYPE>> {
    /// Drives the pin high.
    pub fn set_high(&self) {
        self.set_bop();
    }
    /// Drives the pin low.
    pub fn set_low(&self) {
        self.set_bc();
    }
    /// Inverts the driven level.
    pub fn toggle(&self) {
        self.set_tg();
    }
    /// Returns whether the pin is *being driven* high.
    ///
    /// Reads back `OCTL`, i.e. what was last written, not what the wire is at —
    /// for the latter on an open-drain pin see [`is_high`](Pin::is_high).
    pub fn is_set_high(&self) -> bool {
        self.read_octl()
    }
    /// Returns whether the pin is *being driven* low.
    pub fn is_set_low(&self) -> bool {
        !self.read_octl()
    }
}

impl<const P: char, const N: u8> Pin<P, N, Input> {
    /// Returns whether the input reads high.
    pub fn is_high(&self) -> bool {
        self.read_pin()
    }
    /// Returns whether the input reads low.
    pub fn is_low(&self) -> bool {
        !self.read_pin()
    }
}

impl<const P: char, const N: u8> Pin<P, N, Output<OpenDrain>> {
    /// Returns whether the wire reads high.
    ///
    /// Open-drain can only pull low, so this is the actual line level, which
    /// another device on a shared bus may be holding down.
    pub fn is_high(&self) -> bool {
        self.read_pin()
    }
    /// Returns whether the wire reads low.
    pub fn is_low(&self) -> bool {
        !self.read_pin()
    }
}

impl<MODE> ErasedPin<MODE> {
    /// Returns which port the pin came from.
    pub fn port(&self) -> Port {
        self.port
    }
    /// Returns the pin's number within its port.
    pub fn number(&self) -> u8 {
        self.number
    }
}

impl<OTYPE> ErasedPin<Output<OTYPE>> {
    /// Drives the pin high.
    pub fn set_high(&self) {
        set_bop(self.port, self.number);
    }
    /// Drives the pin low.
    pub fn set_low(&self) {
        set_bc(self.port, self.number);
    }
    /// Flips the pin, atomically against the rest of the port.
    pub fn toggle(&self) {
        set_tg(self.port, self.number);
    }
    /// Returns whether the pin is *being driven* high.
    ///
    /// Reads back `OCTL`, i.e. what was last written, not what the wire is at —
    /// for the latter on an open-drain pin see [`is_high`](ErasedPin::is_high).
    pub fn is_set_high(&self) -> bool {
        read_octl(self.port, self.number)
    }
    /// Returns whether the pin is *being driven* low.
    pub fn is_set_low(&self) -> bool {
        !read_octl(self.port, self.number)
    }
}

impl ErasedPin<Input> {
    /// Returns whether the input reads high.
    pub fn is_high(&self) -> bool {
        read_pin(self.port, self.number)
    }
    /// Returns whether the input reads low.
    pub fn is_low(&self) -> bool {
        !read_pin(self.port, self.number)
    }
}

impl ErasedPin<Output<OpenDrain>> {
    /// Returns whether the wire reads high.
    ///
    /// Open-drain can only pull low, so this is the actual line level, which
    /// another device on a shared bus may be holding down.
    pub fn is_high(&self) -> bool {
        read_pin(self.port, self.number)
    }
    /// Returns whether the wire reads low.
    pub fn is_low(&self) -> bool {
        !read_pin(self.port, self.number)
    }
}

impl<const P: char, const N: u8, OTYPE> ErrorType for Pin<P, N, Output<OTYPE>> {
    type Error = Infallible;
}
impl<const P: char, const N: u8, OTYPE> OutputPin for Pin<P, N, Output<OTYPE>> {
    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.set_bop();
        Ok(())
    }
    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.set_bc();
        Ok(())
    }
}

impl<const P: char, const N: u8, OTYPE> StatefulOutputPin for Pin<P, N, Output<OTYPE>> {
    fn is_set_high(&mut self) -> Result<bool, Self::Error> {
        Ok(self.read_octl())
    }
    fn is_set_low(&mut self) -> Result<bool, Self::Error> {
        Ok(!self.read_octl())
    }
    fn toggle(&mut self) -> Result<(), Self::Error> {
        self.set_tg();
        Ok(())
    }
}

impl<const P: char, const N: u8> ErrorType for Pin<P, N, Input> {
    type Error = Infallible;
}
impl<const P: char, const N: u8> InputPin for Pin<P, N, Input> {
    fn is_high(&mut self) -> Result<bool, Self::Error> {
        Ok(self.read_pin())
    }
    fn is_low(&mut self) -> Result<bool, Self::Error> {
        Ok(!self.read_pin())
    }
}

impl<const P: char, const N: u8> InputPin for Pin<P, N, Output<OpenDrain>> {
    fn is_high(&mut self) -> Result<bool, Self::Error> {
        Ok(self.read_pin())
    }
    fn is_low(&mut self) -> Result<bool, Self::Error> {
        Ok(!self.read_pin())
    }
}

impl<OTYPE> ErrorType for ErasedPin<Output<OTYPE>> {
    type Error = Infallible;
}
impl<OTYPE> OutputPin for ErasedPin<Output<OTYPE>> {
    fn set_high(&mut self) -> Result<(), Self::Error> {
        set_bop(self.port, self.number);
        Ok(())
    }
    fn set_low(&mut self) -> Result<(), Self::Error> {
        set_bc(self.port, self.number);
        Ok(())
    }
}

impl<OTYPE> StatefulOutputPin for ErasedPin<Output<OTYPE>> {
    fn is_set_high(&mut self) -> Result<bool, Self::Error> {
        Ok(read_octl(self.port, self.number))
    }
    fn is_set_low(&mut self) -> Result<bool, Self::Error> {
        Ok(!read_octl(self.port, self.number))
    }
    fn toggle(&mut self) -> Result<(), Self::Error> {
        set_tg(self.port, self.number);
        Ok(())
    }
}

impl ErrorType for ErasedPin<Input> {
    type Error = Infallible;
}
impl InputPin for ErasedPin<Input> {
    fn is_high(&mut self) -> Result<bool, Self::Error> {
        Ok(read_pin(self.port, self.number))
    }
    fn is_low(&mut self) -> Result<bool, Self::Error> {
        Ok(!read_pin(self.port, self.number))
    }
}

impl InputPin for ErasedPin<Output<OpenDrain>> {
    fn is_high(&mut self) -> Result<bool, Self::Error> {
        Ok(read_pin(self.port, self.number))
    }
    fn is_low(&mut self) -> Result<bool, Self::Error> {
        Ok(!read_pin(self.port, self.number))
    }
}

impl<const P: char, const N: u8, MODE> Pin<P, N, Locked<MODE>>
where
    Pin<P, N, MODE>: OutputPin,
{
    /// Drives the pin high.
    pub fn set_high(&self) {
        self.set_bop();
    }
    /// Drives the pin low.
    pub fn set_low(&self) {
        self.set_bc();
    }
}

impl<const P: char, const N: u8, MODE> Pin<P, N, Locked<MODE>>
where
    Pin<P, N, MODE>: StatefulOutputPin,
{
    /// Inverts the driven level.
    pub fn toggle(&self) {
        self.set_tg();
    }
    /// Returns whether the pin is *being driven* high, read back from `OCTL`.
    pub fn is_set_high(&self) -> bool {
        self.read_octl()
    }
    /// Returns whether the pin is *being driven* low, read back from `OCTL`.
    pub fn is_set_low(&self) -> bool {
        !self.read_octl()
    }
}

impl<const P: char, const N: u8, MODE> Pin<P, N, Locked<MODE>>
where
    Pin<P, N, MODE>: InputPin,
{
    /// Returns whether the pin reads high.
    pub fn is_high(&self) -> bool {
        self.read_pin()
    }
    /// Returns whether the pin reads low.
    pub fn is_low(&self) -> bool {
        !self.read_pin()
    }
}

impl<const P: char, const N: u8, MODE> ErrorType for Pin<P, N, Locked<MODE>> {
    type Error = Infallible;
}

impl<const P: char, const N: u8, MODE> OutputPin for Pin<P, N, Locked<MODE>>
where
    Pin<P, N, MODE>: OutputPin,
{
    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.set_bop();
        Ok(())
    }
    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.set_bc();
        Ok(())
    }
}

impl<const P: char, const N: u8, MODE> StatefulOutputPin for Pin<P, N, Locked<MODE>>
where
    Pin<P, N, MODE>: StatefulOutputPin,
{
    fn is_set_high(&mut self) -> Result<bool, Self::Error> {
        Ok(self.read_octl())
    }
    fn is_set_low(&mut self) -> Result<bool, Self::Error> {
        Ok(!self.read_octl())
    }
    fn toggle(&mut self) -> Result<(), Self::Error> {
        self.set_tg();
        Ok(())
    }
}

impl<const P: char, const N: u8, MODE> InputPin for Pin<P, N, Locked<MODE>>
where
    Pin<P, N, MODE>: InputPin,
{
    fn is_high(&mut self) -> Result<bool, Self::Error> {
        Ok(self.read_pin())
    }
    fn is_low(&mut self) -> Result<bool, Self::Error> {
        Ok(!self.read_pin())
    }
}

/// Extension trait splitting a GPIO port into its individual pins.
pub trait GpioExt {
    /// The struct of individual pins this port yields.
    type Parts;

    /// Enables the port's clock and returns its pins.
    ///
    /// Consumes the port, so its pins can only be obtained once.
    fn split(self, rcu: &mut Rcu) -> Self::Parts;
}

macro_rules! gpio {
    ($Parts:ident, $Gpio:ty, $P:literal, [ $($name:ident : $num:literal : $mode:ty),+ $(,)? ]) => {
        /// The pins of this port, in their reset modes.
        pub struct $Parts {
            $(
                #[doc = concat!("Pin ", stringify!($name), ".")]
                pub $name: Pin<$P, $num, $mode>,
            )+
        }

        impl GpioExt for $Gpio {
            type Parts = $Parts;
            fn split(self, rcu: &mut Rcu) -> Self::Parts {
                <$Gpio as crate::rcu::Enable>::enable(rcu);
                $Parts { $( $name: Pin::<$P, $num, $mode> { _mode: PhantomData }, )+ }
            }
        }
    };
}

gpio!(PartsA, pac::Gpioa, 'A',
    [pa0:0:Input, pa1:1:Input, pa2:2:Input, pa3:3:Input, pa4:4:Input, pa5:5:Input, pa6:6:Input, pa7:7:Input,
     pa8:8:Input, pa9:9:Input, pa10:10:Input, pa11:11:Input, pa12:12:Input, pa13:13:Debugger, pa14:14:Debugger, pa15:15:Input]);

gpio!(PartsB, pac::Gpiob, 'B',
    [pb0:0:Input, pb1:1:Input, pb2:2:Input, pb3:3:Input, pb4:4:Input, pb5:5:Input, pb6:6:Input, pb7:7:Input,
     pb8:8:Input, pb9:9:Input, pb10:10:Input, pb11:11:Input, pb12:12:Input, pb13:13:Input, pb14:14:Input, pb15:15:Input]);

gpio!(PartsF, pac::Gpiof, 'F', [pf0:0:Input, pf1:1:Input]);
