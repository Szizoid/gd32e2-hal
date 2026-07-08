#![no_std]
#![no_main]

use panic_halt as _;

use cortex_m_rt::entry;
use gd32e2::gd32e230;

mod hal;

#[entry]
fn main() -> ! {
    loop {}
}
