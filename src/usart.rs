use core::ops::Deref;
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
        clocks.pclk2()
    }
}

impl BusClocks for gd32e230::Usart1 {
    fn clock(clocks: &Clocks) -> Hertz {
        clocks.pclk1()
    }
}

pub struct Usart<USARTX, TX, RX> {
    usart: USARTX,
    tx_pin: TX,
    rx_pin: RX,
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
        baud: u32,
        clocks: Clocks,
    ) -> Self {
        USARTX::enable(rcu);
        USARTX::reset(rcu);
        usart
            .baud()
            .write(|w| unsafe { w.bits((USARTX::clock(&clocks).0 + baud / 2) / baud) });
        usart
            .ctl0()
            .modify(|_, w| w.uen().enabled().ren().enabled().ten().enabled());
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
    pub fn read_byte(&self) -> u8 {
        while !self.usart.stat().read().rbne().bit() {}
        self.usart.rdata().read().bits() as u8
    }
}
