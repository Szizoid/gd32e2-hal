//! USART0 in pure 9-bit mode, echoing whatever it receives.
//!
//! 9-bit frames carry no parity — all nine bits are data (0x000..=0x1FF). A
//! plain terminal can't send them, so the easy check is loopback: wire TX (PA9)
//! to RX (PA10) and drive it from another example, or talk to a device that
//! speaks 9-bit. Runs at 115200 8N1-style timing, ×16 oversampling.
//!
//! Covers: `Usart::new_word`, `UsartConfig9` (`baud`/`oversampling`),
//! `write_word`, and `read_word`.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::pac;
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::{ClockConfig, PllFreq, SysClk};
use gd32e2_hal::usart::{Oversampling, Usart, UsartConfig9};

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

    // The bit rate is a `time::Bps`; `usart::baud` has the standard ones named,
    // and `usart-parity.rs` uses those instead.
    let config = UsartConfig9::default()
        .baud(115_200.bps())
        .oversampling(Oversampling::X16);
    let usart0 = Usart::new_word(&mut rcu, dp.usart0, tx, rx, &clocks, config);

    defmt::info!("9-bit echo ready, waiting for a word on PA10");

    loop {
        if let Ok(word) = usart0.read_word() {
            usart0.write_word(word);
            // The ninth bit is exactly what a plain 8-bit terminal cannot show,
            // so log it apart from the low byte.
            defmt::info!("echoed {=u16:#x} (bit8 = {})", word, word & 0x100 != 0);
        }
    }
}
