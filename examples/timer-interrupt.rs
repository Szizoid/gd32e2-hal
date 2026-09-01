//! TIMER5 update interrupt toggling PA1, over RTT.
//!
//! The core sleeps in `main` (`wfi`) between ticks instead of polling; the
//! toggle and the log line happen entirely inside the handler. The timer and
//! the pin move into a `critical_section::Mutex<RefCell<Option<T>>>` so the
//! handler can reach them — `main` cannot hand out a borrow across two
//! contexts that can pre-empt each other.
//!
//! Covers: `CountDownTimer::listen`/`clear_interrupt`,
//! `cortex_m::peripheral::NVIC::unmask`, a `#[interrupt]` handler.

#![no_std]
#![no_main]

use core::cell::RefCell;

use cortex_m::peripheral::NVIC;
use cortex_m_rt::entry;
use critical_section::Mutex;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::gpio::{ErasedPin, Output, PushPull};
use gd32e2_hal::pac::{self, interrupt};
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::{ClockConfig, PllFreq, SysClk};
use gd32e2_hal::time::SecsDuration;
use gd32e2_hal::timer::{CountDownTimer, Event};

type Shared = (CountDownTimer<pac::Timer5>, ErasedPin<Output<PushPull>>);

static SHARED: Mutex<RefCell<Option<Shared>>> = Mutex::new(RefCell::new(None));

#[entry]
fn main() -> ! {
    let dp = pac::Peripherals::take().unwrap();
    let mut fmc = dp.fmc.constrain();
    let config = ClockConfig::default().sysclk(SysClk::Pll(PllFreq::Mhz48));
    let mut rcu = dp.rcu.constrain().freeze(&mut fmc, config);

    let gpioa = dp.gpioa.split(&mut rcu);
    let led = gpioa.pa1.into_push_pull_output().erase();

    let period: SecsDuration = 1u32.secs();
    let mut timer = dp.timer5.constrain(&mut rcu).start_interval(period);
    timer.listen(Event::Update);

    critical_section::with(|cs| {
        SHARED.borrow(cs).replace(Some((timer, led)));
    });

    // Safe: the handler only ever reaches SHARED, and only through the same
    // mutex `main` just released — no register of TIMER5 is touched here.
    unsafe { NVIC::unmask(pac::Interrupt::TIMER5) };

    defmt::info!("TIMER5 armed, PA1 toggles once a second from the handler");

    loop {
        cortex_m::asm::wfi();
    }
}

#[interrupt]
fn TIMER5() {
    critical_section::with(|cs| {
        let mut shared = SHARED.borrow(cs).borrow_mut();
        let (timer, led) = shared.as_mut().unwrap();
        timer.clear_interrupt(Event::Update);
        led.toggle();
        defmt::info!("tick");
    });
}
