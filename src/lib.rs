//! Hardware abstraction layer for the GD32E23x series (Cortex-M23), built on top
//! of the [`gd32e2`] peripheral access crate. Only the GD32E230 is implemented so
//! far; the crate is named for the family it is meant to grow into.
//!
//! The API leans on the type system: a pin's port, number and mode live in its
//! type, so an invalid alternate function or a method that makes no sense for the
//! current mode fails to compile rather than misbehaving on the board.
//!
//! # Chip variants
//!
//! The alternate-function map differs across the GD32E230x series — the same pin
//! at the same AF number can reach a different peripheral depending on the part
//! (`PA2` AF1 is `USART0_TX` on x4 but `USART1_TX` on x8). Exactly one of the
//! `gd32e230x4` / `gd32e230x6` / `gd32e230x8` features must be enabled; zero or
//! several is a compile error rather than a silently wrong pin map. The default
//! is `gd32e230x8`, matching the GD32E230K8U6 this HAL is developed against.
//!
//! # Getting started
//!
//! Clocks come first: [`rcu`] hands out a [`Clocks`](rcu::Clocks) value that the
//! other modules need, and enables each peripheral's clock as it is constructed.
//!
//! ```ignore
//! let mut dp = pac::Peripherals::take().unwrap();
//! let mut rcu = dp.rcu.constrain();
//! let clocks = CFGR::default()
//!     .sysclk(PllFreq::Mhz48)
//!     .freeze(&mut rcu, &mut dp.fmc);
//! let parts = dp.gpioa.split(&mut rcu);
//! let mut led = parts.pa5.into_output();
//! led.set_high().unwrap();
//! ```

#![no_std]
#![warn(missing_docs)]

#[cfg(not(any(feature = "gd32e230x4", feature = "gd32e230x6", feature = "gd32e230x8")))]
compile_error!(
    "select a chip variant: enable exactly one of the \
     `gd32e230x4` / `gd32e230x6` / `gd32e230x8` features"
);

#[cfg(any(
    all(feature = "gd32e230x4", feature = "gd32e230x6"),
    all(feature = "gd32e230x4", feature = "gd32e230x8"),
    all(feature = "gd32e230x6", feature = "gd32e230x8")
))]
compile_error!(
    "the `gd32e230x4` / `gd32e230x6` / `gd32e230x8` features are mutually \
     exclusive: enable exactly one"
);

/// Re-export of the peripheral access crate this HAL is built on. Referring to
/// it as `pac` keeps the door open to other parts of the family behind a single
/// alias, instead of naming a specific chip module throughout.
pub use gd32e2::gd32e230 as pac;

pub mod adc;
pub mod dma;
pub mod gpio;
pub mod rcu;
pub mod spi;
pub mod time;
pub mod usart;
