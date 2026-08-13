//! SPI master.
//!
//! Covers both SPI0 and SPI1 in master, full-duplex, blocking mode with software
//! NSS (chip select is an ordinary GPIO the caller toggles). Frames are 8 or 16
//! bits wide, selected by a typestate parameter.
//!
//! Every operation is a simultaneous *exchange* — a word leaves on MOSI while
//! another arrives on MISO — so [`SpiBus::read`] sends zeros and
//! [`SpiBus::write`] discards what comes back.
//!
//! ```ignore
//! let sck = parts.pa5.into_alternate::<0>();
//! let miso = parts.pa6.into_alternate::<0>();
//! let mosi = parts.pa7.into_alternate::<0>();
//! let spi = Spi::new(&mut rcu, dp.spi0, sck, miso, mosi, SpiConfig::new(SpiPsc::Div8));
//! ```

use core::marker::PhantomData;

use embedded_hal::spi::{ErrorKind, ErrorType, MODE_0, Mode, Phase, Polarity, SpiBus};

use crate::gpio::{Alternate, Pin};
use crate::pac;
use crate::rcu::{Enable, Rcu, Reset};

/// SPI1 `DZ` encodes the frame length as (bits - 1); values below 0b0011 are
/// forced to 8-bit by hardware.
const DZ_8BIT: u8 = 0b0111;
const DZ_16BIT: u8 = 0b1111;

/// SCK prescaler: divides `pclk` down to the serial clock.
///
/// Discriminants are the `PSC` register encoding. There is no universal default
/// — the right divider depends on `pclk` and the slave's maximum clock.
#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[allow(missing_docs)]
pub enum SpiPsc {
    Div2 = 0b000,
    Div4 = 0b001,
    Div8 = 0b010,
    Div16 = 0b011,
    Div32 = 0b100,
    Div64 = 0b101,
    Div128 = 0b110,
    Div256 = 0b111,
}

/// Order in which the bits of a word are shifted onto the wire.
///
/// Both ends of the link must agree, or every word arrives bit-reversed with no
/// error reported. Most devices are MSB-first, which is the default.
#[derive(Clone, Copy, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum BitOrder {
    /// Most significant bit first.
    MsbFirst,
    /// Least significant bit first.
    LsbFirst,
}

/// Bus configuration passed to [`Spi::new`] / [`Spi::new_word`].
///
/// Built with [`SpiConfig::new`], which requires a prescaler; [`mode`](Self::mode)
/// and [`bit_order`](Self::bit_order) refine the defaults fluently.
///
/// ```ignore
/// SpiConfig::new(SpiPsc::Div16)
///     .mode(embedded_hal::spi::MODE_3)
///     .bit_order(BitOrder::LsbFirst)
/// ```
pub struct SpiConfig {
    psc: SpiPsc,
    mode: Mode,
    bit_order: BitOrder,
}

impl SpiConfig {
    /// Creates a configuration with the given prescaler, Mode 0 and MSB-first.
    ///
    /// The prescaler is a required argument and this type has no `Default`: an
    /// SCK divider has no conventional value, so it must be chosen deliberately.
    pub fn new(psc: SpiPsc) -> Self {
        Self {
            psc,
            mode: MODE_0,
            bit_order: BitOrder::MsbFirst,
        }
    }
    /// Sets the clock polarity and phase (CPOL/CPHA), per the slave's datasheet.
    pub fn mode(mut self, mode: Mode) -> Self {
        self.mode = mode;
        self
    }
    /// Sets the bit order. Defaults to [`BitOrder::MsbFirst`].
    pub fn bit_order(mut self, bit_order: BitOrder) -> Self {
        self.bit_order = bit_order;
        self
    }
}

/// Marks a pin usable as `SCK` for `SPI`, in the right alternate function.
pub trait SckPin<SPI> {}
/// Marks a pin usable as `MISO` for `SPI`, in the right alternate function.
pub trait MisoPin<SPI> {}
/// Marks a pin usable as `MOSI` for `SPI`, in the right alternate function.
pub trait MosiPin<SPI> {}

macro_rules! spi_pins {
    ( $( $SPI:ty:
        SCK:  [ $($(#[$sck_cfg:meta])? $sck_p:literal  $sck_n:literal  : $sck_af:literal),* $(,)? ]
        MISO: [ $($(#[$miso_cfg:meta])? $miso_p:literal $miso_n:literal : $miso_af:literal),* $(,)? ]
        MOSI: [ $($(#[$mosi_cfg:meta])? $mosi_p:literal $mosi_n:literal : $mosi_af:literal),* $(,)? ]
    ),* $(,)? ) => {
        $(
            $( $(#[$sck_cfg])?  impl SckPin<$SPI>  for Pin<$sck_p,  $sck_n,  Alternate<$sck_af>>  {} )*
            $( $(#[$miso_cfg])? impl MisoPin<$SPI> for Pin<$miso_p, $miso_n, Alternate<$miso_af>> {} )*
            $( $(#[$mosi_cfg])? impl MosiPin<$SPI> for Pin<$mosi_p, $mosi_n, Alternate<$mosi_af>> {} )*
        )*
    };
}

// PB13/PB14/PB15 at AF0 belong to a *different* SPI depending on the chip
// variant (datasheet Table 2-14 footnotes): SPI0 on GD32E230x4, SPI1 on
// GD32E230x8. They are therefore listed in the gated blocks, not here.
//
// The `pads_ge_*` gates say the package bonds the pin at all, and match the ones in
// `gpio::Parts` — an entry for an unbonded pad would advertise in the docs a pin
// nobody can obtain.
spi_pins!(
    pac::Spi0:
        SCK: ['A' 5 : 0, #[cfg(pads_ge_28)] 'B' 3 : 0]
        MISO: ['A' 6 : 0, #[cfg(pads_ge_28)] 'B' 4 : 0]
        MOSI: ['A' 7 : 0, #[cfg(pads_ge_28)] 'B' 5 : 0]
);

// ---- (1) GD32E230x4 only: PB13/14/15 AF0 are SPI0 ----
#[cfg(chip_x4)]
spi_pins!(
    pac::Spi0:
        SCK: [#[cfg(pads_ge_48)] 'B' 13 : 0]
        MISO: [#[cfg(pads_ge_48)] 'B' 14 : 0]
        MOSI: [#[cfg(pads_ge_48)] 'B' 15 : 0]
);

// ---- (3) GD32E230x8 only: SPI1 exists, and PB13/14/15 AF0 belong to it ----
// NB: below 48 pins this leaves PB1 plus PA13/PA14 — the SWD pair, reachable only
// through the `activate_into_*` family, at the cost of the debug port.
#[cfg(chip_x8)]
spi_pins!(
    pac::Spi1:
        SCK: ['B' 1 : 6, #[cfg(pads_ge_48)] 'B' 10 : 7, #[cfg(pads_ge_48)] 'B' 13 : 0]
        MISO: ['A' 13 : 6, #[cfg(pads_ge_48)] 'B' 14 : 0]
        MOSI: ['A' 14 : 6, #[cfg(pads_ge_48)] 'B' 15 : 0]
);

/// Word-width marker: 8-bit frames ([`Spi::transfer_byte`], `SpiBus<u8>`).
pub struct Byte;
/// Word-width marker: 16-bit frames ([`Spi::transfer_word`], `SpiBus<u16>`).
pub struct Word;

/// An error the peripheral flagged in `STAT`.
///
/// Its own type rather than [`ErrorKind`], which has no variant for a CRC
/// mismatch; [`kind`] gives the portable classification.
///
/// [`kind`]: embedded_hal::spi::Error::kind
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Error {
    /// A received word was overwritten before it had been read (`RXORERR`).
    Overrun,
    /// `NSS` was pulled low while configured as a master (`CONFERR`) — another
    /// master is driving the bus.
    ModeFault,
    /// The received CRC does not match the one computed locally (`CRCERR`).
    Crc,
    /// A frame boundary arrived where none was expected (`FERR`), which only
    /// the TI frame format can report.
    Framing,
}

impl embedded_hal::spi::Error for Error {
    fn kind(&self) -> ErrorKind {
        match self {
            Self::Overrun => ErrorKind::Overrun,
            Self::ModeFault => ErrorKind::ModeFault,
            // No CRC variant exists upstream; this is the loss our own type
            // is here to keep out of the driver-facing API.
            Self::Crc => ErrorKind::Other,
            Self::Framing => ErrorKind::FrameFormat,
        }
    }
}

/// A peripheral that [`Spi`] can drive.
///
/// SPI0 and SPI1 have distinct register block types whose bits do not line up —
/// frame width is `FF16` in `CTL0` on SPI0 but `DZ` in `CTL1` on SPI1, where that
/// position means something else. No generic bound over a shared block is
/// possible, so this trait abstracts the peripheral at the *operation* level:
/// every register access lives in the impls, and [`Spi`] touches none.
pub trait Instance: Enable + Reset {
    /// Writes the full master configuration, leaving the peripheral enabled.
    ///
    /// `wide` selects the frame width, and the impl handles what follows from it
    /// (on SPI1 the FIFO access size must match, or reception stalls).
    fn apply_config(&self, config: SpiConfig, wide: bool);
    /// Transmit buffer empty — ready to accept the next word.
    fn tbe(&self) -> bool;
    /// Receive buffer not empty — a word has arrived.
    fn rbne(&self) -> bool;
    /// Writes a word to the data register, which starts the clock in master mode.
    fn write_data(&self, word: u16);
    /// Reads the received word from the data register.
    fn read_data(&self) -> u16;
    /// Returns the first pending error, if any, clearing it as the manual requires.
    fn take_error(&self) -> Option<Error>;
    /// Enables or disables the peripheral (`SPIEN`).
    fn set_enabled(&self, on: bool);
}

impl Instance for pac::Spi0 {
    fn apply_config(&self, config: SpiConfig, wide: bool) {
        self.ctl0().modify(|_, w| {
            let w = w
                .mstmod()
                .set_bit()
                .swnssen()
                .set_bit()
                .swnss()
                .set_bit()
                .lf()
                .bit(config.bit_order == BitOrder::LsbFirst)
                .ff16()
                .bit(wide)
                .spien()
                .set_bit()
                .ckpl()
                .bit(config.mode.polarity == Polarity::IdleHigh)
                .ckph()
                .bit(config.mode.phase == Phase::CaptureOnSecondTransition);
            unsafe { w.psc().bits(config.psc as u8) }
        });
    }
    fn tbe(&self) -> bool {
        self.stat().read().tbe().bit_is_set()
    }
    fn rbne(&self) -> bool {
        self.stat().read().rbne().bit_is_set()
    }
    fn write_data(&self, word: u16) {
        self.data().write(|w| unsafe { w.data().bits(word) });
    }
    fn read_data(&self) -> u16 {
        self.data().read().data().bits()
    }
    fn take_error(&self) -> Option<Error> {
        let stat = self.stat().read();
        if stat.rxorerr().bit_is_set() {
            // clear: read DATA (done in transfer_byte) + read STAT (above)
            Some(Error::Overrun)
        } else if stat.conferr().bit_is_set() {
            // clear: read STAT (above) + write CTL0
            self.ctl0().modify(|_, w| w);
            Some(Error::ModeFault)
        } else if stat.crcerr().bit_is_set() {
            self.stat().modify(|_, w| w.crcerr().clear_bit());
            Some(Error::Crc)
        } else if stat.ferr().bit_is_set() {
            self.stat().modify(|_, w| w.ferr().clear_bit());
            Some(Error::Framing)
        } else {
            None
        }
    }
    fn set_enabled(&self, on: bool) {
        self.ctl0().modify(|_, w| w.spien().bit(on));
    }
}

impl Instance for pac::Spi1 {
    fn apply_config(&self, config: SpiConfig, wide: bool) {
        self.ctl1().modify(|_, w| {
            let w = w.byten().bit(!wide);
            unsafe { w.dz().bits(if wide { DZ_16BIT } else { DZ_8BIT }) }
        });
        self.ctl0().modify(|_, w| {
            let w = w
                .mstmod()
                .set_bit()
                .swnssen()
                .set_bit()
                .swnss()
                .set_bit()
                .lf()
                .bit(config.bit_order == BitOrder::LsbFirst)
                .spien()
                .set_bit()
                .ckpl()
                .bit(config.mode.polarity == Polarity::IdleHigh)
                .ckph()
                .bit(config.mode.phase == Phase::CaptureOnSecondTransition);
            unsafe { w.psc().bits(config.psc as u8) }
        });
    }
    fn tbe(&self) -> bool {
        self.stat().read().tbe().bit_is_set()
    }
    fn rbne(&self) -> bool {
        self.stat().read().rbne().bit_is_set()
    }
    fn write_data(&self, word: u16) {
        self.data().write(|w| unsafe { w.data().bits(word) });
    }
    fn read_data(&self) -> u16 {
        self.data().read().data().bits()
    }
    fn take_error(&self) -> Option<Error> {
        let stat = self.stat().read();
        if stat.rxorerr().bit_is_set() {
            // clear: read DATA (done in transfer_byte) + read STAT (above)
            Some(Error::Overrun)
        } else if stat.conferr().bit_is_set() {
            // clear: read STAT (above) + write CTL0
            self.ctl0().modify(|_, w| w);
            Some(Error::ModeFault)
        } else if stat.crcerr().bit_is_set() {
            self.stat().modify(|_, w| w.crcerr().clear_bit());
            Some(Error::Crc)
        } else if stat.ferr().bit_is_set() {
            self.stat().modify(|_, w| w.ferr().clear_bit());
            Some(Error::Framing)
        } else {
            None
        }
    }
    fn set_enabled(&self, on: bool) {
        self.ctl0().modify(|_, w| w.spien().bit(on));
    }
}

/// A configured SPI master, owning the peripheral and its three pins.
///
/// `WORD` records the frame width, so methods of the wrong width don't exist:
/// [`transfer_byte`](Self::transfer_byte) and `SpiBus<u8>` are available only on
/// `Spi<.., Byte>`, [`transfer_word`](Self::transfer_word) and `SpiBus<u16>` only
/// on `Spi<.., Word>`. It defaults to [`Byte`], so the parameter can be omitted.
///
/// Chip select is not handled here — NSS is software-managed, so drive the
/// slave's CS with an ordinary output pin around each transaction.
#[derive(Debug)]
pub struct Spi<SPIX, SCK, MISO, MOSI, WORD = Byte> {
    spi: SPIX,
    sck_pin: SCK,
    miso_pin: MISO,
    mosi_pin: MOSI,
    _word: PhantomData<WORD>,
}

impl<SPIX, SCK, MISO, MOSI> Spi<SPIX, SCK, MISO, MOSI, Byte>
where
    SPIX: Instance,
    SCK: SckPin<SPIX>,
    MISO: MisoPin<SPIX>,
    MOSI: MosiPin<SPIX>,
{
    /// Enables the peripheral's clock, resets it and configures 8-bit master mode.
    ///
    /// The pins must already be in this SPI's alternate function; the bounds
    /// reject anything else at compile time. [`release`](Spi::release) hands them
    /// back.
    pub fn new(
        rcu: &mut Rcu,
        spi: SPIX,
        sck_pin: SCK,
        miso_pin: MISO,
        mosi_pin: MOSI,
        config: SpiConfig,
    ) -> Self {
        SPIX::enable(rcu);
        SPIX::reset(rcu);
        spi.apply_config(config, false);
        Self {
            spi,
            sck_pin,
            miso_pin,
            mosi_pin,
            _word: PhantomData,
        }
    }
}

impl<SPIX, SCK, MISO, MOSI> Spi<SPIX, SCK, MISO, MOSI, Word>
where
    SPIX: Instance,
    SCK: SckPin<SPIX>,
    MISO: MisoPin<SPIX>,
    MOSI: MosiPin<SPIX>,
{
    /// Same as [`new`](Spi::new), but configures 16-bit frames.
    pub fn new_word(
        rcu: &mut Rcu,
        spi: SPIX,
        sck_pin: SCK,
        miso_pin: MISO,
        mosi_pin: MOSI,
        config: SpiConfig,
    ) -> Self {
        SPIX::enable(rcu);
        SPIX::reset(rcu);
        spi.apply_config(config, true);
        Self {
            spi,
            sck_pin,
            miso_pin,
            mosi_pin,
            _word: PhantomData,
        }
    }
}

impl<SPIX, SCK, MISO, MOSI, WORD> Spi<SPIX, SCK, MISO, MOSI, WORD>
where
    SPIX: Instance,
{
    /// Disables the peripheral and returns it along with the three pins.
    ///
    /// The clock is left enabled and no reset is performed — a later `new()`
    /// does both anyway.
    pub fn release(self) -> (SPIX, SCK, MISO, MOSI) {
        self.spi.set_enabled(false);
        (self.spi, self.sck_pin, self.miso_pin, self.mosi_pin)
    }
}

impl<SPIX, SCK, MISO, MOSI> Spi<SPIX, SCK, MISO, MOSI, Byte>
where
    SPIX: Instance,
{
    /// Exchanges one byte: sends `byte` on MOSI and returns what arrived on MISO.
    ///
    /// Blocks until the exchange has completed, so nothing is left pending on the
    /// bus when it returns.
    pub fn transfer_byte(&self, byte: u8) -> Result<u8, Error> {
        while !self.spi.tbe() {}
        self.spi.write_data(byte as u16);
        while !self.spi.rbne() {}
        let received = self.spi.read_data() as u8;
        match self.spi.take_error() {
            Some(e) => Err(e),
            None => Ok(received),
        }
    }

    /// Exchanges two buffers of independent length.
    ///
    /// The bus clocks `max(read.len(), write.len())` bytes either way: once
    /// `write` runs out `0x00` is sent, and once `read` is full the incoming
    /// bytes are discarded.
    pub fn transfer_bytes(&self, read: &mut [u8], write: &[u8]) -> Result<(), Error> {
        let n = read.len().max(write.len());
        for i in 0..n {
            let sent = write.get(i).copied().unwrap_or(0x00);
            let received = self.transfer_byte(sent)?;
            if let Some(slot) = read.get_mut(i) {
                *slot = received;
            }
        }
        Ok(())
    }
    /// Exchanges `words` against itself: each byte is replaced by what came back.
    pub fn transfer_bytes_in_place(&self, words: &mut [u8]) -> Result<(), Error> {
        for word in words {
            *word = self.transfer_byte(*word)?;
        }
        Ok(())
    }
    /// Clocks `words.len()` bytes in, sending `0x00` to drive the bus.
    pub fn read_bytes(&self, words: &mut [u8]) -> Result<(), Error> {
        for slot in words {
            *slot = self.transfer_byte(0x00)?;
        }
        Ok(())
    }
    /// Clocks `words` out, discarding whatever arrives on MISO.
    pub fn write_bytes(&self, words: &[u8]) -> Result<(), Error> {
        for &b in words {
            self.transfer_byte(b)?;
        }
        Ok(())
    }
}

impl<SPIX, SCK, MISO, MOSI> Spi<SPIX, SCK, MISO, MOSI, Word>
where
    SPIX: Instance,
{
    /// Exchanges one 16-bit word: sends `word` on MOSI, returns what arrived on MISO.
    ///
    /// Blocks until the exchange has completed, so nothing is left pending on the
    /// bus when it returns.
    pub fn transfer_word(&self, word: u16) -> Result<u16, Error> {
        while !self.spi.tbe() {}
        self.spi.write_data(word);
        while !self.spi.rbne() {}
        let received = self.spi.read_data();
        match self.spi.take_error() {
            Some(e) => Err(e),
            None => Ok(received),
        }
    }

    /// Exchanges two buffers of independent length.
    ///
    /// The bus clocks `max(read.len(), write.len())` words either way: once
    /// `write` runs out `0x0000` is sent, and once `read` is full the incoming
    /// words are discarded.
    pub fn transfer_words(&self, read: &mut [u16], write: &[u16]) -> Result<(), Error> {
        let n = read.len().max(write.len());
        for i in 0..n {
            let sent = write.get(i).copied().unwrap_or(0x0000);
            let received = self.transfer_word(sent)?;
            if let Some(slot) = read.get_mut(i) {
                *slot = received;
            }
        }
        Ok(())
    }
    /// Exchanges `words` against itself: each word is replaced by what came back.
    pub fn transfer_words_in_place(&self, words: &mut [u16]) -> Result<(), Error> {
        for word in words {
            *word = self.transfer_word(*word)?;
        }
        Ok(())
    }
    /// Clocks `words.len()` words in, sending `0x0000` to drive the bus.
    pub fn read_words(&self, words: &mut [u16]) -> Result<(), Error> {
        for slot in words {
            *slot = self.transfer_word(0x0000)?;
        }
        Ok(())
    }
    /// Clocks `words` out, discarding whatever arrives on MISO.
    pub fn write_words(&self, words: &[u16]) -> Result<(), Error> {
        for &w in words {
            self.transfer_word(w)?;
        }
        Ok(())
    }
}

impl<SPIX, SCK, MISO, MOSI, WORD> ErrorType for Spi<SPIX, SCK, MISO, MOSI, WORD>
where
    SPIX: Instance,
{
    type Error = Error;
}

impl<SPIX, SCK, MISO, MOSI> SpiBus<u8> for Spi<SPIX, SCK, MISO, MOSI, Byte>
where
    SPIX: Instance,
{
    fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Self::Error> {
        self.transfer_bytes(read, write)
    }
    fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
        self.transfer_bytes_in_place(words)
    }
    fn flush(&mut self) -> Result<(), Self::Error> {
        // No-op: transfer_byte blocks until RBNE (the byte is fully exchanged),
        // so nothing is ever pending on the bus when a method returns.
        Ok(())
    }
    fn read(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
        self.read_bytes(words)
    }
    fn write(&mut self, words: &[u8]) -> Result<(), Self::Error> {
        self.write_bytes(words)
    }
}

impl<SPIX, SCK, MISO, MOSI> SpiBus<u16> for Spi<SPIX, SCK, MISO, MOSI, Word>
where
    SPIX: Instance,
{
    fn transfer(&mut self, read: &mut [u16], write: &[u16]) -> Result<(), Self::Error> {
        self.transfer_words(read, write)
    }
    fn transfer_in_place(&mut self, words: &mut [u16]) -> Result<(), Self::Error> {
        self.transfer_words_in_place(words)
    }
    fn flush(&mut self) -> Result<(), Self::Error> {
        // No-op: transfer_word blocks until RBNE (the word is fully exchanged),
        // so nothing is ever pending on the bus when a method returns.
        Ok(())
    }
    fn read(&mut self, words: &mut [u16]) -> Result<(), Self::Error> {
        self.read_words(words)
    }
    fn write(&mut self, words: &[u16]) -> Result<(), Self::Error> {
        self.write_words(words)
    }
}
