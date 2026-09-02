//! Comparator: sweeping the threshold past a fixed input.
//!
//! PA1 is the only non-inverting input, and on this board it sits on a
//! 1 MOhm / 100 kOhm divider off VCC_IN, which is not powered — so the level
//! comes from PA4 instead, wired to +3V3 and tied to PA1 by closing `CMPSW`.
//! The threshold is what moves: the four VREFINT taps sit at 0.3, 0.6, 0.9 and
//! 1.2 V, all below the supply, so every tap must read the input as high.
//!
//! `CMPO` is taken before the polarity multiplexer, so the inverted half of the
//! run repeats the first half rather than mirroring it.
//!
//! Covers: `Cmp::new`, `enable`, `output`, `disable`, `release`, `CMPSW` through
//! the `(PA1, PA4)` pair, every `InvertingInput` tap and both `Polarity`
//! settings. Output goes over RTT.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::adc::SampTime;
use gd32e2_hal::cmp::{
    Cmp, CmpConfig, InvertingInput, Polarity, Speed, Vrefint, VrefintHalf, VrefintQuarter,
    VrefintThreeQuarters,
};
use gd32e2_hal::gpio::{Analog, Pin};
use gd32e2_hal::pac;
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::{AdcPsc, AdcSel, ClockConfig, PllFreq, Rcu, SysClk};

/// The non-inverting input: PA1 with PA4 switched onto it.
type Pos = (Pin<'A', 1, Analog>, Pin<'A', 4, Analog>);

/// Runs one configuration end to end and hands the peripheral and pins back.
fn probe<INV: InvertingInput>(
    rcu: &mut Rcu,
    cmp: pac::Cmp,
    pos: Pos,
    inv: INV,
    polarity: Polarity,
    label: &str,
) -> (pac::Cmp, Pos) {
    let config = CmpConfig::new(Speed::High).polarity(polarity);
    let cmp = Cmp::new(rcu, cmp, pos, inv, config).enable();
    // The comparator needs a few microseconds to start; reading CMPO in the next
    // instruction would report the output before it settles.
    cortex_m::asm::delay(4_800);
    defmt::info!("{}: output = {}", label, cmp.output());
    let (cmp, pos, _inv) = cmp.disable().release();
    (cmp, pos)
}

#[entry]
fn main() -> ! {
    let dp = pac::Peripherals::take().unwrap();
    let mut fmc = dp.fmc.constrain();
    let config = ClockConfig::default()
        .sysclk(SysClk::Pll(PllFreq::Mhz48))
        .adc_sel(AdcSel::Prescaled(AdcPsc::Apb2Div8));
    let mut rcu = dp.rcu.constrain().freeze(&mut fmc, config);

    let gpioa = dp.gpioa.split(&mut rcu);
    let pa1 = gpioa.pa1.into_analog();
    let pa4 = gpioa.pa4.into_analog();

    let mut adc = dp.adc.constrain(&mut rcu);
    let vdda = adc.read_vref();
    // PA4 is the level under test; PA1 still reads its own divider here, since
    // the switch only closes once the comparator is configured.
    let raw = adc.read(&pa4, SampTime::Cycles239_5);
    // 12-bit conversion against VDDA.
    let pa4_mv = (raw as u32 * vdda as u32) / 4095;
    defmt::info!("PA4 = {} mV (VDDA = {} mV)", pa4_mv, vdda);

    // With PA4 at the supply, every tap sits below the input, so all four must
    // read true.
    let cmp = dp.cmp;
    let pos = (pa1, pa4);
    let (cmp, pos) = probe(
        &mut rcu,
        cmp,
        pos,
        VrefintQuarter,
        Polarity::NotInverted,
        "300 mV",
    );
    let (cmp, pos) = probe(
        &mut rcu,
        cmp,
        pos,
        VrefintHalf,
        Polarity::NotInverted,
        "600 mV",
    );
    let (cmp, pos) = probe(
        &mut rcu,
        cmp,
        pos,
        VrefintThreeQuarters,
        Polarity::NotInverted,
        "900 mV",
    );
    let (cmp, pos) = probe(
        &mut rcu,
        cmp,
        pos,
        Vrefint,
        Polarity::NotInverted,
        "1200 mV",
    );

    // Same thresholds inverted. The answers do NOT mirror: CMPO is read before
    // the polarity multiplexer, so these must repeat the four above verbatim.
    // Polarity is only observable on the CMP_OUT pin, EXTI or the timer.
    let (cmp, pos) = probe(
        &mut rcu,
        cmp,
        pos,
        VrefintQuarter,
        Polarity::Inverted,
        "300 mV inverted",
    );
    let (cmp, pos) = probe(
        &mut rcu,
        cmp,
        pos,
        VrefintHalf,
        Polarity::Inverted,
        "600 mV inverted",
    );
    let (cmp, pos) = probe(
        &mut rcu,
        cmp,
        pos,
        VrefintThreeQuarters,
        Polarity::Inverted,
        "900 mV inverted",
    );
    // Left running on purpose, so CMP_CS can be read live over SWD: CMPEN and
    // CMPO are only meaningful while the comparator is on.
    let config = CmpConfig::new(Speed::High).polarity(Polarity::Inverted);
    let cmp = Cmp::new(&mut rcu, cmp, pos, Vrefint, config).enable();
    cortex_m::asm::delay(4_800);
    defmt::info!("1200 mV inverted, left on: output = {}", cmp.output());

    defmt::info!("done");
    loop {
        cortex_m::asm::nop();
    }
}
