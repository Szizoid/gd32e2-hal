//! USART0 — 115200 8N1, только передача.

use crate::config;
use gd32e2::gd32e230;

pub fn init(usart: &gd32e230::usart0::RegisterBlock) {
    usart.baud().write(|w| {
        w.intdiv()
            .bits(config::USART_INTDIV)
            .fradiv()
            .bits(config::USART_FRADIV)
    });
    usart
        .ctl0()
        .modify(|_, w| w.uen().enabled().ten().enabled());
}

pub fn send(usart: &gd32e230::usart0::RegisterBlock, bytes: &[u8]) {
    for &b in bytes {
        while !usart.stat().read().tbe().bit_is_set() {}
        usart.tdata().write(|w| unsafe { w.tdata().bits(b as u16) });
    }
    while !usart.stat().read().tc().bit_is_set() {}
}
