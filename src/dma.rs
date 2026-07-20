//! Direct memory access: five independent channels moving data without the core.
//!
//! [`DmaExt::split`] consumes the peripheral and hands out one [`Channel`] per
//! hardware channel:
//!
//! ```ignore
//! let channels = dp.dma.split(&mut rcu);
//! let ch0 = channels.ch0;
//! ```
//!
//! A channel is a unique ownership token — the only ones in existence are those
//! `split` produced, and a transfer takes one by value for as long as it runs, so
//! a channel cannot be driving two transfers at once.
//!
//! Channels share a register block but not registers: each touches only its own
//! `CHxCTL`/`CHxCNT`/`CHxPADDR`/`CHxMADDR`, which the channel number in the type
//! guarantees. The two genuinely shared registers stay safe by construction —
//! `INTF` is only ever read, and `INTC` is write-one-to-clear, so a channel
//! clearing its own flags cannot disturb another's.

use core::marker::PhantomData;

use gd32e2::gd32e230;

use crate::rcu::Rcu;

/// One DMA channel, identified by its number at the type level.
///
/// Cannot be constructed outside this module: the only channels that exist come
/// from [`DmaExt::split`], which is what makes a channel a unique token.
pub struct Channel<const N: u8> {
    _marker: PhantomData<()>,
}

impl<const N: u8> Channel<N> {
    fn reg(&self) -> &gd32e230::dma::RegisterBlock {
        unsafe { &*gd32e230::Dma::ptr() }
    }
}

macro_rules! channels {
    ($($N:literal => $ctl:ident, $cnt:ident, $paddr:ident, $maddr:ident,
                     $ftfif:ident, $errif:ident, $gifc:ident;)+) => {
        $(
            impl Channel<$N> {
                /// Points the channel at the peripheral data register.
                pub(crate) fn set_paddr(&mut self, addr: u32) {
                    self.reg().$paddr().write(|w| unsafe { w.bits(addr) });
                }

                /// Points the channel at the memory buffer.
                pub(crate) fn set_maddr(&mut self, addr: u32) {
                    self.reg().$maddr().write(|w| unsafe { w.bits(addr) });
                }

                /// Sets how many transfers the channel performs.
                pub(crate) fn set_cnt(&mut self, cnt: u16) {
                    self.reg().$cnt().write(|w| w.cnt().bits(cnt));
                }

                /// Transfers still outstanding; counts down as the channel runs.
                pub(crate) fn cnt(&self) -> u16 {
                    self.reg().$cnt().read().cnt().bits()
                }

                /// Starts or stops the channel.
                pub(crate) fn set_enabled(&mut self, enabled: bool) {
                    self.reg().$ctl().modify(|_, w| w.chen().bit(enabled));
                }

                /// Whether the channel has finished its last transfer.
                pub(crate) fn is_complete(&self) -> bool {
                    self.reg().intf().read().$ftfif().bit_is_set()
                }

                /// Whether the channel hit a bus error.
                pub(crate) fn is_error(&self) -> bool {
                    self.reg().intf().read().$errif().bit_is_set()
                }

                /// Clears every interrupt flag of this channel.
                ///
                /// `INTC` is write-one-to-clear, so writing zeroes into the other
                /// channels' bits leaves them alone — this must never become a
                /// `modify`, which would read and write back their flags too.
                pub(crate) fn clear_flags(&mut self) {
                    self.reg().intc().write(|w| w.$gifc().set_bit());
                }
            }
        )+
    };
}

channels! {
    0 => ch0ctl, ch0cnt, ch0paddr, ch0maddr, ftfif0, errif0, gifc0;
    1 => ch1ctl, ch1cnt, ch1paddr, ch1maddr, ftfif1, errif1, gifc1;
    2 => ch2ctl, ch2cnt, ch2paddr, ch2maddr, ftfif2, errif2, gifc2;
    3 => ch3ctl, ch3cnt, ch3paddr, ch3maddr, ftfif3, errif3, gifc3;
    4 => ch4ctl, ch4cnt, ch4paddr, ch4maddr, ftfif4, errif4, gifc4;
}

/// The DMA channels, as handed out by [`DmaExt::split`].
#[allow(missing_docs)]
pub struct Channels {
    pub ch0: Channel<0>,
    pub ch1: Channel<1>,
    pub ch2: Channel<2>,
    pub ch3: Channel<3>,
    pub ch4: Channel<4>,
}

/// Splits the DMA peripheral into its individual channels.
pub trait DmaExt {
    /// The channels this peripheral hands out.
    type Channels;

    /// Switches the DMA clock on and hands out the channels.
    ///
    /// Consumes the peripheral, so the channels it returns are the only ones
    /// that will ever exist.
    fn split(self, rcu: &mut Rcu) -> Self::Channels;
}

impl DmaExt for gd32e230::Dma {
    type Channels = Channels;
    fn split(self, rcu: &mut Rcu) -> Self::Channels {
        <gd32e230::Dma as crate::rcu::Enable>::enable(rcu);
        Channels {
            ch0: Channel {
                _marker: PhantomData,
            },
            ch1: Channel {
                _marker: PhantomData,
            },
            ch2: Channel {
                _marker: PhantomData,
            },
            ch3: Channel {
                _marker: PhantomData,
            },
            ch4: Channel {
                _marker: PhantomData,
            },
        }
    }
}
