//! SPI0 in 8-bit master mode, verified by loopback.
//!
//! Wire `MOSI` (PA7) straight to `MISO` (PA6): every byte the master shifts out
//! comes back on the same clock, so a correct bus echoes whatever was sent.
//! Results go to the RTT log, so the only wires this needs are the loopback one
//! and the probe.
//!
//! Covers: `Spi::new`, `SpiConfig` (`new`/`mode`/`bit_order`), `SpiPsc`,
//! `transfer_byte`, and the `embedded-hal` `SpiBus<u8>` impl.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use embedded_hal::spi::MODE_0;
use panic_halt as _;

use gd32e2_hal::pac;
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::{ClockConfig, PllFreq, SysClk};
use gd32e2_hal::spi::{BitOrder, Spi, SpiConfig, SpiPsc};

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    // SPI takes its clock from the bus, so nothing here reads the frequencies —
    // but the tree still has to be frozen before `split`.
    let _clocks = ClockConfig::default()
        .sysclk(SysClk::Pll(PllFreq::Mhz48))
        .freeze(&mut rcu, &mut dp.fmc);

    let gpioa = dp.gpioa.split(&mut rcu);

    // SPI0: SCK on PA5, MISO on PA6, MOSI on PA7 — all AF0.
    let sck = gpioa.pa5.into_alternate::<0>();
    let miso = gpioa.pa6.into_alternate::<0>();
    let mosi = gpioa.pa7.into_alternate::<0>();
    let config = SpiConfig::new(SpiPsc::Div16)
        .mode(MODE_0)
        .bit_order(BitOrder::MsbFirst);
    let mut spi = Spi::new(&mut rcu, dp.spi0, sck, miso, mosi, config);

    defmt::info!("SPI0 loopback test (wire PA7 -> PA6)");

    // Inherent, one byte at a time.
    for byte in [0x00u8, 0x42, 0xA5, 0xFF] {
        match spi.transfer_byte(byte) {
            Ok(got) if got == byte => {
                defmt::info!("transfer_byte {=u8:#04x} -> {=u8:#04x} ok", byte, got);
            }
            Ok(got) => {
                defmt::warn!("transfer_byte {=u8:#04x} -> {=u8:#04x} MISMATCH", byte, got);
            }
            Err(e) => {
                defmt::error!("transfer_byte {=u8:#04x} error {}", byte, e);
            }
        }
    }

    // embedded-hal SpiBus: a whole buffer transferred in place.
    let mut buf = [0x11u8, 0x22, 0x33, 0x44];
    match spi.transfer_in_place(&mut buf) {
        Ok(()) => {
            defmt::info!("SpiBus transfer_in_place -> {=[u8]:#04x}", buf);
        }
        Err(e) => {
            defmt::error!("SpiBus error {}", e);
        }
    }

    defmt::info!("done");
    loop {}
}
