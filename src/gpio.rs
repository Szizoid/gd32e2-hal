use core::convert::Infallible;
use core::marker::PhantomData;
use embedded_hal::digital::{ErrorType, InputPin, OutputPin};
use gd32e2::gd32e230;

pub struct Input;
pub struct PushPull;
pub struct OpenDrain;
pub struct Output<OTYPE> {
    _otype: PhantomData<OTYPE>,
}
pub struct Analog;
pub struct Alternate;

pub enum Port {
    A,
    B,
}

pub struct Pin<MODE> {
    port: Port,
    pin: u8,
    _mode: PhantomData<MODE>,
}

pub enum Pull {
    None,
    Up,
    Down,
}

pub enum Speed {
    M2,
    M10,
    M50,
}

impl<OTYPE> ErrorType for Pin<Output<OTYPE>> {
    type Error = Infallible;
}
impl<OTYPE> OutputPin for Pin<Output<OTYPE>> {
    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.gpio_reg()
            .bop()
            .write(|w| unsafe { w.bits(1 << self.pin) });
        Ok(())
    }
    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.gpio_reg()
            .bc()
            .write(|w| unsafe { w.bits(1 << self.pin) });
        Ok(())
    }
}

impl ErrorType for Pin<Input> {
    type Error = Infallible;
}
impl InputPin for Pin<Input> {
    fn is_high(&mut self) -> Result<bool, Self::Error> {
        let bits = self.gpio_reg().istat().read().bits();
        Ok(((bits >> self.pin) & 0b1) == 0b1)
    }
    fn is_low(&mut self) -> Result<bool, Self::Error> {
        Ok(!self.is_high()?)
    }
}

impl<MODE> Pin<MODE> {
    fn gpio_reg(&self) -> &gd32e230::gpioa::RegisterBlock {
        let ptr = match self.port {
            Port::A => gd32e230::Gpioa::ptr(),
            Port::B => gd32e230::Gpiob::ptr() as *const _,
        };
        unsafe { &*ptr }
    }

    fn set_mode(&self, mode: u32) {
        let offset = self.pin * 2;
        self.gpio_reg()
            .ctl()
            .modify(|r, w| unsafe { w.bits((r.bits() & !(0b11 << offset)) | (mode << offset)) });
    }
    fn set_af(&self, af: u32) {
        let is_afsel0 = self.pin < 8;
        let offset = (self.pin % 8) * 4;
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
        let offset = self.pin * 2;
        self.gpio_reg()
            .pud()
            .modify(|r, w| unsafe { w.bits((r.bits() & !(0b11 << offset)) | (bits << offset)) });
    }
    fn set_ospd(&self, bits: u32) {
        let offset = self.pin * 2;
        self.gpio_reg()
            .ospd()
            .modify(|r, w| unsafe { w.bits((r.bits() & !(0b11 << offset)) | (bits << offset)) });
    }
    fn set_omode(&self, bits: u32) {
        let offset = self.pin;
        self.gpio_reg()
            .omode()
            .modify(|r, w| unsafe { w.bits((r.bits() & !(0b1 << offset)) | (bits << offset)) });
    }

    pub fn into_input(self) -> Pin<Input> {
        self.set_mode(0b00);
        Pin {
            port: self.port,
            pin: self.pin,
            _mode: PhantomData,
        }
    }
    pub fn into_push_pull_output(self) -> Pin<Output<PushPull>> {
        self.set_mode(0b01);
        self.set_omode(0b0);
        Pin {
            port: self.port,
            pin: self.pin,
            _mode: PhantomData,
        }
    }
    pub fn into_open_drain_output(self) -> Pin<Output<OpenDrain>> {
        self.set_mode(0b01);
        self.set_omode(0b1);
        Pin {
            port: self.port,
            pin: self.pin,
            _mode: PhantomData,
        }
    }
    pub fn into_output(self) -> Pin<Output<PushPull>> {
        self.into_push_pull_output()
    }
    pub fn into_analog(self) -> Pin<Analog> {
        self.set_mode(0b11);
        Pin {
            port: self.port,
            pin: self.pin,
            _mode: PhantomData,
        }
    }
    pub fn into_alternate(self, af: u8) -> Pin<Alternate> {
        self.set_mode(0b10);
        self.set_af(af as u32);
        Pin {
            port: self.port,
            pin: self.pin,
            _mode: PhantomData,
        }
    }

    pub fn set_pull(&self, p: Pull) {
        match p {
            Pull::None => self.set_pud(0b00),
            Pull::Up => self.set_pud(0b01),
            Pull::Down => self.set_pud(0b10),
        }
    }
    pub fn set_speed(&self, s: Speed) {
        match s {
            Speed::M2 => self.set_ospd(0b00),
            Speed::M10 => self.set_ospd(0b01),
            Speed::M50 => self.set_ospd(0b11),
        }
    }
}

pub trait GpioExt {
    type Parts;
    fn split(self) -> Self::Parts;
}

macro_rules! gpio {
    ($Parts:ident, $Gpio:ty, $port:expr, [ $($name:ident : $num:literal),+ $(,)? ]) => {
        pub struct $Parts {
            $( pub $name: Pin<Input>, )+
        }

        impl GpioExt for $Gpio {
            type Parts = $Parts;
            fn split(self) -> Self::Parts {
                $Parts {
                    $( $name: Pin { port: $port, pin: $num, _mode: PhantomData }, )+
                }
            }
        }
    };
}

gpio!(PartsA, gd32e230::Gpioa, Port::A,
    [pa0:0, pa1:1, pa2:2, pa3:3, pa4:4, pa5:5, pa6:6, pa7:7,
     pa8:8, pa9:9, pa10:10, pa11:11, pa12:12, pa13:13, pa14:14, pa15:15]);

gpio!(PartsB, gd32e230::Gpiob, Port::B,
    [pb0:0, pb1:1, pb2:2, pb3:3, pb4:4, pb5:5, pb6:6, pb7:7,
     pb8:8, pb9:9, pb10:10, pb11:11, pb12:12, pb13:13, pb14:14, pb15:15]);
