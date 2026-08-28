//! `Event::ParityError` on USART1, raised by a sender that disagrees with it.
//!
//! **Wiring: a direct jumper from `PA9` (USART0 TX) to `PA3` (USART1 RX).**
//! `PA10` and `PA2` are claimed as the other half of each peripheral and stay
//! unconnected.
//!
//! A receive error cannot be produced on a single-USART loopback: one block
//! transmits and receives with one configuration, so the parity it puts on the
//! wire is the parity it expects back. Two USARTs are needed, and only the
//! receiver's parity changes between the two phases here — the sender stays
//! `E8` throughout, the wire never changes.
//!
//! Phase one has the receiver on `O8`: every frame carries the wrong parity, so
//! `PERR` fires on all of them and no data is delivered. Phase two rebuilds the
//! receiver on `E8`, and the same bytes arrive clean.
//!
//! Each phase pauses before sending, the receiver having just been enabled; see
//! the comment on it.
//!
//! `Event::Error` (framing, noise, overrun) is not covered: its enable is ANDed
//! with the DMA request line in hardware, so it needs a DMA receive to reach the
//! NVIC at all.
//!
//! Covers: `Event::ParityError`, `take_error` as the acknowledge for it,
//! `FrameFormat`, and rebuilding a USART through `release` / `new`.

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
use gd32e2_hal::time::Bps;
use gd32e2_hal::usart::{Event, FrameFormat, Usart, UsartConfig, baud};

type Sender = Usart<pac::Usart0, Pin<'A', 9, Alternate<1>>, Pin<'A', 10, Alternate<1>>>;
type Receiver = Usart<pac::Usart1, Pin<'A', 2, Alternate<1>>, Pin<'A', 3, Alternate<1>>>;

type Shared = (
    Receiver,
    [u8; LENGTH], // received
    usize,        // bytes delivered
    usize,        // parity errors
);

static RECEIVER: Mutex<RefCell<Option<Shared>>> = Mutex::new(RefCell::new(None));

const BAUD: Bps = baud::B9600;
/// Two frame times at 9600 baud on a 48 MHz core.
const IDLE_CYCLES: u32 = 100_000;
const MESSAGE: [u8; 8] = *b"ParityOK";
const LENGTH: usize = MESSAGE.len();

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    let clocks = ClockConfig::default()
        .sysclk(SysClk::Pll(PllFreq::Mhz48))
        .freeze(&mut rcu, &mut dp.fmc);

    let gpioa = dp.gpioa.split(&mut rcu);

    // The sender goes up first: its TX idles high, so the receiver never sees a
    // floating line as a start bit.
    let mut sender = Usart::new(
        &mut rcu,
        dp.usart0,
        gpioa.pa9.into_alternate::<1>(),
        gpioa.pa10.into_alternate::<1>(),
        &clocks,
        UsartConfig::default()
            .baud(BAUD)
            .frame_format(FrameFormat::E8),
    );
    let receiver = Usart::new(
        &mut rcu,
        dp.usart1,
        gpioa.pa2.into_alternate::<1>(),
        gpioa.pa3.into_alternate::<1>(),
        &clocks,
        UsartConfig::default()
            .baud(BAUD)
            .frame_format(FrameFormat::O8),
    );

    unsafe {
        NVIC::unmask(pac::Interrupt::USART1);
    };

    let (receiver, got, errors) = run_phase(receiver, &mut sender);
    if errors == LENGTH && got == 0 {
        defmt::info!("odd against even: {} parity errors, as expected", errors);
    } else {
        defmt::error!(
            "odd against even: expected {} errors and no data, got {} errors and {} bytes",
            LENGTH,
            errors,
            got
        );
    }

    // Only the receiver changes; the sender keeps putting even parity on the
    // same wire.
    let (usart1, tx, rx) = receiver.release();
    let receiver = Usart::new(
        &mut rcu,
        usart1,
        tx,
        rx,
        &clocks,
        UsartConfig::default()
            .baud(BAUD)
            .frame_format(FrameFormat::E8),
    );

    let (_receiver, got, errors) = run_phase(receiver, &mut sender);
    if errors == 0 && got == LENGTH {
        defmt::info!("even against even: {} bytes, no errors", got);
    } else {
        defmt::error!(
            "even against even: expected {} clean bytes, got {} bytes and {} errors",
            LENGTH,
            got,
            errors
        );
    }

    loop {
        cortex_m::asm::wfi();
    }
}

/// Sends the message once and returns the receiver with what it made of it.
fn run_phase(mut receiver: Receiver, sender: &mut Sender) -> (Receiver, usize, usize) {
    receiver.listen(Event::Rbne);
    receiver.listen(Event::ParityError);
    critical_section::with(|cs| {
        RECEIVER
            .borrow(cs)
            .replace(Some((receiver, [0; LENGTH], 0, 0)))
    });

    // The receiver was enabled moments ago and needs the line idle before the
    // first start bit. The first phase gets that for free — a sender raising
    // `TEN` sends an idle frame — but the second one reuses a sender that is
    // already up, so the idle has to come from here.
    cortex_m::asm::delay(IDLE_CYCLES);

    sender.write_bytes(&MESSAGE);
    // Full path: with a `&mut` in hand the trait `flush` would win, and this one
    // is the inherent blocking wait for `TC`.
    Usart::flush(sender);

    // Every frame ends as either a byte or an error, so the two counts together
    // say when the phase is over. The wait sits inside the critical section on
    // purpose: masked interrupts still wake `wfi`, and checking outside it would
    // race the last frame into an endless sleep.
    loop {
        let done = critical_section::with(|cs| {
            let shared = RECEIVER.borrow(cs).borrow();
            let (_, _, got, errors) = shared.as_ref().unwrap();
            if got + errors == LENGTH {
                true
            } else {
                cortex_m::asm::wfi();
                false
            }
        });
        if done {
            break;
        }
    }

    let (mut receiver, received, got, errors) =
        critical_section::with(|cs| RECEIVER.borrow(cs).take()).unwrap();
    receiver.unlisten(Event::Rbne);
    receiver.unlisten(Event::ParityError);
    defmt::debug!("delivered {=[u8]:#04x}", &received[..got]);
    (receiver, got, errors)
}

#[interrupt]
fn USART1() {
    critical_section::with(|cs| {
        let mut shared = RECEIVER.borrow(cs).borrow_mut();
        let Some((usart, received, got, errors)) = shared.as_mut() else {
            return;
        };

        // Drains RDATA as well, so the damaged frame is gone and RBNE with it.
        if let Some(err) = usart.take_error() {
            defmt::debug!("receive error {}", err);
            *errors += 1;
            return;
        }
        if Usart::read_ready(usart)
            && let Ok(byte) = usart.read_byte()
        {
            if *got < LENGTH {
                received[*got] = byte;
                *got += 1;
            }
        }
    })
}
