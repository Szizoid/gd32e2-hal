//! Typed units for frequencies and durations, aliased from `fugit`.
//!
//! The units themselves come from [`fugit`], which keeps rates and durations
//! apart as distinct kinds and converts between scales at compile time. This
//! module names its `u32` instances, so signatures stay short and the dependency
//! is pinned in one place; `u64` is not aliased, the core being 32-bit.
//!
//! The scale lives in the type and costs nothing at runtime, so a value crosses
//! between units by conversion rather than by arithmetic on a bare number.
//!
//! The generic [`Duration`] and [`Rate`] are re-exported for signatures that
//! stay open to any scale, as are the suffix traits: [`ExtU32`] for durations
//! and [`RateExtU32`] for frequencies, joined by our own [`BpsExtU32`] for bit
//! rates, which `fugit` has no suffix for. The whole crate is re-exported too, so
//! `Instant`, the tick-based `TimerDuration` and the `u64` widths are reachable
//! without adding a matching dependency downstream.
//!
//! ```ignore
//! use gd32e2_hal::time::ExtU32;
//! let period = 5.secs();
//! ```

pub use fugit;
pub use fugit::{Duration, ExtU32, Rate, RateExtU32};

/// Frequency in hertz.
pub type Hertz = fugit::HertzU32;
/// Frequency in kilohertz.
pub type Kilohertz = fugit::KilohertzU32;
/// Frequency in megahertz.
pub type Megahertz = fugit::MegahertzU32;
/// Frequency in gigahertz.
pub type Gigahertz = fugit::GigahertzU32;

/// Bit rate in symbols per second.
///
/// The very same type as [`Hertz`], a bit rate being a rate on the one-second
/// scale: the name reads right in a signature but checks nothing, and a clock
/// frequency passed as a bit rate still compiles.
pub type Bps = fugit::Rate<u32, 1, 1>;

/// Duration in picoseconds.
pub type PicosDuration = fugit::PicosDurationU32;
/// Duration in nanoseconds.
pub type NanosDuration = fugit::NanosDurationU32;
/// Duration in microseconds.
pub type MicrosDuration = fugit::MicrosDurationU32;
/// Duration in milliseconds.
pub type MillisDuration = fugit::MillisDurationU32;
/// Duration in seconds.
pub type SecsDuration = fugit::SecsDurationU32;
/// Duration in minutes.
pub type MinutesDuration = fugit::MinutesDurationU32;
/// Duration in hours.
pub type HoursDuration = fugit::HoursDurationU32;

/// Suffix for bit rates, `fugit` having none: `115_200.bps()`.
pub trait BpsExtU32 {
    /// Reads the number as symbols per second.
    fn bps(self) -> Bps;
}

impl BpsExtU32 for u32 {
    fn bps(self) -> Bps {
        Bps::from_raw(self)
    }
}
