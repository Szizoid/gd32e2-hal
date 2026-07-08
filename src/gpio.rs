//! GPIO — ножки портов A и B.

use gd32e2::gd32e230;

/// PA5/PA7 → аналог (АЦП); PA9 → AF1 (USART0_TX); PB1 → AF1 (TIMER2_CH3, ШИМ).
pub fn init(gpioa: &gd32e230::gpioa::RegisterBlock, gpiob: &gd32e230::gpiob::RegisterBlock) {
    gpioa.ctl().modify(|_, w| w.ctl5().analog().ctl7().analog());
    gpioa.ctl().modify(|_, w| w.ctl9().alternate());
    gpioa.afsel1().modify(|_, w| w.sel9().af1());

    gpiob.ctl().modify(|_, w| w.ctl1().alternate());
    gpiob.afsel0().modify(|_, w| w.sel1().af1());
}
