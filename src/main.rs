#![no_std]
#![no_main]

use panic_halt as _;

use cortex_m_rt::entry;
use gd32e2::gd32e230;

use gd32e230_hal::gpio::split_gpioa;

#[entry]
fn main() -> ! {
    let dp = gd32e230::Peripherals::take().unwrap();
    let parts = split_gpioa(dp.gpioa);
    let mut pa5 = parts.pa5.into_output();
    pa5.set_high();
    loop {}
}
