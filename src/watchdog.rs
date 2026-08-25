//! The chip's two watchdogs.
//!
//! Both reset the chip unless fed in time and neither can be stopped once
//! started, so each is a pair of types with no way back. What separates them is
//! what they can catch:
//!
//! - [`Fwdgt`] runs off IRC40K, independent of the system clock, and spans
//!   milliseconds to 26 seconds. It notices that code stopped feeding it —
//!   nothing more.
//! - [`Wwdgt`] runs off `PCLK1` and spans tens of milliseconds. Feeding it too
//!   early is a fault as well, so it also notices code that kept running but
//!   lost its pace, and it can raise an interrupt one tick before the reset.
//!
//! A watchdog that has to survive a broken clock tree is [`Fwdgt`]; one that
//! checks the rhythm of a tight control loop is [`Wwdgt`].

mod fwdgt;
mod wwdgt;

pub use fwdgt::*;
pub use wwdgt::*;
