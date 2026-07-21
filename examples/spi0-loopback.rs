//! SPI0 in 8-bit master mode, verified by loopback.
//!
//! Wire `MOSI` (PA7) straight to `MISO` (PA6): every byte the master shifts out
//! comes back on the same clock, so a correct bus echoes whatever was sent.
//! Results are reported over USART0 (PA9/PA10) at 115200 8N1 — the only output
//! path on this board, since there is no debug probe.
//!
//! Covers: `Spi::new`, `SpiConfig` (`new`/`mode`/`bit_order`), `SpiPsc`,
//! `transfer_byte`, and the `embedded-hal` `SpiBus<u8>` impl.

#![no_std]
#![no_main]

use core::fmt::Write as _;

use cortex_m_rt::entry;
use embedded_hal::spi::{MODE_0, SpiBus};
use gd32e2::gd32e230;
use panic_halt as _;

use gd32e230_hal::gpio::GpioExt;
use gd32e230_hal::rcu::{CFGR, PllFreq, RcuExt};
use gd32e230_hal::spi::{BitOrder, Spi, SpiConfig, SpiPsc};
use gd32e230_hal::usart::{Usart, UsartConfig};

/// Wraps anything that can send bytes so `write!`/`writeln!` work over it.
struct Serial<W>(W);

impl<W: embedded_hal_nb::serial::Write<u8>> core::fmt::Write for Serial<W> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for &b in s.as_bytes() {
            let _ = nb::block!(self.0.write(b));
        }
        Ok(())
    }
}

#[entry]
fn main() -> ! {
    let mut dp = gd32e230::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    let clocks = CFGR::default()
        .sysclk(PllFreq::Mhz48)
        .freeze(&mut rcu, &mut dp.fmc);

    let gpioa = dp.gpioa.split(&mut rcu);

    // USART0 for logging the results.
    let tx = gpioa.pa9.into_alternate::<1>();
    let rx = gpioa.pa10.into_alternate::<1>();
    let usart0 = Usart::new(&mut rcu, dp.usart0, tx, rx, clocks, UsartConfig::default());
    let mut log = Serial(usart0);

    // SPI0: SCK on PA5, MISO on PA6, MOSI on PA7 — all AF0.
    let sck = gpioa.pa5.into_alternate::<0>();
    let miso = gpioa.pa6.into_alternate::<0>();
    let mosi = gpioa.pa7.into_alternate::<0>();
    let config = SpiConfig::new(SpiPsc::Div16)
        .mode(MODE_0)
        .bit_order(BitOrder::MsbFirst);
    let mut spi = Spi::new(&mut rcu, dp.spi0, sck, miso, mosi, config);

    let _ = writeln!(log, "SPI0 loopback test (wire PA7 -> PA6)");

    // Inherent, one byte at a time.
    for byte in [0x00u8, 0x42, 0xA5, 0xFF] {
        match spi.transfer_byte(byte) {
            Ok(got) if got == byte => {
                let _ = writeln!(log, "transfer_byte 0x{:02X} -> 0x{:02X} ok", byte, got);
            }
            Ok(got) => {
                let _ = writeln!(
                    log,
                    "transfer_byte 0x{:02X} -> 0x{:02X} MISMATCH",
                    byte, got
                );
            }
            Err(e) => {
                let _ = writeln!(log, "transfer_byte 0x{:02X} error {:?}", byte, e);
            }
        }
    }

    // embedded-hal SpiBus: a whole buffer transferred in place.
    let mut buf = [0x11u8, 0x22, 0x33, 0x44];
    match spi.transfer_in_place(&mut buf) {
        Ok(()) => {
            let _ = writeln!(log, "SpiBus transfer_in_place -> {:02X?}", buf);
        }
        Err(e) => {
            let _ = writeln!(log, "SpiBus error {:?}", e);
        }
    }

    let _ = writeln!(log, "done");
    loop {}
}
