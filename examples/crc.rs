//! Hardware CRC-8, checked against a known reference value.
//!
//! Needs no external wiring — the CRC unit only ever touches data already in
//! memory. Feeds the ASCII string `"123456789"` through CRC-8/SMBUS
//! (polynomial `0x07`, seed `0x00`, no reversal), whose published check value
//! is `0xF4`.
//!
//! Covers: `Crc::new_8bit`, `write_8bit`, `read_8bit`, `reset_with`.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::crc::{Crc, CrcConfig, ReverseInput, ReverseOutput};
use gd32e2_hal::pac;
use gd32e2_hal::prelude::*;

/// The standard CRC-8/SMBUS check value for `CHECK_INPUT`.
const EXPECTED: u8 = 0xF4;
/// Catalogue test vector shared by every CRC variant's "check" value.
const CHECK_INPUT: &[u8] = b"123456789";

#[entry]
fn main() -> ! {
    let dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();

    let config = CrcConfig::new(ReverseInput::Disabled, ReverseOutput::Disabled);
    let crc = Crc::new_8bit(&mut rcu, dp.crc, 0x07, config);

    // new_8bit doesn't touch IDATA/RST, so start from a known seed explicitly.
    crc.reset_with(0);
    for &byte in CHECK_INPUT {
        crc.write_8bit(byte);
    }
    let result = crc.read_8bit();

    if result == EXPECTED {
        defmt::info!("CRC-8/SMBUS(\"123456789\") = {:#04x}, ok", result);
    } else {
        defmt::warn!(
            "CRC-8/SMBUS(\"123456789\") = {:#04x}, expected {:#04x}",
            result,
            EXPECTED
        );
    }

    defmt::info!("done");
    loop {}
}
