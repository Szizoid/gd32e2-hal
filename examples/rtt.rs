//! Smoke test for the `defmt` + RTT log channel.
//!
//! Prints an incrementing counter and touches no peripheral at all: nothing is
//! clocked, no pin is configured, the chip runs from the reset default IRC8M.
//! Whatever appears in `probe-rs run` therefore says exactly one thing — the
//! log path itself works end to end.
//!
//! `defmt_rtt` and `pac` are imported for their link-time side effects only:
//! the former defines the global logger symbols `defmt` declares, the latter
//! the interrupt vector table `cortex-m-rt` expects from a device crate.
//! Dropping either import breaks the link, not the logic.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
#[allow(unused)]
use gd32e2_hal::pac as _;
use panic_halt as _;

#[entry]
fn main() -> ! {
    let mut x = 0;
    loop {
        defmt::info!("x is {}", x);
        x += 1;
        cortex_m::asm::delay(1_000_000);
    }
}
