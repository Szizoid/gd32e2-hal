//! Free watchdog resetting the chip, and the reset flag that proves it did.
//!
//! Self-checking across a reset rather than within one run: the first line of
//! every round says what brought the chip up. On the first power-up that is not
//! the watchdog; after the round below stops feeding it, the board resets and
//! the next round announces the watchdog by name. Seeing that line alternate is
//! the whole test — it can only appear if IRC40K really runs, since nothing
//! else clocks the counter. Each round opens with a few idle seconds, which is
//! the window for reattaching the probe: the board reboots itself here, and a
//! line printed before the RTT session is back is a line nobody sees. The
//! reset also throws `probe-rs run` off the core, which reports it as an
//! exception — that is the watchdog working, not the firmware faulting.
//!
//! The watchdog keeps counting while a debugger holds the core, so the board
//! resets under a paused probe too. That is the hardware default: stopping it
//! in debug mode takes the DBG module, which this HAL does not cover.
//!
//! Covers: `Fwdgt::new` / `start_timeout` / `FwdgtRunning::feed`, and
//! `Rcu::reset_flag` / `clear_reset_flags`.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::pac;
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::{ClockConfig, PllFreq, ResetFlag, SysClk};
use gd32e2_hal::time::SecsDuration;
use gd32e2_hal::watchdog::Fwdgt;

const TIMEOUT: SecsDuration = SecsDuration::from_secs(2);
const MEALS: u32 = 5;
const MEAL_INTERVAL_MS: u32 = 1_500;
/// Head start for reattaching the probe after the board resets itself, so the
/// line naming the reset cause is not printed into a closed RTT session.
const STARTUP_DELAY_MS: u32 = 3_000;

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    let clocks = ClockConfig::default()
        .sysclk(SysClk::Pll(PllFreq::Mhz48))
        .freeze(&mut rcu, &mut dp.fmc);

    let mut delay = dp.timer5.constrain(&mut rcu, clocks).into_delay();
    delay.delay_ms(STARTUP_DELAY_MS);

    if rcu.reset_flag(ResetFlag::FreeWatchdog) {
        defmt::info!("up after a watchdog reset");
    } else {
        defmt::info!("up without a watchdog reset");
    }
    // Otherwise the flag stands for good and every later round reports a
    // watchdog reset whether or not one happened.
    rcu.clear_reset_flags();

    let mut fwdgt = Fwdgt::new(&mut rcu, dp.fwdgt).start_timeout(TIMEOUT);
    defmt::info!("watchdog started, timeout {} s", TIMEOUT.as_secs());

    // Fed with room to spare, so these rounds pass and prove the counter is
    // being reloaded rather than merely running slowly.
    for meal in 1..=MEALS {
        delay.delay_ms(MEAL_INTERVAL_MS);
        fwdgt.feed();
        defmt::info!("fed {}/{} after {} ms", meal, MEALS, MEAL_INTERVAL_MS);
    }

    defmt::info!(
        "no more feeding — expect a reset within {} s",
        TIMEOUT.as_secs()
    );
    loop {}
}
