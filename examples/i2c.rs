//! I²C0 bus scan, then reads of every length from whatever answered.
//!
//! Wiring: PB6 = SCL, PB7 = SDA (AF1), both open-drain, and any 7-bit device on
//! the bus. Open-drain lines need pulling up from outside the driver: 4.7 kΩ to
//! +3V3 on each is what a bus at full rate wants. The internal pull-ups enabled
//! below are tens of kilohms, too weak for that, but they let a two-board bench
//! run with no resistors at all if the rate is dropped — hence the 50 kHz here.
//!
//! Nothing here writes data to the device: the only byte sent is a register
//! index, which moves its read pointer. Pointing this at an unknown device is
//! therefore safe — the write path lives in `examples/i2c-registers.rs`, which
//! needs a device whose registers are known.
//!
//! The scan probes every address the standard leaves to devices (0x08..=0x77)
//! with a zero-length write: the address phase alone says whether anyone
//! acknowledges, and no data byte reaches the device at all.
//!
//! The four exchanges after it are one per read length the driver treats
//! differently — 1, 2, 3 and more — since where the closing NAK falls changes
//! with the count.
//!
//! Covers: `I2c::new`, `I2cMode::standard`, inherent `read` / `write` /
//! `write_read`, and the `embedded-hal` `I2c::transaction` on top.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use embedded_hal::i2c::Operation;
use panic_halt as _;

use gd32e2_hal::gpio::Pull;
use gd32e2_hal::i2c::{Error, I2c, I2cMode};
use gd32e2_hal::pac;
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::{ClockConfig, PllFreq, SysClk};

/// SCL frequency. 100 kHz is the standard-mode rate; anything near it needs real
/// pull-up resistors, not the internal ones.
const SCL_KHZ: u32 = 50;
/// Address range the standard leaves to devices; the rest is reserved.
const ADDR_FIRST: u8 = 0x08;
const ADDR_LAST: u8 = 0x77;
/// Register the reads ask for. Devices differ — most have something at 0.
const REG: u8 = 0x00;

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    // I²C derives CLKC, RISETIME and I2CCLK from pclk1, so the tree has to be
    // frozen first. Standard mode needs pclk1 of 2 MHz, fast 8, fast plus 24.
    let clocks = ClockConfig::default()
        .sysclk(SysClk::Pll(PllFreq::Mhz48))
        .freeze(&mut rcu, &mut dp.fmc);

    let gpiob = dp.gpiob.split(&mut rcu);
    let mut scl = gpiob.pb6.into_alternate_open_drain::<1>();
    let mut sda = gpiob.pb7.into_alternate_open_drain::<1>();
    scl.set_pull(Pull::Up);
    sda.set_pull(Pull::Up);
    let mut i2c = I2c::new(
        &mut rcu,
        dp.i2c0,
        sda,
        scl,
        &clocks,
        I2cMode::standard(SCL_KHZ.kHz()),
    );

    defmt::info!(
        "I2C0 scan at {} kHz, pclk1 {} Hz",
        SCL_KHZ,
        clocks.pclk1().to_Hz()
    );

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

    // One byte: the NAK has to stand before ADDSEND goes down, because the byte
    // starts arriving the moment it does. The write phase moves the device's
    // pointer, and the repeated START holds the bus so nobody else can.
    let mut one = [0u8];
    match i2c.write_read(addr, &[REG], &mut one) {
        Ok(()) => defmt::info!("write_read 1 {=u8:#04x} -> {=u8:#04x}", REG, one[0]),
        Err(e) => defmt::error!("write_read 1: {}", e),
    }

    // Two bytes, and through the portable trait at that: the change of direction
    // is where the repeated START goes, and only the end carries a STOP. This is
    // the length that needs POAP, which moves the NAK one byte along.
    let mut two = [0u8; 2];
    let result = {
        let mut ops = [Operation::Write(&[REG]), Operation::Read(&mut two)];
        i2c.transaction(addr, &mut ops)
    };
    match result {
        Ok(()) => defmt::info!("transaction 2 {=u8:#04x} -> {=[u8]:#04x}", REG, two),
        Err(e) => defmt::error!("transaction 2: {}", e),
    }

    // Three is the shortest read that takes the manual's "Solution B", where the
    // last bytes are left to stretch SCL instead of racing software. No write
    // phase here, so it continues from wherever the pointer stands.
    let mut three = [0u8; 3];
    match i2c.read(addr, &mut three) {
        Ok(()) => defmt::info!("read 3 -> {=[u8]:#04x}", three),
        Err(e) => defmt::error!("read 3: {}", e),
    }

    // Four takes the same path with one plain byte ahead of it.
    let mut four = [0u8; 4];
    match i2c.write_read(addr, &[REG], &mut four) {
        Ok(()) => defmt::info!("write_read 4 {=u8:#04x} -> {=[u8]:#04x}", REG, four),
        Err(e) => defmt::error!("write_read 4: {}", e),
    }

    defmt::info!("done");
    loop {}
}
