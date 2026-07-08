//! ШИМ управления ИК-источником: TIMER2, канал 3, выход на PB1.
//! high = MOSFET открыт = ИК включён. Меандр 50% на config::PWM_FREQ_HZ.
//! TIMER2 — general-purpose таймер, POEN (как у TIMER0) не требуется.

use crate::config;
use gd32e2::gd32e230;

pub fn init(timer: &gd32e230::timer2::RegisterBlock) {
    timer.psc().write(|w| w.psc().bits(config::PWM_PSC));
    timer
        .car()
        .write(|w| unsafe { w.car().bits(config::PWM_ARR) });
    timer
        .ch3cv()
        .write(|w| unsafe { w.ch3val().bits(config::PWM_CCR) });

    timer.chctl1_output().modify(|_, w| {
        w.ch3ms()
            .output()
            .ch3comctl()
            .pwm_mode0()
            .ch3comsen()
            .enabled()
    });
    timer
        .chctl2()
        .modify(|_, w| w.ch3en().enabled().ch3p().not_inverted());

    timer.swevg().write(|w| w.upg().update());
    timer.ctl0().modify(|_, w| w.cen().enabled());
}

/// Включён ли сейчас ИК-источник (выход ШИМ высокий: CNT < CH3CV).
pub fn is_ir_on(timer: &gd32e230::timer2::RegisterBlock) -> bool {
    timer.cnt().read().cnt().bits() < config::PWM_CCR
}
