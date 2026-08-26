//! Free watchdog timer.
//!
//! Clocked from IRC40K rather than a bus, so it keeps counting whatever the
//! system clock does, and resets the chip unless it is fed before the counter
//! reaches zero.
//!
//! Starting it is irreversible — the hardware has no way to stop it — so
//! [`start`](Fwdgt::start) consumes [`Fwdgt`] and [`FwdgtRunning`] offers no way
//! back.

use crate::pac;
use crate::rcu::{IRC40K, Rcu};
use crate::time::Duration;

/// Largest reload value: `RLD` is 12 bits.
const RLD_MAX: u16 = 0xFFF;
/// Counter ticks one full reload spans, the reload value being counted from zero.
const RLD_SPAN: u32 = RLD_MAX as u32 + 1;

/// Divider on IRC40K ahead of the counter (Table 12-1).
///
/// Sets the timeout range together with the reload value: `/4` spans up to
/// 409 ms, `/256` up to 26 s.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[allow(missing_docs)]
pub enum FwdgtPsc {
    Div4 = 0b000,
    Div8 = 0b001,
    Div16 = 0b010,
    Div32 = 0b011,
    Div64 = 0b100,
    Div128 = 0b101,
    Div256 = 0b110,
}

impl FwdgtPsc {
    /// Every divider, smallest first — the order [`dividers`] searches in.
    const ALL: [Self; 7] = [
        Self::Div4,
        Self::Div8,
        Self::Div16,
        Self::Div32,
        Self::Div64,
        Self::Div128,
        Self::Div256,
    ];

    /// The divider itself, derived from the `PSC` code so the two cannot part ways.
    const fn divider(self) -> u32 {
        4 << (self as u8)
    }
}

/// Splits a timeout given in IRC40K ticks into the smallest divider that spans
/// it and the reload value to go with it.
///
/// The smallest divider is the one with the finest resolution, so the realised
/// timeout sits as close under the requested one as the hardware allows —
/// division truncates, and for a watchdog erring short is the safe direction.
/// Past what the dividers reach (26 s) both saturate.
fn dividers(ticks: u32) -> (FwdgtPsc, u16) {
    let ticks = ticks.max(1);
    for psc in FwdgtPsc::ALL {
        let div = psc.divider();
        if ticks <= RLD_SPAN * div {
            return (psc, (ticks / div).saturating_sub(1) as u16);
        }
    }
    (FwdgtPsc::Div256, RLD_MAX)
}

fn timeout_to_ticks<const NOM: u64, const DENOM: u64>(
    timeout: Duration<u32, NOM, DENOM>,
    clock: u32,
) -> u32 {
    let raw_time = u64::from(timeout.as_ticks());
    let raw_freq = u64::from(clock);
    (raw_time.saturating_mul(raw_freq).saturating_mul(NOM) / DENOM).min(u32::MAX.into()) as u32
}

/// The watchdog before it is started, the only state its period can be set in.
pub struct Fwdgt {
    fwdgt: pac::Fwdgt,
}

impl Fwdgt {
    /// Starts IRC40K, which clocks the watchdog, and takes the peripheral.
    ///
    /// Nothing is written to the watchdog itself — its registers stay write
    /// protected until [`start`](Self::start) opens them.
    pub fn new(rcu: &mut Rcu, fwdgt: pac::Fwdgt) -> Self {
        rcu.enable_irc40k();
        Self { fwdgt }
    }

    /// Sets the period and starts counting down; there is no way back.
    ///
    /// The timeout is `(rld + 1)` ticks of `IRC40K / psc`. Blocks while the
    /// dividers reach the counter, which runs at 40 kHz.
    ///
    /// # Panics
    ///
    /// If `rld` exceeds 12 bits. Masking it silently would hand back a working
    /// watchdog with a period nobody asked for.
    pub fn start(self, psc: FwdgtPsc, rld: u16) -> FwdgtRunning {
        assert!(rld <= RLD_MAX, "FWDGT reload must fit in 12 bits");
        self.fwdgt.ctl().write(|w| w.cmd().enable());
        self.fwdgt.psc().write(|w| w.psc().bits(psc as u8));
        self.fwdgt.rld().write(|w| w.rld().bits(rld));
        while self.fwdgt.stat().read().pud().is_ongoing() {}
        while self.fwdgt.stat().read().rud().is_ongoing() {}
        self.fwdgt.ctl().write(|w| w.cmd().reset());
        self.fwdgt.ctl().write(|w| w.cmd().start());
        FwdgtRunning { fwdgt: self.fwdgt }
    }

    /// Same as [`start`](Self::start), but takes the period as a duration and
    /// picks the dividers itself.
    ///
    /// Any scale works with no conversion at the call site — `2.secs()`,
    /// `500.millis()`. Truncating, so the realised timeout is at most one tick
    /// short of the request; anything past 26 s saturates there, and zero
    /// becomes one tick.
    pub fn start_timeout<const NOM: u64, const DENOM: u64>(
        self,
        timeout: Duration<u32, NOM, DENOM>,
    ) -> FwdgtRunning {
        let (psc, rld) = dividers(timeout_to_ticks(timeout, IRC40K));
        self.start(psc, rld)
    }
}

/// The watchdog once it is counting down.
///
/// No way out by design: the hardware cannot stop it, and neither the
/// peripheral nor the period can be recovered.
pub struct FwdgtRunning {
    fwdgt: pac::Fwdgt,
}

impl FwdgtRunning {
    /// Reloads the counter, postponing the reset by one full period.
    ///
    /// The manual requires 7 or more IRC40K cycles (~175 µs) between two
    /// reloads; nothing here enforces it, since the watchdog exposes no counter
    /// to read and timing it off the system clock would tie it to the very
    /// clock it is meant to survive.
    pub fn feed(&mut self) {
        self.fwdgt.ctl().write(|w| w.cmd().reset());
    }
}
