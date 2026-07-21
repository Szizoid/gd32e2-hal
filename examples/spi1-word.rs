//! SPI1 in 16-bit master mode, verified by loopback.
//!
//! Wire `MOSI` (PB15) straight to `MISO` (PB14). SPI1 exists only on the x8
//! variant, and its AF0 pins are PB13/14/15. Results go over USART0
//! (PA9/PA10) at 115200 8N1.
//!
//! Covers: `Spi::new_word`, `transfer_word`, the `SpiBus<u16>` impl, and
//! `Spi::release`.

#![no_std]
#![no_main]

use core::fmt::Write as _;

use cortex_m_rt::entry;
use embedded_hal::spi::SpiBus;
use panic_halt as _;

use gd32e2_hal::gpio::GpioExt;
use gd32e2_hal::pac;
use gd32e2_hal::rcu::{CFGR, PllFreq, RcuExt};
use gd32e2_hal::spi::{Spi, SpiConfig, SpiPsc};
use gd32e2_hal::usart::{Usart, UsartConfig};

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
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    let clocks = CFGR::default()
        .sysclk(PllFreq::Mhz48)
        .freeze(&mut rcu, &mut dp.fmc);

    let gpioa = dp.gpioa.split(&mut rcu);
    let gpiob = dp.gpiob.split(&mut rcu);

    let tx = gpioa.pa9.into_alternate::<1>();
    let rx = gpioa.pa10.into_alternate::<1>();
    let usart0 = Usart::new(&mut rcu, dp.usart0, tx, rx, clocks, UsartConfig::default());
    let mut log = Serial(usart0);

    // SPI1: SCK PB13, MISO PB14, MOSI PB15 — all AF0.
    let sck = gpiob.pb13.into_alternate::<0>();
    let miso = gpiob.pb14.into_alternate::<0>();
    let mosi = gpiob.pb15.into_alternate::<0>();
    let config = SpiConfig::new(SpiPsc::Div16);
    let mut spi = Spi::new_word(&mut rcu, dp.spi1, sck, miso, mosi, config);

    let _ = writeln!(log, "SPI1 16-bit loopback test (wire PB15 -> PB14)");

    for word in [0x0000u16, 0x1234, 0xBEEF, 0xFFFF] {
        match spi.transfer_word(word) {
            Ok(got) if got == word => {
                let _ = writeln!(log, "transfer_word 0x{:04X} -> 0x{:04X} ok", word, got);
            }
            Ok(got) => {
                let _ = writeln!(
                    log,
                    "transfer_word 0x{:04X} -> 0x{:04X} MISMATCH",
                    word, got
                );
            }
            Err(e) => {
                let _ = writeln!(log, "transfer_word 0x{:04X} error {:?}", word, e);
            }
        }
    }

    let mut buf = [0xAAAAu16, 0x5555, 0x0FF0];
    match spi.transfer_in_place(&mut buf) {
        Ok(()) => {
            let _ = writeln!(log, "SpiBus<u16> transfer_in_place -> {:04X?}", buf);
        }
        Err(e) => {
            let _ = writeln!(log, "SpiBus error {:?}", e);
        }
    }

    // Hand the peripheral and pins back.
    let (_spi1, _sck, _miso, _mosi) = spi.release();
    let _ = writeln!(log, "released, done");
    loop {}
}
