use gd32e2::gd32e230;

use crate::{
    gpio::{Analog, Pin},
    rcu::{Clocks, Enable, Rcu, Reset},
};

const VREFINT_CAL_ADDR: *const u16 = 0x1FFFF7C0 as *const u16;
const NOMINAL_VDDA_MV: i32 = 3300;
const ADC_MAX_CODE: i32 = 4095;
const V25_MV: i32 = 1450;
const AVG_SLOPE_X10: i32 = 43;

const TEMP_CHANNEL: u8 = 16;
const VREF_CHANNEL: u8 = 17;

/// Cycles of CK_ADC to wait between ADCON and calibration (manual, 11.4.1).
const CALIBRATION_DELAY_CYCLES: u32 = 14;
/// Minimum sampling time for the temperature sensor: 17.1 us (manual, 10.4.11).
const TEMP_MIN_SAMPTIME_US_X10: u64 = 171;
/// The longest available `SampTime` (`Cycles239_5`), scaled by 10 to stay integer.
const MAX_SAMPTIME_CYCLES_X10: u64 = 2395;
const US_PER_S: u64 = 1_000_000;

#[derive(Clone, Copy)]
pub enum SampTime {
    Cycles1_5 = 0b000,
    Cycles7_5 = 0b001,
    Cycles13_5 = 0b010,
    Cycles28_5 = 0b011,
    Cycles41_5 = 0b100,
    Cycles55_5 = 0b101,
    Cycles71_5 = 0b110,
    Cycles239_5 = 0b111,
}

pub trait Channel {
    const CHANNEL: u8;
}

macro_rules! channel {
    ($($port:literal $pin:literal => $channel:literal),+ $(,)?) => {
        $(impl Channel for Pin<$port, $pin, Analog> { const CHANNEL: u8 = $channel; })+
    };
}

channel!(
    'A' 0 => 0,
    'A' 1 => 1,
    'A' 2 => 2,
    'A' 3 => 3,
    'A' 4 => 4,
    'A' 5 => 5,
    'A' 6 => 6,
    'A' 7 => 7,
    'B' 0 => 8,
    'B' 1 => 9,
);

pub struct Adc {
    adc: gd32e230::Adc,
    clocks: Clocks,
}

impl Adc {
    pub fn new(rcu: &mut Rcu, adc: gd32e230::Adc, clocks: Clocks) -> Self {
        <gd32e230::Adc as Enable>::enable(rcu);
        <gd32e230::Adc as Reset>::reset(rcu);
        adc.ctl1().modify(|_, w| w.adcon().enabled());
        cortex_m::asm::delay(
            (CALIBRATION_DELAY_CYCLES * clocks.hclk().0).div_ceil(clocks.ck_adc().0),
        );
        adc.ctl1().modify(|_, w| w.rstclb().start());
        adc.ctl1().modify(|_, w| w.clb().start());
        while adc.ctl1().read().clb().is_not_complete() {}
        Self { adc, clocks }
    }

    fn set_channel(&self, channel: u8) {
        self.adc.rsq0().modify(|_, w| w.rl().bits(0b0));
        self.adc
            .rsq2()
            .modify(|_, w| unsafe { w.rsq0().bits(channel) });
    }
    fn set_sample_time(&self, channel: u8, time: SampTime) {
        self.adc.sampt1().modify(|_, w| match channel {
            0 => w.spt0().bits(time as u8),
            1 => w.spt1().bits(time as u8),
            2 => w.spt2().bits(time as u8),
            3 => w.spt3().bits(time as u8),
            4 => w.spt4().bits(time as u8),
            5 => w.spt5().bits(time as u8),
            6 => w.spt6().bits(time as u8),
            7 => w.spt7().bits(time as u8),
            8 => w.spt8().bits(time as u8),
            9 => w.spt9().bits(time as u8),
            _ => unreachable!(),
        });
    }
    fn set_internal_channel(&self, channel: u8) {
        self.set_channel(channel);
    }
    // Cycles239_5 / ck_adc >= 17.1 us, with both sides scaled by 10 to stay integer.
    fn sample_time_sufficient(&self) -> bool {
        TEMP_MIN_SAMPTIME_US_X10 * self.clocks.ck_adc().0 as u64
            <= MAX_SAMPTIME_CYCLES_X10 * US_PER_S
    }
    fn set_internal_sample_time(&self, channel: u8, time: SampTime) {
        self.adc.sampt0().modify(|_, w| match channel {
            TEMP_CHANNEL => w.spt16().bits(time as u8),
            VREF_CHANNEL => w.spt17().bits(time as u8),
            _ => unreachable!(),
        });
    }
    fn convert(&self) -> u16 {
        self.adc
            .ctl1()
            .modify(|_, w| w.etsrc().swrcst().swrcst().start());
        while self.adc.stat().read().eoc().is_not_complete() {}
        self.adc.rdata().read().rdata().bits()
    }
    fn with_internal<R>(&self, f: impl FnOnce(&Self) -> R) -> R {
        let was_enabled = self.adc.ctl1().read().tsvren().is_enabled();
        if !was_enabled {
            self.adc.ctl1().modify(|_, w| w.tsvren().enabled());
        }
        let result = f(self);
        if !was_enabled {
            self.adc.ctl1().modify(|_, w| w.tsvren().disabled());
        }
        result
    }

    pub fn read<PIN: Channel>(&self, _pin: &PIN, time: SampTime) -> u16 {
        self.set_channel(PIN::CHANNEL);
        self.set_sample_time(PIN::CHANNEL, time);
        self.convert()
    }
    /// Return temperature * 10 in Celsius
    pub fn read_temperature(&self) -> Option<i32> {
        if !self.sample_time_sufficient() {
            return None;
        }
        let vdda_mv = self.read_vref();
        let raw = self.with_internal(|s| {
            s.set_internal_channel(TEMP_CHANNEL);
            s.set_internal_sample_time(TEMP_CHANNEL, SampTime::Cycles239_5);
            s.convert()
        });
        let v_temperature_mv = raw as i32 * vdda_mv / ADC_MAX_CODE;
        let temperature_x10 = 100 * (V25_MV - v_temperature_mv) / AVG_SLOPE_X10 + 250;
        Some(temperature_x10)
    }
    /// Return VDDA in mV
    pub fn read_vref(&self) -> i32 {
        let vrefint_cal = unsafe { core::ptr::read_volatile(VREFINT_CAL_ADDR) };
        let raw = self.with_internal(|s| {
            s.set_internal_channel(VREF_CHANNEL);
            s.set_internal_sample_time(VREF_CHANNEL, SampTime::Cycles239_5);
            s.convert()
        });
        NOMINAL_VDDA_MV * vrefint_cal as i32 / raw as i32
    }
}
