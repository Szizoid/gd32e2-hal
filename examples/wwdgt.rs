//! Window watchdog: both of its bounds, and the interrupt one tick before the end.
//!
//! Each round opens with a few idle seconds — the window for reattaching the
//! probe, since the board reboots itself here — and then branches on what caused
//! the last reset, so a single binary shows two different failures in turn:
//!
//! Both rounds start the same way, feeding correctly a few times, and differ
//! only in which bound they then break:
//!
//! - **first round**, no watchdog reset yet: feeds far too early. That is the
//!   bound a plain watchdog does not have, and it resets the board just as a
//!   missed feed would.
//! - **every round after**, having been reset by the watchdog: stops feeding.
//!   The counter runs down to `0x40`, the early wakeup interrupt fires, the
//!   handler reports it — and one tick later the reset arrives anyway, because
//!   the handler deliberately does not feed.
//!
//! So the log alternates: an early-feed reset once, then a late-feed round
//! repeating every few seconds for as long as the board is left alone.
//!
//! Timings come out of `PCLK1` = 48 MHz with `Div8`: one tick is ~683 µs, the
//! period `cnt = 63` is ~44 ms, and the window opens ~22 ms into it. Feeding at
//! 30 ms is inside; feeding at 5 ms is the early violation.
//!
//! Covers: `WwdgtExt::constrain` / `start` / `feed`, `listen` / `is_pending` /
//! `clear_interrupt`, and `Rcu::reset_flag` / `clear_reset_flags`.

#![no_std]
#![no_main]

use core::cell::RefCell;

use cortex_m::peripheral::NVIC;
use cortex_m_rt::entry;
use critical_section::Mutex;
use defmt_rtt as _;
use embedded_hal::delay::DelayNs;
use panic_halt as _;

use gd32e2_hal::pac::{self, interrupt};
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::{ClockConfig, PllFreq, ResetFlag, SysClk};
use gd32e2_hal::watchdog::{WwdgtPsc, WwdgtRunning};

/// Full period, in ticks: ~44 ms at `PCLK1 / 4096 / 8`.
const PERIOD_TICKS: u8 = 63;
/// Window, in ticks: feeding is allowed only below this, i.e. after ~22 ms.
const WINDOW_TICKS: u8 = 31;
/// Inside the window, comfortably away from both bounds.
const GOOD_FEED_MS: u32 = 30;
/// Above the window, so the watchdog treats it as running away.
const EARLY_FEED_MS: u32 = 5;
const MEALS: u32 = 5;
/// Head start for reattaching the probe after the board resets itself.
const STARTUP_DELAY_MS: u32 = 3_000;

static WATCHDOG: Mutex<RefCell<Option<WwdgtRunning>>> = Mutex::new(RefCell::new(None));

#[entry]
fn main() -> ! {
    let dp = pac::Peripherals::take().unwrap();
    let mut fmc = dp.fmc.constrain();
    let config = ClockConfig::default().sysclk(SysClk::Pll(PllFreq::Mhz48));
    let mut rcu = dp.rcu.constrain().freeze(&mut fmc, config);

    let mut delay = dp.timer5.constrain(&mut rcu).into_delay();
    delay.delay_ms(STARTUP_DELAY_MS);

    let after_watchdog = rcu.reset_flag(ResetFlag::WindowWatchdog);
    // Otherwise the flag stands for good and every later round takes the same
    // branch whether or not the watchdog fired again.
    rcu.clear_reset_flags();

    let mut wwdgt = dp
        .wwdgt
        .constrain(&mut rcu)
        .start(WwdgtPsc::Div8, PERIOD_TICKS, WINDOW_TICKS);

    if after_watchdog {
        defmt::info!("up after a window watchdog reset — will miss the deadline");
        wwdgt.listen();
        feed_inside_window(&mut wwdgt, &mut delay);

        defmt::info!("no more feeding — expect the early wakeup, then a reset");
        critical_section::with(|cs| WATCHDOG.borrow(cs).replace(Some(wwdgt)));
        // SAFETY: the handler reaches the watchdog through WATCHDOG, which is
        // filled in above, so the line cannot fire on an empty cell.
        unsafe { NVIC::unmask(interrupt::WWDGT) };
        loop {
            cortex_m::asm::wfi();
        }
    }

    defmt::info!("up without a window watchdog reset — will feed too early");
    feed_inside_window(&mut wwdgt, &mut delay);

    defmt::info!("feeding early, at {} ms — expect a reset", EARLY_FEED_MS);
    delay.delay_ms(EARLY_FEED_MS);
    wwdgt.feed();

    defmt::error!("still alive: the early feed did not reset the board");
    loop {}
}

/// Feeds the watchdog `MEALS` times, each one inside the window, which both
/// rounds do before breaking a bound in their own way.
fn feed_inside_window(wwdgt: &mut WwdgtRunning, delay: &mut impl DelayNs) {
    for meal in 1..=MEALS {
        delay.delay_ms(GOOD_FEED_MS);
        wwdgt.feed();
        defmt::info!(
            "fed {}/{} at {} ms into the period",
            meal,
            MEALS,
            GOOD_FEED_MS
        );
    }
}

#[interrupt]
fn WWDGT() {
    critical_section::with(|cs| {
        let mut watchdog = WATCHDOG.borrow(cs).borrow_mut();
        let wwdgt = watchdog.as_mut().unwrap();
        if wwdgt.is_pending() {
            wwdgt.clear_interrupt();
            // Not fed on purpose: clearing the flag only stops the handler from
            // re-entering, and the reset still lands one tick from here.
            defmt::info!("one tick left — the reset lands next");
        }
    })
}
