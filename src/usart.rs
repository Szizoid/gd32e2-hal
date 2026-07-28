//! Asynchronous serial (USART), blocking and non-blocking.
//!
//! Word width is a typestate: a [`Usart<.., Byte>`](Usart) moves `u8` values and
//! is what [`new`](Usart::new) builds, while [`new_word`](Usart::new_word) gives a
//! `Usart<.., Word>` for raw 9-bit frames carrying `u16`. Methods of the other
//! width don't exist on either, so the two can't be mixed up.
//!
//! ```ignore
//! let tx = parts.pa9.into_alternate::<1>();
//! let rx = parts.pa10.into_alternate::<1>();
//! let serial = Usart::new(&mut rcu, dp.usart0, tx, rx, clocks, UsartConfig::default());
//! serial.write_byte(b'x');
//! ```
//!
//! Which pins are valid depends on the chip variant — on the x8 part `PA2`/`PA3`
//! reach USART1, not USART0 — but that is settled at compile time by the pin
//! bounds, so a wrong pin simply fails to build.

use core::marker::PhantomData;
use core::ops::Deref;

use crate::gpio::{Alternate, Pin};
use crate::pac;
use crate::rcu::{Clocks, Enable, Rcu, Reset};
use crate::time::Hertz;

/// 7 data bits: parity occupies bit 7 inside the u8 (`E7`/`O7`).
const DATA_7BIT_MASK: u8 = 0x7F;
/// 9 data bits: the full 9-bit word in `WL=1, PCEN=0` mode.
const DATA_9BIT_MASK: u32 = 0x1FF;

/// Marks a pin usable as `TX` for `USART`, in the right alternate function.
pub trait TxPin<USART> {}
/// Marks a pin usable as `RX` for `USART`, in the right alternate function.
pub trait RxPin<USART> {}

macro_rules! usart_pins {
    ( $( $USART:ty: TX: [ $($tx_p:literal $tx_n:literal : $tx_af:literal),* $(,)? ] RX: [ $($rx_p:literal $rx_n:literal : $rx_af:literal),* $(,)? ] ),* $(,)? ) => {
        $(
            $( impl TxPin<$USART> for Pin<$tx_p, $tx_n, Alternate<$tx_af>> {} )*
            $( impl RxPin<$USART> for Pin<$rx_p, $rx_n, Alternate<$rx_af>> {} )*
        )*
    };
}

// PA2/PA3/PA14/PA15 at AF1 belong to a *different* USART depending on the chip
// variant (datasheet Table 2-13 footnotes): USART0 on GD32E230x4, USART1 on
// GD32E230x8/6. They are therefore listed in the gated blocks, not here.
usart_pins! {
    pac::Usart0:
        TX: [ 'A' 9:1, 'B' 6:0 ]
        RX: [ 'A' 10:1, 'B' 7:0 ],
}

// ---- (1) GD32E230x4 only: PA2/PA3/PA14/PA15 AF1 are USART0 ----
#[cfg(feature = "gd32e230x4")]
usart_pins! {
    pac::Usart0:
        TX: [ 'A' 2:1, 'A' 14:1 ]
        RX: [ 'A' 3:1, 'A' 15:1 ],
}

// ---- (2) GD32E230x8/6: PA2/PA3/PA14/PA15 AF1 are USART1; USART1 exists ----
#[cfg(any(feature = "gd32e230x6", feature = "gd32e230x8"))]
usart_pins! {
    pac::Usart1:
        TX: [ 'A' 2:1, 'A' 8:4, 'A' 14:1 ]
        RX: [ 'A' 3:1, 'A' 15:1, 'B' 0:4 ],
}

/// Supplies the clock frequency feeding a given USART.
///
/// USART0 can be reclocked away from its bus (see
/// [`Usart0Sel`](crate::rcu::Usart0Sel)) while USART1 always runs off APB1.
/// Resolving that per peripheral type keeps the baud divisor from ever being
/// computed against the wrong frequency.
pub trait BusClocks {
    /// Returns the frequency actually clocking this USART.
    fn clock(clocks: &Clocks) -> Hertz;
}

impl BusClocks for pac::Usart0 {
    fn clock(clocks: &Clocks) -> Hertz {
        clocks.usart0()
    }
}

impl BusClocks for pac::Usart1 {
    fn clock(clocks: &Clocks) -> Hertz {
        clocks.pclk1()
    }
}

/// Named constants for the standard bit rates.
///
/// Purely a readability aid — [`UsartConfig::baud`] takes any `u32`, since the
/// hardware divisor is not restricted to these values.
#[allow(missing_docs)]
pub mod baud {
    pub const B110: u32 = 110;
    pub const B300: u32 = 300;
    pub const B600: u32 = 600;
    pub const B1200: u32 = 1_200;
    pub const B2400: u32 = 2_400;
    pub const B4800: u32 = 4_800;
    pub const B9600: u32 = 9_600;
    pub const B14400: u32 = 14_400;
    pub const B19200: u32 = 19_200;
    pub const B38400: u32 = 38_400;
    pub const B57600: u32 = 57_600;
    pub const B115200: u32 = 115_200;
    pub const B230400: u32 = 230_400;
    pub const B460800: u32 = 460_800;
    pub const B921600: u32 = 921_600;
}

/// How many times each bit is sampled.
///
/// ×16 is the default and more tolerant of clock error; ×8 halves the sampling
/// rate, which allows higher bit rates from the same peripheral clock.
#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Oversampling {
    /// 8 samples per bit — allows twice the bit rate from the same clock.
    X8,
    /// 16 samples per bit — the default, more tolerant of clock error.
    X16,
}

/// Word length and parity, as a single setting.
///
/// Named for how the frame appears to the caller rather than for the register
/// bits: `E7`/`O7` leave only 7 real data bits, because parity replaces the top
/// bit of the byte, while `E8`/`O8` keep all 8 by widening the frame to 9 bits
/// and putting parity in the extra one. There is no `N7` — without parity a
/// frame always carries the full 8 bits, which is `N8`.
///
/// For raw 9-bit words with no parity, see [`Usart::new_word`], which moves
/// `u16` rather than `u8`.
#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum FrameFormat {
    /// 8 data bits, no parity.
    N8,
    /// 8 data bits, even parity.
    E8,
    /// 8 data bits, odd parity.
    O8,
    /// 7 data bits, even parity.
    E7,
    /// 7 data bits, odd parity.
    O7,
}

/// Configuration for [`Usart::new`].
///
/// [`Default`] is 115200 baud, ×16 oversampling, [`FrameFormat::N8`] — i.e. the
/// usual "115200 8N1".
pub struct UsartConfig {
    baud: u32,
    oversampling: Oversampling,
    frame_format: FrameFormat,
}

impl UsartConfig {
    /// Sets the bit rate. See the [`baud`] module for named constants.
    pub fn baud(mut self, baud: u32) -> Self {
        self.baud = baud;
        self
    }
    /// Sets the oversampling ratio.
    pub fn oversampling(mut self, oversampling: Oversampling) -> Self {
        self.oversampling = oversampling;
        self
    }
    /// Sets the word length and parity.
    pub fn frame_format(mut self, frame_format: FrameFormat) -> Self {
        self.frame_format = frame_format;
        self
    }
}

impl Default for UsartConfig {
    fn default() -> Self {
        Self {
            baud: baud::B115200,
            oversampling: Oversampling::X16,
            frame_format: FrameFormat::N8,
        }
    }
}

/// Configuration for [`Usart::new_word`].
///
/// Deliberately has no frame-format field: the 9-bit path is always
/// "9 data bits, no parity", so there would be nothing meaningful to choose.
pub struct UsartConfig9 {
    baud: u32,
    oversampling: Oversampling,
}

impl UsartConfig9 {
    /// Sets the bit rate. See the [`baud`] module for named constants.
    pub fn baud(mut self, baud: u32) -> Self {
        self.baud = baud;
        self
    }
    /// Sets the oversampling ratio.
    pub fn oversampling(mut self, oversampling: Oversampling) -> Self {
        self.oversampling = oversampling;
        self
    }
}

impl Default for UsartConfig9 {
    fn default() -> Self {
        Self {
            baud: baud::B115200,
            oversampling: Oversampling::X16,
        }
    }
}

fn configure<USARTX>(
    rcu: &mut Rcu,
    usart: &USARTX,
    clocks: &Clocks,
    baud: u32,
    oversampling: Oversampling,
) where
    USARTX: Deref<Target = pac::usart0::RegisterBlock> + Enable + Reset + BusClocks,
{
    USARTX::enable(rcu);
    USARTX::reset(rcu);
    let pclk = USARTX::clock(clocks).0;
    // round(pclk / baud) in integers: adding half the divisor before truncating rounds.
    let usartdiv = (pclk + baud / 2) / baud;
    usart.baud().write(|w| unsafe {
        match oversampling {
            Oversampling::X16 => w.bits(usartdiv),
            Oversampling::X8 => {
                let intdiv = usartdiv / 8;
                let fradiv_8 = usartdiv % 8;
                w.bits((intdiv << 4) | (fradiv_8 & 0x7))
            }
        }
    });

    // UEN is left off here: WL/parity must be written while the USART is
    // disabled, so the callers set those and enable UEN last.
    usart.ctl0().modify(|_, w| {
        let w = w.ten().enabled().ren().enabled();
        match oversampling {
            Oversampling::X16 => w.ovsmod().oversampling16(),
            Oversampling::X8 => w.ovsmod().oversampling8(),
        }
    });
}

/// Word-width marker: 8-bit words ([`Usart::write_byte`], [`Usart::read_byte`]),
/// optionally with parity.
pub struct Byte;
/// Word-width marker: raw 9-bit words ([`Usart::write_word`],
/// [`Usart::read_word`]), no parity possible.
pub struct Word;

/// A line error the receiver reported for one frame.
///
/// Its own type rather than a foreign `ErrorKind` so that what the `STAT`
/// register distinguishes stays distinguishable; the portable classifications
/// are still one `kind` call away, one per ecosystem —
/// [`embedded_hal_nb::serial::Error::kind`] and [`embedded_io::Error::kind`].
/// Both are lossy on purpose: neither foreign enum has a variant for every
/// line condition this one names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Error {
    /// A frame arrived before the previous one had been read, and was lost.
    Overrun,
    /// The sampling logic disagreed with itself about a bit's level.
    Noise,
    /// The stop bit was not where the configured frame said it would be —
    /// usually a baud rate or frame format mismatch between the two ends.
    Framing,
    /// The parity bit contradicts the data bits.
    Parity,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Overrun => write!(f, "receive overrun, a frame was lost"),
            Self::Noise => write!(f, "noise detected on the line"),
            Self::Framing => write!(f, "framing error, no stop bit where expected"),
            Self::Parity => write!(f, "parity check failed"),
        }
    }
}

impl core::error::Error for Error {}

impl embedded_io::Error for Error {
    fn kind(&self) -> embedded_io::ErrorKind {
        match self {
            Self::Framing | Self::Noise | Self::Parity => embedded_io::ErrorKind::InvalidData,
            Self::Overrun => embedded_io::ErrorKind::Other,
        }
    }
}

impl embedded_hal_nb::serial::Error for Error {
    fn kind(&self) -> embedded_hal_nb::serial::ErrorKind {
        match self {
            Self::Overrun => embedded_hal_nb::serial::ErrorKind::Overrun,
            Self::Noise => embedded_hal_nb::serial::ErrorKind::Noise,
            Self::Framing => embedded_hal_nb::serial::ErrorKind::FrameFormat,
            Self::Parity => embedded_hal_nb::serial::ErrorKind::Parity,
        }
    }
}

/// A configured USART, owning the peripheral and both pins.
///
/// `WORD` records the word width, so methods of the wrong width don't exist:
/// `write_byte`/`read_byte` are available only on `Usart<.., Byte>`, and
/// `write_word`/`read_word` only on `Usart<.., Word>`. It defaults to [`Byte`],
/// so the parameter can be omitted.
pub struct Usart<USARTX, TX, RX, WORD = Byte> {
    usart: USARTX,
    tx_pin: TX,
    rx_pin: RX,
    frame_format: FrameFormat,
    _word: PhantomData<WORD>,
}

impl<USARTX, TX, RX, WORD> Usart<USARTX, TX, RX, WORD>
where
    USARTX: Deref<Target = pac::usart0::RegisterBlock>,
{
    fn take_error(&self) -> Option<Error> {
        let stat = self.usart.stat().read();
        let error = if stat.orerr().bit_is_set() {
            self.usart.intc().write(|w| w.orec().clear());
            Some(Error::Overrun)
        } else if stat.nerr().bit_is_set() {
            self.usart.intc().write(|w| w.nec().clear());
            Some(Error::Noise)
        } else if stat.ferr().bit_is_set() {
            self.usart.intc().write(|w| w.fec().clear());
            Some(Error::Framing)
        } else if stat.perr().bit_is_set() {
            self.usart.intc().write(|w| w.pec().clear());
            Some(Error::Parity)
        } else {
            None
        };

        if error.is_some() {
            self.usart.rdata().read();
        }

        error
    }

    fn rbne(&self) -> bool {
        self.usart.stat().read().rbne().bit_is_set()
    }

    fn tbe(&self) -> bool {
        self.usart.stat().read().tbe().bit_is_set()
    }

    fn wait_tc(&self) {
        while self.usart.stat().read().tc().bit_is_clear() {}
    }

    /// Returns whether a received word is waiting to be read.
    ///
    /// Only guarantees that the *next* single read will not block; a buffered
    /// read may still block once it has taken what was already there.
    pub fn read_ready(&self) -> bool {
        self.rbne()
    }
    /// Returns whether the transmit buffer can accept a word right now.
    ///
    /// Only guarantees that the *next* single write will not block.
    pub fn write_ready(&self) -> bool {
        self.tbe()
    }

    /// Blocks until everything handed to the peripheral has left the wire.
    ///
    /// Waits for `TC`, not `TBE`: the latter only reports that `TDATA` was
    /// copied into the shift register, while the byte is still being clocked
    /// out. Call this before cutting power to a transceiver or sleeping, or the
    /// last frame is truncated mid-flight.
    ///
    /// Cannot fail — transmission has no error conditions.
    pub fn flush(&self) {
        self.wait_tc();
    }

    /// Disables the peripheral and returns it along with both pins.
    ///
    /// The clock is left enabled and no reset is performed — a later `new()`
    /// does both anyway.
    pub fn release(self) -> (USARTX, TX, RX) {
        self.usart.ctl0().modify(|_, w| w.uen().disabled());
        (self.usart, self.tx_pin, self.rx_pin)
    }
}

impl<USARTX, TX, RX> Usart<USARTX, TX, RX, Byte>
where
    USARTX: Deref<Target = pac::usart0::RegisterBlock>,
{
    fn received_byte(&self) -> u8 {
        let raw = self.usart.rdata().read().bits() as u8;
        match self.frame_format {
            FrameFormat::E7 | FrameFormat::O7 => raw & DATA_7BIT_MASK,
            FrameFormat::N8 | FrameFormat::E8 | FrameFormat::O8 => raw,
        }
    }

    /// Sends one byte, blocking until the transmit buffer can accept it.
    ///
    /// Returning does not mean the byte has left the wire — for that, see
    /// [`flush`](Usart::flush).
    pub fn write_byte(&self, byte: u8) {
        while !self.tbe() {}
        self.usart.tdata().write(|w| unsafe { w.bits(byte as u32) });
    }
    /// Sends every byte of `buf`, blocking until the last one is handed over.
    ///
    /// Returning does not mean the buffer has left the wire — for that, see
    /// [`flush`](Usart::flush). Cannot fail: transmission has no error
    /// conditions.
    pub fn write_bytes(&self, buf: &[u8]) {
        for &byte in buf {
            self.write_byte(byte);
        }
    }
    /// Receives one byte, blocking until one arrives.
    ///
    /// A line error consumes the offending frame and is reported instead of the
    /// data, so a damaged byte is never mistaken for a good one.
    pub fn read_byte(&self) -> Result<u8, Error> {
        while !self.rbne() {}
        if let Some(e) = self.take_error() {
            Err(e)
        } else {
            Ok(self.received_byte())
        }
    }
    /// Receives into `buf` and returns how many bytes were placed there.
    ///
    /// Blocks until at least one byte arrives, then takes whatever else is
    /// already waiting and returns — it deliberately does *not* wait for `buf`
    /// to fill. A peer that sends a short command and then waits for the answer
    /// would otherwise deadlock this call. Returns `0` only for an empty `buf`.
    ///
    /// A line error ends the call immediately; bytes copied before it are
    /// unreachable, since the count is not reported alongside an error.
    pub fn read_bytes(&self, buf: &mut [u8]) -> Result<usize, Error> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut index = 0;
        while !self.rbne() {}
        while index < buf.len() && self.rbne() {
            buf[index] = self.read_byte()?;
            index += 1;
        }
        Ok(index)
    }
}

impl<USARTX, TX, RX> Usart<USARTX, TX, RX, Byte>
where
    USARTX: Deref<Target = pac::usart0::RegisterBlock> + Enable + Reset + BusClocks,
    TX: TxPin<USARTX>,
    RX: RxPin<USARTX>,
{
    /// Enables the peripheral's clock, resets it and configures 8-bit words.
    ///
    /// The pins must already be in the alternate function this USART uses; the
    /// bounds reject any other pin at compile time. They are moved in and handed
    /// back by [`release`](Usart::release). `clocks` supplies the frequency the
    /// baud divisor is computed from.
    pub fn new(
        rcu: &mut Rcu,
        usart: USARTX,
        tx_pin: TX,
        rx_pin: RX,
        clocks: Clocks,
        config: UsartConfig,
    ) -> Self {
        configure(rcu, &usart, &clocks, config.baud, config.oversampling);

        usart.ctl0().modify(|_, w| match config.frame_format {
            FrameFormat::N8 => w.pcen().disabled().wl().bit8(),
            FrameFormat::E8 => w.pcen().enabled().pm().even().wl().bit9(),
            FrameFormat::O8 => w.pcen().enabled().pm().odd().wl().bit9(),
            FrameFormat::E7 => w.pcen().enabled().pm().even().wl().bit8(),
            FrameFormat::O7 => w.pcen().enabled().pm().odd().wl().bit8(),
        });
        usart.ctl0().modify(|_, w| w.uen().enabled());
        Self {
            usart,
            tx_pin,
            rx_pin,
            frame_format: config.frame_format,
            _word: PhantomData,
        }
    }
}

impl<USARTX, TX, RX> Usart<USARTX, TX, RX, Word>
where
    USARTX: Deref<Target = pac::usart0::RegisterBlock>,
{
    /// Sends one 9-bit word, blocking until the transmit buffer can accept it.
    ///
    /// Bits above the ninth are discarded.
    pub fn write_word(&self, word: u16) {
        while !self.tbe() {}
        self.usart
            .tdata()
            .write(|w| unsafe { w.bits(word as u32 & DATA_9BIT_MASK) });
    }
    /// Sends every word of `buf`, blocking until the last one is handed over.
    ///
    /// Returning does not mean the buffer has left the wire — for that, see
    /// [`flush`](Usart::flush). Cannot fail: transmission has no error
    /// conditions.
    pub fn write_words(&self, buf: &[u16]) {
        for &word in buf {
            self.write_word(word);
        }
    }
    /// Receives one 9-bit word, blocking until one arrives.
    pub fn read_word(&self) -> Result<u16, Error> {
        while !self.rbne() {}
        if let Some(e) = self.take_error() {
            Err(e)
        } else {
            Ok((self.usart.rdata().read().bits() & DATA_9BIT_MASK) as u16)
        }
    }
    /// Receives into `buf` and returns how many words were placed there.
    ///
    /// Same blocking rule as [`read_bytes`](Usart::read_bytes): waits for the
    /// first word, then takes only what is already waiting.
    pub fn read_words(&self, buf: &mut [u16]) -> Result<usize, Error> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut index = 0;
        while !self.rbne() {}
        while index < buf.len() && self.rbne() {
            buf[index] = self.read_word()?;
            index += 1;
        }
        Ok(index)
    }
}

impl<USARTX, TX, RX> Usart<USARTX, TX, RX, Word>
where
    USARTX: Deref<Target = pac::usart0::RegisterBlock> + Enable + Reset + BusClocks,
    TX: TxPin<USARTX>,
    RX: RxPin<USARTX>,
{
    /// Same as [`new`](Usart::new), but configures raw 9-bit words with no parity.
    ///
    /// All nine bits carry data, so there is no frame format to choose and the
    /// peripheral moves `u16` rather than `u8`.
    pub fn new_word(
        rcu: &mut Rcu,
        usart: USARTX,
        tx_pin: TX,
        rx_pin: RX,
        clocks: Clocks,
        config: UsartConfig9,
    ) -> Self {
        configure(rcu, &usart, &clocks, config.baud, config.oversampling);
        usart.ctl0().modify(|_, w| w.pcen().disabled().wl().bit9());
        usart.ctl0().modify(|_, w| w.uen().enabled());
        Self {
            usart,
            tx_pin,
            rx_pin,
            // Never read: only `Byte`'s `received_byte` looks at `frame_format`.
            frame_format: FrameFormat::N8,
            _word: PhantomData,
        }
    }
}

impl<USARTX, TX, RX, WORD> embedded_hal_nb::serial::ErrorType for Usart<USARTX, TX, RX, WORD> {
    type Error = Error;
}

impl<USARTX, TX, RX, WORD> embedded_io::ErrorType for Usart<USARTX, TX, RX, WORD> {
    type Error = Error;
}

impl<USARTX, TX, RX> embedded_hal_nb::serial::Read<u8> for Usart<USARTX, TX, RX, Byte>
where
    USARTX: Deref<Target = pac::usart0::RegisterBlock>,
{
    fn read(&mut self) -> nb::Result<u8, Self::Error> {
        if !self.rbne() {
            return Err(nb::Error::WouldBlock);
        }
        if let Some(e) = self.take_error() {
            Err(nb::Error::Other(e))
        } else {
            Ok(self.received_byte())
        }
    }
}

impl<USARTX, TX, RX> embedded_io::Read for Usart<USARTX, TX, RX, Byte>
where
    USARTX: Deref<Target = pac::usart0::RegisterBlock>,
{
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.read_bytes(buf)
    }
}

impl<USARTX, TX, RX> embedded_hal_nb::serial::Write<u8> for Usart<USARTX, TX, RX, Byte>
where
    USARTX: Deref<Target = pac::usart0::RegisterBlock>,
{
    fn write(&mut self, byte: u8) -> nb::Result<(), Self::Error> {
        if !self.tbe() {
            Err(nb::Error::WouldBlock)
        } else {
            self.usart.tdata().write(|w| unsafe { w.bits(byte as u32) });
            Ok(())
        }
    }
    fn flush(&mut self) -> nb::Result<(), Self::Error> {
        if self.usart.stat().read().tc().bit_is_clear() {
            Err(nb::Error::WouldBlock)
        } else {
            Ok(())
        }
    }
}

impl<USARTX, TX, RX> embedded_io::Write for Usart<USARTX, TX, RX, Byte>
where
    USARTX: Deref<Target = pac::usart0::RegisterBlock>,
{
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        match buf.first() {
            Some(&b) => {
                self.write_byte(b);
                Ok(1usize)
            }
            None => Ok(0usize),
        }
    }
    fn flush(&mut self) -> Result<(), Self::Error> {
        self.wait_tc();
        Ok(())
    }
}

impl<USARTX, TX, RX> embedded_io::ReadReady for Usart<USARTX, TX, RX, Byte>
where
    USARTX: Deref<Target = pac::usart0::RegisterBlock>,
{
    fn read_ready(&mut self) -> Result<bool, Self::Error> {
        Ok(self.rbne())
    }
}

impl<USARTX, TX, RX> embedded_io::WriteReady for Usart<USARTX, TX, RX, Byte>
where
    USARTX: Deref<Target = pac::usart0::RegisterBlock>,
{
    fn write_ready(&mut self) -> Result<bool, Self::Error> {
        Ok(self.tbe())
    }
}
