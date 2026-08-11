//! I²C0 bus scan, then a register read from whatever answered.
//!
//! Wiring: PB6 = SCL, PB7 = SDA (AF1), both open-drain and each pulled up to
//! +3V3 by an external resistor — 4.7 kΩ is the usual choice; the chip has
//! nothing to pull the lines up with. Any 7-bit device will do.
//!
//! **Not verified on hardware** — this board has no I²C wiring at all. If you
//! run it, please report what happened:
//! <https://github.com/Szizoid/gd32e2-hal/issues>.
//!
//! The scan probes every address the standard leaves to devices (0x08..=0x77)
//! with a zero-length write: the address phase alone says whether anyone
//! acknowledges, and no data byte ever reaches the device.
//!
//! Covers: `I2c::new`, `I2cMode::standard`, inherent `write` / `read` /
//! `write_read`, and the `embedded-hal` `I2c::transaction` on top.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use embedded_hal::i2c::Operation;
use panic_halt as _;

use gd32e2_hal::i2c::{Error, I2c, I2cMode};
use gd32e2_hal::pac;
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::{CFGR, PllFreq};

/// Address range the standard leaves to devices; the rest is reserved.
const ADDR_FIRST: u8 = 0x08;
const ADDR_LAST: u8 = 0x77;
/// Register the read demos ask for. Devices differ — most have something at 0.
const REG: u8 = 0x00;

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    // I²C derives CLKC, RISETIME and I2CCLK from pclk1, so the tree has to be
    // frozen first. Standard mode needs pclk1 of 2 MHz, fast 8, fast plus 24.
    let clocks = CFGR::default()
        .sysclk(PllFreq::Mhz48)
        .freeze(&mut rcu, &mut dp.fmc);

    let gpiob = dp.gpiob.split(&mut rcu);
    let scl = gpiob.pb6.into_alternate_open_drain::<1>();
    let sda = gpiob.pb7.into_alternate_open_drain::<1>();
    let mut i2c = I2c::new(
        &mut rcu,
        dp.i2c0,
        sda,
        scl,
        &clocks,
        I2cMode::standard(100.kHz()),
    );

    defmt::info!("I2C0 scan at 100 kHz, pclk1 {} Hz", clocks.pclk1().to_Hz());

    let mut found = None;
    for addr in ADDR_FIRST..=ADDR_LAST {
        match i2c.write(addr, &[]) {
            Ok(()) => {
                defmt::info!("{=u8:#04x}: device", addr);
                found = found.or(Some(addr));
            }
            // An empty address is the ordinary case, not worth a line each.
            Err(Error::NoAcknowledge(_)) => {}
            Err(e) => defmt::error!("{=u8:#04x}: {}", addr, e),
        }
    }

    let Some(addr) = found else {
        defmt::warn!("nobody answered — check the pull-ups and the wiring");
        loop {}
    };
    defmt::info!("talking to {=u8:#04x}", addr);

    // The write phase moves the device's read pointer, the repeated START holds
    // the bus so no other master can move it before the read.
    let mut byte = [0u8];
    match i2c.write_read(addr, &[REG], &mut byte) {
        Ok(()) => defmt::info!("write_read {=u8:#04x} -> {=u8:#04x}", REG, byte[0]),
        Err(e) => defmt::error!("write_read: {}", e),
    }

    // The same exchange through the portable trait: the change of direction is
    // where the repeated START goes, and only the end carries a STOP.
    let mut pair = [0u8; 2];
    let result = {
        let mut ops = [Operation::Write(&[REG]), Operation::Read(&mut pair)];
        i2c.transaction(addr, &mut ops)
    };
    match result {
        Ok(()) => defmt::info!("transaction {=u8:#04x} -> {=[u8]:#04x}", REG, pair),
        Err(e) => defmt::error!("transaction: {}", e),
    }

    // A read on its own, from wherever the pointer now stands. Two bytes is the
    // length whose NAK has to be moved one byte ahead (`POAP`).
    let mut two = [0u8; 2];
    match i2c.read(addr, &mut two) {
        Ok(()) => defmt::info!("read -> {=[u8]:#04x}", two),
        Err(e) => defmt::error!("read: {}", e),
    }

    defmt::info!("done");
    loop {}
}
