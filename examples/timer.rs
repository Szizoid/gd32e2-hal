//! TIMER5 counting a fixed interval, logged over RTT.
//!
//! The basic timer is set up to roll over every 5 seconds and the main loop
//! blocks on it, so the elapsed count printed on each pass is also a check that
//! the period is real: the log lines should appear 5 seconds apart.
//!
//! TIMER13 runs alongside as a source of delays, spacing the counter readings
//! far enough apart to see them climb.
//!
//! Covers: `TimerExt::constrain`, `Timer::start_interval` taking a `fugit`
//! duration, the blocking `CountDownTimer::wait`, and `CountDownTimer::cnt`
//! reading the counter as it advances.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::pac;
use gd32e2_hal::rcu::{CFGR, PllFreq, RcuExt};
use gd32e2_hal::time::{ExtU32, MillisDuration, SecsDuration};
use gd32e2_hal::timer::TimerExt;

/// How many counter readings to log on each pass of the interval.
const SAMPLES_PER_CYCLE: u8 = 8;
/// How long to wait between two readings, so that the counter moves visibly.
const SAMPLE_GAP: MillisDuration = MillisDuration::from_millis(200);

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    let clocks = CFGR::default()
        .sysclk(PllFreq::Mhz48)
        .freeze(&mut rcu, &mut dp.fmc);

    let period: SecsDuration = 5u32.secs();
    let timer = dp.timer5.constrain(&mut rcu, clocks).start_interval(period);
    let mut delay = dp.timer13.constrain(&mut rcu, clocks).into_delay();

    let raw_period = period.as_secs();
    defmt::info!("TIMER5 started, rolling over every {} s", raw_period);

    let mut cycles = 0;
    loop {
        // Spaced out by a second timer: read back to back the counter barely
        // moves, and the log says nothing about the pace it advances at.
        for _ in 0..SAMPLES_PER_CYCLE {
            defmt::info!("cnt = {}", timer.cnt());
            delay.delay(SAMPLE_GAP);
        }

        timer.wait();
        cycles += 1;
        defmt::info!("cycle {}, {} s elapsed", cycles, cycles * raw_period);
    }
}
