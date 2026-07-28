//! Sends a fixed message over USART0 without the core touching a single byte.
//!
//! Same wiring as `usart-echo`: PA9/PA10 at 115200 8N1. Channel 1 is the one the
//! hardware ties to USART0_TX, so any other channel would fail to compile.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::dma::{DmaExt, Prio};
use gd32e2_hal::gpio::GpioExt;
use gd32e2_hal::pac;
use gd32e2_hal::rcu::{CFGR, PllFreq, RcuExt};
use gd32e2_hal::usart::{Usart, UsartConfig};

/// One second at 48 MHz, so the messages are distinguishable in a terminal.
const DELAY_CYCLES: u32 = 48_000_000;

const MSG: &[u8] = b"dma works\r\n";

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    let clocks = CFGR::default()
        .sysclk(PllFreq::Mhz48)
        .freeze(&mut rcu, &mut dp.fmc);
    let parts = dp.gpioa.split(&mut rcu);
    let pa6 = parts.pa6.into_output();
    pa6.set_high();

    let tx_pin = parts.pa9.into_alternate::<1>();
    let rx_pin = parts.pa10.into_alternate::<1>();
    let mut usart0 = Usart::new(
        &mut rcu,
        dp.usart0,
        tx_pin,
        rx_pin,
        clocks,
        UsartConfig::default(),
    );

    let channels = dp.dma.split(&mut rcu);
    let mut ch1 = channels.ch1;

    loop {
        // The channel, the USART and the buffer all move into the transfer and
        // come back only from `wait`, so nothing here can touch a byte the DMA
        // is still moving.
        let transfer = ch1.write_to(usart0, MSG, Prio::Low);

        // Logging between the start and `wait` is the whole point: the core is
        // free while the channel drains the buffer on its own. `remaining` is a
        // live snapshot, so it is normally already below the message length.
        defmt::info!(
            "transfer started, {} of {} bytes left",
            transfer.remaining(),
            MSG.len()
        );

        let (channel, usart, _buf) = transfer.wait();
        ch1 = channel;
        usart0 = usart;
        defmt::info!("transfer done");

        cortex_m::asm::delay(DELAY_CYCLES);
    }
}
