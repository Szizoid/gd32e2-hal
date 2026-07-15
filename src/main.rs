#![no_std]
#![no_main]

use embedded_hal::digital::OutputPin;
use gd32e230_hal::gpio::GpioExt;
use gd32e230_hal::rcu::{CFGR, PllFreq, RcuExt};
use gd32e230_hal::usart::{Usart, UsartConfig};
use panic_halt as _;

use cortex_m_rt::entry;
use gd32e2::gd32e230;

#[entry]
fn main() -> ! {
    let mut dp = gd32e230::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    let clocks = CFGR::default()
        .sysclk(PllFreq::Mhz48)
        .freeze(&mut rcu, &mut dp.fmc);
    let parts = dp.gpioa.split(&mut rcu);
    let mut pa6 = parts.pa6.into_output();
    pa6.set_high().unwrap();

    let tx_pin = parts.pa9.into_alternate::<1>();
    let rx_pin = parts.pa10.into_alternate::<1>();
    let usart0 = Usart::new(
        &mut rcu,
        dp.usart0,
        tx_pin,
        rx_pin,
        clocks,
        UsartConfig::default(),
    );

    loop {
        if let Ok(byte) = usart0.read_byte() {
            usart0.write_byte(byte);
        }
    }
}
