//! RCU — тактирование периферии.

use gd32e2::gd32e230;

pub fn init(rcu: &gd32e230::rcu::RegisterBlock) {
    // GPIOA/GPIOB — шина AHB.
    rcu.ahben()
        .modify(|_, w| w.paen().enabled().pben().enabled());
    // ADC, USART0 — шина APB2.
    rcu.apb2en()
        .modify(|_, w| w.adcen().enabled().usart0en().enabled());
    // TIMER2 (ШИМ на PB1) — шина APB1.
    rcu.apb1en().modify(|_, w| w.timer2en().enabled());
    // Такт АЦП = APB2/2 = 4 МГц (макс. 14 МГц).
    rcu.cfg0().modify(|_, w| w.adcpsc().div2());
    // ВАЖНО: по умолчанию источник CK_ADC — неактивный CK_IRC28M, а не APB2.
    // Без этого АЦП не имеет тактов и самокалибровка в adc::init виснет навечно.
    rcu.cfg2().modify(|_, w| w.adcsel().set_bit());
}
