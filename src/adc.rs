use gd32e2::gd32e230;

use crate::{
    gpio::{Analog, Pin},
    rcu::{Clocks, Enable, Rcu, Reset},
};

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

pub struct Adc {
    adc: gd32e230::Adc,
}

impl Adc {
    pub fn new(rcu: &mut Rcu, adc: gd32e230::Adc, clocks: Clocks) -> Self {
        <gd32e230::Adc as Enable>::enable(rcu);
        <gd32e230::Adc as Reset>::reset(rcu);
        adc.ctl1().modify(|_, w| w.adcon().enabled());
        cortex_m::asm::delay((14 * clocks.hclk().0).div_ceil(clocks.ck_adc().0));
        adc.ctl1().modify(|_, w| w.rstclb().start());
        adc.ctl1().modify(|_, w| w.clb().start());
        while adc.ctl1().read().clb().is_not_complete() {}
        Self { adc }
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

    pub fn read<PIN: Channel>(&self, _pin: &PIN, time: SampTime) -> u16 {
        self.set_channel(PIN::CHANNEL);
        self.set_sample_time(PIN::CHANNEL, time);
        self.adc
            .ctl1()
            .modify(|_, w| w.etsrc().swrcst().swrcst().start());
        while self.adc.stat().read().eoc().is_not_complete() {}
        self.adc.rdata().read().rdata().bits()
    }
}

pub trait Channel {
    const CHANNEL: u8;
}

macro_rules! channel {
    ($($port:literal $pin:literal => $channel:literal),+ $(,)*) => {
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
