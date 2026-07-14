use core::convert::Infallible;
use core::marker::PhantomData;
use embedded_hal::digital::{ErrorType, InputPin, OutputPin, StatefulOutputPin};
use gd32e2::gd32e230;

use crate::rcu::Rcu;

pub struct Input;
pub struct PushPull;
pub struct OpenDrain;
pub struct Output<OTYPE> {
    _otype: PhantomData<OTYPE>,
}
pub struct Analog;
pub struct Alternate<const AF: u8>;
pub struct Debugger;

pub struct Pin<const P: char, const N: u8, MODE> {
    _mode: PhantomData<MODE>,
}

pub enum Pull {
    Floating,
    Up,
    Down,
}

pub enum Speed {
    Mhz2,
    Mhz10,
    Mhz50,
}

pub trait ValidAf<const AF: u8> {}

macro_rules! pin_af {
    ( $( $p:literal $n:literal => [ $($af:literal),* $(,)? ] ),* $(,)? ) => {
        $( $( impl<MODE> ValidAf<$af> for Pin<$p, $n, MODE> {} )* )*
    };
}

// map table AF из datasheet Table 2-13/2-14 (die-level)
pin_af! {
    // ---- Port A ----
    'A' 0  => [1, 4, 7],          // 1:USART0_CTS/USART1_CTS  4:I2C1_SCL  7:CMP_OUT
    'A' 1  => [0, 1, 4, 5],       // 0:EVENTOUT 1:USART0_RTS/DE 4:I2C1_SDA 5:TIMER14_CH0_ON
    'A' 2  => [0, 1],             // 0:TIMER14_CH0  1:USART0_TX/USART1_TX
    'A' 3  => [0, 1],             // 0:TIMER14_CH1  1:USART0_RX/USART1_RX
    'A' 4  => [0, 1, 4, 6],       // 0:SPI0_NSS/I2S0_WS 1:USART0_CK/USART1_CK 4:TIMER13_CH0 6:SPI1_NSS
    'A' 5  => [0],                // 0:SPI0_SCK/I2S0_CK
    'A' 6  => [0, 1, 2, 5, 6, 7], // 0:SPI0_MISO 1:TIMER2_CH0 2:TIMER0_BRKIN 5:TIMER15_CH0 6:EVENTOUT 7:CMP_OUT
    'A' 7  => [0, 1, 2, 4, 5, 6], // 0:SPI0_MOSI 1:TIMER2_CH1 2:TIMER0_CH0_ON 4:TIMER13_CH0 5:TIMER16_CH0 6:EVENTOUT
    'A' 8  => [0, 1, 2, 3, 4],    // 0:CK_OUT 1:USART0_CK 2:TIMER0_CH0 3:EVENTOUT 4:USART1_TX
    'A' 9  => [0, 1, 2, 4, 5],    // 0:TIMER14_BRKIN 1:USART0_TX 2:TIMER0_CH1 4:I2C0_SCL 5:CK_OUT
    'A' 10 => [0, 1, 2, 4],       // 0:TIMER16_BRKIN 1:USART0_RX 2:TIMER0_CH2 4:I2C0_SDA
    'A' 11 => [0, 1, 2, 4, 5, 6, 7], // 0:EVENTOUT 1:USART0_CTS 2:TIMER0_CH3 4:I2C0_SMBA 5:I2C1_SCL 6:SPI1_IO2 7:CMP_OUT
    'A' 12 => [0, 1, 2, 4, 5, 6], // 0:EVENTOUT 1:USART0_RTS/DE 2:TIMER0_ETI 4:I2C0_TXFRAME 5:I2C1_SDA 6:SPI1_IO3
    'A' 13 => [0, 1, 6],          // 0:SWDIO 1:IFRP_OUT 6:SPI1_MISO
    'A' 14 => [0, 1, 6],          // 0:SWCLK 1:USART0_TX/USART1_TX 6:SPI1_MOSI
    'A' 15 => [0, 1, 3, 6],       // 0:SPI0_NSS/I2S0_WS 1:USART0_RX/USART1_RX 3:EVENTOUT 6:SPI1_NSS
    // ---- Port B ----
    'B' 0  => [0, 1, 2, 4],       // 0:EVENTOUT 1:TIMER2_CH2 2:TIMER0_CH1_ON 4:USART1_RX
    'B' 1  => [1, 2, 3, 6],       // 1:TIMER2_CH3 2:TIMER13_CH0 3:TIMER0_CH2_ON 6:SPI1_SCK
    'B' 2  => [1],                // 1:TIMER2_ETI
    'B' 3  => [0, 1],             // 0:SPI0_SCK/I2S0_CK 1:EVENTOUT
    'B' 4  => [0, 1, 2, 4, 6],    // 0:SPI0_MISO 1:TIMER2_CH0 2:EVENTOUT 4:I2C0_TXFRAME 6:TIMER16_BRKIN
    'B' 5  => [0, 1, 2, 3],       // 0:SPI0_MOSI 1:TIMER2_CH1 2:I2C0_SMBA 3:TIMER15_BRKIN
    'B' 6  => [0, 1, 2],          // 0:USART0_TX 1:I2C0_SCL 2:TIMER15_CH0_ON
    'B' 7  => [0, 1, 2],          // 0:USART0_RX 1:I2C0_SDA 2:TIMER16_CH0_ON
    'B' 8  => [1, 2],             // 1:I2C0_SCL 2:TIMER15_CH0
    'B' 9  => [0, 1, 2, 3, 5, 6], // 0:IFRP_OUT 1:I2C0_SDA 2:TIMER16_CH0 3:EVENTOUT 5:I2S0_MCK 6:SPI1_NSS
    'B' 10 => [1, 6, 7],          // 1:I2C0_SCL/I2C1_SCL 6:SPI1_IO2 7:SPI1_SCK
    'B' 11 => [0, 1, 6],          // 0:EVENTOUT 1:I2C0_SDA/I2C1_SDA 6:SPI1_IO3
    'B' 12 => [0, 1, 2, 4],       // 0:SPI0_NSS/SPI1_NSS 1:EVENTOUT 2:TIMER0_BRKIN 4:I2C1_SMBA
    'B' 13 => [0, 1, 2, 5],       // 0:SPI0_SCK/SPI1_SCK 1:I2C1_TXFRAME 2:TIMER0_CH0_ON 5:I2C1_SCL
    'B' 14 => [0, 1, 2, 5],       // 0:SPI0_MISO/SPI1_MISO 1:TIMER14_CH0 2:TIMER0_CH1_ON 5:I2C1_SDA
    'B' 15 => [0, 1, 2, 3],       // 0:SPI0_MOSI/SPI1_MOSI 1:TIMER14_CH1 2:TIMER0_CH2_ON 3:TIMER14_CH0_ON
}

pub trait Active {} // No Debugger

impl Active for Input {}
impl Active for Analog {}
impl<const AF: u8> Active for Alternate<AF> {}
impl<OTYPE> Active for Output<OTYPE> {}

impl<const P: char, const N: u8, OTYPE> ErrorType for Pin<P, N, Output<OTYPE>> {
    type Error = Infallible;
}
impl<const P: char, const N: u8, OTYPE> OutputPin for Pin<P, N, Output<OTYPE>> {
    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.gpio_reg().bop().write(|w| unsafe { w.bits(1 << N) });
        Ok(())
    }
    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.gpio_reg().bc().write(|w| unsafe { w.bits(1 << N) });
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
        self.gpio_reg().tg().write(|w| unsafe { w.bits(1 << N) });
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

impl<const P: char, const N: u8> Pin<P, N, Debugger> {
    pub unsafe fn activate(self) -> Pin<P, N, Input> {
        Pin { _mode: PhantomData }
    }
}

impl<const P: char, const N: u8, MODE> Pin<P, N, MODE>
where
    MODE: Active,
{
    fn gpio_reg(&self) -> &gd32e230::gpioa::RegisterBlock {
        let ptr = match P {
            'A' => gd32e230::Gpioa::ptr(),
            'B' => gd32e230::Gpiob::ptr() as *const _,
            'F' => gd32e230::Gpiof::ptr() as *const _, // AFSEL0/1 and LOCK Registers is unvailable
            _ => unreachable!(),
        };
        unsafe { &*ptr }
    }

    fn read_pin(&self) -> bool {
        let bits = self.gpio_reg().istat().read().bits();
        ((bits >> N) & 0b1) == 0b1
    }

    fn read_octl(&self) -> bool {
        let bits = self.gpio_reg().octl().read().bits();
        ((bits >> N) & 0b1) == 0b1
    }

    fn set_mode(&self, mode: u32) {
        let offset = N * 2;
        self.gpio_reg()
            .ctl()
            .modify(|r, w| unsafe { w.bits((r.bits() & !(0b11 << offset)) | (mode << offset)) });
    }
    fn set_af(&self, af: u32) {
        let is_afsel0 = N < 8;
        let offset = (N % 8) * 4;
        if is_afsel0 {
            self.gpio_reg().afsel0().modify(|r, w| unsafe {
                w.bits((r.bits() & !(0b1111 << offset)) | (af << offset))
            });
        } else {
            self.gpio_reg().afsel1().modify(|r, w| unsafe {
                w.bits((r.bits() & !(0b1111 << offset)) | (af << offset))
            });
        }
    }
    fn set_pud(&self, bits: u32) {
        let offset = N * 2;
        self.gpio_reg()
            .pud()
            .modify(|r, w| unsafe { w.bits((r.bits() & !(0b11 << offset)) | (bits << offset)) });
    }
    fn set_ospd(&self, bits: u32) {
        let offset = N * 2;
        self.gpio_reg()
            .ospd()
            .modify(|r, w| unsafe { w.bits((r.bits() & !(0b11 << offset)) | (bits << offset)) });
    }
    fn set_omode(&self, bits: u32) {
        let offset = N;
        self.gpio_reg()
            .omode()
            .modify(|r, w| unsafe { w.bits((r.bits() & !(0b1 << offset)) | (bits << offset)) });
    }

    pub fn into_input(self) -> Pin<P, N, Input> {
        self.set_mode(0b00);
        Pin { _mode: PhantomData }
    }
    pub fn into_push_pull_output(self) -> Pin<P, N, Output<PushPull>> {
        self.set_mode(0b01);
        self.set_omode(0b0);
        Pin { _mode: PhantomData }
    }
    pub fn into_open_drain_output(self) -> Pin<P, N, Output<OpenDrain>> {
        self.set_mode(0b01);
        self.set_omode(0b1);
        Pin { _mode: PhantomData }
    }
    pub fn into_output(self) -> Pin<P, N, Output<PushPull>> {
        self.into_push_pull_output()
    }
    pub fn into_analog(self) -> Pin<P, N, Analog> {
        self.set_mode(0b11);
        Pin { _mode: PhantomData }
    }
    pub fn into_alternate<const AF: u8>(self) -> Pin<P, N, Alternate<AF>>
    where
        Self: ValidAf<AF>,
    {
        self.set_mode(0b10);
        self.set_af(AF as u32);
        Pin { _mode: PhantomData }
    }

    pub fn set_pull(&self, p: Pull) {
        match p {
            Pull::Floating => self.set_pud(0b00),
            Pull::Up => self.set_pud(0b01),
            Pull::Down => self.set_pud(0b10),
        }
    }
    pub fn set_speed(&self, s: Speed) {
        match s {
            Speed::Mhz2 => self.set_ospd(0b00),
            Speed::Mhz10 => self.set_ospd(0b01),
            Speed::Mhz50 => self.set_ospd(0b11),
        }
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
