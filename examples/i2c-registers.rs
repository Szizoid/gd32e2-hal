//! I²C0 register write, verified by reading it back.
//!
//! **This one writes to the device.** Two bytes go into register `0x04` of
//! whatever sits at [`ADDRESS`], so it needs a device whose register map is
//! known — on a random sensor that address could be a configuration register.
//! For a look at an unfamiliar bus use `examples/i2c.rs`, which only ever sends
//! a register index.
//!
//! Wiring is the same: PB6 = SCL, PB7 = SDA (AF1), pulled up. The defaults match
//! the RP2040 I²C target the driver was tested against — address `0x42`, eight
//! registers, the first byte of a write phase selecting one.
//!
//! Covers: multi-byte `write`, which is the path that waits on `TBE` between
//! bytes, and `write_read` as the read-back.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::gpio::Pull;
use gd32e2_hal::i2c::{I2c, I2cMode};
use gd32e2_hal::pac;
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::{CFGR, PllFreq};

/// SCL frequency. 100 kHz is the standard-mode rate; anything near it needs real
/// pull-up resistors, not the internal ones.
const SCL_KHZ: u32 = 50;
/// The device written to. No scan here — writing blind is exactly what this
/// example must not do.
const ADDRESS: u8 = 0x42;
/// Register written, and the two bytes put there.
const REG: u8 = 0x04;
const VALUES: [u8; 2] = [0xDE, 0xAD];

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    let clocks = CFGR::default()
        .sysclk(PllFreq::Mhz48)
        .freeze(&mut rcu, &mut dp.fmc);

    let gpiob = dp.gpiob.split(&mut rcu);
    let scl = gpiob.pb6.into_alternate_open_drain::<1>();
    let sda = gpiob.pb7.into_alternate_open_drain::<1>();
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
        "I2C0 at {} kHz, writing {=[u8]:#04x} to {=u8:#04x} reg {=u8:#04x}",
        SCL_KHZ,
        VALUES,
        ADDRESS,
        REG
    );

    // Index plus data in one phase: three bytes out, no STOP in between.
    let out = [REG, VALUES[0], VALUES[1]];
    if let Err(e) = i2c.write(ADDRESS, &out) {
        defmt::error!("write: {}", e);
        loop {}
    }

    // Read back through a repeated START, which is what keeps the pointer this
    // write left behind from being moved by anyone else.
    let mut back = [0u8; VALUES.len()];
    match i2c.write_read(ADDRESS, &[REG], &mut back) {
        Ok(()) if back == VALUES => defmt::info!("read back {=[u8]:#04x} ok", back),
        Ok(()) => defmt::warn!(
            "read back {=[u8]:#04x}, expected {=[u8]:#04x}",
            back,
            VALUES
        ),
        Err(e) => defmt::error!("write_read: {}", e),
    }

    defmt::info!("done");
    loop {}
}
