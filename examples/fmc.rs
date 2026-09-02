//! Erasing and programming the last page of the flash.
//!
//! Needs no external wiring. Works on `P63`, the last kilobyte of a 64K part,
//! which the firmware itself never reaches — the binary sits at the start of the
//! flash and is a few kilobytes long.
//!
//! Erase and program share one `with_unlocked`, showing that the closure body
//! takes a whole sequence rather than a single call. Reads go through plain
//! pointers: the flash is memory-mapped and readable without unlocking anything. The last step programs a word twice without
//! erasing in between, which the silicon must refuse with
//! [`Error::Program`](gd32e2_hal::fmc::Error::Program) — bits only ever go from
//! one to zero.
//!
//! Covers: `with_unlocked`, `erase_page`, `program`, `take_error`.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::fmc::{Error, Page};
use gd32e2_hal::pac;
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::ClockConfig;

/// The page under test, the last one of a 64K part.
const PAGE: Page = Page::P63;
/// Value programmed into it, chosen so every byte differs from the erased state.
const MARKER: u32 = 0xDEAD_BEEF;
/// What a word of an erased page reads back as.
const ERASED: u32 = 0xFFFF_FFFF;

/// Reads one word of `PAGE`, `index` counting words from its start.
fn read_word(index: u8) -> u32 {
    let addr = PAGE as u32 + index as u32 * 4;
    // The flash is mapped for reading like any other memory, and the address is
    // in it by construction.
    unsafe { core::ptr::read_volatile(addr as *const u32) }
}

#[entry]
fn main() -> ! {
    let dp = pac::Peripherals::take().unwrap();
    let mut fmc = dp.fmc.constrain();
    let _rcu = dp.rcu.constrain().freeze(&mut fmc, ClockConfig::default());

    defmt::info!(
        "page at {:#010x}, before: {:#010x}",
        PAGE as u32,
        read_word(0)
    );

    // One unlock covers as many operations as the closure body holds: `?` stops
    // at the first failure, and what the last one returns leaves the call.
    let result = fmc.with_unlocked(|f| {
        f.erase_page(PAGE)?;
        defmt::info!("erased, word 0 = {:#010x}", read_word(0));
        if read_word(0) != ERASED || read_word(255) != ERASED {
            defmt::error!(
                "erase left {:#010x} / {:#010x}",
                read_word(0),
                read_word(255)
            );
        }
        f.program(PAGE, 0, MARKER)
    });
    match result {
        Ok(()) => defmt::info!("programmed, word 0 = {:#010x}", read_word(0)),
        Err(e) => defmt::error!("erase or program failed: {}", e),
    }
    if read_word(0) != MARKER {
        defmt::error!("word 0 = {:#010x}, expected {:#010x}", read_word(0), MARKER);
    }

    // Programming over a word that is not erased: the cells would have to go
    // from zero back to one, so the controller raises PGERR instead.
    match fmc.with_unlocked(|f| f.program(PAGE, 0, MARKER)) {
        Err(Error::Program) => defmt::info!("second program rejected, as it should be"),
        Err(e) => defmt::error!("second program failed with {}, expected Program", e),
        Ok(()) => defmt::error!(
            "second program was accepted, word 0 = {:#010x}",
            read_word(0)
        ),
    }

    defmt::info!("done");
    loop {}
}
