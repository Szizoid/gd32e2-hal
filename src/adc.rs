use gd32e2::gd32e230;

use crate::rcu::{Clocks, Enable, Rcu, Reset};

pub struct Adc {
    adc: gd32e230::Adc,
}

impl Adc {
    pub fn new(rcu: &mut Rcu, adc: gd32e230::Adc, clocks: Clocks) -> Self {
        <gd32e230::Adc as Enable>::enable(rcu);
        <gd32e230::Adc as Reset>::reset(rcu);
        adc.ctl1().modify(|_, w| w.adcon().enabled());
        cortex_m::asm::delay((14 * clocks.hclk().0).div_ceil(clocks.ck_adc().0));
        adc.ctl1().modify(|_, w| w.rstclb().start());
        adc.ctl1().modify(|_, w| w.clb().start());
        while adc.ctl1().read().clb().is_not_complete() {}
        Self { adc }
    }
}
