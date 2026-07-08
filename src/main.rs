#![no_std]
#![no_main]

use panic_halt as _;

use cortex_m_rt::entry;
use gd32e2::gd32e230;

mod adc;
mod config;
mod fmt;
mod gpio;
mod pwm;
mod rcu;
mod usart;

// Минимальный работающий скелет: такты → GPIO → USART0, дальше шлёт heartbeat.
// adc.rs и pwm.rs — готовые, проверенные на железе модули для двух самых частых
// задач (см. README.md в этой папке) — подключай их вызовы в main() по мере
// надобности, конфигурацию подгоняй в config.rs (каналы АЦП, пины, частоты).

#[entry]
fn main() -> ! {
    let dp = gd32e230::Peripherals::take().unwrap();
    let _cp = cortex_m::Peripherals::take().unwrap();

    rcu::init(&dp.rcu);
    gpio::init(&dp.gpioa, &dp.gpiob);
    usart::init(&dp.usart0);

    // Раскомментируй при необходимости АЦП/ШИМ (не забудь такты в rcu::init):
    // pwm::init(&dp.timer2);
    // adc::init(&dp.adc);

    loop {
        usart::send(&dp.usart0, b"ALIVE\r\n");
        cortex_m::asm::delay(8_000_000); // ~1 с при 8 МГц
    }
}
