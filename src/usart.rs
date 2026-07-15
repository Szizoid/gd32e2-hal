use core::ops::Deref;
use embedded_hal_nb::serial::{ErrorType, Read, Write};
use gd32e2::gd32e230;

use crate::{
    gpio::{Alternate, Pin},
    rcu::{Clocks, Enable, Rcu, Reset},
    time::Hertz,
};

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

usart_pins! {
    gd32e230::Usart0:
        TX: [ 'A' 2:1, 'A' 9:1, 'A' 14:1, 'B' 6:0 ]
        RX: [ 'A' 3:1, 'A' 10:1, 'A' 15:1, 'B' 7:0 ],
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

pub enum Oversampling {
    X8,
    X16,
}

pub enum Parity {
    None,
    Even,
    Odd,
}

pub struct UsartConfig {
    baud: u32,
    oversampling: Oversampling,
    parity: Parity,
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
    pub fn parity(mut self, parity: Parity) -> Self {
        self.parity = parity;
        self
    }
}

impl Default for UsartConfig {
    fn default() -> Self {
        Self {
            baud: baud::B115200,
            oversampling: Oversampling::X16,
            parity: Parity::None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Overrun,
    Noise,
    Framing,
    Parity,
}

impl embedded_hal_nb::serial::Error for Error {
    fn kind(&self) -> embedded_hal_nb::serial::ErrorKind {
        match self {
            Error::Overrun => embedded_hal_nb::serial::ErrorKind::Overrun,
            Error::Noise => embedded_hal_nb::serial::ErrorKind::Noise,
            Error::Framing => embedded_hal_nb::serial::ErrorKind::FrameFormat,
            Error::Parity => embedded_hal_nb::serial::ErrorKind::Parity,
        }
    }
}

pub struct Usart<USARTX, TX, RX> {
    usart: USARTX,
    tx_pin: TX,
    rx_pin: RX,
}

impl<USARTX, TX, RX> Usart<USARTX, TX, RX>
where
    USARTX: Deref<Target = gd32e230::usart0::RegisterBlock>,
{
    fn take_error(&self) -> Option<Error> {
        let stat = self.usart.stat().read();
        let error = if stat.orerr().bit() {
            self.usart.intc().write(|w| w.orec().clear());
            Option::Some(Error::Overrun)
        } else if stat.nerr().bit() {
            self.usart.intc().write(|w| w.nec().clear());
            Option::Some(Error::Noise)
        } else if stat.ferr().bit() {
            self.usart.intc().write(|w| w.fec().clear());
            Option::Some(Error::Framing)
        } else if stat.perr().bit() {
            self.usart.intc().write(|w| w.pec().clear());
            Option::Some(Error::Parity)
        } else {
            Option::None
        };

        if error.is_some() {
            self.usart.rdata().read();
        }

        error
    }
}

impl<USARTX, TX, RX> Usart<USARTX, TX, RX>
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
        USARTX::enable(rcu);
        USARTX::reset(rcu);
        let pclk = USARTX::clock(&clocks).0;
        let usartdiv = (pclk + config.baud / 2) / config.baud;
        usart.baud().write(|w| unsafe {
            match config.oversampling {
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
            let w = match config.oversampling {
                Oversampling::X16 => w.ovsmod().oversampling16(),
                Oversampling::X8 => w.ovsmod().oversampling8(),
            };
            match config.parity {
                Parity::None => w.pcen().disabled().wl().bit8(),
                Parity::Even => w.pcen().enabled().pm().even().wl().bit9(),
                Parity::Odd => w.pcen().enabled().pm().odd().wl().bit9(),
            }
        });
        Self {
            usart,
            tx_pin,
            rx_pin,
        }
    }

    pub fn write_byte(&self, byte: u8) {
        while !self.usart.stat().read().tbe().bit() {}
        self.usart.tdata().write(|w| unsafe { w.bits(byte as u32) });
    }
    pub fn read_byte(&self) -> Result<u8, Error> {
        while !self.usart.stat().read().rbne().bit() {}
        if let Some(e) = self.take_error() {
            Err(e)
        } else {
            Ok(self.usart.rdata().read().bits() as u8)
        }
    }
    pub fn release(self) -> (USARTX, TX, RX) {
        self.usart.ctl0().modify(|_, w| w.uen().disabled());
        (self.usart, self.tx_pin, self.rx_pin)
    }
}

impl<USARTX, TX, RX> ErrorType for Usart<USARTX, TX, RX> {
    type Error = Error;
}

impl<USARTX, TX, RX> Read<u8> for Usart<USARTX, TX, RX>
where
    USARTX: Deref<Target = gd32e230::usart0::RegisterBlock>,
{
    fn read(&mut self) -> nb::Result<u8, Self::Error> {
        if !self.usart.stat().read().rbne().bit() {
            return nb::Result::Err(nb::Error::WouldBlock);
        }
        if let Some(e) = self.take_error() {
            nb::Result::Err(nb::Error::Other(e))
        } else {
            nb::Result::Ok(self.usart.rdata().read().bits() as u8)
        }
    }
}

impl<USARTX, TX, RX> Write<u8> for Usart<USARTX, TX, RX>
where
    USARTX: Deref<Target = gd32e230::usart0::RegisterBlock>,
{
    fn write(&mut self, byte: u8) -> nb::Result<(), Self::Error> {
        if !self.usart.stat().read().tbe().bit() {
            nb::Result::Err(nb::Error::WouldBlock)
        } else {
            self.usart.tdata().write(|w| unsafe { w.bits(byte as u32) });
            nb::Result::Ok(())
        }
    }
    fn flush(&mut self) -> nb::Result<(), Self::Error> {
        if !self.usart.stat().read().tc().bit() {
            nb::Result::Err(nb::Error::WouldBlock)
        } else {
            nb::Result::Ok(())
        }
    }
}
