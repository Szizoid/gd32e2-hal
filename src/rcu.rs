use gd32e2::gd32e230;

pub struct Rcu {
    rcu: gd32e230::Rcu, // владеем сырым перифералом
}

pub trait RcuExt {
    fn constrain(self) -> Rcu;
}

impl RcuExt for gd32e230::Rcu {
    fn constrain(self) -> Rcu {
        Rcu { rcu: self }
    }
}

impl Rcu {
    pub fn enable_gpioa(&mut self) {
        self.rcu.ahben().modify(|_, w| w.paen().enabled());
    }
    pub fn disable_gpioa(&mut self) {
        self.rcu.ahben().modify(|_, w| w.paen().disabled());
    }
    pub fn enable_gpiob(&mut self) {
        self.rcu.ahben().modify(|_, w| w.pben().enabled());
    }
    pub fn disable_gpiob(&mut self) {
        self.rcu.ahben().modify(|_, w| w.pben().disabled());
    }
}
