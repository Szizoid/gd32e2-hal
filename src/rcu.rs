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
}
