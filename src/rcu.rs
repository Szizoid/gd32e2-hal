use gd32e2::gd32e230;

const IRC8M: u32 = 8_000_000;
const PLL_SRC: u32 = IRC8M / 2;

#[derive(Clone, Copy)]
pub struct Clocks {
    hclk: u32,
    pclk1: u32,
    pclk2: u32,
    sysclk: u32,
}

impl Clocks {
    pub fn hclk(&self) -> u32 {
        self.hclk
    }
    pub fn pclk1(&self) -> u32 {
        self.pclk1
    }
    pub fn pclk2(&self) -> u32 {
        self.pclk2
    }
    pub fn sysclk(&self) -> u32 {
        self.sysclk
    }
}

pub struct CFGR {
    hclk: Option<u32>,
    pclk1: Option<u32>,
    pclk2: Option<u32>,
    sysclk: Option<u32>,
}

impl Default for CFGR {
    fn default() -> Self {
        CFGR {
            hclk: Option::None,
            pclk1: Option::None,
            pclk2: Option::None,
            sysclk: Option::None,
        }
    }
}

impl CFGR {
    fn nearest_devisor_ahb(source: u32, desired: Option<u32>) -> (u32, u32) {
        match desired {
            Option::None => (1, source),
            Option::Some(d) => {
                let div = match source / d {
                    0..=1 => 1,
                    2 => 2,
                    2..=5 => 4,
                    6..=11 => 8,
                    12..=39 => 16,
                    40..=95 => 64,
                    96..=191 => 128,
                    192..=383 => 256,
                    _ => 512,
                };
                let real_freq = source / div;
                (div, real_freq)
            }
        }
    }
    fn nearest_devisor_apb(source: u32, desired: Option<u32>) -> (u32, u32) {
        let (div, _) = Self::nearest_devisor_ahb(source, desired);
        let real_div = div.min(16);
        let real_freq = source / real_div;
        (real_div, real_freq)
    }
    fn nearest_multiplier_pll(desired: u32) -> (u32, u32) {
        let mult = (desired / PLL_SRC).clamp(2, 18);
        let real_freq = mult * PLL_SRC;
        (mult, real_freq)
    }

    pub fn hclk(mut self, freq: u32) -> Self {
        self.hclk = Some(freq);
        self
    }
    pub fn pclk1(mut self, freq: u32) -> Self {
        self.pclk1 = Some(freq);
        self
    }
    pub fn pclk2(mut self, freq: u32) -> Self {
        self.pclk2 = Some(freq);
        self
    }
    pub fn sysclk(mut self, freq: u32) -> Self {
        self.sysclk = Some(freq);
        self
    }

    pub fn freeze(self, rcu: &mut Rcu, fmc: &mut gd32e230::Fmc) -> Clocks {
        let sysclk = match self.sysclk {
            Option::None => IRC8M,
            Option::Some(desired) => {
                let (mult, real_sysclk) = Self::nearest_multiplier_pll(desired);
                rcu.rcu.cfg0().modify(|_, w| {
                    let w = w.pllsel().irc8m_2();
                    match mult {
                        2 => w.pllmf().mul2().pllmf_msb().none(),
                        3 => w.pllmf().mul3().pllmf_msb().none(),
                        4 => w.pllmf().mul4().pllmf_msb().none(),
                        5 => w.pllmf().mul5().pllmf_msb().none(),
                        6 => w.pllmf().mul6().pllmf_msb().none(),
                        7 => w.pllmf().mul7().pllmf_msb().none(),
                        8 => w.pllmf().mul8().pllmf_msb().none(),
                        9 => w.pllmf().mul9().pllmf_msb().none(),
                        10 => w.pllmf().mul10().pllmf_msb().none(),
                        11 => w.pllmf().mul11().pllmf_msb().none(),
                        12 => w.pllmf().mul12().pllmf_msb().none(),
                        13 => w.pllmf().mul13().pllmf_msb().none(),
                        14 => w.pllmf().mul14().pllmf_msb().none(),
                        15 => w.pllmf().mul15().pllmf_msb().none(),
                        16 => w.pllmf().mul16().pllmf_msb().none(),
                        17 => w.pllmf().mul2().pllmf_msb().plus15(),
                        18 => w.pllmf().mul3().pllmf_msb().plus15(),
                        _ => unreachable!(),
                    }
                });
                rcu.rcu.ctl0().modify(|_, w| w.pllen().on());
                while rcu.rcu.ctl0().read().pllstb().is_not_ready() {}
                real_sysclk
            }
        };
        let (ahb_div, hclk) = Self::nearest_devisor_ahb(sysclk, self.hclk);
        let (apb1_div, pclk1) = Self::nearest_devisor_apb(hclk, self.pclk1);
        let (apb2_div, pclk2) = Self::nearest_devisor_apb(hclk, self.pclk2);

        fmc.ws().modify(|_, w| match hclk {
            0..=24_000_000 => w.wscnt().ws0(),
            24_000_001..=48_000_000 => w.wscnt().ws1(),
            48_000_001..=72_000_000 => w.wscnt().ws2(),
            _ => unreachable!(),
        });

        rcu.rcu.cfg0().modify(|_, w| {
            let w = match ahb_div {
                1 => w.ahbpsc().div1(),
                2 => w.ahbpsc().div2(),
                4 => w.ahbpsc().div4(),
                8 => w.ahbpsc().div8(),
                16 => w.ahbpsc().div16(),
                64 => w.ahbpsc().div64(),
                128 => w.ahbpsc().div128(),
                256 => w.ahbpsc().div256(),
                512 => w.ahbpsc().div512(),
                _ => unreachable!(),
            };
            let w = match apb1_div {
                1 => w.apb1psc().div1(),
                2 => w.apb1psc().div2(),
                4 => w.apb1psc().div4(),
                8 => w.apb1psc().div8(),
                16 => w.apb1psc().div16(),
                _ => unreachable!(),
            };
            let w = match apb2_div {
                1 => w.apb2psc().div1(),
                2 => w.apb2psc().div2(),
                4 => w.apb2psc().div4(),
                8 => w.apb2psc().div8(),
                16 => w.apb2psc().div16(),
                _ => unreachable!(),
            };
            if self.sysclk.is_some() {
                w.scs().pll()
            } else {
                w
            }
        });
        Clocks {
            hclk,
            pclk1,
            pclk2,
            sysclk,
        }
    }
}

pub struct Rcu {
    rcu: gd32e230::Rcu, // Own raw Peripheral
}

pub trait RcuExt {
    fn constrain(self) -> Rcu;
}

impl RcuExt for gd32e230::Rcu {
    fn constrain(self) -> Rcu {
        Rcu { rcu: self }
    }
}

pub trait Enable {
    fn enable(rcu: &mut Rcu);
    fn disable(rcu: &mut Rcu);
}

macro_rules! bus {
    ($($Periph:ty => $reg:ident, $bit:ident,)+) => {
        $(
            impl Enable for $Periph {
                fn enable(rcu: &mut Rcu) {
                    rcu.rcu.$reg().modify(|_, w| w.$bit().enabled());
                }
                fn disable(rcu: &mut Rcu) {
                    rcu.rcu.$reg().modify(|_, w| w.$bit().disabled());
                }
            }
        )+
    };
}

bus! {
    gd32e230::Gpioa => ahben, paen,
    gd32e230::Gpiob => ahben, pben,
    gd32e230::Gpiof => ahben, pfen,
}
