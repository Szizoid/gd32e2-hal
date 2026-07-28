//! TIMER5 counting a fixed interval, logged over RTT.
//!
//! The basic timer is set up to roll over every 5 seconds and the main loop
//! blocks on it, so the elapsed count printed on each pass is also a check that
//! the period is real: the log lines should appear 5 seconds apart.
//!
//! Covers: `Timer::new`, `Timer::start` with a raw prescaler and reload value,
//! and the blocking `CountDownTimer::wait`.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::pac;
use gd32e2_hal::rcu::{CFGR, PllFreq, RcuExt};
use gd32e2_hal::timer::Timer;

/// `pclk1_tim` with `sysclk` at 48 MHz and the APB1 prescaler left undivided.
const TIMER_CLK: u32 = 48_000_000;
/// Counter tick rate chosen so that one tick is a millisecond.
const TICK_HZ: u32 = 1_000;
/// Interval between rollovers.
const PERIOD_S: u32 = 5;

const PSC: u16 = (TIMER_CLK / TICK_HZ - 1) as u16;
const CAR: u16 = (TICK_HZ * PERIOD_S - 1) as u16;

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    let clocks = CFGR::default()
        .sysclk(PllFreq::Mhz48)
        .freeze(&mut rcu, &mut dp.fmc);

    let timer = Timer::new(&mut rcu, dp.timer5, clocks);
    let timer = timer.start(PSC, CAR);
    defmt::info!("TIMER5 started, rolling over every {} s", PERIOD_S);

    let mut cycles = 0;
    loop {
        timer.wait();
        cycles += 1;
        defmt::info!("cycle {}, {} s elapsed", cycles, cycles * PERIOD_S);
    }
}
