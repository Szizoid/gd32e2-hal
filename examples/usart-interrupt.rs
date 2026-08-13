//! USART0 echoing a fixed message over a PA9-PA10 loopback, over RTT.
//!
//! **Wiring: a direct jumper from PA9 to PA10.**
//!
//! `main` seeds `write_buf` with the message and arms both `Rbne` and `Tbe`;
//! the handler drains `write_buf` onto the wire byte by byte and fills
//! `read_buf` from what comes back. Once `read_buf` is full it is compared
//! against `write_buf` — a mismatch would mean a byte was dropped or
//! reordered somewhere in the round trip.
//!
//! Covers: `Usart::listen`/`unlisten`/`is_listening`, a `TBE` handler turning
//! itself off once nothing is left to send.

#![no_std]
#![no_main]

use core::cell::RefCell;

use cortex_m::peripheral::NVIC;
use cortex_m_rt::entry;
use critical_section::Mutex;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::gpio::{Alternate, Pin};
use gd32e2_hal::pac::{self, interrupt};
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::{ClockConfig, PllFreq, SysClk};
use gd32e2_hal::usart::{Event, Usart, UsartConfig, baud};

type Uart = Usart<pac::Usart0, Pin<'A', 9, Alternate<1>>, Pin<'A', 10, Alternate<1>>>;

type Shared = (
    Uart,
    [u8; BUF_LENGH], // READ
    usize,
    [u8; BUF_LENGH], // WRITE
    usize,
);

static SHARED: Mutex<RefCell<Option<Shared>>> = Mutex::new(RefCell::new(None));

const MESSAGE: &[u8] = b"Some test message, made long enough that a slow baud rate stretches the \
      whole round trip out over several seconds instead of finishing at once.";
const BUF_LENGH: usize = MESSAGE.len();
const WRITE_BUF: &[u8; BUF_LENGH] = MESSAGE.first_chunk().unwrap();

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    let clocks = ClockConfig::default()
        .sysclk(SysClk::Pll(PllFreq::Mhz48))
        .freeze(&mut rcu, &mut dp.fmc);

    let gpioa = dp.gpioa.split(&mut rcu);

    let tx = gpioa.pa9.into_alternate::<1>();
    let rx = gpioa.pa10.into_alternate::<1>();
    let mut usart = Usart::new(
        &mut rcu,
        dp.usart0,
        tx,
        rx,
        &clocks,
        UsartConfig::default().baud(baud::B110),
    );

    usart.listen(Event::Rbne);
    usart.listen(Event::Tbe);

    critical_section::with(|cs| {
        SHARED
            .borrow(cs)
            .replace(Some((usart, [0; BUF_LENGH], 0, *WRITE_BUF, 0)))
    });

    unsafe {
        NVIC::unmask(pac::Interrupt::USART0);
    };

    defmt::info!(
        "USART0 armed, echoing {} bytes over the PA9-PA10 loopback",
        BUF_LENGH
    );

    loop {
        cortex_m::asm::wfi();
    }
}

#[interrupt]
fn USART0() {
    critical_section::with(|cs| {
        let mut shared = SHARED.borrow(cs).borrow_mut();
        let (usart, read_buf, to_read, write_buf, to_write) = shared.as_mut().unwrap();
        if usart.is_listening(Event::Rbne) && Usart::read_ready(usart) {
            if let Ok(byte) = usart.read_byte() {
                if *to_read < BUF_LENGH {
                    read_buf[*to_read] = byte;
                    *to_read += 1;
                    if *to_read == BUF_LENGH {
                        if read_buf == write_buf {
                            defmt::info!("echo ok");
                        } else {
                            defmt::error!("echo not ok: {} != {}", read_buf, write_buf);
                        }
                    }
                }
            }
        }
        if usart.is_listening(Event::Tbe) && Usart::write_ready(usart) {
            if *to_write < BUF_LENGH {
                usart.write_byte(write_buf[*to_write]);
                *to_write += 1;
            } else {
                usart.unlisten(Event::Tbe);
            }
        }
    })
}
