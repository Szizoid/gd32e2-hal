use core::marker::PhantomData;
use embedded_hal::spi::{ErrorKind, ErrorType, MODE_0, Mode, Phase, Polarity, SpiBus};
use gd32e2::gd32e230;

use crate::{
    gpio::{Alternate, Pin},
    rcu::{Enable, Rcu, Reset},
};

/// SPI1 `DZ` encodes the frame length as (bits - 1); values below 0b0011 are
/// forced to 8-bit by hardware.
const DZ_8BIT: u8 = 0b0111;
const DZ_16BIT: u8 = 0b1111;

#[derive(Clone, Copy)]
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

#[derive(Clone, Copy, PartialEq)]
pub enum BitOrder {
    MsbFirst,
    LsbFirst,
}

pub struct SpiConfig {
    psc: SpiPsc,
    mode: Mode,
    bit_order: BitOrder,
}

impl SpiConfig {
    /// `psc` is required: there is no universal default SCK divider (it depends
    /// on `pclk` and the slave's max clock), so it must be chosen explicitly.
    /// `mode`/`bit_order` default to Mode 0 / MSB-first and can be overridden
    /// fluently. (No `Default` impl — a config can't be built without `psc`.)
    pub fn new(psc: SpiPsc) -> Self {
        Self {
            psc,
            mode: MODE_0,
            bit_order: BitOrder::MsbFirst,
        }
    }
    pub fn mode(mut self, mode: Mode) -> Self {
        self.mode = mode;
        self
    }
    pub fn bit_order(mut self, bit_order: BitOrder) -> Self {
        self.bit_order = bit_order;
        self
    }
}

pub trait SckPin<SPI> {}
pub trait MisoPin<SPI> {}
pub trait MosiPin<SPI> {}

macro_rules! spi_pins {
    ( $( $SPI:ty:
        SCK:  [ $($sck_p:literal  $sck_n:literal  : $sck_af:literal),* $(,)? ]
        MISO: [ $($miso_p:literal $miso_n:literal : $miso_af:literal),* $(,)? ]
        MOSI: [ $($mosi_p:literal $mosi_n:literal : $mosi_af:literal),* $(,)? ]
    ),* $(,)? ) => {
        $(
            $( impl SckPin<$SPI>  for Pin<$sck_p,  $sck_n,  Alternate<$sck_af>>  {} )*
            $( impl MisoPin<$SPI> for Pin<$miso_p, $miso_n, Alternate<$miso_af>> {} )*
            $( impl MosiPin<$SPI> for Pin<$mosi_p, $mosi_n, Alternate<$mosi_af>> {} )*
        )*
    };
}

// PB13/PB14/PB15 at AF0 belong to a *different* SPI depending on the chip
// variant (datasheet Table 2-14 footnotes): SPI0 on GD32E230x4, SPI1 on
// GD32E230x8. They are therefore listed in the gated blocks, not here.
spi_pins!(
    gd32e230::Spi0:
        SCK: ['A' 5 : 0, 'B' 3 : 0]
        MISO: ['A' 6 : 0, 'B' 4 : 0]
        MOSI: ['A' 7 : 0, 'B' 5 : 0]
);

// ---- (1) GD32E230x4 only: PB13/14/15 AF0 are SPI0 ----
#[cfg(feature = "gd32e230x4")]
spi_pins!(
    gd32e230::Spi0:
        SCK: ['B' 13 : 0]
        MISO: ['B' 14 : 0]
        MOSI: ['B' 15 : 0]
);

// ---- (3) GD32E230x8 only: SPI1 exists, and PB13/14/15 AF0 belong to it ----
// NB: PA13/PA14 are SWDIO/SWCLK — reaching them needs `unsafe activate()` on
// the `Debugger` typestate first, and doing so gives up SWD debugging.
#[cfg(feature = "gd32e230x8")]
spi_pins!(
    gd32e230::Spi1:
        SCK: ['B' 1 : 6, 'B' 10 : 7, 'B' 13 : 0]
        MISO: ['A' 13 : 6, 'B' 14 : 0]
        MOSI: ['A' 14 : 6, 'B' 15 : 0]
);

/// Word-width marker: 8-bit frames (`transfer_byte`, `SpiBus<u8>`).
pub struct Byte;
/// Word-width marker: 16-bit frames (`transfer_word`, `SpiBus<u16>`).
pub struct Word;

pub trait Instance: Enable + Reset {
    fn apply_config(&self, config: SpiConfig, wide: bool); // `wide`: false = 8-bit, true = 16-bit.
    fn tbe(&self) -> bool;
    fn rbne(&self) -> bool;
    fn write_data(&self, word: u16);
    fn read_data(&self) -> u16;
    fn take_error(&self) -> Option<ErrorKind>;
    fn set_enabled(&self, on: bool);
}

impl Instance for gd32e230::Spi0 {
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
    fn take_error(&self) -> Option<ErrorKind> {
        let stat = self.stat().read();
        if stat.rxorerr().bit_is_set() {
            // clear: read DATA (done in transfer_byte) + read STAT (above)
            Some(ErrorKind::Overrun)
        } else if stat.conferr().bit_is_set() {
            // clear: read STAT (above) + write CTL0
            self.ctl0().modify(|_, w| w);
            Some(ErrorKind::ModeFault)
        } else if stat.crcerr().bit_is_set() {
            self.stat().modify(|_, w| w.crcerr().clear_bit());
            Some(ErrorKind::Other)
        } else if stat.ferr().bit_is_set() {
            self.stat().modify(|_, w| w.ferr().clear_bit());
            Some(ErrorKind::FrameFormat)
        } else {
            None
        }
    }
    fn set_enabled(&self, on: bool) {
        self.ctl0().modify(|_, w| w.spien().bit(on));
    }
}

impl Instance for gd32e230::Spi1 {
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
    fn take_error(&self) -> Option<ErrorKind> {
        let stat = self.stat().read();
        if stat.rxorerr().bit_is_set() {
            // clear: read DATA (done in transfer_byte) + read STAT (above)
            Some(ErrorKind::Overrun)
        } else if stat.conferr().bit_is_set() {
            // clear: read STAT (above) + write CTL0
            self.ctl0().modify(|_, w| w);
            Some(ErrorKind::ModeFault)
        } else if stat.crcerr().bit_is_set() {
            self.stat().modify(|_, w| w.crcerr().clear_bit());
            Some(ErrorKind::Other)
        } else if stat.ferr().bit_is_set() {
            self.stat().modify(|_, w| w.ferr().clear_bit());
            Some(ErrorKind::FrameFormat)
        } else {
            None
        }
    }
    fn set_enabled(&self, on: bool) {
        self.ctl0().modify(|_, w| w.spien().bit(on));
    }
}

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
    pub fn release(self) -> (SPIX, SCK, MISO, MOSI) {
        self.spi.set_enabled(false);
        (self.spi, self.sck_pin, self.miso_pin, self.mosi_pin)
    }
}

impl<SPIX, SCK, MISO, MOSI> Spi<SPIX, SCK, MISO, MOSI, Byte>
where
    SPIX: Instance,
{
    pub fn transfer_byte(&self, byte: u8) -> Result<u8, ErrorKind> {
        while !self.spi.tbe() {}
        self.spi.write_data(byte as u16);
        while !self.spi.rbne() {}
        let received = self.spi.read_data() as u8;
        match self.spi.take_error() {
            Some(e) => Err(e),
            None => Ok(received),
        }
    }
}

impl<SPIX, SCK, MISO, MOSI> Spi<SPIX, SCK, MISO, MOSI, Word>
where
    SPIX: Instance,
{
    pub fn transfer_word(&self, word: u16) -> Result<u16, ErrorKind> {
        while !self.spi.tbe() {}
        self.spi.write_data(word);
        while !self.spi.rbne() {}
        let received = self.spi.read_data();
        match self.spi.take_error() {
            Some(e) => Err(e),
            None => Ok(received),
        }
    }
}

impl<SPIX, SCK, MISO, MOSI, WORD> ErrorType for Spi<SPIX, SCK, MISO, MOSI, WORD>
where
    SPIX: Instance,
{
    type Error = ErrorKind;
}

impl<SPIX, SCK, MISO, MOSI> SpiBus<u8> for Spi<SPIX, SCK, MISO, MOSI, Byte>
where
    SPIX: Instance,
{
    fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Self::Error> {
        let n = read.len().max(write.len());
        for i in 0..n {
            // MOSI: send write[i], or a dummy 0x00 once write is exhausted
            let sent = write.get(i).copied().unwrap_or(0x00);
            let received = self.transfer_byte(sent)?;
            // MISO: store into read[i] if it still has room, else discard
            if let Some(slot) = read.get_mut(i) {
                *slot = received;
            }
        }
        Ok(())
    }
    fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
        for word in words {
            *word = self.transfer_byte(*word)?;
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<(), Self::Error> {
        // No-op: transfer_byte blocks until RBNE (the byte is fully exchanged),
        // so nothing is ever pending on the bus when a method returns.
        Ok(())
    }
    fn read(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
        for slot in words {
            *slot = self.transfer_byte(0x00)?;
        }
        Ok(())
    }
    fn write(&mut self, words: &[u8]) -> Result<(), Self::Error> {
        for &b in words {
            self.transfer_byte(b)?;
        }
        Ok(())
    }
}

impl<SPIX, SCK, MISO, MOSI> SpiBus<u16> for Spi<SPIX, SCK, MISO, MOSI, Word>
where
    SPIX: Instance,
{
    fn transfer(&mut self, read: &mut [u16], write: &[u16]) -> Result<(), Self::Error> {
        let n = read.len().max(write.len());
        for i in 0..n {
            // MOSI: send write[i], or a dummy 0x00 once write is exhausted
            let sent = write.get(i).copied().unwrap_or(0x0000);
            let received = self.transfer_word(sent)?;
            // MISO: store into read[i] if it still has room, else discard
            if let Some(slot) = read.get_mut(i) {
                *slot = received;
            }
        }
        Ok(())
    }
    fn transfer_in_place(&mut self, words: &mut [u16]) -> Result<(), Self::Error> {
        for word in words {
            *word = self.transfer_word(*word)?;
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<(), Self::Error> {
        // No-op: transfer_byte blocks until RBNE (the byte is fully exchanged),
        // so nothing is ever pending on the bus when a method returns.
        Ok(())
    }
    fn read(&mut self, words: &mut [u16]) -> Result<(), Self::Error> {
        for slot in words {
            *slot = self.transfer_word(0x00)?;
        }
        Ok(())
    }
    fn write(&mut self, words: &[u16]) -> Result<(), Self::Error> {
        for &b in words {
            self.transfer_word(b)?;
        }
        Ok(())
    }
}
