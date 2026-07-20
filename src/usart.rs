use core::marker::PhantomData;
use core::ops::Deref;
use embedded_hal_nb::serial::{ErrorKind, ErrorType, Read, Write};
use gd32e2::gd32e230;

use crate::{
    gpio::{Alternate, Pin},
    rcu::{Clocks, Enable, Rcu, Reset},
    time::Hertz,
};

/// 7 data bits: parity occupies bit 7 inside the u8 (`E7`/`O7`).
const DATA_7BIT_MASK: u8 = 0x7F;
/// 9 data bits: the full 9-bit word in `WL=1, PCEN=0` mode.
const DATA_9BIT_MASK: u32 = 0x1FF;

pub trait TxPin<USART> {}
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
    gd32e230::Usart0:
        TX: [ 'A' 9:1, 'B' 6:0 ]
        RX: [ 'A' 10:1, 'B' 7:0 ],
}

// ---- (1) GD32E230x4 only: PA2/PA3/PA14/PA15 AF1 are USART0 ----
#[cfg(feature = "gd32e230x4")]
usart_pins! {
    gd32e230::Usart0:
        TX: [ 'A' 2:1, 'A' 14:1 ]
        RX: [ 'A' 3:1, 'A' 15:1 ],
}

// ---- (2) GD32E230x8/6: PA2/PA3/PA14/PA15 AF1 are USART1; USART1 exists ----
#[cfg(any(feature = "gd32e230x6", feature = "gd32e230x8"))]
usart_pins! {
    gd32e230::Usart1:
        TX: [ 'A' 2:1, 'A' 8:4, 'A' 14:1 ]
        RX: [ 'A' 3:1, 'A' 15:1, 'B' 0:4 ],
}

pub trait BusClocks {
    fn clock(clocks: &Clocks) -> Hertz;
}

impl BusClocks for gd32e230::Usart0 {
    fn clock(clocks: &Clocks) -> Hertz {
        clocks.usart0()
    }
}

impl BusClocks for gd32e230::Usart1 {
    fn clock(clocks: &Clocks) -> Hertz {
        clocks.pclk1()
    }
}

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

#[derive(Clone, Copy)]
pub enum Oversampling {
    X8,
    X16,
}

/// 8-bit-word frame formats (`WL`/`PCEN`/`PM` in `CTL0`). `E7`/`O7` give only 7 real
/// data bits (parity replaces the MSB); `E8`/`O8` keep the full 8 bits by widening the
/// frame to 9 bits and putting parity in the extra bit. See `Usart::new_word` for the
/// 9-bit-word, no-parity case, which needs a `u16`, not `u8`.
#[derive(Clone, Copy)]
pub enum FrameFormat {
    N8,
    E8,
    O8,
    E7,
    O7,
}

pub struct UsartConfig {
    baud: u32,
    oversampling: Oversampling,
    frame_format: FrameFormat,
}

impl UsartConfig {
    pub fn baud(mut self, baud: u32) -> Self {
        self.baud = baud;
        self
    }
    pub fn oversampling(mut self, oversampling: Oversampling) -> Self {
        self.oversampling = oversampling;
        self
    }
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

pub struct UsartConfig9 {
    baud: u32,
    oversampling: Oversampling,
}

impl UsartConfig9 {
    pub fn baud(mut self, baud: u32) -> Self {
        self.baud = baud;
        self
    }
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
    USARTX: Deref<Target = gd32e230::usart0::RegisterBlock> + Enable + Reset + BusClocks,
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

    usart.ctl0().modify(|_, w| {
        let w = w.uen().enabled().ten().enabled().ren().enabled();
        match oversampling {
            Oversampling::X16 => w.ovsmod().oversampling16(),
            Oversampling::X8 => w.ovsmod().oversampling8(),
        }
    });
}

/// Word-width marker: 8-bit words (`write_byte`/`read_byte`), optionally with parity.
pub struct Byte;
/// Word-width marker: raw 9-bit words (`write_word`/`read_word`), no parity possible.
pub struct Word;

pub struct Usart<USARTX, TX, RX, WORD = Byte> {
    usart: USARTX,
    tx_pin: TX,
    rx_pin: RX,
    frame_format: FrameFormat,
    _word: PhantomData<WORD>,
}

impl<USARTX, TX, RX, WORD> Usart<USARTX, TX, RX, WORD>
where
    USARTX: Deref<Target = gd32e230::usart0::RegisterBlock>,
{
    fn take_error(&self) -> Option<ErrorKind> {
        let stat = self.usart.stat().read();
        let error = if stat.orerr().bit() {
            self.usart.intc().write(|w| w.orec().clear());
            Some(ErrorKind::Overrun)
        } else if stat.nerr().bit() {
            self.usart.intc().write(|w| w.nec().clear());
            Some(ErrorKind::Noise)
        } else if stat.ferr().bit() {
            self.usart.intc().write(|w| w.fec().clear());
            Some(ErrorKind::FrameFormat)
        } else if stat.perr().bit() {
            self.usart.intc().write(|w| w.pec().clear());
            Some(ErrorKind::Parity)
        } else {
            None
        };

        if error.is_some() {
            self.usart.rdata().read();
        }

        error
    }

    pub fn release(self) -> (USARTX, TX, RX) {
        self.usart.ctl0().modify(|_, w| w.uen().disabled());
        (self.usart, self.tx_pin, self.rx_pin)
    }
}

impl<USARTX, TX, RX> Usart<USARTX, TX, RX, Byte>
where
    USARTX: Deref<Target = gd32e230::usart0::RegisterBlock>,
{
    fn received_byte(&self) -> u8 {
        let raw = self.usart.rdata().read().bits() as u8;
        match self.frame_format {
            FrameFormat::E7 | FrameFormat::O7 => raw & DATA_7BIT_MASK,
            FrameFormat::N8 | FrameFormat::E8 | FrameFormat::O8 => raw,
        }
    }
}

impl<USARTX, TX, RX> Usart<USARTX, TX, RX, Byte>
where
    USARTX: Deref<Target = gd32e230::usart0::RegisterBlock> + Enable + Reset + BusClocks,
    TX: TxPin<USARTX>,
    RX: RxPin<USARTX>,
{
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
        Self {
            usart,
            tx_pin,
            rx_pin,
            frame_format: config.frame_format,
            _word: PhantomData,
        }
    }

    pub fn write_byte(&self, byte: u8) {
        while !self.usart.stat().read().tbe().bit() {}
        self.usart.tdata().write(|w| unsafe { w.bits(byte as u32) });
    }
    pub fn read_byte(&self) -> Result<u8, ErrorKind> {
        while !self.usart.stat().read().rbne().bit() {}
        if let Some(e) = self.take_error() {
            Err(e)
        } else {
            Ok(self.received_byte())
        }
    }
}

impl<USARTX, TX, RX> Usart<USARTX, TX, RX, Word>
where
    USARTX: Deref<Target = gd32e230::usart0::RegisterBlock> + Enable + Reset + BusClocks,
    TX: TxPin<USARTX>,
    RX: RxPin<USARTX>,
{
    pub fn new_word(
        rcu: &mut Rcu,
        usart: USARTX,
        tx_pin: TX,
        rx_pin: RX,
        clocks: Clocks,
        config: UsartConfig9,
    ) -> Self {
        configure(rcu, &usart, &clocks, config.baud, config.oversampling);

        // WL=1 (9-bit frame), PCEN disabled: no parity possible in this mode, all 9
        // bits are real data.
        usart.ctl0().modify(|_, w| w.pcen().disabled().wl().bit9());
        Self {
            usart,
            tx_pin,
            rx_pin,
            // Never read: only `Byte`'s `received_byte` looks at `frame_format`.
            frame_format: FrameFormat::N8,
            _word: PhantomData,
        }
    }

    pub fn write_word(&self, word: u16) {
        while !self.usart.stat().read().tbe().bit() {}
        self.usart
            .tdata()
            .write(|w| unsafe { w.bits(word as u32 & DATA_9BIT_MASK) });
    }
    pub fn read_word(&self) -> Result<u16, ErrorKind> {
        while !self.usart.stat().read().rbne().bit() {}
        if let Some(e) = self.take_error() {
            Err(e)
        } else {
            Ok((self.usart.rdata().read().bits() & DATA_9BIT_MASK) as u16)
        }
    }
}

impl<USARTX, TX, RX, WORD> ErrorType for Usart<USARTX, TX, RX, WORD> {
    type Error = ErrorKind;
}

impl<USARTX, TX, RX> Read<u8> for Usart<USARTX, TX, RX, Byte>
where
    USARTX: Deref<Target = gd32e230::usart0::RegisterBlock>,
{
    fn read(&mut self) -> nb::Result<u8, Self::Error> {
        if !self.usart.stat().read().rbne().bit() {
            return Err(nb::Error::WouldBlock);
        }
        if let Some(e) = self.take_error() {
            Err(nb::Error::Other(e))
        } else {
            Ok(self.received_byte())
        }
    }
}

impl<USARTX, TX, RX> Write<u8> for Usart<USARTX, TX, RX, Byte>
where
    USARTX: Deref<Target = gd32e230::usart0::RegisterBlock>,
{
    fn write(&mut self, byte: u8) -> nb::Result<(), Self::Error> {
        if !self.usart.stat().read().tbe().bit() {
            Err(nb::Error::WouldBlock)
        } else {
            self.usart.tdata().write(|w| unsafe { w.bits(byte as u32) });
            Ok(())
        }
    }
    fn flush(&mut self) -> nb::Result<(), Self::Error> {
        if !self.usart.stat().read().tc().bit() {
            Err(nb::Error::WouldBlock)
        } else {
            Ok(())
        }
    }
}
