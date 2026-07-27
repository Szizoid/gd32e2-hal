//! USART0 echoing every byte it receives, at 115200 8N1 on PA9/PA10.
//!
//! Needs a terminal on the other end of the wire: type into it and the same
//! characters come back. What travels over the USART is the payload under test,
//! so the running commentary goes to the RTT log instead — the two never share
//! a channel and cannot be confused for one another.
//!
//! Covers: `Usart::new`, `UsartConfig::default`, `read_byte`, `write_byte`.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use embedded_hal::digital::OutputPin;
use panic_halt as _;

use gd32e2_hal::gpio::GpioExt;
use gd32e2_hal::pac;
use gd32e2_hal::rcu::{CFGR, PllFreq, RcuExt};
use gd32e2_hal::usart::{Usart, UsartConfig};

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
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

    defmt::info!("echo ready at 115200 8N1 on PA9/PA10");

    loop {
        if let Ok(byte) = usart0.read_byte() {
            usart0.write_byte(byte);
            defmt::info!("echoed {=u8:#04x}", byte);
        }
    }
}
