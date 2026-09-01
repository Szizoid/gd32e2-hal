//! PWM on PA2 ramping a duty, averaged back on PA3 by an ADC interrupt.
//!
//! **Wiring: a direct jumper from PA2 to PA3**, with nothing in between — so
//! each conversion reads the instantaneous level, 0 or full scale, never a
//! point in between. The ramp is recovered statistically, the way `pwm.rs`
//! recovers it: average enough samples and the mean *is* the duty.
//!
//! That only works if the samples land at different points of the PWM period.
//! At 48 MHz one PWM period is 24000 ticks and one sample period 6576, whose
//! greatest common divisor is 48 — so the sampling point steps through exactly
//! 500 distinct phases before repeating, and a batch of 500 visits each one
//! once. The mean is then the duty exactly, not approximately. A round ratio
//! like 200 ms over 500 us instead locks onto a single phase and reports a
//! constant 0 or 4095 forever, which is what this example did first.
//!
//! `main` ramps the duty in a plain blocking loop — no interrupt needed there,
//! it is the thing actively changing. `TIMER5`'s update interrupt fires on a
//! fixed period and does nothing but trigger a conversion (`Adc::start`); the
//! ADC's own `Eoc` interrupt fires once that conversion is ready, accumulates
//! it (`Adc::result`) and reports every full batch. Two independent sources,
//! chained through the ADC hardware rather than through each other.
//!
//! Covers: `Adc::start`/`result`/`listen`, `CountDownTimer::listen` driving
//! another peripheral's work instead of the caller's own.

#![no_std]
#![no_main]

use core::cell::RefCell;

use cortex_m::peripheral::NVIC;
use cortex_m_rt::entry;
use critical_section::Mutex;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::adc::{Adc, Event as AdcEvent, SampTime};
use gd32e2_hal::gpio::{Analog, Pin};
use gd32e2_hal::pac::{self, interrupt};
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::{AdcPsc, AdcSel, ClockConfig, PllFreq, SysClk};
use gd32e2_hal::time::{MicrosDuration, MillisDuration};
use gd32e2_hal::timer::{CountDownTimer, Event as TimerEvent};

/// PWM period on PA2: 24000 ticks of the 48 MHz timer clock.
const PWM_PERIOD: MicrosDuration = MicrosDuration::from_micros(500);
/// Sampling period: 6576 ticks. Deliberately not a divisor of the PWM period —
/// see the module docs for why the ratio matters more than the rate.
const SAMPLE_PERIOD: MicrosDuration = MicrosDuration::from_micros(137);
/// Samples per reported average: exactly the number of distinct phases the
/// sampling point visits, so every part of the period is weighted once.
const BATCH: u16 = 500;
/// Full scale of a 12-bit conversion.
const ADC_MAX_CODE: u32 = 4095;
/// Duty change per step, in percent.
const DUTY_STEP: i32 = 5;
/// How long each duty step holds — several batches long, so a reported average
/// belongs to one duty rather than straddling two.
const RAMP_STEP: MillisDuration = MillisDuration::from_millis(500);

type AdcPin = Pin<'A', 3, Analog>;
/// The ADC, the timer triggering it, the pin being read, and the batch so far
/// as a running sum and a count.
type Shared = (Adc, CountDownTimer<pac::Timer5>, AdcPin, u32, u16);

static SHARED: Mutex<RefCell<Option<Shared>>> = Mutex::new(RefCell::new(None));

#[entry]
fn main() -> ! {
    let dp = pac::Peripherals::take().unwrap();
    let mut fmc = dp.fmc.constrain();
    let config = ClockConfig::default()
        .sysclk(SysClk::Pll(PllFreq::Mhz48))
        .adc_sel(AdcSel::Prescaled(AdcPsc::Apb2Div8));
    let mut rcu = dp.rcu.constrain().freeze(&mut fmc, config);

    let gpioa = dp.gpioa.split(&mut rcu);
    let pwm_pin = gpioa.pa2.into_alternate::<0>();
    let adc_pin = gpioa.pa3.into_analog();

    let mut pwm = dp.timer14.constrain(&mut rcu).into_pwm_interval(PWM_PERIOD);
    let mut channel = pwm.channel(pwm_pin);
    channel.enable();
    pwm.enable_output();

    let mut adc = dp.adc.constrain(&mut rcu);
    adc.listen(AdcEvent::Eoc);

    let mut trigger = dp.timer5.constrain(&mut rcu).start_interval(SAMPLE_PERIOD);
    trigger.listen(TimerEvent::Update);

    critical_section::with(|cs| {
        SHARED
            .borrow(cs)
            .replace(Some((adc, trigger, adc_pin, 0, 0)));
    });

    unsafe {
        NVIC::unmask(pac::Interrupt::TIMER5);
        NVIC::unmask(pac::Interrupt::ADC_CMP);
    }

    defmt::info!("ramping PA2, averaging {} samples of PA3 per report", BATCH);

    let mut delay = dp.timer2.constrain(&mut rcu).into_delay();
    let mut duty: i32 = 0;
    let mut step: i32 = DUTY_STEP;
    loop {
        channel.set_duty_cycle_percent(duty as u8).unwrap();
        delay.delay(RAMP_STEP);
        duty += step;
        if duty == 0 || duty == 100 {
            step = -step;
        }
    }
}

#[interrupt]
fn TIMER5() {
    critical_section::with(|cs| {
        let mut shared = SHARED.borrow(cs).borrow_mut();
        let (adc, trigger, pin, _sum, _count) = shared.as_mut().unwrap();
        trigger.clear_interrupt(TimerEvent::Update);
        adc.start(pin, SampTime::Cycles55_5);
    });
}

#[interrupt]
fn ADC_CMP() {
    critical_section::with(|cs| {
        let mut shared = SHARED.borrow(cs).borrow_mut();
        let (adc, _trigger, _pin, sum, count) = shared.as_mut().unwrap();
        *sum += u32::from(adc.result());
        *count += 1;
        if *count == BATCH {
            // Rounded, and only once per batch: logging inside the sampling
            // window would stretch the very intervals being measured.
            let mean = (*sum + u32::from(BATCH) / 2) / u32::from(BATCH);
            defmt::info!("mean = {} ({} %)", mean, mean * 100 / ADC_MAX_CODE);
            *sum = 0;
            *count = 0;
        }
    });
}
