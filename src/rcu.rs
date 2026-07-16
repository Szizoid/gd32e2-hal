use gd32e2::gd32e230;

use crate::time::Hertz;

const IRC8M: u32 = 8_000_000;
const IRC28M: u32 = 28_000_000;
const LXTAL: u32 = 32_768;
const PLL_SRC: u32 = IRC8M / 2;

const IRC28MDIV_DIV1: bool = true;
const IRC28MDIV_DIV2: bool = false;
const ADCSEL_IRC28M: bool = false;
const ADCSEL_PRESCALED: bool = true;
const ADCPSC_MSB_APB2: bool = false;
const ADCPSC_MSB_AHB: bool = true;

const WS0_MAX_HCLK: u32 = 24_000_000;
const WS1_MAX_HCLK: u32 = 48_000_000;
const WS2_MAX_HCLK: u32 = 72_000_000;

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
pub enum AhbPsc {
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
pub enum ApbPsc {
    Div1 = 1,
    Div2 = 2,
    Div4 = 4,
    Div8 = 8,
    Div16 = 16,
}

#[derive(Clone, Copy)]
pub enum AdcPsc {
    Apb2Div2,
    Apb2Div4,
    Apb2Div6,
    Apb2Div8,
    AhbDiv3,
    AhbDiv5,
    AhbDiv7,
    AhbDiv9,
}

#[derive(Clone, Copy)]
pub enum Irc28mDiv {
    Div1,
    Div2,
}

#[derive(Clone, Copy)]
pub enum AdcSel {
    Irc28m(Irc28mDiv),
    Prescaled(AdcPsc),
}

#[derive(Clone, Copy)]
pub enum Usart0Sel {
    Apb2,
    Sysclk,
    Lxtal,
    Irc8m,
}

#[derive(Clone, Copy)]
pub struct Clocks {
    hclk: Hertz,
    pclk1: Hertz,
    pclk2: Hertz,
    sysclk: Hertz,
    usart0: Hertz,
    ck_adc: Hertz,
}

impl Clocks {
    pub fn hclk(&self) -> Hertz {
        self.hclk
    }
    pub fn pclk1(&self) -> Hertz {
        self.pclk1
    }
    pub fn pclk2(&self) -> Hertz {
        self.pclk2
    }
    pub fn sysclk(&self) -> Hertz {
        self.sysclk
    }
    pub fn usart0(&self) -> Hertz {
        self.usart0
    }
    pub fn ck_adc(&self) -> Hertz {
        self.ck_adc
    }
}

#[derive(Default)]
pub struct CFGR {
    hclk: Option<AhbPsc>,
    pclk1: Option<ApbPsc>,
    pclk2: Option<ApbPsc>,
    sysclk: Option<PllFreq>,
    usart0_sel: Option<Usart0Sel>,
    adc_sel: Option<AdcSel>,
}

impl CFGR {
    fn pll_mul(desired: PllFreq) -> u32 {
        (desired as u32) / PLL_SRC
    }

    pub fn hclk(mut self, psc: AhbPsc) -> Self {
        self.hclk = Some(psc);
        self
    }
    pub fn pclk1(mut self, psc: ApbPsc) -> Self {
        self.pclk1 = Some(psc);
        self
    }
    pub fn pclk2(mut self, psc: ApbPsc) -> Self {
        self.pclk2 = Some(psc);
        self
    }
    pub fn sysclk(mut self, freq: PllFreq) -> Self {
        self.sysclk = Some(freq);
        self
    }
    pub fn usart0_sel(mut self, src: Usart0Sel) -> Self {
        self.usart0_sel = Some(src);
        self
    }
    pub fn adc_sel(mut self, sel: AdcSel) -> Self {
        self.adc_sel = Some(sel);
        self
    }

    pub fn freeze(self, rcu: &mut Rcu, fmc: &mut gd32e230::Fmc) -> Clocks {
        let sysclk = match self.sysclk {
            None => IRC8M,
            Some(desired) => {
                let mul = Self::pll_mul(desired);
                rcu.rcu.cfg0().modify(|_, w| {
                    let w = w.pllsel().irc8m_2();
                    match mul {
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

        let ahb_psc = self.hclk.unwrap_or(AhbPsc::Div1);
        let hclk = sysclk / (ahb_psc as u32);
        let apb1_psc = self.pclk1.unwrap_or(ApbPsc::Div1);
        let pclk1 = hclk / (apb1_psc as u32);
        let apb2_psc = self.pclk2.unwrap_or(ApbPsc::Div1);
        let pclk2 = hclk / (apb2_psc as u32);

        let usart0_sel = self.usart0_sel.unwrap_or(Usart0Sel::Apb2);
        let usart0 = match usart0_sel {
            Usart0Sel::Apb2 => pclk2,
            Usart0Sel::Sysclk => sysclk,
            Usart0Sel::Lxtal => LXTAL,
            Usart0Sel::Irc8m => IRC8M,
        };
        rcu.rcu.cfg2().modify(|_, w| match usart0_sel {
            Usart0Sel::Apb2 => w.usart0sel().apb2(),
            Usart0Sel::Sysclk => w.usart0sel().sys(),
            Usart0Sel::Lxtal => w.usart0sel().lxtal(),
            Usart0Sel::Irc8m => w.usart0sel().irc8m(),
        });

        // None => ADC clock stays at reset (IRC28M selected but off), 0 Hz.
        let ck_adc = match self.adc_sel {
            None => 0,
            Some(sel) => {
                match sel {
                    AdcSel::Irc28m(div) => {
                        rcu.rcu.ctl1().modify(|_, w| w.irc28men().on());
                        while rcu.rcu.ctl1().read().irc28mstb().is_not_ready() {}
                        rcu.rcu.cfg2().modify(|_, w| {
                            let w = match div {
                                Irc28mDiv::Div1 => w.irc28mdiv().bit(IRC28MDIV_DIV1),
                                Irc28mDiv::Div2 => w.irc28mdiv().bit(IRC28MDIV_DIV2),
                            };
                            w.adcsel().bit(ADCSEL_IRC28M)
                        });
                    }
                    AdcSel::Prescaled(psc) => {
                        // ADCPSC = 3-bit code split CFG0[15:14] + CFG2[31] (like PLLMF+MSB)
                        rcu.rcu.cfg0().modify(|_, w| match psc {
                            AdcPsc::Apb2Div2 | AdcPsc::AhbDiv3 => w.adcpsc().div2(),
                            AdcPsc::Apb2Div4 | AdcPsc::AhbDiv5 => w.adcpsc().div4(),
                            AdcPsc::Apb2Div6 | AdcPsc::AhbDiv7 => w.adcpsc().div6(),
                            AdcPsc::Apb2Div8 | AdcPsc::AhbDiv9 => w.adcpsc().div8(),
                        });
                        rcu.rcu.cfg2().modify(|_, w| {
                            let w = match psc {
                                AdcPsc::Apb2Div2
                                | AdcPsc::Apb2Div4
                                | AdcPsc::Apb2Div6
                                | AdcPsc::Apb2Div8 => w.adcpsc().bit(ADCPSC_MSB_APB2),
                                AdcPsc::AhbDiv3
                                | AdcPsc::AhbDiv5
                                | AdcPsc::AhbDiv7
                                | AdcPsc::AhbDiv9 => w.adcpsc().bit(ADCPSC_MSB_AHB),
                            };
                            w.adcsel().bit(ADCSEL_PRESCALED)
                        });
                    }
                }

                match sel {
                    AdcSel::Irc28m(Irc28mDiv::Div1) => IRC28M,
                    AdcSel::Irc28m(Irc28mDiv::Div2) => IRC28M / 2,
                    AdcSel::Prescaled(AdcPsc::Apb2Div2) => pclk2 / 2,
                    AdcSel::Prescaled(AdcPsc::Apb2Div4) => pclk2 / 4,
                    AdcSel::Prescaled(AdcPsc::Apb2Div6) => pclk2 / 6,
                    AdcSel::Prescaled(AdcPsc::Apb2Div8) => pclk2 / 8,
                    AdcSel::Prescaled(AdcPsc::AhbDiv3) => hclk / 3,
                    AdcSel::Prescaled(AdcPsc::AhbDiv5) => hclk / 5,
                    AdcSel::Prescaled(AdcPsc::AhbDiv7) => hclk / 7,
                    AdcSel::Prescaled(AdcPsc::AhbDiv9) => hclk / 9,
                }
            }
        };

        fmc.ws().modify(|_, w| {
            if hclk <= WS0_MAX_HCLK {
                w.wscnt().ws0()
            } else if hclk <= WS1_MAX_HCLK {
                w.wscnt().ws1()
            } else if hclk <= WS2_MAX_HCLK {
                w.wscnt().ws2()
            } else {
                unreachable!()
            }
        });

        rcu.rcu.cfg0().modify(|_, w| {
            let w = match ahb_psc {
                AhbPsc::Div1 => w.ahbpsc().div1(),
                AhbPsc::Div2 => w.ahbpsc().div2(),
                AhbPsc::Div4 => w.ahbpsc().div4(),
                AhbPsc::Div8 => w.ahbpsc().div8(),
                AhbPsc::Div16 => w.ahbpsc().div16(),
                AhbPsc::Div64 => w.ahbpsc().div64(),
                AhbPsc::Div128 => w.ahbpsc().div128(),
                AhbPsc::Div256 => w.ahbpsc().div256(),
                AhbPsc::Div512 => w.ahbpsc().div512(),
            };
            let w = match apb1_psc {
                ApbPsc::Div1 => w.apb1psc().div1(),
                ApbPsc::Div2 => w.apb1psc().div2(),
                ApbPsc::Div4 => w.apb1psc().div4(),
                ApbPsc::Div8 => w.apb1psc().div8(),
                ApbPsc::Div16 => w.apb1psc().div16(),
            };
            let w = match apb2_psc {
                ApbPsc::Div1 => w.apb2psc().div1(),
                ApbPsc::Div2 => w.apb2psc().div2(),
                ApbPsc::Div4 => w.apb2psc().div4(),
                ApbPsc::Div8 => w.apb2psc().div8(),
                ApbPsc::Div16 => w.apb2psc().div16(),
            };
            if self.sysclk.is_some() {
                w.scs().pll()
            } else {
                w
            }
        });
        Clocks {
            hclk: Hertz(hclk),
            pclk1: Hertz(pclk1),
            pclk2: Hertz(pclk2),
            sysclk: Hertz(sysclk),
            usart0: Hertz(usart0),
            ck_adc: Hertz(ck_adc),
        }
    }
}

#[derive(Clone, Copy)]
pub enum PllDiv {
    Div1,
    Div2,
}

#[derive(Clone, Copy)]
pub enum CkOutSrc {
    None,
    Irc14m,
    Lsi40k,
    Lxtal,
    Sysclk,
    Irc8m,
    Hxtal,
    Pll(PllDiv),
}

#[derive(Clone, Copy)]
pub enum CkOutDiv {
    Div1,
    Div2,
    Div4,
    Div8,
    Div16,
    Div32,
    Div64,
    Div128,
}

pub struct Rcu {
    rcu: gd32e230::Rcu, // Own raw Peripheral
}

impl Rcu {
    pub fn ck_out(&mut self, src: CkOutSrc, div: CkOutDiv) {
        self.rcu.cfg0().modify(|_, w| {
            let w = match div {
                CkOutDiv::Div1 => w.ckoutdiv().div1(),
                CkOutDiv::Div2 => w.ckoutdiv().div2(),
                CkOutDiv::Div4 => w.ckoutdiv().div4(),
                CkOutDiv::Div8 => w.ckoutdiv().div8(),
                CkOutDiv::Div16 => w.ckoutdiv().div16(),
                CkOutDiv::Div32 => w.ckoutdiv().div32(),
                CkOutDiv::Div64 => w.ckoutdiv().div64(),
                CkOutDiv::Div128 => w.ckoutdiv().div128(),
            };
            match src {
                CkOutSrc::None => w.ckoutsel().none(),
                CkOutSrc::Irc14m => w.ckoutsel().irc14m(),
                CkOutSrc::Lsi40k => w.ckoutsel().lsi40k(),
                CkOutSrc::Lxtal => w.ckoutsel().lxtal(),
                CkOutSrc::Sysclk => w.ckoutsel().sysclk(),
                CkOutSrc::Irc8m => w.ckoutsel().irc8m(),
                CkOutSrc::Hxtal => w.ckoutsel().hxtal(),
                CkOutSrc::Pll(d) => {
                    let w = w.ckoutsel().pll();
                    match d {
                        PllDiv::Div1 => w.plldv().div1(),
                        PllDiv::Div2 => w.plldv().div2(),
                    }
                }
            }
        });
    }
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
    gd32e230::Usart0 => apb2en, usart0en, apb2rst, usart0rst,
    gd32e230::Usart1 => apb1en, usart1en, apb1rst, usart1rst,
    gd32e230::Adc => apb2en, adcen, apb2rst, adcrst,
    gd32e230::Spi0 => apb2en, spi0en, apb2rst, spi0rst,
}
