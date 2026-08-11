//! One import for the traits whose methods this HAL is used through.
//!
//! Traits are re-exported anonymously (`as _`), so the methods arrive without
//! the names: a prelude cannot collide with anything the user has defined.
//! Types are not re-exported — import those from their own modules.
//!
//! ```ignore
//! use gd32e2_hal::prelude::*;
//!
//! let mut rcu = dp.rcu.constrain();
//! let parts = dp.gpioa.split(&mut rcu);
//! let timer = dp.timer5.constrain(&mut rcu, clocks).start_interval(500.millis());
//! ```

pub use crate::adc::AdcExt as _;
pub use crate::dma::DmaExt as _;
pub use crate::gpio::GpioExt as _;
pub use crate::rcu::RcuExt as _;
pub use crate::timer::TimerExt as _;

pub use crate::time::{ExtU32 as _, RateExtU32 as _};

pub use embedded_hal::delay::DelayNs as _;
pub use embedded_hal::digital::{InputPin as _, OutputPin as _, StatefulOutputPin as _};
pub use embedded_hal::i2c::I2c as _;
pub use embedded_hal::pwm::SetDutyCycle as _;
pub use embedded_hal::spi::SpiBus as _;
pub use embedded_hal_nb::serial::{Read as _, Write as _};
pub use embedded_io::{Read as _, ReadReady as _, Write as _, WriteReady as _};

/// Retries a non-blocking call until it returns something other than
/// [`WouldBlock`](nb::Error::WouldBlock).
pub use nb::block;
