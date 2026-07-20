#![no_std]

// Exactly one chip-variant feature must be selected: the alternate-function map
// differs between variants, so building with none (or several) would produce a
// silently wrong pin map instead of a compile error.
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

pub mod adc;
pub mod gpio;
pub mod rcu;
pub mod spi;
pub mod time;
pub mod usart;
