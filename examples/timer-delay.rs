//! TIMER5 as a source of blocking delays, logged over RTT.
//!
//! Four waits run in a loop, each announced before it starts and followed by a
//! running total. The totals are the check: they only match the wall-clock gap
//! between log lines if every scale was converted correctly, so a delay that is
//! off by a factor of a thousand shows up as lines arriving at the wrong pace
//! rather than as a wrong number.
//!
//! Covers: `Timer::into_delay`, the inherent `Delay::delay` on three different
//! `fugit` scales, and the same timer driven through `embedded_hal`'s `DelayNs`.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::pac;
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::{CFGR, PllFreq};
use gd32e2_hal::time::{MicrosDuration, MillisDuration, SecsDuration};

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    let clocks = CFGR::default()
        .sysclk(PllFreq::Mhz24)
        .freeze(&mut rcu, &mut dp.fmc);

    let mut timer = dp.timer5.constrain(&mut rcu, clocks).into_delay();

    let mut spent = MillisDuration::from_ticks(0);
    defmt::info!("Starting delay cycle");
    loop {
        let delay = MillisDuration::from_millis(500);
        defmt::info!("Starting {} ms delay", delay.as_millis());
        timer.delay(delay);
        spent += delay;
        defmt::info!("Spent {} ms", spent.as_millis());

        let delay = SecsDuration::from_secs(2);
        defmt::info!("Starting {} s delay", delay.as_secs());
        timer.delay(delay);
        spent += delay.convert();
        defmt::info!("Spent {} ms", spent.as_millis());

        let delay = MicrosDuration::from_millis(100);
        defmt::info!("Starting {} us delay", delay.as_micros());
        timer.delay(delay);
        spent += delay.convert();
        defmt::info!("Spent {} ms", spent.as_millis());

        let delay = 3_000;
        defmt::info!("Starting {} ms delay through the DelayNs trait", delay);
        timer.delay_ms(delay);
        spent += MillisDuration::from_millis(delay);
        defmt::info!("Spent {} ms", spent.as_millis());
    }
}
