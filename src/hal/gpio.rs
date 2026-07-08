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

pub struct PartsA {
    pub pa0: Pin<Input>,
    pub pa1: Pin<Input>,
    pub pa2: Pin<Input>,
    pub pa3: Pin<Input>,
    pub pa4: Pin<Input>,
    pub pa5: Pin<Input>,
    pub pa6: Pin<Input>,
    pub pa7: Pin<Input>,
    pub pa8: Pin<Input>,
    pub pa9: Pin<Input>,
    pub pa10: Pin<Input>,
    pub pa11: Pin<Input>,
    pub pa12: Pin<Input>,
    pub pa13: Pin<Input>,
    pub pa14: Pin<Input>,
    pub pa15: Pin<Input>,
}

pub fn split_gpioa(_gpioa: gd32e230::Gpioa) -> PartsA {
    PartsA {
        pa0: Pin {
            port: Port::A,
            pin: 0,
            _mode: PhantomData,
        },
        pa1: Pin {
            port: Port::A,
            pin: 1,
            _mode: PhantomData,
        },
        pa2: Pin {
            port: Port::A,
            pin: 2,
            _mode: PhantomData,
        },
        pa3: Pin {
            port: Port::A,
            pin: 3,
            _mode: PhantomData,
        },
        pa4: Pin {
            port: Port::A,
            pin: 4,
            _mode: PhantomData,
        },
        pa5: Pin {
            port: Port::A,
            pin: 5,
            _mode: PhantomData,
        },
        pa6: Pin {
            port: Port::A,
            pin: 6,
            _mode: PhantomData,
        },
        pa7: Pin {
            port: Port::A,
            pin: 7,
            _mode: PhantomData,
        },
        pa8: Pin {
            port: Port::A,
            pin: 8,
            _mode: PhantomData,
        },
        pa9: Pin {
            port: Port::A,
            pin: 9,
            _mode: PhantomData,
        },
        pa10: Pin {
            port: Port::A,
            pin: 10,
            _mode: PhantomData,
        },
        pa11: Pin {
            port: Port::A,
            pin: 11,
            _mode: PhantomData,
        },
        pa12: Pin {
            port: Port::A,
            pin: 12,
            _mode: PhantomData,
        },
        pa13: Pin {
            port: Port::A,
            pin: 13,
            _mode: PhantomData,
        },
        pa14: Pin {
            port: Port::A,
            pin: 14,
            _mode: PhantomData,
        },
        pa15: Pin {
            port: Port::A,
            pin: 15,
            _mode: PhantomData,
        },
    }
}
