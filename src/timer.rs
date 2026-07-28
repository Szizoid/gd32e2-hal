//! Timers: the counter core shared by every TIMER peripheral.
//!
//! A timer counts `CK_TIMERx` ticks divided by `PSC + 1`, and restarts from zero
//! once the count reaches `CAR`. Everything else a timer can do — PWM, input
//! capture, triggering other peripherals — is built on top of that core.
//!
//! ```ignore
//! let timer = Timer::new(&mut rcu, dp.timer5, clocks);
//! ```

use crate::pac;
use crate::rcu::{Clocks, Enable, Rcu, Reset};
use crate::time::Hertz;

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

impl<TIMERX> Timer<TIMERX>
where
    TIMERX: Instance,
{
    /// Clocks the peripheral, resets it, and records the clock feeding it.
    pub fn new(rcu: &mut Rcu, timer: TIMERX, clocks: Clocks) -> Timer<TIMERX> {
        TIMERX::enable(rcu);
        TIMERX::reset(rcu);
        Timer {
            timer,
            clk: TIMERX::clk(&clocks),
        }
    }

    /// Starts the counter, which then rolls over every `car + 1` ticks of
    /// `clk / (psc + 1)`.
    ///
    /// Both dividers are written before the counter runs: `UPG` loads them out
    /// of their shadow registers, and the update event it raises is consumed
    /// here so the first wait on the running timer sees a real rollover.
    pub fn start(self, psc: u16, car: u16) -> CountDownTimer<TIMERX> {
        self.timer.set_psc(psc);
        self.timer.set_car(car);
        self.timer.gen_update();
        self.timer.clear_upif();
        self.timer.set_cen(true);
        CountDownTimer {
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

/// A running timer, counting down the interval [`Timer::start`] was given.
///
/// Free-running: the counter reloads and starts over on every rollover, so the
/// interval repeats until the timer is stopped.
pub struct CountDownTimer<TIMERX> {
    timer: TIMERX,
    clk: Hertz,
}

impl<TIMERX> CountDownTimer<TIMERX>
where
    TIMERX: Instance,
{
    /// Blocks until the counter rolls over, then clears the update flag.
    ///
    /// Leaves the timer running, so calling this in a loop yields one full
    /// interval per call.
    pub fn wait(&self) {
        while !self.timer.upif() {}
        self.timer.clear_upif();
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
