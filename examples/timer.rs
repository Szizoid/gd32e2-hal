//! TIMER5 counting a fixed interval, logged over RTT.
//!
//! The basic timer is set up to roll over every 5 seconds and the main loop
//! blocks on it, so the elapsed count printed on each pass is also a check that
//! the period is real: the log lines should appear 5 seconds apart.
//!
//! Covers: `TimerExt::constrain`, `Timer::start_interval` taking a `fugit`
//! duration, and the blocking `CountDownTimer::wait`.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::pac;
use gd32e2_hal::rcu::{CFGR, PllFreq, RcuExt};
use gd32e2_hal::time::{ExtU32, SecsDuration};
use gd32e2_hal::timer::TimerExt;

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    let clocks = CFGR::default()
        .sysclk(PllFreq::Mhz48)
        .freeze(&mut rcu, &mut dp.fmc);

    let period: SecsDuration = 5u32.secs();
    let timer = dp.timer5.constrain(&mut rcu, clocks);
    let timer = timer.start_interval(period);

    let raw_period = period.as_secs();
    defmt::info!("TIMER5 started, rolling over every {} s", raw_period);

    let mut cycles = 0;
    loop {
        timer.wait();
        cycles += 1;
        defmt::info!("cycle {}, {} s elapsed", cycles, cycles * raw_period);
    }
}
