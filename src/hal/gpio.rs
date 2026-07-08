use core::marker::PhantomData;
use gd32e2::gd32e230;

pub struct Input;
pub struct Output;
pub struct Analog;

pub enum Port {
    A,
    B,
}

pub struct Pin<MODE> {
    port: Port,
    pin: u8,
    _mode: PhantomData<MODE>,
}

impl Pin<Input> {
    pub fn is_high(&self) -> bool {
        let bits = match self.port {
            Port::A => {
                let gpio = unsafe { &*gd32e230::Gpioa::ptr() };
                gpio.istat().read().bits()
            }
            Port::B => {
                let gpio = unsafe { &*gd32e230::Gpiob::ptr() };
                gpio.istat().read().bits()
            }
        };
        ((bits >> self.pin) & 0b1) == 0b1
    }
    pub fn is_low(&self) -> bool {
        !self.is_high()
    }
}

impl Pin<Output> {
    pub fn set_high(&mut self) {
        match self.port {
            Port::A => {
                let gpio = unsafe { &*gd32e230::Gpioa::ptr() };
                gpio.bop().write(|w| unsafe { w.bits(1 << self.pin) });
            }
            Port::B => {
                let gpio = unsafe { &*gd32e230::Gpiob::ptr() };
                gpio.bop().write(|w| unsafe { w.bits(1 << self.pin) });
            }
        }
    }

    pub fn set_low(&mut self) {
        match self.port {
            Port::A => {
                let gpio = unsafe { &*gd32e230::Gpioa::ptr() };
                gpio.bc().write(|w| unsafe { w.bits(1 << self.pin) });
            }
            Port::B => {
                let gpio = unsafe { &*gd32e230::Gpiob::ptr() };
                gpio.bc().write(|w| unsafe { w.bits(1 << self.pin) });
            }
        }
    }
}

impl<MODE> Pin<MODE> {
    fn set_mode(&self, mode: u32) {
        let offset = self.pin * 2;
        match self.port {
            Port::A => {
                let gpio = unsafe { &*gd32e230::Gpioa::ptr() };
                gpio.ctl().modify(|r, w| unsafe {
                    w.bits((r.bits() & !(0b11 << offset)) | (mode << offset))
                });
            }
            Port::B => {
                let gpio = unsafe { &*gd32e230::Gpiob::ptr() };
                gpio.ctl().modify(|r, w| unsafe {
                    w.bits((r.bits() & !(0b11 << offset)) | (mode << offset))
                });
            }
        }
    }

    pub fn into_input(self) -> Pin<Input> {
        self.set_mode(0b00);
        Pin {
            port: self.port,
            pin: self.pin,
            _mode: PhantomData,
        }
    }
    pub fn into_output(self) -> Pin<Output> {
        self.set_mode(0b01);
        Pin {
            port: self.port,
            pin: self.pin,
            _mode: PhantomData,
        }
    }
    pub fn into_analog(self) -> Pin<Analog> {
        self.set_mode(0b11);
        Pin {
            port: self.port,
            pin: self.pin,
            _mode: PhantomData,
        }
    }
}

macro_rules! gpio {
    ($Parts:ident, $split:ident, $Gpio:ty, $port:expr, [ $($name:ident : $num:literal),+ $(,)? ]) => {
        pub struct $Parts {
            $( pub $name: Pin<Input>, )+
        }
        pub fn $split(_gpio: $Gpio) -> $Parts {
            $Parts {
                $( $name: Pin { port: $port, pin: $num, _mode: PhantomData }, )+
            }
        }
    };
}

gpio!(PartsA, split_gpioa, gd32e230::Gpioa, Port::A,
    [pa0:0, pa1:1, pa2:2, pa3:3, pa4:4, pa5:5, pa6:6, pa7:7,
     pa8:8, pa9:9, pa10:10, pa11:11, pa12:12, pa13:13, pa14:14, pa15:15]);

gpio!(PartsB, split_gpiob, gd32e230::Gpiob, Port::B,
    [pb0:0, pb1:1, pb2:2, pb3:3, pb4:4, pb5:5, pb6:6, pb7:7,
     pb8:8, pb9:9, pb10:10, pb11:11, pb12:12, pb13:13, pb14:14, pb15:15]);
