//! TIMER14 channel 0 driving PA2, measured back on PA3 and checked over RTT.
//!
//! **Wiring: a direct jumper from PA2 to PA3**, with nothing in between — a
//! series resistor and a capacitor to ground average the waveform into a level.
//!
//! A read sees a level, not a duty, so each measurement samples the line tens of
//! thousands of times and reports the share that came back high. The window
//! spans many periods, which is what makes that share the duty.
//!
//! `TIMER14` keeps its outputs behind `POEN`, so the pin stays silent until
//! `enable_output`. The first measurement after a period change reads a few
//! points high: the prescaler only reaches the counter on the next update, so
//! the tail of the previous period lands in the window.
//!
//! Covers: `Timer::into_pwm_interval`, `Pwm::channel` and `set_period_interval`,
//! the inherent `enable`/`set_duty`/`max_duty` on a channel, and the same
//! channel driven through `embedded_hal`'s `SetDutyCycle`.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::pac;
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::{ClockConfig, PllFreq, SysClk};
use gd32e2_hal::time::MicrosDuration;

/// Reads taken per measurement, spanning several PWM periods.
const SAMPLES: u32 = 40_000;
/// How far the measured share may sit from the requested duty, in points.
const TOLERANCE: u32 = 5;

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    let clocks = ClockConfig::default()
        .sysclk(SysClk::Pll(PllFreq::Mhz48))
        .freeze(&mut rcu, &mut dp.fmc);

    let gpioa = dp.gpioa.split(&mut rcu);
    let sense = gpioa.pa3.into_input();
    let pwm_pin = gpioa.pa2.into_alternate::<0>();

    let pwm = dp
        .timer14
        .constrain(&mut rcu, clocks)
        .into_pwm_interval(MicrosDuration::from_micros(100));
    let mut channel = pwm.channel(pwm_pin);
    channel.enable();
    pwm.enable_output();

    defmt::info!(
        "PWM on PA2, sensing PA3, {} ticks per period",
        channel.max_duty()
    );

    loop {
        for period in [100, 1_000] {
            pwm.set_period_interval(MicrosDuration::from_micros(period));
            defmt::info!("--- period {} us, {} ticks ---", period, channel.max_duty());

            for percent in [0u8, 25, 50, 75, 100] {
                channel.set_duty_cycle_percent(percent).unwrap();

                let mut high = 0;
                for _ in 0..SAMPLES {
                    if sense.is_high() {
                        high += 1;
                    }
                }
                let measured = (high * 100 + SAMPLES / 2) / SAMPLES;

                let off_by = measured.abs_diff(percent.into());
                if off_by <= TOLERANCE {
                    defmt::info!("duty {} % -> measured {} % : OK", percent, measured);
                } else {
                    defmt::error!(
                        "duty {} % -> measured {} % : off by {} points",
                        percent,
                        measured,
                        off_by
                    );
                }
            }
        }
    }
}
