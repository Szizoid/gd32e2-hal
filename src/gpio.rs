use core::convert::Infallible;
use core::marker::PhantomData;
use embedded_hal::digital::{ErrorType, InputPin, OutputPin, StatefulOutputPin};
use gd32e2::gd32e230;

use crate::rcu::Rcu;

const CTL_INPUT: u32 = 0b00;
const CTL_OUTPUT: u32 = 0b01;
const CTL_AF: u32 = 0b10;
const CTL_ANALOG: u32 = 0b11;

const OMODE_PUSH_PULL: u32 = 0b0;
const OMODE_OPEN_DRAIN: u32 = 0b1;

pub struct Input;
pub struct PushPull;
pub struct OpenDrain;
pub struct Output<OTYPE> {
    _otype: PhantomData<OTYPE>,
}
pub struct Analog;
pub struct Alternate<const AF: u8>;
pub struct Debugger;
pub struct Locked<MODE> {
    _mode: PhantomData<MODE>,
}

pub struct Pin<const P: char, const N: u8, MODE> {
    _mode: PhantomData<MODE>,
}

#[derive(Clone, Copy)]
pub enum Pull {
    Floating = 0b00,
    Up = 0b01,
    Down = 0b10,
}

#[derive(Clone, Copy)]
pub enum Speed {
    Mhz2 = 0b00,
    Mhz10 = 0b01,
    Mhz50 = 0b11,
}

pub trait ValidAf<const AF: u8> {}

macro_rules! pin_af {
    ( $( $p:literal $n:literal => [ $($af:literal),* $(,)? ] ),* $(,)? ) => {
        $( $( impl<MODE> ValidAf<$af> for Pin<$p, $n, MODE> {} )* )*
    };
}

// AF map from datasheet Table 2-13/2-14 (die-level). The table carries three
// footnotes marking functions that exist only on some variants of the series:
//   (1) GD32E230x4 only          -> feature `gd32e230x4`
//   (2) GD32E230x8/6             -> features `gd32e230x6` + `gd32e230x8`
//   (3) GD32E230x8 only          -> feature `gd32e230x8`
// Entries whose AF number exists on every variant live in the common block
// below, even when the *function* behind it differs per variant (e.g. PA2 AF1
// is USART0_TX on x4 but USART1_TX on x8) — `ValidAf` only gates the number.
// The AF numbers that exist on some variants only are added by the gated
// blocks that follow. Which peripheral a pin belongs to is decided separately,
// by `usart_pins!` / `spi_pins!`, which are gated the same way.
pin_af! {
    // ---- Port A ----
    'A' 0  => [1, 7],                // 1:USART0_CTS(1)/USART1_CTS(2) 7:CMP_OUT
    'A' 1  => [0, 1],                // 0:EVENTOUT 1:USART0_RTS/DE(1) | USART1_RTS/DE(2)
    'A' 2  => [1],                   // 1:USART0_TX(1) | USART1_TX(2)
    'A' 3  => [1],                   // 1:USART0_RX(1) | USART1_RX(2)
    'A' 4  => [0, 1, 4],             // 0:SPI0_NSS/I2S0_WS 1:USART0_CK(1)|USART1_CK(2) 4:TIMER13_CH0
    'A' 5  => [0],                   // 0:SPI0_SCK/I2S0_CK
    'A' 6  => [0, 1, 2, 5, 6, 7],    // 0:SPI0_MISO 1:TIMER2_CH0 2:TIMER0_BRKIN 5:TIMER15_CH0 6:EVENTOUT 7:CMP_OUT
    'A' 7  => [0, 1, 2, 4, 5, 6],    // 0:SPI0_MOSI 1:TIMER2_CH1 2:TIMER0_CH0_ON 4:TIMER13_CH0 5:TIMER16_CH0 6:EVENTOUT
    'A' 8  => [0, 1, 2, 3],          // 0:CK_OUT 1:USART0_CK 2:TIMER0_CH0 3:EVENTOUT
    'A' 9  => [1, 2, 4, 5],          // 1:USART0_TX 2:TIMER0_CH1 4:I2C0_SCL 5:CK_OUT
    'A' 10 => [0, 1, 2, 4],          // 0:TIMER16_BRKIN 1:USART0_RX 2:TIMER0_CH2 4:I2C0_SDA
    'A' 11 => [0, 1, 2, 4, 7],       // 0:EVENTOUT 1:USART0_CTS 2:TIMER0_CH3 4:I2C0_SMBA 7:CMP_OUT
    'A' 12 => [0, 1, 2, 4],          // 0:EVENTOUT 1:USART0_RTS/DE 2:TIMER0_ETI 4:I2C0_TXFRAME
    'A' 13 => [0, 1],                // 0:SWDIO 1:IFRP_OUT
    'A' 14 => [0, 1],                // 0:SWCLK 1:USART0_TX(1) | USART1_TX(2)
    'A' 15 => [0, 1, 3],             // 0:SPI0_NSS/I2S0_WS 1:USART0_RX(1)|USART1_RX(2) 3:EVENTOUT
    // ---- Port B ----
    'B' 0  => [0, 1, 2],             // 0:EVENTOUT 1:TIMER2_CH2 2:TIMER0_CH1_ON
    'B' 1  => [1, 2, 3],             // 1:TIMER2_CH3 2:TIMER13_CH0 3:TIMER0_CH2_ON
    'B' 2  => [1],                   // 1:TIMER2_ETI
    'B' 3  => [0, 1],                // 0:SPI0_SCK/I2S0_CK 1:EVENTOUT
    'B' 4  => [0, 1, 2, 4, 6],       // 0:SPI0_MISO 1:TIMER2_CH0 2:EVENTOUT 4:I2C0_TXFRAME 6:TIMER16_BRKIN
    'B' 5  => [0, 1, 2, 3],          // 0:SPI0_MOSI 1:TIMER2_CH1 2:I2C0_SMBA 3:TIMER15_BRKIN
    'B' 6  => [0, 1, 2],             // 0:USART0_TX 1:I2C0_SCL 2:TIMER15_CH0_ON
    'B' 7  => [0, 1, 2],             // 0:USART0_RX 1:I2C0_SDA 2:TIMER16_CH0_ON
    'B' 8  => [1, 2],                // 1:I2C0_SCL 2:TIMER15_CH0
    'B' 9  => [0, 1, 2, 3, 5],       // 0:IFRP_OUT 1:I2C0_SDA 2:TIMER16_CH0 3:EVENTOUT 5:I2S0_MCK
    'B' 11 => [0],                   // 0:EVENTOUT
    'B' 12 => [1, 2],                // 1:EVENTOUT 2:TIMER0_BRKIN
    'B' 13 => [2],                   // 2:TIMER0_CH0_ON
    'B' 14 => [2],                   // 2:TIMER0_CH1_ON
    'B' 15 => [2],                   // 2:TIMER0_CH2_ON
    // NB: 'B' 10 has no variant-independent AF — every one of its functions is
    // footnoted, so it appears only in the gated blocks below.
}

// ---- (1) GD32E230x4 only ----
#[cfg(feature = "gd32e230x4")]
pin_af! {
    'B' 10 => [1],                   // 1:I2C0_SCL
    'B' 11 => [1],                   // 1:I2C0_SDA
    'B' 12 => [0],                   // 0:SPI0_NSS
    'B' 13 => [0],                   // 0:SPI0_SCK
    'B' 14 => [0],                   // 0:SPI0_MISO
    'B' 15 => [0],                   // 0:SPI0_MOSI
}

// ---- (2) GD32E230x8/6 ----
#[cfg(any(feature = "gd32e230x6", feature = "gd32e230x8"))]
pin_af! {
    'A' 8  => [4],                   // 4:USART1_TX
    'B' 0  => [4],                   // 4:USART1_RX
}

// ---- (3) GD32E230x8 only ----
#[cfg(feature = "gd32e230x8")]
pin_af! {
    'A' 0  => [4],                   // 4:I2C1_SCL
    'A' 1  => [4, 5],                // 4:I2C1_SDA 5:TIMER14_CH0_ON
    'A' 2  => [0],                   // 0:TIMER14_CH0
    'A' 3  => [0],                   // 0:TIMER14_CH1
    'A' 4  => [6],                   // 6:SPI1_NSS
    'A' 9  => [0],                   // 0:TIMER14_BRKIN
    'A' 11 => [5, 6],                // 5:I2C1_SCL 6:SPI1_IO2
    'A' 12 => [5, 6],                // 5:I2C1_SDA 6:SPI1_IO3
    'A' 13 => [6],                   // 6:SPI1_MISO
    'A' 14 => [6],                   // 6:SPI1_MOSI
    'A' 15 => [6],                   // 6:SPI1_NSS
    'B' 1  => [6],                   // 6:SPI1_SCK
    'B' 9  => [7],                   // 7:SPI1_NSS
    'B' 10 => [1, 6, 7],             // 1:I2C1_SCL 6:SPI1_IO2 7:SPI1_SCK
    'B' 11 => [1, 6],                // 1:I2C1_SDA 6:SPI1_IO3
    'B' 12 => [0, 4],                // 0:SPI1_NSS 4:I2C1_SMBA
    'B' 13 => [0, 1, 5],             // 0:SPI1_SCK 1:I2C1_TXFRAME 5:I2C1_SCL
    'B' 14 => [0, 1, 5],             // 0:SPI1_MISO 1:TIMER14_CH0 5:I2C1_SDA
    'B' 15 => [0, 1, 3],             // 0:SPI1_MOSI 1:TIMER14_CH1 3:TIMER14_CH0_ON
}

pub trait Active {} // No Debugger, No Locked<MODE>

impl Active for Input {}
impl Active for Analog {}
impl<const AF: u8> Active for Alternate<AF> {}
impl<OTYPE> Active for Output<OTYPE> {}

pub trait HasLock {}
impl<const N: u8, MODE> HasLock for Pin<'A', N, MODE> {}
impl<const N: u8, MODE> HasLock for Pin<'B', N, MODE> {}

impl<const P: char, const N: u8> Pin<P, N, Debugger> {
    /// # Safety
    ///
    /// This only relabels the type from `Debugger` to `Input` — it performs no
    /// register write. The pin remains physically in SWD mode until a
    /// subsequent `into_*()` call reconfigures `CTL`. The caller must follow up
    /// with one, or the type will no longer match the hardware state.
    pub unsafe fn activate(self) -> Pin<P, N, Input> {
        Pin { _mode: PhantomData }
    }
}

impl<const P: char, const N: u8, MODE> Pin<P, N, MODE> {
    fn reg(&self) -> &gd32e230::gpioa::RegisterBlock {
        let ptr = match P {
            'A' => gd32e230::Gpioa::ptr(),
            'B' => gd32e230::Gpiob::ptr() as *const _,
            'F' => gd32e230::Gpiof::ptr() as *const _, // AFSEL0/1 and LOCK registers are unavailable
            _ => unreachable!(),
        };
        unsafe { &*ptr }
    }

    fn read_pin(&self) -> bool {
        let bits = self.reg().istat().read().bits();
        ((bits >> N) & 0b1) == 0b1
    }

    fn read_octl(&self) -> bool {
        let bits = self.reg().octl().read().bits();
        ((bits >> N) & 0b1) == 0b1
    }
}

impl<const P: char, const N: u8, MODE> Pin<P, N, MODE>
where
    MODE: Active,
{
    fn set_mode(&self, mode: u32) {
        let offset = N * 2;
        self.reg()
            .ctl()
            .modify(|r, w| unsafe { w.bits((r.bits() & !(0b11 << offset)) | (mode << offset)) });
    }
    fn set_af(&self, af: u32) {
        let is_afsel0 = N < 8;
        let offset = (N % 8) * 4;
        if is_afsel0 {
            self.reg().afsel0().modify(|r, w| unsafe {
                w.bits((r.bits() & !(0b1111 << offset)) | (af << offset))
            });
        } else {
            self.reg().afsel1().modify(|r, w| unsafe {
                w.bits((r.bits() & !(0b1111 << offset)) | (af << offset))
            });
        }
    }
    fn set_pud(&self, bits: u32) {
        let offset = N * 2;
        self.reg()
            .pud()
            .modify(|r, w| unsafe { w.bits((r.bits() & !(0b11 << offset)) | (bits << offset)) });
    }
    fn set_ospd(&self, bits: u32) {
        let offset = N * 2;
        self.reg()
            .ospd()
            .modify(|r, w| unsafe { w.bits((r.bits() & !(0b11 << offset)) | (bits << offset)) });
    }
    fn set_omode(&self, bits: u32) {
        let offset = N;
        self.reg()
            .omode()
            .modify(|r, w| unsafe { w.bits((r.bits() & !(0b1 << offset)) | (bits << offset)) });
    }
    fn set_lk(&self, lkk: bool) {
        self.reg().lock().modify(|_, w| {
            let w = match N {
                0 => w.lk0().locked(),
                1 => w.lk1().locked(),
                2 => w.lk2().locked(),
                3 => w.lk3().locked(),
                4 => w.lk4().locked(),
                5 => w.lk5().locked(),
                6 => w.lk6().locked(),
                7 => w.lk7().locked(),
                8 => w.lk8().locked(),
                9 => w.lk9().locked(),
                10 => w.lk10().locked(),
                11 => w.lk11().locked(),
                12 => w.lk12().locked(),
                13 => w.lk13().locked(),
                14 => w.lk14().locked(),
                15 => w.lk15().locked(),
                _ => unreachable!(),
            };
            if lkk {
                w.lkk().active()
            } else {
                w.lkk().not_active()
            }
        });
    }

    pub fn into_input(self) -> Pin<P, N, Input> {
        self.set_mode(CTL_INPUT);
        Pin { _mode: PhantomData }
    }
    pub fn into_push_pull_output(self) -> Pin<P, N, Output<PushPull>> {
        self.set_mode(CTL_OUTPUT);
        self.set_omode(OMODE_PUSH_PULL);
        Pin { _mode: PhantomData }
    }
    pub fn into_open_drain_output(self) -> Pin<P, N, Output<OpenDrain>> {
        self.set_mode(CTL_OUTPUT);
        self.set_omode(OMODE_OPEN_DRAIN);
        Pin { _mode: PhantomData }
    }
    pub fn into_output(self) -> Pin<P, N, Output<PushPull>> {
        self.into_push_pull_output()
    }
    pub fn into_analog(self) -> Pin<P, N, Analog> {
        self.set_mode(CTL_ANALOG);
        Pin { _mode: PhantomData }
    }
    pub fn into_alternate<const AF: u8>(self) -> Pin<P, N, Alternate<AF>>
    where
        Self: ValidAf<AF>,
    {
        self.set_mode(CTL_AF);
        self.set_af(AF as u32);
        Pin { _mode: PhantomData }
    }
    pub fn lock(self) -> Pin<P, N, Locked<MODE>>
    where
        Self: HasLock,
    {
        // LKK write sequence from the manual: 1 -> 0 -> 1.
        self.set_lk(true);
        self.set_lk(false);
        self.set_lk(true);
        for _ in 0..2 {
            self.reg().lock().read();
        }
        Pin { _mode: PhantomData }
    }

    pub fn set_pull(&self, p: Pull) {
        self.set_pud(p as u32);
    }
    pub fn set_speed(&self, s: Speed) {
        self.set_ospd(s as u32);
    }
}

impl<const P: char, const N: u8, OTYPE> ErrorType for Pin<P, N, Output<OTYPE>> {
    type Error = Infallible;
}
impl<const P: char, const N: u8, OTYPE> OutputPin for Pin<P, N, Output<OTYPE>> {
    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.reg().bop().write(|w| unsafe { w.bits(1 << N) });
        Ok(())
    }
    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.reg().bc().write(|w| unsafe { w.bits(1 << N) });
        Ok(())
    }
}

impl<const P: char, const N: u8, OTYPE> StatefulOutputPin for Pin<P, N, Output<OTYPE>> {
    fn is_set_high(&mut self) -> Result<bool, Self::Error> {
        Ok(self.read_octl())
    }
    fn is_set_low(&mut self) -> Result<bool, Self::Error> {
        Ok(!self.read_octl())
    }
    fn toggle(&mut self) -> Result<(), Self::Error> {
        self.reg().tg().write(|w| unsafe { w.bits(1 << N) });
        Ok(())
    }
}

impl<const P: char, const N: u8> ErrorType for Pin<P, N, Input> {
    type Error = Infallible;
}
impl<const P: char, const N: u8> InputPin for Pin<P, N, Input> {
    fn is_high(&mut self) -> Result<bool, Self::Error> {
        Ok(self.read_pin())
    }
    fn is_low(&mut self) -> Result<bool, Self::Error> {
        Ok(!self.read_pin())
    }
}

impl<const P: char, const N: u8> InputPin for Pin<P, N, Output<OpenDrain>> {
    fn is_high(&mut self) -> Result<bool, Self::Error> {
        Ok(self.read_pin())
    }
    fn is_low(&mut self) -> Result<bool, Self::Error> {
        Ok(!self.read_pin())
    }
}

impl<const P: char, const N: u8, MODE> ErrorType for Pin<P, N, Locked<MODE>> {
    type Error = Infallible;
}

impl<const P: char, const N: u8, MODE> OutputPin for Pin<P, N, Locked<MODE>>
where
    Pin<P, N, MODE>: OutputPin,
{
    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.reg().bop().write(|w| unsafe { w.bits(1 << N) });
        Ok(())
    }
    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.reg().bc().write(|w| unsafe { w.bits(1 << N) });
        Ok(())
    }
}

impl<const P: char, const N: u8, MODE> StatefulOutputPin for Pin<P, N, Locked<MODE>>
where
    Pin<P, N, MODE>: StatefulOutputPin,
{
    fn is_set_high(&mut self) -> Result<bool, Self::Error> {
        Ok(self.read_octl())
    }
    fn is_set_low(&mut self) -> Result<bool, Self::Error> {
        Ok(!self.read_octl())
    }
    fn toggle(&mut self) -> Result<(), Self::Error> {
        self.reg().tg().write(|w| unsafe { w.bits(1 << N) });
        Ok(())
    }
}

impl<const P: char, const N: u8, MODE> InputPin for Pin<P, N, Locked<MODE>>
where
    Pin<P, N, MODE>: InputPin,
{
    fn is_high(&mut self) -> Result<bool, Self::Error> {
        Ok(self.read_pin())
    }
    fn is_low(&mut self) -> Result<bool, Self::Error> {
        Ok(!self.read_pin())
    }
}

pub trait GpioExt {
    type Parts;
    fn split(self, rcu: &mut Rcu) -> Self::Parts;
}

macro_rules! gpio {
    ($Parts:ident, $Gpio:ty, $P:literal, [ $($name:ident : $num:literal : $mode:ty),+ $(,)? ]) => {
        pub struct $Parts { $( pub $name: Pin<$P, $num, $mode>, )+ }

        impl GpioExt for $Gpio {
            type Parts = $Parts;
            fn split(self, rcu: &mut Rcu) -> Self::Parts {
                <$Gpio as crate::rcu::Enable>::enable(rcu);
                $Parts { $( $name: Pin::<$P, $num, $mode> { _mode: PhantomData }, )+ }
            }
        }
    };
}

gpio!(PartsA, gd32e230::Gpioa, 'A',
    [pa0:0:Input, pa1:1:Input, pa2:2:Input, pa3:3:Input, pa4:4:Input, pa5:5:Input, pa6:6:Input, pa7:7:Input,
     pa8:8:Input, pa9:9:Input, pa10:10:Input, pa11:11:Input, pa12:12:Input, pa13:13:Debugger, pa14:14:Debugger, pa15:15:Input]);

gpio!(PartsB, gd32e230::Gpiob, 'B',
    [pb0:0:Input, pb1:1:Input, pb2:2:Input, pb3:3:Input, pb4:4:Input, pb5:5:Input, pb6:6:Input, pb7:7:Input,
     pb8:8:Input, pb9:9:Input, pb10:10:Input, pb11:11:Input, pb12:12:Input, pb13:13:Input, pb14:14:Input, pb15:15:Input]);

gpio!(PartsF, gd32e230::Gpiof, 'F', [pf0:0:Input, pf1:1:Input]);
