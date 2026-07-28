//! Typed units for frequencies and durations, aliased from `fugit`.
//!
//! The units themselves come from [`fugit`], which keeps rates and durations
//! apart as distinct kinds and converts between scales at compile time. This
//! module only names the instances the HAL uses, so signatures stay short and
//! the dependency is pinned in one place.
//!
//! `fugit` is re-exported: its suffix trait, and any unit not aliased here, are
//! reachable without adding a matching dependency downstream.
//!
//! ```ignore
//! use gd32e2_hal::time::fugit::ExtU32;
//! let period = 5.secs();
//! ```

pub use fugit;

/// Frequency in hertz.
pub type Hertz = fugit::Hertz<u32>;
