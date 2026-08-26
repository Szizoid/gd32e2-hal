//! Window watchdog timer.
//!
//! Resets the chip for feeding it too early as well as too late. The early
//! bound is the point of this watchdog: code that has run away into a loop
//! still calling `feed` keeps a plain watchdog happy, but breaks the pace this
//! one checks.
//!
//! Clocked from `PCLK1`, so unlike [`Fwdgt`](crate::fwdgt::Fwdgt) its timeout
//! moves with the bus clock and spans tens of milliseconds rather than seconds.

use crate::pac;
use crate::rcu::{Enable, Rcu, Reset};

/// Largest period, in counter ticks: the timeout is counted by `CNT[5:0]`.
const TICKS_MAX: u8 = 0x3F;
/// `CNT[6]`, standing for "the counter is still alive" — the chip resets the
/// moment it goes low, so it sits above the timeout rather than inside it.
const ALIVE_BIT: u8 = 1 << 6;

/// Prescaler on the counter clock, named after the `PSC` field it writes.
///
/// The names count that field alone: the counter runs at `PCLK1 / 4096 / N`, so
/// even [`Div1`](Self::Div1) is already `PCLK1` divided by 4096.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[allow(missing_docs)]
pub enum WwdgtPsc {
    Div1 = 0b00,
    Div2 = 0b01,
    Div4 = 0b10,
    Div8 = 0b11,
}

/// Checks a period and a window and returns them as the register expects them.
///
/// # Panics
///
/// If either value needs more than 6 bits, or if `win` exceeds `cnt`, which
/// would leave the window open from the start and silently turn this into an
/// ordinary watchdog.
fn checked_bits(cnt: u8, win: u8) -> (u8, u8) {
    assert!(cnt <= TICKS_MAX, "WWDGT period must fit in 6 bits");
    assert!(win <= TICKS_MAX, "WWDGT window must fit in 6 bits");
    assert!(
        win <= cnt,
        "WWDGT window must not outlast the period it opens in"
    );
    (cnt | ALIVE_BIT, win | ALIVE_BIT)
}

/// The watchdog before it is started, the only state it can be configured in.
pub struct Wwdgt {
    wwdgt: pac::Wwdgt,
}

impl Wwdgt {
    /// Enables the peripheral's clock and resets it.
    ///
    /// Nothing is started here — the counter only begins running once
    /// [`start`](Self::start) sets `WDGTEN`.
    pub fn new(rcu: &mut Rcu, wwdgt: pac::Wwdgt) -> Self {
        <pac::Wwdgt as Enable>::enable(rcu);
        <pac::Wwdgt as Reset>::reset(rcu);
        Self { wwdgt }
    }

    /// Sets the period and the window and starts counting down; there is no way
    /// back.
    ///
    /// Both `cnt` and `win` are counted in ticks of `PCLK1 / 4096 / psc`, from 0
    /// to 63 — the register's own top bit is added here, so a value that would
    /// reset the chip on the spot cannot be passed in. The period lasts
    /// `cnt + 1` ticks, and the window opens `cnt - win` ticks into it: feeding
    /// before that resets the chip just as failing to feed does.
    ///
    /// # Panics
    ///
    /// If either value needs more than 6 bits, or if `win` exceeds `cnt`, which
    /// would leave the window open from the start and silently turn this into an
    /// ordinary watchdog.
    pub fn start(self, psc: WwdgtPsc, cnt: u8, win: u8) -> WwdgtRunning {
        let (cnt, win) = checked_bits(cnt, win);
        self.wwdgt
            .cfg()
            .modify(|_, w| w.psc().bits(psc as u8).win().bits(win));
        self.wwdgt
            .ctl()
            .modify(|_, w| w.cnt().bits(cnt).wdgten().enabled());
        WwdgtRunning {
            wwdgt: self.wwdgt,
            cnt,
        }
    }
}

/// The watchdog once it is counting down.
///
/// No way out by design: `WDGTEN` ignores a written zero and only a hardware
/// reset clears it, so neither the peripheral nor the period comes back.
pub struct WwdgtRunning {
    wwdgt: pac::Wwdgt,
    cnt: u8,
}

impl WwdgtRunning {
    /// Reloads the counter, which must happen inside the window: too early
    /// resets the chip exactly as too late does.
    ///
    /// The write puts a zero in `WDGTEN`, which hardware ignores — that bit
    /// cannot be cleared by software at all.
    pub fn feed(&mut self) {
        self.wwdgt.ctl().write(|w| w.cnt().bits(self.cnt));
    }
    /// Changes the period and the window, taking effect at the next
    /// [`feed`](Self::feed).
    ///
    /// The counter itself is left alone: writing it here would count as a
    /// second feed, and one arriving straight after the last would land above
    /// the new window and reset the chip. The new window reaches `CFG` at once,
    /// so the next feed is already judged by it.
    ///
    /// # Panics
    ///
    /// On the same three conditions as [`Wwdgt::start`].
    pub fn set_period(&mut self, cnt: u8, win: u8) {
        let (cnt, win) = checked_bits(cnt, win);
        self.wwdgt.cfg().modify(|_, w| w.win().bits(win));
        self.cnt = cnt;
    }

    /// Lets the counter reaching `0x40` raise an interrupt — one tick before
    /// the reset, the last moment anything can still run.
    ///
    /// Irreversible, like the watchdog itself: `EWIE` ignores a written zero, so
    /// there is no `unlisten`. Unmasking the line in the NVIC is the caller's.
    pub fn listen(&mut self) {
        self.wwdgt.cfg().modify(|_, w| w.ewie().enable());
    }
    /// Whether the early wakeup interrupt is enabled.
    pub fn is_listening(&self) -> bool {
        self.wwdgt.cfg().read().ewie().bit_is_set()
    }
    /// Whether the counter has reached `0x40`.
    ///
    /// Hardware sets the flag whether or not [`listen`](Self::listen) was
    /// called, so this doubles as a plain poll for "one tick left".
    pub fn is_pending(&self) -> bool {
        self.wwdgt.stat().read().ewif().is_pending()
    }
    /// Clears the flag, which a handler must do before returning — the flag is
    /// the request, and hardware never drops it.
    pub fn clear_interrupt(&mut self) {
        self.wwdgt.stat().write(|w| w.ewif().finished());
    }
}
