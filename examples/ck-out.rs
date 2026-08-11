//! Clock tree: bus prescalers, the USART0 clock source, and CK_OUT.
//!
//! `Clocks` only reports what the HAL *believes* it configured; routing a clock
//! node onto a pin is what proves the hardware agrees. This drives `sysclk / 4`
//! onto PA8 (AF0) for a scope or a frequency counter, sets the bus prescalers,
//! and logs the computed values so the two can be compared.
//!
//! Covers: `CFGR` `hclk`/`pclk1`/`pclk2`/`usart0_sel`, `AhbPsc`/`ApbPsc`,
//! `Usart0Sel`, the `Clocks` getters, and `Rcu::ck_out`.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::pac;
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::{
    AhbPsc, ApbPsc, CFGR, CkOutDiv, CkOutSrc, PllDiv, PllFreq, RcuExt, Usart0Sel,
};

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    let clocks = CFGR::default()
        .sysclk(PllFreq::Mhz48)
        .hclk(AhbPsc::Div1) // hclk = 48 MHz
        .pclk1(ApbPsc::Div2) // pclk1 = 24 MHz
        .pclk2(ApbPsc::Div1) // pclk2 = 48 MHz
        .usart0_sel(Usart0Sel::Sysclk) // USART0 off sysclk, not the APB2 clock
        .freeze(&mut rcu, &mut dp.fmc);

    let gpioa = dp.gpioa.split(&mut rcu);

    // CK_OUT goes on PA8 (AF0); measure sysclk/4 = 12 MHz there.
    let _ck_out = gpioa.pa8.into_alternate::<0>();
    rcu.ck_out(CkOutSrc::Sysclk, CkOutDiv::Div4);
    // The PLL can be tapped too, with its own divide-by-1/2 before CK_OUT, e.g.:
    let _alt_src = CkOutSrc::Pll(PllDiv::Div2);

    defmt::info!("sysclk = {} Hz", clocks.sysclk().to_Hz());
    defmt::info!("hclk   = {} Hz", clocks.hclk().to_Hz());
    defmt::info!("pclk1  = {} Hz", clocks.pclk1().to_Hz());
    defmt::info!("pclk2  = {} Hz", clocks.pclk2().to_Hz());
    defmt::info!("usart0 = {} Hz", clocks.usart0().to_Hz());
    defmt::info!(
        "CK_OUT on PA8 = sysclk/4 = {} Hz",
        clocks.sysclk().to_Hz() / 4
    );

    loop {}
}
