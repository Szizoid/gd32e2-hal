//! TIMER14 channel 1 timing a square wave the CPU drives on PA2, over RTT.
//!
//! **Wiring: a direct jumper from PA2 to PA3** — the same one `pwm.rs` uses,
//! with nothing in between.
//!
//! The wave is driven in software rather than by a second timer: PWM and
//! capture are two states of one peripheral, and the pins reachable here land
//! on `TIMER14` either way, so the timer that captures cannot also drive.
//! `TIMER5` carries the delays, having no channels of its own to spare.
//!
//! Capture runs at one tick per microsecond, which makes the numbers in the log
//! directly comparable to the requested times. The counter is free running and
//! wraps every 65 ms, so the intervals here stay well inside one cycle.
//!
//! Edges are timed as the CPU makes them, so each measurement carries the few
//! microseconds spent between writing the pin and reading the capture — visible
//! at the short end, negligible at the long one.
//!
//! This is the blocking side of capture: the core waits on every edge in
//! `nb::block!`. `capture-interrupt.rs` is the same peripheral driven the other
//! way — the edge wakes the core instead — and counts rollovers so that
//! intervals longer than one counter cycle come out right.
//!
//! Covers: `Timer::into_capture`, `Capture::channel` with an `Edge`,
//! `CaptureChannel::read` through `nb::block!` including the overcapture error,
//! `select_edge` and `interval`.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::pac;
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::{ClockConfig, PllFreq, SysClk};
use gd32e2_hal::time::MicrosDuration;
use gd32e2_hal::timer::Edge;

/// Capture prescaler for one tick per microsecond at 48 MHz.
const CAPTURE_PSC: u16 = 47;
/// How far a measurement may sit from the requested time, in percent.
const TOLERANCE_PERCENT: u32 = 5;
/// High and low times of the wave, in microseconds.
const SHAPES: [(u32, u32); 3] = [(250, 750), (1_000, 4_000), (5_000, 5_000)];

/// Reports a measurement against what it should have been.
fn check(what: &str, measured: u32, expected: u32) {
    let off_by = measured.abs_diff(expected) * 100 / expected.max(1);
    if off_by <= TOLERANCE_PERCENT {
        defmt::info!("{}: {} us, expected {} us : OK", what, measured, expected);
    } else {
        defmt::error!(
            "{}: {} us, expected {} us : off by {} %",
            what,
            measured,
            expected,
            off_by
        );
    }
}

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    let clocks = ClockConfig::default()
        .sysclk(SysClk::Pll(PllFreq::Mhz48))
        .freeze(&mut rcu, &mut dp.fmc);

    let gpioa = dp.gpioa.split(&mut rcu);
    let source = gpioa.pa2.into_push_pull_output();
    let sense_pin = gpioa.pa3.into_alternate::<0>();

    let mut delay = dp.timer5.constrain(&mut rcu, clocks).into_delay();

    let capture = dp
        .timer14
        .constrain(&mut rcu, clocks)
        .into_capture(CAPTURE_PSC);
    let sense = capture.channel(sense_pin, Edge::Rising);
    sense.enable();

    defmt::info!("driving PA2, capturing PA3, 1 us per tick");

    loop {
        for (high, low) in SHAPES {
            defmt::info!("--- {} us high, {} us low ---", high, low);

            // Two rising edges bracket exactly one period. The line starts low,
            // so the first edge below is the first one the channel ever sees.
            source.set_high();
            let first = block!(sense.read());
            delay.delay(MicrosDuration::from_micros(high));
            source.set_low();
            delay.delay(MicrosDuration::from_micros(low));

            source.set_high();
            let second = block!(sense.read());

            match (first, second) {
                (Ok(first), Ok(second)) => {
                    // The fall that ends this pulse bounds the high time. The
                    // switch is a race with the line, which is why it happens
                    // right after a rise rather than anywhere in the cycle.
                    sense.select_edge(Edge::Falling);
                    delay.delay(MicrosDuration::from_micros(high));
                    source.set_low();
                    let fall = block!(sense.read());
                    sense.select_edge(Edge::Rising);

                    // Nothing is logged until the pulse is over: RTT takes long
                    // enough to stretch a measurement it lands inside.
                    let period: MicrosDuration = sense.interval(first, second);
                    check("period", period.as_micros(), high + low);
                    match fall {
                        Ok(fall) => {
                            let width: MicrosDuration = sense.interval(second, fall);
                            check("width", width.as_micros(), high);
                        }
                        Err(_) => defmt::warn!("overcapture on the falling edge"),
                    }
                }
                _ => {
                    defmt::warn!("overcapture between the rising edges, skipping");
                    source.set_low();
                }
            }

            delay.delay(MicrosDuration::from_micros(low));
        }
    }
}
