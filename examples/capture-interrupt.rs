//! TIMER14 capturing PA3 on interrupt, over intervals longer than the counter.
//!
//! **Wiring: a direct jumper from PA2 to PA3** — the same one `capture.rs` and
//! `pwm.rs` use, with nothing in between.
//!
//! Where `capture.rs` spins in `nb::block!` waiting for an edge, here nothing
//! polls: the channel raises an interrupt when it latches one, and `main` only
//! drives the wave and prints results. That example is the one to read for
//! `select_edge` and `interval`, which measure a pulse width inside a single
//! counter cycle; this one measures periods that span several.
//!
//! The point of the example is the range. The counter is 16 bits at 1 us per
//! tick, so it wraps every 65.536 ms and a latched value alone cannot express a
//! longer interval — which is why [`CaptureChannel::interval`] documents spans
//! past one cycle as wrong. Listening for `Update` as well fixes that: the
//! handler counts rollovers, and a timestamp becomes `cycles * 65536 + value`.
//! Two of the three periods driven below do not fit in one cycle.
//!
//! Both events reach one NVIC line, so the handler tells them apart by
//! `is_listening` and `is_pending` rather than by the flag alone — an untouched
//! channel raises its flag once per rollover all on its own.
//!
//! The handler measures but never logs: RTT inside the measured window stretches
//! the very interval being measured, so it hands the result to `main` instead.
//!
//! Covers: `CaptureChannel::listen`/`is_listening`/`is_pending`,
//! `Capture::listen`/`clear_interrupt` for the counter's own event, and the two
//! sharing one handler.

#![no_std]
#![no_main]

use core::cell::RefCell;

use cortex_m::peripheral::NVIC;
use cortex_m_rt::entry;
use critical_section::Mutex;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::gpio::{Alternate, Pin};
use gd32e2_hal::pac::{self, interrupt};
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::{ClockConfig, PllFreq, SysClk};
use gd32e2_hal::time::MillisDuration;
use gd32e2_hal::timer::{Capture, CaptureChannel, Edge, Event};

/// Capture prescaler for one tick per microsecond at 48 MHz.
const CAPTURE_PSC: u16 = 47;
/// Ticks in one counter cycle — what a rollover is worth.
const COUNTER_SPAN: u64 = 1 << 16;
/// Half of it, the line between "latched before the rollover" and "after".
const COUNTER_HALF: u16 = 1 << 15;
/// Half-periods to drive, in milliseconds. The first fits inside one counter
/// cycle, the other two do not — 65.536 ms is the whole span.
const HALF_PERIODS_MS: [u32; 3] = [20, 60, 150];
/// Periods driven per half-period, the first edge only starting the count.
const PERIODS: u8 = 3;
/// How far a measurement may sit from the requested time, in percent.
const TOLERANCE_PERCENT: u64 = 5;

type SensePin = Pin<'A', 3, Alternate<0>>;
/// The time base, the channel latching it, the rollovers counted so far, the
/// timestamp of the previous rising edge, and the interval waiting for `main`.
type Shared = (
    Capture<pac::Timer14>,
    CaptureChannel<pac::Timer14, SensePin, 1>,
    u32,
    Option<u64>,
    Option<u64>,
);

static SHARED: Mutex<RefCell<Option<Shared>>> = Mutex::new(RefCell::new(None));

/// Takes the interval the handler last measured, if there is one.
fn take_interval() -> Option<u64> {
    critical_section::with(|cs| {
        let mut shared = SHARED.borrow(cs).borrow_mut();
        let (_capture, _sense, _rollovers, _last, interval) = shared.as_mut().unwrap();
        interval.take()
    })
}

/// Forgets the last edge, so the next one starts a fresh count.
///
/// Needed whenever the wave being driven changes: the edge that opens a new
/// period would otherwise be measured against the last edge of the old one, and
/// the gap between those two is the *previous* period.
fn forget_last_edge() {
    critical_section::with(|cs| {
        let mut shared = SHARED.borrow(cs).borrow_mut();
        let (_capture, _sense, _rollovers, last, interval) = shared.as_mut().unwrap();
        *last = None;
        *interval = None;
    });
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

    let mut capture = dp
        .timer14
        .constrain(&mut rcu, clocks)
        .into_capture(CAPTURE_PSC);
    let mut sense = capture.channel(sense_pin, Edge::Rising);
    sense.enable();
    sense.listen();
    capture.listen(Event::Update);

    critical_section::with(|cs| {
        SHARED
            .borrow(cs)
            .replace(Some((capture, sense, 0, None, None)));
    });

    unsafe { NVIC::unmask(pac::Interrupt::TIMER14) };

    source.set_low();
    defmt::info!("driving PA2, capturing PA3 on interrupt, 1 us per tick");

    loop {
        for half_ms in HALF_PERIODS_MS {
            let expected_us = u64::from(half_ms) * 2 * 1_000;
            let spans = expected_us / COUNTER_SPAN;
            defmt::info!(
                "--- period {} ms, {} counter rollovers per period ---",
                half_ms * 2,
                spans
            );

            forget_last_edge();

            let half = MillisDuration::from_millis(half_ms);
            for _ in 0..PERIODS {
                source.set_high();
                delay.delay(half);
                source.set_low();
                delay.delay(half);

                // The first rising edge only starts the count, so the first pass
                // of each shape has nothing to report yet.
                if let Some(measured) = take_interval() {
                    let off_by = measured.abs_diff(expected_us) * 100 / expected_us;
                    if off_by <= TOLERANCE_PERCENT {
                        defmt::info!("period {} us, expected {} us : OK", measured, expected_us);
                    } else {
                        defmt::error!(
                            "period {} us, expected {} us : off by {} %",
                            measured,
                            expected_us,
                            off_by
                        );
                    }
                }
            }
        }
    }
}

#[interrupt]
fn TIMER14() {
    critical_section::with(|cs| {
        let mut shared = SHARED.borrow(cs).borrow_mut();
        let (capture, sense, rollovers, last, interval) = shared.as_mut().unwrap();

        // The capture goes first: whether it belongs before or after a rollover
        // that has not been accounted for yet is decided by its own value, and
        // clearing the update flag first would throw that away. A timestamp in
        // the low half with a rollover still pending was latched after that
        // rollover; one in the high half, before it.
        if sense.is_listening() && sense.is_pending() {
            let wrapped = capture.is_pending(Event::Update);
            if let Ok(cv) = sense.read() {
                let cycles = match wrapped && cv < COUNTER_HALF {
                    true => u64::from(*rollovers) + 1,
                    false => u64::from(*rollovers),
                };
                let now = cycles * COUNTER_SPAN + u64::from(cv);
                *interval = last.map(|previous| now - previous);
                *last = Some(now);
            }
        }

        if capture.is_listening(Event::Update) && capture.is_pending(Event::Update) {
            capture.clear_interrupt(Event::Update);
            *rollovers += 1;
        }
    });
}
