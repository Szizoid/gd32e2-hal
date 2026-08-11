//! USART1 with even parity, driven through the `embedded-hal-nb` traits.
//!
//! Loopback self-test: wire TX (PA2) to RX (PA3). Each byte is sent with an
//! 8-bit even-parity frame at 9600 baud, ×8 oversampling, then read back. Uses
//! the non-blocking `Read`/`Write`/`flush` traits (via `nb::block!`) rather than
//! the inherent byte methods.
//!
//! Covers: a second instance (`Usart1`), `FrameFormat::E8`, `Oversampling::X8`,
//! a `baud` constant, the `embedded-hal-nb` `Read`/`Write`/`flush` impls, and
//! `Usart::release`.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::pac;
// Per-peripheral imports rather than the whole prelude: the serial traits come
// from `usart::nb`, and the glob would bring in `usart::io`'s same-named ones.
use gd32e2_hal::prelude::gpio::*;
use gd32e2_hal::prelude::rcu::*;
use gd32e2_hal::prelude::usart::nb::*;
use gd32e2_hal::rcu::{CFGR, PllFreq};
use gd32e2_hal::usart::{FrameFormat, Oversampling, Usart, UsartConfig, baud};

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    let clocks = CFGR::default()
        .sysclk(PllFreq::Mhz48)
        .freeze(&mut rcu, &mut dp.fmc);

    let gpioa = dp.gpioa.split(&mut rcu);
    // USART1 TX on PA2, RX on PA3 — both AF1.
    let tx = gpioa.pa2.into_alternate::<1>();
    let rx = gpioa.pa3.into_alternate::<1>();

    let config = UsartConfig::default()
        .baud(baud::B9600)
        .oversampling(Oversampling::X8)
        .frame_format(FrameFormat::E8);
    let mut usart1 = Usart::new(&mut rcu, dp.usart1, tx, rx, &clocks, config);

    defmt::info!("USART1 E8 loopback at 9600 baud (wire PA2 -> PA3)");

    // Send a few bytes and read each back through the loopback wire.
    for b in *b"HAL" {
        let _ = block!(usart1.write(b));
        // Spelled out because `Usart` also has an inherent, blocking `flush`,
        // which wins plain method-call syntax; this is the `nb` one.
        let _ = block!(embedded_hal_nb::serial::Write::flush(&mut usart1));
        match block!(usart1.read()) {
            Ok(echo) if echo == b => defmt::info!("sent {=u8:a}, got it back", b),
            Ok(echo) => defmt::warn!("sent {=u8:#04x}, got {=u8:#04x}", b, echo),
            // A parity error is the interesting case here.
            Err(e) => defmt::error!("read failed on {=u8:#04x}: {}", b, e),
        }
    }

    // Done exercising it — hand the peripheral and pins back.
    let (_usart1, _tx, _rx) = usart1.release();
    defmt::info!("released, done");
    loop {}
}
