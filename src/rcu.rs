use gd32e2::gd32e230;

const IRC8M: u32 = 8_000_000;
const PLL_SRC: u32 = IRC8M / 2;

#[derive(Clone, Copy)]
pub enum PllFreq {
    Mhz8 = 8_000_000,
    Mhz12 = 12_000_000,
    Mhz16 = 16_000_000,
    Mhz20 = 20_000_000,
    Mhz24 = 24_000_000,
    Mhz28 = 28_000_000,
    Mhz32 = 32_000_000,
    Mhz36 = 36_000_000,
    Mhz40 = 40_000_000,
    Mhz44 = 44_000_000,
    Mhz48 = 48_000_000,
    Mhz52 = 52_000_000,
    Mhz56 = 56_000_000,
    Mhz60 = 60_000_000,
    Mhz64 = 64_000_000,
    Mhz68 = 68_000_000,
    Mhz72 = 72_000_000,
}

#[derive(Clone, Copy)]
pub enum AhbPrescaler {
    Div1 = 1,
    Div2 = 2,
    Div4 = 4,
    Div8 = 8,
    Div16 = 16,
    Div64 = 64,
    Div128 = 128,
    Div256 = 256,
    Div512 = 512,
}

#[derive(Clone, Copy)]
pub enum ApbPrescaler {
    Div1 = 1,
    Div2 = 2,
    Div4 = 4,
    Div8 = 8,
    Div16 = 16,
}

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
    hclk: Option<AhbPrescaler>,
    pclk1: Option<ApbPrescaler>,
    pclk2: Option<ApbPrescaler>,
    sysclk: Option<PllFreq>,
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
    fn pll_multiplier(desired: PllFreq) -> u32 {
        (desired as u32) / PLL_SRC
    }

    pub fn hclk(mut self, prescaler: AhbPrescaler) -> Self {
        self.hclk = Some(prescaler);
        self
    }
    pub fn pclk1(mut self, prescaler: ApbPrescaler) -> Self {
        self.pclk1 = Some(prescaler);
        self
    }
    pub fn pclk2(mut self, prescaler: ApbPrescaler) -> Self {
        self.pclk2 = Some(prescaler);
        self
    }
    pub fn sysclk(mut self, freq: PllFreq) -> Self {
        self.sysclk = Some(freq);
        self
    }

    pub fn freeze(self, rcu: &mut Rcu, fmc: &mut gd32e230::Fmc) -> Clocks {
        let sysclk = match self.sysclk {
            Option::None => IRC8M,
            Option::Some(desired) => {
                let mult = Self::pll_multiplier(desired);
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
                desired as u32
            }
        };

        let ahb_presc = self.hclk.unwrap_or(AhbPrescaler::Div1);
        let hclk = sysclk / (ahb_presc as u32);
        let apb1_presc = self.pclk1.unwrap_or(ApbPrescaler::Div1);
        let pclk1 = hclk / (apb1_presc as u32);
        let apb2_presc = self.pclk2.unwrap_or(ApbPrescaler::Div1);
        let pclk2 = hclk / (apb2_presc as u32);

        fmc.ws().modify(|_, w| match hclk {
            0..=24_000_000 => w.wscnt().ws0(),
            24_000_001..=48_000_000 => w.wscnt().ws1(),
            48_000_001..=72_000_000 => w.wscnt().ws2(),
            _ => unreachable!(),
        });

        rcu.rcu.cfg0().modify(|_, w| {
            let w = match ahb_presc {
                AhbPrescaler::Div1 => w.ahbpsc().div1(),
                AhbPrescaler::Div2 => w.ahbpsc().div2(),
                AhbPrescaler::Div4 => w.ahbpsc().div4(),
                AhbPrescaler::Div8 => w.ahbpsc().div8(),
                AhbPrescaler::Div16 => w.ahbpsc().div16(),
                AhbPrescaler::Div64 => w.ahbpsc().div64(),
                AhbPrescaler::Div128 => w.ahbpsc().div128(),
                AhbPrescaler::Div256 => w.ahbpsc().div256(),
                AhbPrescaler::Div512 => w.ahbpsc().div512(),
            };
            let w = match apb1_presc {
                ApbPrescaler::Div1 => w.apb1psc().div1(),
                ApbPrescaler::Div2 => w.apb1psc().div2(),
                ApbPrescaler::Div4 => w.apb1psc().div4(),
                ApbPrescaler::Div8 => w.apb1psc().div8(),
                ApbPrescaler::Div16 => w.apb1psc().div16(),
            };
            let w = match apb2_presc {
                ApbPrescaler::Div1 => w.apb2psc().div1(),
                ApbPrescaler::Div2 => w.apb2psc().div2(),
                ApbPrescaler::Div4 => w.apb2psc().div4(),
                ApbPrescaler::Div8 => w.apb2psc().div8(),
                ApbPrescaler::Div16 => w.apb2psc().div16(),
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

pub trait Reset {
    fn reset(rcu: &mut Rcu);
}

macro_rules! bus {
    ($($Periph:ty => $en_reg:ident, $en_bit:ident, $rst_reg:ident, $rst_bit:ident,)+) => {
        $(
            impl Enable for $Periph {
                fn enable(rcu: &mut Rcu) {
                    rcu.rcu.$en_reg().modify(|_, w| w.$en_bit().enabled());
                }
                fn disable(rcu: &mut Rcu) {
                    rcu.rcu.$en_reg().modify(|_, w| w.$en_bit().disabled());
                }
            }

            impl Reset for $Periph {
                fn reset(rcu: &mut Rcu) {
                    rcu.rcu.$rst_reg().modify(|_, w| w.$rst_bit().reset());
                    rcu.rcu.$rst_reg().modify(|_, w| w.$rst_bit().clear_bit());
                }
            }
        )+
    };
}

bus! {
    gd32e230::Gpioa => ahben, paen, ahbrst, parst,
    gd32e230::Gpiob => ahben, pben, ahbrst, pbrst,
    gd32e230::Gpiof => ahben, pfen, ahbrst, pfrst,
}
