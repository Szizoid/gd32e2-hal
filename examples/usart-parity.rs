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
use embedded_hal_nb::serial::{Read, Write};
use gd32e2::gd32e230;
use panic_halt as _;

use gd32e230_hal::gpio::GpioExt;
use gd32e230_hal::rcu::{CFGR, PllFreq, RcuExt};
use gd32e230_hal::usart::{FrameFormat, Oversampling, Usart, UsartConfig, baud};

#[entry]
fn main() -> ! {
    let mut dp = gd32e230::Peripherals::take().unwrap();
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
    let mut usart1 = Usart::new(&mut rcu, dp.usart1, tx, rx, clocks, config);

    // Send a few bytes and read each back through the loopback wire.
    for b in *b"HAL" {
        let _ = nb::block!(usart1.write(b));
        let _ = nb::block!(usart1.flush());
        let _echo = nb::block!(usart1.read());
    }

    // Done exercising it — hand the peripheral and pins back.
    let (_usart1, _tx, _rx) = usart1.release();
    loop {}
}
