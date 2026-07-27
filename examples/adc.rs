//! ADC: the internal reference and temperature sensor, plus one external pin.
//!
//! Needs no external wiring to be useful — `read_vref` and `read_temperature`
//! read on-chip sources. The external read is on PA0; leave it floating or tie
//! it to a known voltage. Results go over USART0 (PA9/PA10) at 115200 8N1.
//!
//! `adc_sel` must be set, or `CK_ADC` stays at zero and `Adc::new` would divide
//! by it. Covers: `Adc::new`, `read`, `read_vref`, `read_temperature`,
//! `SampTime`, and `into_analog` / the `Channel` bound.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::adc::{Adc, SampTime};
use gd32e2_hal::gpio::GpioExt;
use gd32e2_hal::pac;
use gd32e2_hal::rcu::{AdcPsc, AdcSel, CFGR, Irc28mDiv, PllFreq, RcuExt};

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    let clocks = CFGR::default()
        .sysclk(PllFreq::Mhz48)
        // CK_ADC = pclk2 / 8 = 6 MHz — slow enough for the temperature sensor.
        .adc_sel(AdcSel::Prescaled(AdcPsc::Apb2Div8))
        .freeze(&mut rcu, &mut dp.fmc);

    // The other ADC clock source is the dedicated 28 MHz oscillator, e.g.:
    let _alt_src = AdcSel::Irc28m(Irc28mDiv::Div2);

    let gpioa = dp.gpioa.split(&mut rcu);

    // A pin must go through into_analog() before it satisfies `Channel`.
    let ain = gpioa.pa0.into_analog();

    let adc = Adc::new(&mut rcu, dp.adc, clocks);

    defmt::info!("ADC test, CK_ADC = {} Hz", clocks.ck_adc().0);

    loop {
        let vdda = adc.read_vref();
        defmt::info!("VDDA = {} mV", vdda);

        match adc.read_temperature() {
            Some(tenths) => {
                defmt::info!("temp = {}.{} C", tenths / 10, (tenths % 10).abs());
            }
            None => {
                defmt::info!("temp unavailable (CK_ADC too fast)");
            }
        }

        let raw = adc.read(&ain, SampTime::Cycles239_5);
        defmt::info!("PA0 raw = {}", raw);

        // Crude spacing between reports.
        for _ in 0..1_000_000 {
            cortex_m::asm::nop();
        }
    }
}
