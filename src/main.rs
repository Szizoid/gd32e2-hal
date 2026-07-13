#![no_std]
#![no_main]

use embedded_hal::digital::OutputPin;
use gd32e230_hal::gpio::GpioExt;
use gd32e230_hal::rcu::{CFGR, RcuExt};
use panic_halt as _;

use cortex_m_rt::entry;
use gd32e2::gd32e230;

#[entry]
fn main() -> ! {
    let dp = gd32e230::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    let _clocks = CFGR::default().freeze(&mut rcu);
    let parts = dp.gpioa.split(&mut rcu);
    let mut pa6 = parts.pa6.into_output();
    pa6.set_high().unwrap();
    loop {}
}
