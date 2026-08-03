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

use core::convert::Infallible;

use embedded_hal::delay::DelayNs;
use embedded_hal::pwm::{ErrorType, SetDutyCycle};

use crate::gpio::{Alternate, Pin};
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
    /// Reads back the prescaler dividing the clock into counter ticks.
    fn read_psc(&self) -> u16;
    /// Writes the auto-reload value the counter rolls over at.
    fn set_car(&self, car: u16);
    /// Reads back the auto-reload value the counter rolls over at.
    fn read_car(&self) -> u16;
    /// Reads the counter, which advances on its own while the timer runs.
    fn read_cnt(&self) -> u16;
    /// Raises an update event in software, loading the shadowed dividers.
    fn gen_update(&self);
    /// Runs or halts the counter.
    fn set_cen(&self, on: bool);
    /// Update flag — set by hardware on every rollover, never cleared by it.
    fn upif(&self) -> bool;
    /// Clears the update flag.
    fn clear_upif(&self);
    /// Produces a second handle to the same peripheral.
    ///
    /// # Safety
    ///
    /// Ownership of a peripheral is what the whole HAL builds its typestate on:
    /// one handle means one configuration in flight. A second handle sidesteps
    /// that, and keeping the two from contradicting each other is on the caller
    /// — either by handing them disjoint registers, or by never reconfiguring
    /// the peripheral through both.
    unsafe fn steal(&self) -> Self;
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
                fn read_psc(&self) -> u16 {
                    self.psc().read().psc().bits()
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
                fn read_car(&self) -> u16 {
                    self.car().read().car().bits()
                }
                fn read_cnt(&self) -> u16 {
                    self.cnt().read().cnt().bits()
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
                unsafe fn steal(&self) -> Self{
                    unsafe { Self::steal() }
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
    /// Hands the timer over to PWM and starts it on the given period.
    ///
    /// The period is the counter cycle every channel of this timer shares, and
    /// `car + 1` is also the duty resolution the channels get: a larger reload
    /// buys finer steps at a lower frequency.
    pub fn into_pwm(self, psc: u16, car: u16) -> Pwm<TIMERX> {
        start_counter(&self.timer, psc, car);
        Pwm {
            timer: self.timer,
            clk: self.clk,
        }
    }
    /// Hands the timer over to PWM, taking the period as a duration.
    ///
    /// Splits the period between the dividers the same way
    /// [`start_interval`](Self::start_interval) does, keeping the reload as
    /// large as it can — which here is what leaves the duty as many steps as
    /// possible.
    pub fn into_pwm_interval<const NOM: u64, const DENOM: u64>(
        self,
        interval: Duration<u32, NOM, DENOM>,
    ) -> Pwm<TIMERX> {
        let (psc, car) = dividers(interval_to_ticks(interval, self.clk));
        self.into_pwm(psc, car)
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
    /// Returns the current counter value, in timer ticks.
    ///
    /// The counter runs from zero up to the reload value and starts over, so
    /// the reading only tells where inside the current interval the timer is,
    /// not how many intervals have passed. One tick lasts `(psc + 1)` cycles
    /// of the clock feeding the timer.
    pub fn cnt(&self) -> u16 {
        self.timer.read_cnt()
    }
    /// Returns the value the counter rolls over at, in timer ticks.
    ///
    /// Read back from the hardware rather than remembered, so it holds whether
    /// the interval came from [`start_interval`](Timer::start_interval) or from
    /// a raw [`start`](Timer::start). One full interval is `car() + 1` ticks,
    /// which is why `cnt()` reaching this value is the last tick before a
    /// rollover, not the rollover itself.
    pub fn car(&self) -> u16 {
        self.timer.read_car()
    }
    /// Returns the prescaler the counter is running on.
    ///
    /// One counter tick lasts `psc() + 1` cycles of the clock feeding the
    /// timer, which is what turns a [`cnt`](Self::cnt) reading into time.
    /// Read back from the hardware, so it holds for a raw
    /// [`start`](Timer::start) as well.
    pub fn psc(&self) -> u16 {
        self.timer.read_psc()
    }
    /// Returns how far into the current interval the counter is, as a duration.
    ///
    /// Reads [`cnt`](Self::cnt) and turns it into time, so the result restarts
    /// from zero at every rollover and never exceeds one interval. Intervals
    /// themselves are not counted: what has passed since the timer started is
    /// the caller's to track.
    ///
    /// The scale comes from the binding, the same way it is given to
    /// [`start_interval`](Timer::start_interval): `let t: MillisDuration =
    /// timer.elapsed();`. Pick it to match the interval — a scale coarser than
    /// one tick floors to zero, and a scale so fine that the full interval no
    /// longer fits `u32` saturates instead of wrapping.
    ///
    /// Resolution is one timer tick, `psc() + 1` cycles of the timer clock.
    pub fn elapsed<const NOM: u64, const DENOM: u64>(&self) -> Duration<u32, NOM, DENOM> {
        let cnt = u64::from(self.cnt());
        let psc = u64::from(self.psc());
        let clk = u64::from(self.clk.to_Hz());
        Duration::<u32, NOM, DENOM>::from_ticks(
            (cnt.saturating_mul(psc + 1).saturating_mul(DENOM) / (clk.saturating_mul(NOM)))
                .min(u32::MAX.into()) as u32,
        )
    }

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

/// A timer running as the period behind one or more PWM channels.
///
/// Owns the counter the channels compare against, which is what keeps the
/// period in one place: channels carry their own duty, and the frequency they
/// all run at is set here, once, on the way in.
pub struct Pwm<TIMERX> {
    timer: TIMERX,
    clk: Hertz,
}

impl<TIMERX: Instance> Pwm<TIMERX> {
    /// Configures one channel of this timer and hands it out on the given pin.
    ///
    /// Which channel it is follows from the pin: the silicon routes each pin to
    /// exactly one channel of one timer, so a pin that reaches this timer at all
    /// leaves no choice to make. The channel comes out configured but not
    /// enabled, and with no duty set yet.
    ///
    /// The pin moves in and stays there for as long as the channel lives, which
    /// is what keeps it from being reconfigured out from under a running output.
    ///
    /// Several pins can reach the same channel — `TIMER2` channel 0 answers on
    /// both `PA6` and `PB4` — and handing over each of them yields two channels
    /// writing one compare register. Both pins then carry the same signal, which
    /// is occasionally the point; the duty of one, however, is the duty of the
    /// other, whichever wrote last.
    pub fn channel<PIN, const C: u8>(&self, pin: PIN) -> PwmChannel<TIMERX, PIN, C>
    where
        PIN: PwmPin<TIMERX, C>,
        TIMERX: PwmOps<C>,
    {
        self.timer.apply_pwm_mode();
        PwmChannel {
            // Each channel reaches its own compare register and its own bits of
            // the shared ones, so the handles this hands out never configure the
            // same thing twice — the obligation `steal` places on the caller.
            timer: unsafe { self.timer.steal() },
            pin,
        }
    }

    /// Changes the period without disturbing the running counter.
    ///
    /// Channels already handed out keep the duty they were given **in ticks**,
    /// so a new reload silently moves what fraction of the period that is: half
    /// of a thousand ticks is all of five hundred. Set the duties again after
    /// changing the period, or read the new [`max_duty`](PwmChannel::max_duty)
    /// and scale them.
    ///
    /// The prescaler reaches the counter at the next update event, so the
    /// cycle in flight when this is called still runs on the old one.
    pub fn set_period(&self, psc: u16, car: u16) {
        self.timer.set_psc(psc);
        self.timer.set_car(car);
    }
    /// Changes the period, taking it as a duration.
    ///
    /// Splits it between the dividers exactly as
    /// [`into_pwm_interval`](Timer::into_pwm_interval) does, keeping the reload
    /// as large as it fits so the duty keeps as many steps as possible.
    pub fn set_period_interval<const NOM: u64, const DENOM: u64>(
        &self,
        interval: Duration<u32, NOM, DENOM>,
    ) {
        let (psc, car) = dividers(interval_to_ticks(interval, self.clk));
        self.set_period(psc, car);
    }
}

impl<TIMERX> Pwm<TIMERX>
where
    TIMERX: PrimaryOutput,
{
    /// Lets the channels reach their pins.
    ///
    /// Timers carrying a `CCHP` register keep their outputs behind this one
    /// switch, and it starts off: a channel of such a timer stays silent no
    /// matter how it is configured until this is called. The timers without the
    /// register have no such gate and need nothing.
    pub fn enable_output(&self) {
        self.timer.set_poen(true);
    }
    /// Cuts every channel off from its pin at once, keeping them configured.
    pub fn disable_output(&self) {
        self.timer.set_poen(false);
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

/// Register operations on compare channel `C` of a timer.
///
/// Implemented once per timer and channel that exist together, so a channel the
/// hardware does not have cannot be named: `TIMER13` carries channel 0 alone,
/// `TIMER5` has no channels at all. Channel registers differ from one channel to
/// the next, which is why the number lives in the type rather than in an
/// argument.
pub trait PwmOps<const C: u8>: Instance {
    /// Configures the channel as a PWM output and readies it for a duty value.
    ///
    /// Covers the whole group of one-time fields at once — direction, compare
    /// mode and polarity — since a channel set to PWM mode while still pointed
    /// at its input drives nothing, and the two are only correct together.
    fn apply_pwm_mode(&self);
    /// Writes the compare value the channel switches its output at.
    ///
    /// The output stays active while the counter is below this value, so one
    /// full period is `car() + 1` and duty is the ratio between the two.
    fn set_chxcv(&self, cv: u16);
    /// Enables or disables the channel output, leaving its setup in place.
    fn set_chxen(&self, on: bool);
}

macro_rules! pwm {
    {$($Timer:ty => [
        $(
            ($Ch:expr, ($chctl_reg:ident, $chms:ident, $chcomctl:ident, $chcomsen:ident), ($chcv:ident, $chval:ident), $chen:ident, $chp:ident)$(,)?
        )+]$(,)?)+
    } => {
        $($(impl PwmOps<$Ch> for $Timer {
            fn apply_pwm_mode(&self) {
                self.chctl2().modify(|_, w| w.$chp().not_inverted());
                self.$chctl_reg().modify(|_, w| {
                    w.$chms()
                        .output()
                        .$chcomctl()
                        .pwm_mode0()
                        .$chcomsen()
                        .enabled()
                });
            }
            #[allow(unused_unsafe)]
            fn set_chxcv(&self, cv: u16) {
                self.$chcv().write(|w| unsafe { w.$chval().bits(cv) });
            }
            fn set_chxen(&self, on: bool) {
                self.chctl2().modify(|_, w| match on {
                    true => w.$chen().enabled(),
                    false => w.$chen().disabled(),
                })
            }
        })+)+
    };
}

pwm! {
    pac::Timer0 => [(0, (chctl0_output, ch0ms, ch0comctl, ch0comsen), (ch0cv, ch0val), ch0en, ch0p),
        (1, (chctl0_output, ch1ms, ch1comctl, ch1comsen), (ch1cv, ch1val), ch1en, ch1p),
        (2, (chctl1_output, ch2ms, ch2comctl, ch2comsen), (ch2cv, ch2val), ch2en, ch2p),
        (3, (chctl1_output, ch3ms, ch3comctl, ch3comsen), (ch3cv, ch3val), ch3en, ch3p)],
    pac::Timer2 => [(0, (chctl0_output, ch0ms, ch0comctl, ch0comsen), (ch0cv, ch0val), ch0en, ch0p),
        (1, (chctl0_output, ch1ms, ch1comctl, ch1comsen), (ch1cv, ch1val), ch1en, ch1p),
        (2, (chctl1_output, ch2ms, ch2comctl, ch2comsen), (ch2cv, ch2val), ch2en, ch2p),
        (3, (chctl1_output, ch3ms, ch3comctl, ch3comsen), (ch3cv, ch3val), ch3en, ch3p)],
    pac::Timer13 => [(0, (chctl0_output, ch0ms, ch0comctl, ch0comsen), (ch0cv, ch0val), ch0en, ch0p)],
    pac::Timer14 => [(0, (chctl0_output, ch0ms, ch0comctl, ch0comsen), (ch0cv, ch0val), ch0en, ch0p),
        (1, (chctl0_output, ch1ms, ch1comctl, ch1comsen), (ch1cv, ch1val), ch1en, ch1p)],
    pac::Timer15 => [(0, (chctl0_output, ch0ms, ch0comctl, ch0comsen), (ch0cv, ch0val), ch0en, ch0p)],
    pac::Timer16 => [(0, (chctl0_output, ch0ms, ch0comctl, ch0comsen), (ch0cv, ch0val), ch0en, ch0p)]
}

/// The output switch shared by every channel of a timer that has one.
///
/// Only the timers carrying a `CCHP` register implement this: `TIMER0`,
/// `TIMER14`, `TIMER15` and `TIMER16`. It sits above the per-channel enables —
/// with it off, the channels stay configured and the counter keeps running
/// while the pins show nothing.
pub trait PrimaryOutput: Instance {
    /// Enables or disables the outputs of all channels at once.
    fn set_poen(&self, on: bool);
}

macro_rules! poen {
    ($($Timer:ty$(,)?)+) => {
        $(impl PrimaryOutput for $Timer {
            fn set_poen(&self, on: bool) {
                self.cchp().modify(|_, w| match on {
                    true => w.poen().enabled(),
                    false => w.poen().disabled(),
                });
            }
        })+
    };
}

poen!(pac::Timer0, pac::Timer14, pac::Timer15, pac::Timer16);

/// Marks a pin the silicon routes to channel `C` of `TIMERX`, in the right
/// alternate function.
pub trait PwmPin<TIMERX, const C: u8> {}

macro_rules! pwm_pins {
    ( $( $TIMERX:ty: $( $C:literal: [ $($p:literal $n:literal : $af:literal),* $(,)? ] )* ),* $(,)? ) => {
        $($($( impl PwmPin<$TIMERX, $C> for Pin<$p, $n, Alternate<$af>> {} )*)*)*
    };
}

// Complementary outputs (`CHx_ON`), break inputs and `ETI` share these same
// pins at other alternate functions; only the plain compare outputs are listed.
pwm_pins! {
    pac::Timer0:
        0: [ 'A' 8:2 ]
        1: [ 'A' 9:2 ]
        2: [ 'A' 10:2 ]
        3: [ 'A' 11:2 ],
    pac::Timer2:
        0: [ 'A' 6:1, 'B' 4:1 ]
        1: [ 'A' 7:1, 'B' 5:1 ]
        2: [ 'B' 0:1 ]
        3: [ 'B' 1:1 ],
    pac::Timer13:
        0: [ 'A' 4:4, 'A' 7:4, 'B' 1:2 ],
    pac::Timer15:
        0: [ 'A' 6:5, 'B' 8:2 ],
    pac::Timer16:
        0: [ 'A' 7:5, 'B' 9:2 ],
}

#[cfg(feature = "gd32e230x8")]
pwm_pins! {
    pac::Timer14:
        0: [ 'A' 2:0, 'B' 14:1 ]
        1: [ 'A' 3:0, 'B' 15:1 ],
}

/// One PWM output of a timer, configured and ready to take a duty value.
///
/// Channels of the same timer are independent in everything but the period:
/// `PSC` and `CAR` belong to the counter they share, so changing the frequency
/// changes it for all of them at once. Duty is the channel's own.
///
/// Each channel carries its own handle to the peripheral, which is what lets
/// four of them exist while the timer is a single value. Writing a duty touches
/// only that channel's compare register, but the enables of all channels live in
/// one register: enabling a channel while another is being enabled elsewhere —
/// from an interrupt, say — can lose one of the two writes.
pub struct PwmChannel<TIMERX, PIN, const C: u8> {
    timer: TIMERX,
    pin: PIN,
}

impl<TIMERX: PwmOps<C>, PIN: PwmPin<TIMERX, C>, const C: u8> PwmChannel<TIMERX, PIN, C> {
    /// Drives the pin from the comparison, keeping the duty already set.
    pub fn enable(&self) {
        self.timer.set_chxen(true);
    }
    /// Stops driving the pin, leaving the channel configured.
    pub fn disable(&self) {
        self.timer.set_chxen(false);
    }

    /// Sets the duty, in timer ticks of the period.
    ///
    /// The output is active for `cv` ticks out of [`max_duty`](Self::max_duty),
    /// so zero is a permanently inactive pin and anything from `max_duty` up is
    /// a permanently active one. The new value reaches the output at the next
    /// rollover, not mid-period.
    pub fn set_duty(&self, cv: u16) {
        self.timer.set_chxcv(cv);
    }
    /// Returns the duty that corresponds to a fully active output.
    ///
    /// This is the period the counter is running on, `car() + 1` ticks, and it
    /// is also the resolution: a period of 100 ticks leaves 100 distinct duties.
    /// A period spanning the whole counter reports one tick short, being the
    /// only value that does not fit `u16`.
    pub fn max_duty(&self) -> u16 {
        self.timer.read_car().saturating_add(1)
    }

    /// Gives the pin back, dropping the channel.
    ///
    /// The output is left exactly as it was — still enabled, still driving its
    /// duty. Call [`disable`](Self::disable) first if the pin is wanted quiet.
    pub fn release(self) -> PIN {
        self.pin
    }
}

/// Writing a duty cannot fail: the value goes straight into a compare register
/// the hardware always accepts.
impl<TIMERX: PwmOps<C>, PIN: PwmPin<TIMERX, C>, const C: u8> ErrorType
    for PwmChannel<TIMERX, PIN, C>
{
    type Error = Infallible;
}

/// Duty control for portable drivers, delegating to [`PwmChannel::set_duty`].
///
/// The trait's `set_duty_cycle_percent`, `_fraction`, `_fully_on` and
/// `_fully_off` are its own defaults built on these two, and scale against the
/// period the timer currently runs on.
impl<TIMERX: PwmOps<C>, PIN: PwmPin<TIMERX, C>, const C: u8> SetDutyCycle
    for PwmChannel<TIMERX, PIN, C>
{
    fn max_duty_cycle(&self) -> u16 {
        self.max_duty()
    }
    fn set_duty_cycle(&mut self, duty: u16) -> Result<(), Self::Error> {
        self.set_duty(duty);
        Ok(())
    }
}
