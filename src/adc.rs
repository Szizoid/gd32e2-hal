//! АЦП — включение, самокалибровка, одиночные измерения по программному запуску.

use crate::config;
use gd32e2::gd32e230;

pub fn init(adc: &gd32e230::adc::RegisterBlock) {
    adc.ctl1().modify(|_, w| w.adcon().enabled());
    cortex_m::asm::delay(config::ADC_STAB_DELAY);

    // Калибровка по User Manual §10.4.1: CLB=1 → ждать CLB=0. RSTCLB — опционален,
    // ждать его очистки не нужно (на практике он не самоочищается — зависание).
    adc.ctl1().modify(|_, w| w.clb().start());
    while !adc.ctl1().read().clb().is_complete() {}

    adc.sampt1()
        .modify(|_, w| w.spt5().cycles55_5().spt7().cycles55_5());
    // RL=0 → последовательность из 1 канала.
    adc.rsq0().modify(|_, w| w.rl().bits(0));
    adc.ctl1()
        .modify(|_, w| w.eterc().enabled().etsrc().swrcst());
}

/// Программный запуск одиночной конверсии `channel`, ожидание EOC, возврат 0..=4095.
pub fn read(adc: &gd32e230::adc::RegisterBlock, channel: u8) -> u16 {
    adc.rsq2().modify(|_, w| unsafe { w.rsq0().bits(channel) });
    adc.stat().modify(|_, w| w.eoc().clear());
    adc.ctl1().modify(|_, w| w.swrcst().start());
    while !adc.stat().read().eoc().is_complete() {}
    adc.rdata().read().rdata().bits()
}

/// Код АЦП (0..=4095) → милливольты, округление к ближайшему целому.
pub fn to_mv(raw: u16) -> u16 {
    let mv = (raw as u32 * config::VREF_MV + config::ADC_FULL_SCALE / 2) / config::ADC_FULL_SCALE;
    mv as u16
}
