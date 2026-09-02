//! Writing the user data bytes of the option byte block.
//!
//! Needs no external wiring. Only [`OptionBytes::data`] is changed: the block is
//! read first, so the protection level, the user byte and the write protection
//! go back exactly as they came. That matters — the option bytes are erased
//! whole before being programmed, and a wrong value in `OB_SPC` would lock the
//! debugger out, with the way back mass-erasing the flash.
//!
//! The run has two halves, split by a reset. First boot finds the data bytes as
//! anything but `TARGET`, writes the block and reloads it, which resets the
//! part; the second boot finds `TARGET` in force, reports it and stops. To run
//! the test again, give `TARGET` another value and reflash.
//!
//! `option_error` is the check that matters at the end: it says the loaded
//! bytes matched their complements, which is what tells us the complements this
//! HAL writes are the ones the silicon expects.
//!
//! Covers: `read_option_bytes`, `write_option_bytes`, `reload_option_bytes`.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::pac;
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::ClockConfig;

/// What to leave in the user data bytes.
const TARGET: u16 = 0x1234;

#[entry]
fn main() -> ! {
    let dp = pac::Peripherals::take().unwrap();
    let mut fmc = dp.fmc.constrain();
    let _rcu = dp.rcu.constrain().freeze(&mut fmc, ClockConfig::default());

    let ob = fmc.read_option_bytes();
    defmt::info!(
        "loaded: protection {}, data {:#06x}, option error {}",
        ob.protection(),
        ob.data_bytes(),
        fmc.option_error()
    );
    // Printed to show they come back unchanged after the block is rewritten.
    defmt::info!(
        "user: watchdog {}, boot1 {}, deep-sleep {}, standby {}, vdda {}, parity {}",
        ob.get_free_watchdog(),
        ob.get_boot1(),
        ob.get_deep_sleep(),
        ob.get_standby(),
        ob.get_vdda_monitor(),
        ob.get_sram_parity()
    );

    if ob.data_bytes() == TARGET {
        defmt::info!("data already {:#06x}, nothing to do", TARGET);
        loop {
            cortex_m::asm::wfi();
        }
    }

    // Everything but the data bytes is carried over from what was read.
    let wanted = ob.data(TARGET);
    match fmc.with_unlocked(|f| f.write_option_bytes(&wanted)) {
        Ok(()) => defmt::info!("written, reloading — the part resets now"),
        Err(e) => {
            defmt::error!("write failed: {}", e);
            loop {
                cortex_m::asm::wfi();
            }
        }
    }

    // Takes effect only on a load, and a load is a system reset: this call does
    // not return, and the log continues from the top after the reset.
    fmc.reload_option_bytes()
}
