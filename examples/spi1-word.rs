//! SPI1 in 16-bit master mode, verified by loopback.
//!
//! Needs a 48-pin x8 part (`gd32e230c8xx`): SPI1 exists only on x8, and its AF0 pins
//! PB13/14/15 are bonded only on the 48-pin package. Smaller packages reach SPI1
//! through PB1 plus PA13/PA14 — the SWD pins, so at the cost of the debug port and
//! the RTT log with it.
//!
//! Wire `MOSI` (PB15) straight to `MISO` (PB14). Results go to the RTT log.
//!
//! Covers: `Spi::new_word`, `transfer_word`, the `SpiBus<u16>` impl, and
//! `Spi::release`.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::pac;
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::{ClockConfig, PllFreq, SysClk};
use gd32e2_hal::spi::{Spi, SpiConfig, SpiPsc};

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut fmc = dp.fmc.constrain();
    // SPI takes its clock from the bus, so nothing here reads the frequencies —
    // but the tree still has to be frozen before `split`.
    let config = ClockConfig::default().sysclk(SysClk::Pll(PllFreq::Mhz48));
    let mut rcu = dp.rcu.constrain().freeze(&mut fmc, config);

    let gpiob = dp.gpiob.split(&mut rcu);

    // SPI1: SCK PB13, MISO PB14, MOSI PB15 — all AF0.
    let sck = gpiob.pb13.into_alternate::<0>();
    let miso = gpiob.pb14.into_alternate::<0>();
    let mosi = gpiob.pb15.into_alternate::<0>();
    let config = SpiConfig::new(SpiPsc::Div16);
    let mut spi = Spi::new_word(&mut rcu, dp.spi1, sck, miso, mosi, config);

    defmt::info!("SPI1 16-bit loopback test (wire PB15 -> PB14)");

    for word in [0x0000u16, 0x1234, 0xBEEF, 0xFFFF] {
        match spi.transfer_word(word) {
            Ok(got) if got == word => {
                defmt::info!("transfer_word {=u16:#06x} -> {=u16:#06x} ok", word, got);
            }
            Ok(got) => {
                defmt::warn!(
                    "transfer_word {=u16:#06x} -> {=u16:#06x} MISMATCH",
                    word,
                    got
                );
            }
            Err(e) => {
                defmt::error!("transfer_word {=u16:#06x} error {}", word, e);
            }
        }
    }

    let mut buf = [0xAAAAu16, 0x5555, 0x0FF0];
    match spi.transfer_in_place(&mut buf) {
        Ok(()) => {
            // No `[u16]` type specifier exists — only `[u8]` is built in, so the
            // slice goes through the generic Format impl instead.
            defmt::info!("SpiBus<u16> transfer_in_place -> {}", buf);
        }
        Err(e) => {
            defmt::error!("SpiBus error {}", e);
        }
    }

    // Hand the peripheral and pins back.
    let (_spi1, _sck, _miso, _mosi) = spi.release();
    defmt::info!("released, done");
    loop {}
}
