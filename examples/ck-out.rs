//! Clock tree: bus prescalers, the USART0 clock source, and CK_OUT.
//!
//! With no debug probe, routing a clock node onto a pin is the only way to
//! measure a real frequency. This drives `sysclk / 4` onto PA8 (AF0) for a
//! scope, sets the bus prescalers, and reports the resulting `Clocks` over
//! USART0 — on PB6/PB7 (AF0) here, since PA8/PA9 are taken by CK_OUT.
//!
//! Covers: `CFGR` `hclk`/`pclk1`/`pclk2`/`usart0_sel`, `AhbPsc`/`ApbPsc`,
//! `Usart0Sel`, the `Clocks` getters, and `Rcu::ck_out`.

#![no_std]
#![no_main]

use core::fmt::Write as _;

use cortex_m_rt::entry;
use gd32e2::gd32e230;
use panic_halt as _;

use gd32e230_hal::gpio::GpioExt;
use gd32e230_hal::rcu::{AhbPsc, ApbPsc, CFGR, CkOutDiv, CkOutSrc, PllFreq, RcuExt, Usart0Sel};
use gd32e230_hal::usart::{Usart, UsartConfig};

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
        .hclk(AhbPsc::Div1) // hclk = 48 MHz
        .pclk1(ApbPsc::Div2) // pclk1 = 24 MHz
        .pclk2(ApbPsc::Div1) // pclk2 = 48 MHz
        .usart0_sel(Usart0Sel::Sysclk) // USART0 off sysclk, not the APB2 clock
        .freeze(&mut rcu, &mut dp.fmc);

    let gpioa = dp.gpioa.split(&mut rcu);
    let gpiob = dp.gpiob.split(&mut rcu);

    // CK_OUT goes on PA8 (AF0); measure sysclk/4 = 12 MHz there.
    let _ck_out = gpioa.pa8.into_alternate::<0>();
    rcu.ck_out(CkOutSrc::Sysclk, CkOutDiv::Div4);

    // USART0 on PB6/PB7 (AF0) — PA9 is unavailable next to CK_OUT.
    let tx = gpiob.pb6.into_alternate::<0>();
    let rx = gpiob.pb7.into_alternate::<0>();
    let usart0 = Usart::new(&mut rcu, dp.usart0, tx, rx, clocks, UsartConfig::default());
    let mut log = Serial(usart0);

    let _ = writeln!(log, "sysclk = {} Hz", clocks.sysclk().0);
    let _ = writeln!(log, "hclk   = {} Hz", clocks.hclk().0);
    let _ = writeln!(log, "pclk1  = {} Hz", clocks.pclk1().0);
    let _ = writeln!(log, "pclk2  = {} Hz", clocks.pclk2().0);
    let _ = writeln!(log, "usart0 = {} Hz", clocks.usart0().0);
    let _ = writeln!(
        log,
        "CK_OUT on PA8 = sysclk/4 = {} Hz",
        clocks.sysclk().0 / 4
    );

    loop {}
}
