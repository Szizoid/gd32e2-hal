//! Timers: the counter core shared by every TIMER peripheral.
//!
//! A timer counts `CK_TIMERx` ticks divided by `PSC + 1`, and restarts from zero
//! once the count reaches `CAR`. Everything else a timer can do — PWM, input
//! capture, triggering other peripherals — is built on top of that core.
//!
//! ```ignore
//! let timer = dp.timer5.constrain(&mut rcu, clocks);
//! let timer = timer.start_interval(500.millis());
//! timer.wait();
//! ```

use embedded_hal::delay::DelayNs;

use crate::pac;
use crate::rcu::{Clocks, Enable, Rcu, Reset};
use crate::time::{Duration, Hertz, NanosDuration};

/// Counts one prescaler or counter cycle can span, both fields being 16-bit.
const MAX_COUNT: u32 = 1 << 16;

/// Splits a tick count into the `PSC` and `CAR` values whose cycle spans it.
///
/// The prescaler is kept as small as the counter width allows, leaving the
/// counter as much of the interval as possible: its step is the resolution
/// everything built on the core inherits. The division truncates, so the
/// realised interval is at most one prescaler cycle short of the requested one.
///
/// A zero tick count is raised to one — the shortest interval the hardware can
/// express, since a counter cycle is at least one tick.
fn dividers(ticks: u32) -> (u16, u16) {
    let ticks = ticks.max(1);
    let psc = ticks.div_ceil(MAX_COUNT);
    let car = ticks / psc;
    ((psc - 1) as u16, (car - 1) as u16)
}

/// Converts an interval of any scale into ticks of `clock`.
///
/// A `Duration` is a plain count whose unit is `NOM / DENOM` seconds, so the
/// interval in seconds is `as_ticks() * NOM / DENOM` and the tick count follows
/// by multiplying with the frequency. The division comes last: taking `NOM /
/// DENOM` first would floor to zero on every scale finer than a second.
///
/// Saturates instead of wrapping, both in the product and on the way down to
/// `u32`. Anything past `u32::MAX` ticks is beyond what the dividers can span
/// anyway — under a minute at 72 MHz — so the result is the longest interval the
/// hardware can express.
fn interval_to_ticks<const NOM: u64, const DENOM: u64>(
    interval: Duration<u32, NOM, DENOM>,
    clock: Hertz,
) -> u32 {
    let raw_time = u64::from(interval.as_ticks());
    let raw_freq = u64::from(clock.to_Hz());
    (raw_time.saturating_mul(raw_freq).saturating_mul(NOM) / DENOM).min(u32::MAX.into()) as u32
}

/// Loads the dividers and sets the counter running.
///
/// Both are written before the counter runs: `UPG` loads them out of their
/// shadow registers, and the update event it raises is consumed here so the
/// first wait afterwards sees a real rollover.
fn start_counter<TIMERX: Instance>(timer: &TIMERX, psc: u16, car: u16) {
    timer.set_psc(psc);
    timer.set_car(car);
    timer.gen_update();
    timer.clear_upif();
    timer.set_cen(true);
}

/// Blocks until the counter rolls over, then clears the flag it raised.
///
/// The counter is left as it was: hardware never clears `UPIF` itself, so the
/// clear here is what makes the next wait measure a fresh cycle.
fn wait_update<TIMERX: Instance>(timer: &TIMERX) {
    while !timer.upif() {}
    timer.clear_upif();
}

/// A timer peripheral, tying it to the bus that clocks it.
///
/// Which APB a timer sits on is fixed in silicon, so the frequency for the
/// period arithmetic comes from the type rather than from an argument.
pub trait Instance: Enable + Reset {
    /// The `CK_TIMERx` branch feeding this timer, taken from a frozen tree.
    fn clk(clocks: &Clocks) -> Hertz;
    /// Writes the prescaler, which reaches the counter on the next update event.
    fn set_psc(&self, psc: u16);
    /// Writes the auto-reload value the counter rolls over at.
    fn set_car(&self, car: u16);
    /// Raises an update event in software, loading the shadowed dividers.
    fn gen_update(&self);
    /// Runs or halts the counter.
    fn set_cen(&self, on: bool);
    /// Update flag — set by hardware on every rollover, never cleared by it.
    fn upif(&self) -> bool;
    /// Clears the update flag.
    fn clear_upif(&self);
}

macro_rules! timer_instance {
    ($($TIMERX:ty => $clk:ident,)+) => {
        $(
            impl Instance for $TIMERX {
                fn clk(clocks: &Clocks) -> Hertz {
                    clocks.$clk()
                }
                fn set_psc(&self, psc: u16) {
                    self.psc().write(|w| w.psc().bits(psc));
                }
                // The `CAR` writer is unsafe on TIMER2 only, where the SVD leaves
                // the field unconstrained, and safe on the other six. One macro
                // body serves all of them, so the block is always written and the
                // lint is silenced for the six that do not need it. Nothing is
                // actually being asserted here: every `u16` is a legal reload
                // value in counting mode, zero included.
                #[allow(unused_unsafe)]
                fn set_car(&self, car: u16) {
                    self.car().write(|w| unsafe { w.car().bits(car) });
                }
                fn gen_update(&self) {
                    self.swevg().write(|w| w.upg().set_bit());
                }
                fn set_cen(&self, on: bool) {
                    self.ctl0().modify(|_, w| w.cen().bit(on));
                }
                fn upif(&self) -> bool {
                    self.intf().read().upif().bit_is_set()
                }
                fn clear_upif(&self) {
                    self.intf().write(|w| w.upif().clear());
                }
            }
        )+
    };
}

timer_instance! {
    pac::Timer2 => pclk1_tim,
    pac::Timer5 => pclk1_tim,
    pac::Timer13 => pclk1_tim,
    pac::Timer0 => pclk2_tim,
    pac::Timer14 => pclk2_tim,
    pac::Timer15 => pclk2_tim,
    pac::Timer16 => pclk2_tim,
}

/// A stopped timer, holding the peripheral and the frequency feeding it.
///
/// Clocked and reset on construction, but the counter is not running: the
/// methods that wait on the count live on the running type, so waiting on a
/// timer that was never started cannot be expressed.
pub struct Timer<TIMERX> {
    timer: TIMERX,
    clk: Hertz,
}

impl<TIMERX: Instance> Timer<TIMERX> {
    /// Clocks the peripheral, resets it, and records the clock feeding it.
    pub fn new(rcu: &mut Rcu, timer: TIMERX, clocks: Clocks) -> Timer<TIMERX> {
        TIMERX::enable(rcu);
        TIMERX::reset(rcu);
        Timer {
            timer,
            clk: TIMERX::clk(&clocks),
        }
    }
    /// Returns the peripheral.
    ///
    /// The clock is left enabled and no reset is performed — a later `new()`
    /// does both anyway.
    pub fn release(self) -> TIMERX {
        self.timer
    }

    /// Starts the counter, which then rolls over every `car + 1` ticks of
    /// `clk / (psc + 1)`.
    ///
    /// The dividers reach the counter before it runs, so the first
    /// [`wait`](CountDownTimer::wait) already measures the full interval.
    pub fn start(self, psc: u16, car: u16) -> CountDownTimer<TIMERX> {
        start_counter(&self.timer, psc, car);
        CountDownTimer {
            timer: self.timer,
            clk: self.clk,
        }
    }

    /// Starts the counter, which then rolls over once per `interval`.
    ///
    /// The interval is taken in whatever scale the caller wrote it in — `5.secs()`,
    /// `500.millis()`, `100.micros()` all work, with no conversion at the call
    /// site. The dividers are derived from it against this timer's own clock,
    /// truncating: the realised interval is at most one prescaler cycle short.
    ///
    /// Intervals past what the 16-bit dividers can span (just under a minute at
    /// 72 MHz) saturate to the longest the hardware can produce, and a zero
    /// interval becomes a single tick.
    pub fn start_interval<const NOM: u64, const DENOM: u64>(
        self,
        interval: Duration<u32, NOM, DENOM>,
    ) -> CountDownTimer<TIMERX> {
        let (psc, car) = dividers(interval_to_ticks(interval, self.clk));
        self.start(psc, car)
    }

    /// Hands the timer over to blocking delays.
    ///
    /// The counter carries no interval of its own afterwards: every
    /// [`delay`](Delay::delay) sets up its own and tears it down again.
    pub fn into_delay(self) -> Delay<TIMERX> {
        Delay {
            timer: self.timer,
            clk: self.clk,
        }
    }
}

/// A running timer, counting down the interval [`Timer::start`] was given.
///
/// Free-running: the counter reloads and starts over on every rollover, so the
/// interval repeats until the timer is stopped.
pub struct CountDownTimer<TIMERX> {
    timer: TIMERX,
    clk: Hertz,
}

impl<TIMERX: Instance> CountDownTimer<TIMERX> {
    /// Blocks until the counter rolls over, then clears the update flag.
    ///
    /// Leaves the timer running, so calling this in a loop yields one full
    /// interval per call.
    pub fn wait(&self) {
        wait_update(&self.timer);
    }

    /// Halts the counter and hands the timer back in its stopped form.
    ///
    /// The counter keeps its current value; a later [`start`](Timer::start)
    /// reloads the dividers and restarts from zero.
    pub fn stop(self) -> Timer<TIMERX> {
        self.timer.set_cen(false);
        Timer {
            timer: self.timer,
            clk: self.clk,
        }
    }
    /// Halts the counter and returns the peripheral, skipping the stopped form.
    pub fn release(self) -> TIMERX {
        self.stop().timer
    }
}

/// A timer given over to blocking delays.
///
/// Unlike [`CountDownTimer`] this type promises no interval at all: the value
/// only says which timer the delays run on. Each call configures the dividers,
/// waits, and stops the counter again, so nothing survives between calls and a
/// delay can be asked for any length at any time.
pub struct Delay<TIMERX> {
    timer: TIMERX,
    clk: Hertz,
}

impl<TIMERX: Instance> Delay<TIMERX> {
    /// Blocks for `interval`, in whatever scale the caller wrote it in.
    ///
    /// Rounding and the saturation ceiling are the same as in
    /// [`Timer::start_interval`], the dividers being derived the same way.
    pub fn delay<const NOM: u64, const DENOM: u64>(&mut self, interval: Duration<u32, NOM, DENOM>) {
        let (psc, car) = dividers(interval_to_ticks(interval, self.clk));
        start_counter(&self.timer, psc, car);
        wait_update(&self.timer);
        self.timer.set_cen(false);
    }

    /// Takes the timer back out of delay duty, stopped and ready to be started.
    pub fn into_timer(self) -> Timer<TIMERX> {
        Timer {
            timer: self.timer,
            clk: self.clk,
        }
    }

    /// Returns the peripheral.
    ///
    /// The clock is left enabled and no reset is performed — a later `new()`
    /// does both anyway.
    pub fn release(self) -> TIMERX {
        self.timer
    }
}

/// Blocking delays for portable drivers, delegating to [`Delay::delay`].
///
/// The resolution is one timer tick — around 20 ns at 48 MHz — so a request
/// finer than that is served as a single tick rather than not at all. The
/// trait's `delay_us` and `delay_ms` are its own defaults, which split long
/// waits into chunks of at most `u32::MAX` nanoseconds; a chunk that size is
/// well inside what the dividers can span, so the saturation ceiling of
/// [`Timer::start_interval`] is never reached through this trait.
impl<TIMERX: Instance> DelayNs for Delay<TIMERX> {
    fn delay_ns(&mut self, ns: u32) {
        self.delay(NanosDuration::from_nanos(ns));
    }
}

/// Entry point on the raw peripheral, mirroring `GpioExt` and `DmaExt`.
pub trait TimerExt: Sized {
    /// Consumes the peripheral and returns it clocked, reset and stopped.
    ///
    /// Same thing [`Timer::new`] does, reached from the peripheral instead.
    fn constrain(self, rcu: &mut Rcu, clocks: Clocks) -> Timer<Self>;
}

impl<TIMERX: Instance> TimerExt for TIMERX {
    fn constrain(self, rcu: &mut Rcu, clocks: Clocks) -> Timer<Self> {
        Timer::new(rcu, self, clocks)
    }
}
