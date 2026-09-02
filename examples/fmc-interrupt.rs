//! The FMC end-of-operation interrupt, raised by an erase and by a program.
//!
//! Needs no external wiring; works on `P63`, the last kilobyte of a 64K part,
//! which the firmware never reaches.
//!
//! Unlike the other interrupt examples, the peripheral is not handed to the
//! handler: `erase_page` and `program` block until the operation ends, so `main`
//! is holding the `Fmc` at the very moment the interrupt fires. `ENDF` is
//! therefore still standing when the handler runs, and — the flag being a level,
//! not a pulse — a handler that only returned would be re-entered forever, the
//! same trap as `EWIF` on the watchdog. So the handler masks its own line in the
//! NVIC and counts; `main` clears the flag on its way out of the blocking call
//! and unmasks the line again before the next operation.
//!
//! Covers: `UnlockedFmc::listen`/`unlisten`, `Fmc::is_listening`,
//! `cortex_m::peripheral::NVIC::{mask, unmask, unpend}`, a `#[interrupt]`
//! handler.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, Ordering};

use cortex_m::peripheral::NVIC;
use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::fmc::{Event, Page};
use gd32e2_hal::pac::{self, interrupt};
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::ClockConfig;

/// The page under test, the last one of a 64K part.
const PAGE: Page = Page::P63;
/// Value programmed into it.
const MARKER: u32 = 0x0BAD_C0DE;

/// How many times the handler has run.
static ENDS: AtomicU32 = AtomicU32::new(0);

/// Lets the line raise an interrupt again, after `ENDF` has been cleared.
///
/// The pending bit is dropped first: it was set while the flag stood, and
/// unmasking with it still set would re-enter the handler at once.
fn rearm() {
    NVIC::unpend(pac::Interrupt::FMC);
    // Safe: the handler touches ENDS and the NVIC only, never a register that
    // `main` is using.
    unsafe { NVIC::unmask(pac::Interrupt::FMC) };
}

#[entry]
fn main() -> ! {
    let dp = pac::Peripherals::take().unwrap();
    let mut fmc = dp.fmc.constrain();
    let _rcu = dp.rcu.constrain().freeze(&mut fmc, ClockConfig::default());

    // `ENDIE` lives in CTL, which the lock covers whole, so it can only be set
    // from inside `with_unlocked` — but it stays set once the flash locks again.
    fmc.with_unlocked(|f| f.listen(Event::End));
    defmt::info!("listening: {}", fmc.is_listening(Event::End));
    rearm();

    match fmc.with_unlocked(|f| f.erase_page(PAGE)) {
        Ok(()) => defmt::info!(
            "erased, handler ran {} time(s)",
            ENDS.load(Ordering::Relaxed)
        ),
        Err(e) => defmt::error!("erase failed: {}", e),
    }
    rearm();

    match fmc.with_unlocked(|f| f.program(PAGE, 0, MARKER)) {
        Ok(()) => defmt::info!(
            "programmed, handler ran {} time(s)",
            ENDS.load(Ordering::Relaxed)
        ),
        Err(e) => defmt::error!("program failed: {}", e),
    }

    fmc.with_unlocked(|f| f.unlisten(Event::End));
    let ends = ENDS.load(Ordering::Relaxed);
    if ends == 2 {
        defmt::info!("two operations, two interrupts");
    } else {
        defmt::error!("handler ran {} time(s), expected 2", ends);
    }

    defmt::info!("done, listening: {}", fmc.is_listening(Event::End));
    loop {
        cortex_m::asm::wfi();
    }
}

#[interrupt]
fn FMC() {
    // `ENDF` is cleared by the blocking call `main` is still inside, so the only
    // way out of a level-triggered request here is to mask the line.
    NVIC::mask(pac::Interrupt::FMC);
    ENDS.fetch_add(1, Ordering::Relaxed);
}
