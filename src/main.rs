#![no_std]
#![no_main]

use gd32e230_hal::gpio::GpioExt;
use panic_halt as _;

use cortex_m_rt::entry;
use gd32e2::gd32e230;

#[entry]
fn main() -> ! {
    let dp = gd32e230::Peripherals::take().unwrap();
    let parts = dp.gpioa.split();
    let mut pa5 = parts.pa5.into_output();
    pa5.set_high();
    loop {}
}
