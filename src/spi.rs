use core::{marker::PhantomData, ops::Deref};
use embedded_hal::spi::{ErrorKind, ErrorType, Mode, Phase, Polarity, SpiBus};
use gd32e2::gd32e230;

use crate::{
    gpio::{Alternate, Pin},
    rcu::{Enable, Rcu, Reset},
};

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

// SPI1 deliberately omitted — its registers diverge from SPI0 at the bit level
// (FF16 vs CRCL, BYTEN, ...); It will get its own type later.
spi_pins!(
    gd32e230::Spi0:
        SCK: ['A' 5 : 0, 'B' 3 : 0, 'B' 13 : 0]
        MISO: ['A' 6 : 0, 'B' 4 : 0, 'B' 14 : 0]
        MOSI: ['A' 7 : 0, 'B' 5 : 0, 'B' 15 : 0]
);

/// Word-width marker: 8-bit frames (`transfer_byte`, `SpiBus<u8>`).
pub struct Byte;
/// Word-width marker: 16-bit frames (`transfer_word`, `SpiBus<u16>`).
pub struct Word;

fn configure<SPIX>(rcu: &mut Rcu, spi: &SPIX, psc: SpiPsc, mode: Mode, ff16: bool)
where
    SPIX: Deref<Target = gd32e230::spi0::RegisterBlock> + Enable + Reset,
{
    SPIX::enable(rcu);
    SPIX::reset(rcu);
    spi.ctl0().modify(|_, w| {
        let w = w
            .mstmod()
            .set_bit()
            .swnssen()
            .set_bit()
            .swnss()
            .set_bit()
            .lf()
            .clear_bit()
            .ff16()
            .bit(ff16)
            .spien()
            .set_bit()
            .ckpl()
            .bit(mode.polarity == Polarity::IdleHigh)
            .ckph()
            .bit(mode.phase == Phase::CaptureOnSecondTransition);
        unsafe { w.psc().bits(psc as u8) }
    });
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
    SPIX: Deref<Target = gd32e230::spi0::RegisterBlock> + Enable + Reset,
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
        psc: SpiPsc,
        mode: Mode,
    ) -> Self {
        configure(rcu, &spi, psc, mode, false);
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
    SPIX: Deref<Target = gd32e230::spi0::RegisterBlock> + Enable + Reset,
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
        psc: SpiPsc,
        mode: Mode,
    ) -> Self {
        configure(rcu, &spi, psc, mode, true);
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
    SPIX: Deref<Target = gd32e230::spi0::RegisterBlock>,
{
    fn take_error(&self) -> Option<ErrorKind> {
        let stat = self.spi.stat().read();
        if stat.rxorerr().bit_is_set() {
            // clear: read DATA (done in transfer_byte) + read STAT (above)
            Some(ErrorKind::Overrun)
        } else if stat.conferr().bit_is_set() {
            // clear: read STAT (above) + write CTL0
            self.spi.ctl0().modify(|_, w| w);
            Some(ErrorKind::ModeFault)
        } else if stat.crcerr().bit_is_set() {
            self.spi.stat().modify(|_, w| w.crcerr().clear_bit());
            Some(ErrorKind::Other)
        } else if stat.ferr().bit_is_set() {
            self.spi.stat().modify(|_, w| w.ferr().clear_bit());
            Some(ErrorKind::FrameFormat)
        } else {
            None
        }
    }

    pub fn release(self) -> (SPIX, SCK, MISO, MOSI) {
        self.spi.ctl0().modify(|_, w| w.spien().clear_bit());
        (self.spi, self.sck_pin, self.miso_pin, self.mosi_pin)
    }
}

impl<SPIX, SCK, MISO, MOSI> Spi<SPIX, SCK, MISO, MOSI, Byte>
where
    SPIX: Deref<Target = gd32e230::spi0::RegisterBlock>,
{
    pub fn transfer_byte(&self, byte: u8) -> Result<u8, ErrorKind> {
        while self.spi.stat().read().tbe().bit_is_clear() {}
        self.spi
            .data()
            .write(|w| unsafe { w.data().bits(byte as u16) });
        while self.spi.stat().read().rbne().bit_is_clear() {}
        let received = self.spi.data().read().data().bits() as u8;
        match self.take_error() {
            Some(e) => Err(e),
            None => Ok(received),
        }
    }
}

impl<SPIX, SCK, MISO, MOSI> Spi<SPIX, SCK, MISO, MOSI, Word>
where
    SPIX: Deref<Target = gd32e230::spi0::RegisterBlock>,
{
    pub fn transfer_word(&self, word: u16) -> Result<u16, ErrorKind> {
        while self.spi.stat().read().tbe().bit_is_clear() {}
        self.spi.data().write(|w| unsafe { w.data().bits(word) });
        while self.spi.stat().read().rbne().bit_is_clear() {}
        let received = self.spi.data().read().data().bits();
        match self.take_error() {
            Some(e) => Err(e),
            None => Ok(received),
        }
    }
}

impl<SPIX, SCK, MISO, MOSI, WORD> ErrorType for Spi<SPIX, SCK, MISO, MOSI, WORD>
where
    SPIX: Deref<Target = gd32e230::spi0::RegisterBlock>,
{
    type Error = ErrorKind;
}

impl<SPIX, SCK, MISO, MOSI> SpiBus<u8> for Spi<SPIX, SCK, MISO, MOSI, Byte>
where
    SPIX: Deref<Target = gd32e230::spi0::RegisterBlock>,
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
    SPIX: Deref<Target = gd32e230::spi0::RegisterBlock>,
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
