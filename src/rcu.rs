//! Reset and clock unit: the system clock tree and per-peripheral clock gating.
//!
//! Start with [`RcuExt::constrain`], build the clock tree with [`CFGR`], then
//! freeze it into a [`Clocks`] value that the other modules take as an argument.
//! Freezing writes the registers once; the resulting frequencies are read-only
//! afterwards.
//!
//! ```ignore
//! let mut rcu = dp.rcu.constrain();
//! let clocks = CFGR::default()
//!     .sysclk(PllFreq::Mhz48)
//!     .adc_sel(AdcSel::Prescaled(AdcPsc::Apb2Div8))
//!     .freeze(&mut rcu, &mut dp.fmc);
//! ```
//!
//! Peripheral clocks are gated through the [`Enable`] trait, which drivers call
//! from their constructors, so a peripheral cannot be used unclocked. [`Reset`]
//! is separate because not every peripheral has a reset bit — DMA has none.
//!
//! `HXTAL` and `LXTAL` are not started — no crystal is fitted on the target board.

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

/// Target system clock produced by the PLL, in 4 MHz steps up to the 72 MHz limit.
///
/// Named by the resulting frequency rather than the multiplier because the PLL
/// source is fixed (IRC8M/2 = 4 MHz), so the two map one-to-one. Only reachable
/// frequencies exist as variants, which makes an impossible request a compile
/// error instead of a silently rounded one.
#[derive(Clone, Copy)]
#[allow(missing_docs)]
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

/// AHB prescaler: divides the system clock down to `hclk`.
///
/// Named by the divider, not the resulting frequency, because the source
/// (`sysclk`) varies with configuration. Division can't exceed the source, so
/// every variant is legal at any `sysclk`.
#[derive(Clone, Copy)]
#[allow(missing_docs)]
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

/// APB prescaler: divides `hclk` down to `pclk1` (APB1) or `pclk2` (APB2).
#[derive(Clone, Copy)]
#[allow(missing_docs)]
pub enum ApbPsc {
    Div1 = 1,
    Div2 = 2,
    Div4 = 4,
    Div8 = 8,
    Div16 = 16,
}

/// Divider for the prescaled `CK_ADC` branch, including which bus it taps.
#[derive(Clone, Copy)]
#[allow(missing_docs)]
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

/// Divider on the internal 28 MHz oscillator feeding `CK_ADC`.
#[derive(Clone, Copy)]
#[allow(missing_docs)]
pub enum Irc28mDiv {
    Div1,
    Div2,
}

/// Source of the ADC clock.
///
/// Each branch carries its own divider inside the variant, so a divider can't be
/// specified for the branch it doesn't belong to.
#[derive(Clone, Copy)]
pub enum AdcSel {
    /// The dedicated internal 28 MHz oscillator, which [`CFGR::freeze`] starts.
    Irc28m(Irc28mDiv),
    /// A prescaled tap off APB2 or AHB.
    Prescaled(AdcPsc),
}

/// Source of the USART0 clock, independent of the APB2 bus clock.
#[derive(Clone, Copy)]
pub enum Usart0Sel {
    /// The APB2 bus clock — the reset default.
    Apb2,
    /// The system clock, unaffected by the bus prescalers.
    Sysclk,
    /// The 32.768 kHz crystal.
    ///
    /// Selecting this without starting `LXTAL` leaves USART0 unclocked, and its
    /// blocking reads and writes will never return. The HAL cannot know what is
    /// fitted on a given board, so this is left to the caller.
    Lxtal,
    /// The internal 8 MHz RC oscillator, independent of the system clock.
    Irc8m,
}

/// Frozen clock frequencies, produced by [`CFGR::freeze`].
///
/// Passed by value into the drivers that need it (USART for its baud divisor,
/// ADC for its calibration delay). There are no setters — once frozen, the tree
/// matches what was actually written to the registers.
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
    /// AHB clock, which also clocks the core.
    pub fn hclk(&self) -> Hertz {
        self.hclk
    }
    /// APB1 bus clock.
    pub fn pclk1(&self) -> Hertz {
        self.pclk1
    }
    /// APB2 bus clock.
    pub fn pclk2(&self) -> Hertz {
        self.pclk2
    }
    /// System clock, before the AHB prescaler.
    pub fn sysclk(&self) -> Hertz {
        self.sysclk
    }
    /// Clock actually feeding USART0, per [`Usart0Sel`].
    pub fn usart0(&self) -> Hertz {
        self.usart0
    }
    /// Clock feeding the ADC. Zero if [`CFGR::adc_sel`] was never called.
    pub fn ck_adc(&self) -> Hertz {
        self.ck_adc
    }
}

/// Builder for the clock tree, applied by [`freeze`](CFGR::freeze).
///
/// Every field is optional: anything left unset keeps its reset value and its
/// registers are not written. With no calls at all, the system clock stays on
/// IRC8M at 8 MHz with no bus division.
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

    /// Sets the AHB prescaler, dividing `sysclk` down to `hclk`.
    pub fn hclk(mut self, psc: AhbPsc) -> Self {
        self.hclk = Some(psc);
        self
    }
    /// Sets the APB1 prescaler, dividing `hclk` down to `pclk1`.
    pub fn pclk1(mut self, psc: ApbPsc) -> Self {
        self.pclk1 = Some(psc);
        self
    }
    /// Sets the APB2 prescaler, dividing `hclk` down to `pclk2`.
    pub fn pclk2(mut self, psc: ApbPsc) -> Self {
        self.pclk2 = Some(psc);
        self
    }
    /// Runs the system clock off the PLL at the given frequency.
    ///
    /// Without this the system clock stays on IRC8M at 8 MHz.
    pub fn sysclk(mut self, freq: PllFreq) -> Self {
        self.sysclk = Some(freq);
        self
    }
    /// Picks the USART0 clock source. Defaults to the APB2 bus clock.
    pub fn usart0_sel(mut self, src: Usart0Sel) -> Self {
        self.usart0_sel = Some(src);
        self
    }
    /// Picks the ADC clock source and starts it if needed.
    ///
    /// Without this the ADC is left unclocked and [`Clocks::ck_adc`] stays zero —
    /// constructing an [`Adc`](crate::adc::Adc) would then divide by zero rather
    /// than silently hang in calibration.
    pub fn adc_sel(mut self, sel: AdcSel) -> Self {
        self.adc_sel = Some(sel);
        self
    }

    /// Applies the configuration and returns the resulting frequencies.
    ///
    /// Flash wait states are raised from the new `hclk` *before* the system clock
    /// switches over, so the flash is never read faster than it can respond.
    /// `fmc` is taken because those wait states live in a separate peripheral.
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

/// Divider on the PLL branch feeding `CK_OUT`, ahead of the source multiplexer.
#[derive(Clone, Copy)]
#[allow(missing_docs)]
pub enum PllDiv {
    Div1,
    Div2,
}

/// Clock node to route out on the `CK_OUT` pin.
///
/// The PLL branch carries its own pre-multiplexer divider inside the variant, so
/// it cannot be set for a source it doesn't apply to. Selecting a source that
/// isn't running simply leaves the pin quiet.
#[derive(Clone, Copy)]
pub enum CkOutSrc {
    /// Nothing driven out.
    None,
    /// The internal RC oscillator dedicated to the ADC.
    Irc14m,
    /// The internal low-speed RC oscillator.
    Lsi40k,
    /// The external low-speed crystal.
    Lxtal,
    /// The system clock.
    Sysclk,
    /// The internal 8 MHz RC oscillator.
    Irc8m,
    /// The external high-speed crystal.
    Hxtal,
    /// The PLL output, through its own divider.
    Pll(PllDiv),
}

/// Divider applied to `CK_OUT` after the source multiplexer, for any source.
#[derive(Clone, Copy)]
#[allow(missing_docs)]
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

/// Owns the RCU peripheral; obtained from [`RcuExt::constrain`].
pub struct Rcu {
    rcu: gd32e230::Rcu,
}

impl Rcu {
    /// Routes an internal clock node out onto `PA8` (AF0) or `PA9` (AF5).
    ///
    /// The pin still has to be put into the matching alternate function. With no
    /// debug probe on this board, this is the only way to measure a real clock
    /// frequency with a scope or logic analyser.
    ///
    /// Unlike the [`CFGR`] settings this is applied immediately and not recorded
    /// in [`Clocks`] — nothing else needs to know about it afterwards.
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

/// Extension trait turning the raw RCU peripheral into the managed [`Rcu`].
pub trait RcuExt {
    /// Consumes the raw peripheral and returns the managed wrapper.
    fn constrain(self) -> Rcu;
}

impl RcuExt for gd32e230::Rcu {
    fn constrain(self) -> Rcu {
        Rcu { rcu: self }
    }
}

/// Clock gating for a peripheral, implemented per peripheral type.
///
/// Drivers call [`enable`](Enable::enable) from their constructors, so a
/// peripheral cannot be used before its clock is running.
pub trait Enable {
    /// Switches the peripheral's clock on.
    fn enable(rcu: &mut Rcu);
    /// Switches the peripheral's clock off.
    fn disable(rcu: &mut Rcu);
}

/// Reset control for a peripheral, implemented per peripheral type.
pub trait Reset {
    /// Pulses the peripheral's reset line, returning its registers to defaults.
    fn reset(rcu: &mut Rcu);
}

macro_rules! bus_en {
    ($($Periph:ty => $en_reg:ident, $en_bit:ident,)+) => {
        $(
            impl Enable for $Periph {
                fn enable(rcu: &mut Rcu) {
                    rcu.rcu.$en_reg().modify(|_, w| w.$en_bit().enabled());
                }
                fn disable(rcu: &mut Rcu) {
                    rcu.rcu.$en_reg().modify(|_, w| w.$en_bit().disabled());
                }
            }
        )+
    };
}

macro_rules! bus_rst {
    ($($Periph:ty => $rst_reg:ident, $rst_bit:ident,)+) => {
        $(
            impl Reset for $Periph {
                fn reset(rcu: &mut Rcu) {
                    rcu.rcu.$rst_reg().modify(|_, w| w.$rst_bit().reset());
                    rcu.rcu.$rst_reg().modify(|_, w| w.$rst_bit().clear_bit());
                }
            }
        )+
    };
}

bus_en! {
    gd32e230::Gpioa => ahben, paen,
    gd32e230::Gpiob => ahben, pben,
    gd32e230::Gpiof => ahben, pfen,
    gd32e230::Dma => ahben, dmaen,
    gd32e230::Usart1 => apb1en, usart1en,
    gd32e230::Spi1 => apb1en, spi1en,
    gd32e230::Usart0 => apb2en, usart0en,
    gd32e230::Adc => apb2en, adcen,
    gd32e230::Spi0 => apb2en, spi0en,
}

bus_rst! {
    gd32e230::Gpioa => ahbrst, parst,
    gd32e230::Gpiob => ahbrst, pbrst,
    gd32e230::Gpiof => ahbrst, pfrst,
    gd32e230::Usart1 => apb1rst, usart1rst,
    gd32e230::Spi1 => apb1rst, spi1rst,
    gd32e230::Usart0 => apb2rst, usart0rst,
    gd32e230::Adc => apb2rst, adcrst,
    gd32e230::Spi0 => apb2rst, spi0rst,
}
