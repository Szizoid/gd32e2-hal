//! SPI0 exchanging a fixed message over a loopback, driven by interrupts.
//!
//! **Wiring: a direct jumper from `PB5` (MOSI) to `PB4` (MISO).** `PB3` is SCK
//! and goes nowhere — the master clocks itself.
//!
//! A master paces the bus, so the exchange is driven by `Rbne`: one byte is in
//! flight at a time, and the byte that comes back is what releases the next.
//! `Tbe` is armed only to start the run — it is true whenever nothing is queued,
//! so arming it fires at once, and the handler sends the first byte and drops
//! it. Pumping on `Tbe` instead would queue a second byte while the first answer
//! is still on the wire, and the receive buffer, one word deep, would overrun.
//!
//! Covers: `Spi::listen`/`unlisten`/`is_listening`, `read_ready`/`write_ready`,
//! the `write_byte`/`read_byte` halves of an exchange, and `take_error`.

#![no_std]
#![no_main]

use core::cell::RefCell;

use cortex_m::peripheral::NVIC;
use cortex_m_rt::entry;
use critical_section::Mutex;
use defmt_rtt as _;
use embedded_hal::spi::MODE_0;
use panic_halt as _;

use gd32e2_hal::gpio::{Alternate, Pin};
use gd32e2_hal::pac::{self, interrupt};
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::{ClockConfig, PllFreq, SysClk};
use gd32e2_hal::spi::{BitOrder, Event, Spi, SpiConfig, SpiPsc};

type Bus =
    Spi<pac::Spi0, Pin<'B', 3, Alternate<0>>, Pin<'B', 4, Alternate<0>>, Pin<'B', 5, Alternate<0>>>;

type Shared = (
    Bus,
    [u8; LENGTH], // received
    usize,
    [u8; LENGTH], // sent
    usize,
);

static SHARED: Mutex<RefCell<Option<Shared>>> = Mutex::new(RefCell::new(None));

const MESSAGE: [u8; 8] = [0x00, 0x42, 0xA5, 0xFF, 0x01, 0x80, 0x5A, 0x7E];
const LENGTH: usize = MESSAGE.len();

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    let _clocks = ClockConfig::default()
        .sysclk(SysClk::Pll(PllFreq::Mhz48))
        .freeze(&mut rcu, &mut dp.fmc);

    let gpiob = dp.gpiob.split(&mut rcu);

    let sck = gpiob.pb3.into_alternate::<0>();
    let miso = gpiob.pb4.into_alternate::<0>();
    let mosi = gpiob.pb5.into_alternate::<0>();
    let config = SpiConfig::new(SpiPsc::Div16)
        .mode(MODE_0)
        .bit_order(BitOrder::MsbFirst);
    let mut spi = Spi::new(&mut rcu, dp.spi0, sck, miso, mosi, config);

    spi.listen(Event::Error);
    spi.listen(Event::Tbe);

    critical_section::with(|cs| {
        SHARED
            .borrow(cs)
            .replace(Some((spi, [0; LENGTH], 0, MESSAGE, 0)))
    });

    unsafe {
        NVIC::unmask(pac::Interrupt::SPI0);
    };

    defmt::info!(
        "SPI0 armed, exchanging {} bytes over the PB5-PB4 loopback",
        LENGTH
    );

    loop {
        cortex_m::asm::wfi();
    }
}

#[interrupt]
fn SPI0() {
    critical_section::with(|cs| {
        let mut shared = SHARED.borrow(cs).borrow_mut();
        let (spi, received, to_read, sent, to_write) = shared.as_mut().unwrap();

        if let Some(e) = spi.take_error() {
            defmt::error!("SPI0 error {}", e);
        }

        // Starts the run and hands pacing over to Rbne.
        if spi.is_listening(Event::Tbe) && spi.write_ready() {
            spi.unlisten(Event::Tbe);
            spi.listen(Event::Rbne);
            spi.write_byte(sent[*to_write]);
            *to_write += 1;
        }

        if spi.is_listening(Event::Rbne) && spi.read_ready() {
            received[*to_read] = spi.read_byte();
            *to_read += 1;
            if *to_write < LENGTH {
                spi.write_byte(sent[*to_write]);
                *to_write += 1;
            } else {
                spi.unlisten(Event::Rbne);
                if received == sent {
                    defmt::info!("loopback ok: {=[u8]:#04x}", &received[..]);
                } else {
                    defmt::error!(
                        "loopback MISMATCH: {=[u8]:#04x} != {=[u8]:#04x}",
                        &received[..],
                        &sent[..]
                    );
                }
            }
        }
    })
}
