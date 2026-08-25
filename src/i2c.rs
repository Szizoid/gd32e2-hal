//! I²C master.
//!
//! Blocking, 7-bit addressing, both peripherals. Transactions are `write`,
//! `read` and `write_read` (the last joined by a repeated START); both lines
//! must be open-drain, which the pin bounds enforce.
//!
//! ```ignore
//! let sda = parts.pb7.into_alternate_open_drain::<1>();
//! let scl = parts.pb6.into_alternate_open_drain::<1>();
//! let mut i2c = I2c::new(&mut rcu, dp.i2c0, sda, scl, &clocks,
//!                        I2cMode::standard(100.kHz()));
//! ```

use core::ops::Deref;

use embedded_hal::i2c::{ErrorKind, ErrorType, NoAcknowledgeSource, Operation};

use crate::gpio::{Alternate, OpenDrain, Pin};
use crate::pac;
use crate::rcu::{Clocks, Enable, Rcu, Reset};
use crate::time::Hertz;

/// Smallest `CLKC` the hardware honours.
const CLKC_MIN_STANDARD: u32 = 4;
const CLKC_MIN_FAST: u32 = 1;

/// Slowest `pclk1` each mode can be driven from, per the manual.
const MIN_PCLK1_STANDARD_HZ: u32 = 2_000_000;
const MIN_PCLK1_FAST_HZ: u32 = 8_000_000;
const MIN_PCLK1_FAST_PLUS_HZ: u32 = 24_000_000;

/// Longest SCL rise time the I²C specification allows per mode, in nanoseconds
/// (NXP UM10204) — a property of the pulled-up line, not of the controller.
const RISE_TIME_STANDARD_NS: u32 = 1000;
const RISE_TIME_FAST_NS: u32 = 300;
const RISE_TIME_FAST_PLUS_NS: u32 = 120;

/// A peripheral that [`I2c`] can drive.
///
/// I2C0 and I2C1 share one register block layout, so the driver is written once
/// over [`Deref`]; the supertraits are what the constructor needs.
pub trait Instance: Deref<Target = pac::i2c0::RegisterBlock> + Enable + Reset {}

impl Instance for pac::I2c0 {}
impl Instance for pac::I2c1 {}

/// Marks a pin usable as `SDA` for `I2C`, in the right alternate function.
pub trait SdaPin<I2C> {}
/// Marks a pin usable as `SCL` for `I2C`, in the right alternate function.
pub trait SclPin<I2C> {}

macro_rules! i2c_pins {
    ( $( $I2C:ty:
        SDA: [ $($(#[$sda_cfg:meta])? $sda_p:literal $sda_n:literal : $sda_af:literal),* $(,)? ]
        SCL: [ $($(#[$scl_cfg:meta])? $scl_p:literal $scl_n:literal : $scl_af:literal),* $(,)? ]
    ),* $(,)? ) => {
        $(
            $($(#[$sda_cfg])? impl SdaPin<$I2C> for Pin<$sda_p, $sda_n, Alternate<$sda_af, OpenDrain>> {})*
            $($(#[$scl_cfg])? impl SclPin<$I2C> for Pin<$scl_p, $scl_n, Alternate<$scl_af, OpenDrain>> {})*
        )*
    };
}

// PB10/PB11 at AF1 belong to a *different* I²C depending on the chip variant
// (datasheet Table 2-14 footnotes): I2C0 on GD32E230x4, I2C1 on GD32E230x8.
// They are therefore listed in the gated blocks, not here.
//
// The `pads_ge_*` gates say the package bonds the pin at all, and match the ones in
// `gpio::Parts` — an entry for an unbonded pad would advertise in the docs a pin
// nobody can obtain.
i2c_pins!(
    pac::I2c0:
        SDA: ['A' 10 : 4, #[cfg(pads_ge_24)] 'B' 7 : 1, #[cfg(pads_ge_48)] 'B' 9 : 1]
        SCL: ['A' 9 : 4, #[cfg(pads_ge_24)] 'B' 6 : 1, #[cfg(pads_ge_qfn32)] 'B' 8 : 1],
);

// ---- (1) GD32E230x4 only: PB10/PB11 AF1 are I2C0 ----
#[cfg(chip_x4)]
i2c_pins!(
    pac::I2c0:
        SDA: [#[cfg(pads_ge_48)] 'B' 11 : 1]
        SCL: [#[cfg(pads_ge_48)] 'B' 10 : 1],
);

// ---- (3) GD32E230x8 only: I2C1 exists, and PB10/PB11 AF1 belong to it ----
#[cfg(chip_x8)]
i2c_pins!(
    pac::I2c1:
        SDA: ['A' 1 : 4, #[cfg(pads_ge_lqfp32)] 'A' 12 : 5, #[cfg(pads_ge_48)] 'B' 11 : 1, #[cfg(pads_ge_48)] 'B' 14 : 5]
        SCL: ['A' 0 : 4, #[cfg(pads_ge_lqfp32)] 'A' 11 : 5, #[cfg(pads_ge_48)] 'B' 10 : 1, #[cfg(pads_ge_48)] 'B' 13 : 5]
);

/// Writes the bus timing to `CTL1`, `CKCFG` and `RT`, leaving the peripheral enabled.
///
/// `I2CCLK` takes `pclk1` in whole megahertz: `RISETIME` and the analog filter
/// are measured in real time, so the absolute frequency is needed, not a divider.
/// `CKCFG` and `I2CCLK` are only taken while `I2CEN` is clear, so the peripheral
/// is disabled for the duration.
///
/// # Panics
///
/// If `pclk1` is below the minimum the mode needs (2 / 8 / 24 MHz), or if the
/// requested frequency puts `CLKC` below what the hardware honours.
fn apply_config<I2C: Instance>(i2c: &I2C, mode: I2cMode, pclk1: Hertz) {
    let pclk1 = pclk1.to_Hz();
    let pclk1_mhz = pclk1 / 1_000_000;
    // t_rise / T_pclk1 + 1, and `1 ns × pclk1` is exactly `pclk1` in MHz.
    let risetime = |t_rise_ns: u32| (pclk1_mhz * t_rise_ns / 1_000 + 1) as u8;

    i2c.ctl0().modify(|_, w| w.i2cen().disabled());
    i2c.ctl1()
        .modify(|_, w| unsafe { w.i2cclk().bits(pclk1_mhz as u8) });

    match mode {
        // Both halves of the period are `CLKC` cycles long, so the period is
        // twice that.
        I2cMode::Standard { frequency } => {
            assert!(
                pclk1 >= MIN_PCLK1_STANDARD_HZ,
                "I2C standard mode needs pclk1 of at least 2 MHz"
            );
            let clkc = pclk1 / (2 * frequency.to_Hz());
            assert!(
                clkc >= CLKC_MIN_STANDARD,
                "I2C frequency too high for this pclk1"
            );
            i2c.rt()
                .write(|w| w.risetime().bits(risetime(RISE_TIME_STANDARD_NS)));
            i2c.ckcfg()
                .write(|w| w.fast().standard().clkc().bits(clkc as u16));
        }
        I2cMode::Fast {
            frequency,
            duty_cycle,
        } => {
            assert!(
                pclk1 >= MIN_PCLK1_FAST_HZ,
                "I2C fast mode needs pclk1 of at least 8 MHz"
            );
            i2c.rt()
                .write(|w| w.risetime().bits(risetime(RISE_TIME_FAST_NS)));
            write_fast_ckcfg(i2c, pclk1, frequency, duty_cycle);
        }
        I2cMode::FastPlus {
            frequency,
            duty_cycle,
        } => {
            assert!(
                pclk1 >= MIN_PCLK1_FAST_PLUS_HZ,
                "I2C fast mode plus needs pclk1 of at least 24 MHz"
            );
            i2c.rt()
                .write(|w| w.risetime().bits(risetime(RISE_TIME_FAST_PLUS_NS)));
            write_fast_ckcfg(i2c, pclk1, frequency, duty_cycle);
            i2c.fmpcfg().write(|w| w.fmpen().set_bit());
        }
    }

    i2c.ctl0().modify(|_, w| w.i2cen().enabled());
}

/// Writes `CKCFG` for the two fast modes, which share their formulas.
///
/// The duty cycle splits the period into `2 + 1` or `16 + 9` parts of `CLKC`
/// cycles each.
fn write_fast_ckcfg<I2C: Instance>(i2c: &I2C, pclk1: u32, frequency: Hertz, duty_cycle: DutyCycle) {
    let parts = match duty_cycle {
        DutyCycle::Ratio2to1 => 3,
        DutyCycle::Ratio16to9 => 25,
    };
    let clkc = pclk1 / (parts * frequency.to_Hz());
    assert!(
        clkc >= CLKC_MIN_FAST,
        "I2C frequency too high for this pclk1"
    );
    i2c.ckcfg().write(|w| {
        let w = match duty_cycle {
            DutyCycle::Ratio2to1 => w.dtcy().duty2(),
            DutyCycle::Ratio16to9 => w.dtcy().duty16_9(),
        };
        w.fast().fast().clkc().bits(clkc as u16)
    });
}

/// Ratio of the low to the high half of an SCL period (`DTCY`).
///
/// Only the fast modes can shape it, so it is a field of their variants alone.
#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[allow(missing_docs)]
pub enum DutyCycle {
    Ratio2to1,
    Ratio16to9,
}

/// Bus speed, and whatever else that speed implies.
///
/// Each variant has its own minimum `pclk1`: 2 / 8 / 24 MHz.
#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum I2cMode {
    /// Up to 100 kHz, duty cycle fixed at 1:1.
    Standard {
        /// SCL frequency.
        frequency: Hertz,
    },
    /// Up to 400 kHz.
    Fast {
        /// SCL frequency.
        frequency: Hertz,
        /// Shape of the SCL period.
        duty_cycle: DutyCycle,
    },
    /// Up to 1 MHz; also enables the stronger line driver (`FMPCFG`).
    FastPlus {
        /// SCL frequency.
        frequency: Hertz,
        /// Shape of the SCL period.
        duty_cycle: DutyCycle,
    },
}

impl I2cMode {
    /// Standard mode at `frequency`.
    pub fn standard(frequency: Hertz) -> Self {
        Self::Standard { frequency }
    }
    /// Fast mode at `frequency`, with the given duty cycle.
    pub fn fast(frequency: Hertz, duty_cycle: DutyCycle) -> Self {
        Self::Fast {
            frequency,
            duty_cycle,
        }
    }
    /// Fast mode plus at `frequency`, with the given duty cycle.
    pub fn fast_plus(frequency: Hertz, duty_cycle: DutyCycle) -> Self {
        Self::FastPlus {
            frequency,
            duty_cycle,
        }
    }
}

/// An error the peripheral flagged in `STAT0` (manual Table 17-3).
///
/// Its own type rather than [`ErrorKind`], which has no variant for a PEC
/// mismatch or an SMBus alert; [`kind`] gives the portable classification.
///
/// [`kind`]: embedded_hal::i2c::Error::kind
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Error {
    /// A START or STOP arrived where the protocol allows none (`BERR`).
    Bus,
    /// Another master won the bus; this one fell back to slave mode (`LOSTARB`).
    ArbitrationLoss,
    /// A byte was overwritten before being read, or needed before being written
    /// (`OUERR`). Slave mode only, with clock stretching disabled.
    Overrun,
    /// Nobody acknowledged (`AERR`) — the ordinary answer from an empty address,
    /// which is what bus scans rely on.
    ///
    /// One flag covers both cases, so the source comes from the step that ran.
    NoAcknowledge(NoAcknowledgeSource),
    /// The received PEC does not match the one computed locally (`PECERR`).
    Pec,
    /// An SMBus transaction exceeded its timeout (`SMBTO`).
    SmbusTimeout,
    /// An SMBus device pulled the alert line (`SMBALT`).
    SmbusAlert,
}

impl embedded_hal::i2c::Error for Error {
    fn kind(&self) -> ErrorKind {
        match self {
            Self::Bus => ErrorKind::Bus,
            Self::ArbitrationLoss => ErrorKind::ArbitrationLoss,
            Self::Overrun => ErrorKind::Overrun,
            Self::NoAcknowledge(source) => ErrorKind::NoAcknowledge(*source),
            // Nothing upstream describes these three.
            Self::Pec | Self::SmbusTimeout | Self::SmbusAlert => ErrorKind::Other,
        }
    }
}

/// A configured I²C master, owning the peripheral and its two pins.
///
/// Both lines are open-drain and pulled up externally. There is no chip select:
/// the target is named by the first byte of every transaction.
pub struct I2c<I2CX, SDA, SCL> {
    i2c: I2CX,
    sda_pin: SDA,
    scl_pin: SCL,
}

impl<I2CX, SDA, SCL> I2c<I2CX, SDA, SCL>
where
    I2CX: Instance,
    SDA: SdaPin<I2CX>,
    SCL: SclPin<I2CX>,
{
    /// Enables the peripheral's clock, resets it and applies `mode`.
    ///
    /// The pins must already be open-drain in this I²C's alternate function; the
    /// bounds reject anything else at compile time. [`release`](Self::release)
    /// hands them back. `clocks` supplies `pclk1`, which goes into `I2CCLK`.
    ///
    /// # Panics
    ///
    /// If `pclk1` is too slow for `mode` (2 / 8 / 24 MHz), or if `mode`'s
    /// frequency is unreachable from it.
    pub fn new(
        rcu: &mut Rcu,
        i2c: I2CX,
        sda_pin: SDA,
        scl_pin: SCL,
        clocks: &Clocks,
        mode: I2cMode,
    ) -> Self {
        I2CX::enable(rcu);
        I2CX::reset(rcu);

        apply_config(&i2c, mode, clocks.pclk1());
        Self {
            i2c,
            sda_pin,
            scl_pin,
        }
    }
    /// Returns the peripheral and both pins.
    ///
    /// The clock is left enabled and no reset is performed — a later `new()`
    /// does both anyway.
    pub fn release(self) -> (I2CX, SDA, SCL) {
        (self.i2c, self.sda_pin, self.scl_pin)
    }
}

impl<I2CX, SDA, SCL> I2c<I2CX, SDA, SCL>
where
    I2CX: Instance,
{
    /// Releases the bus with a STOP condition.
    ///
    /// Every path out of a transaction goes through here, failures included: a
    /// skipped STOP leaves the master holding SCL, and the bus never goes idle
    /// again.
    fn stop(&mut self) {
        self.i2c.ctl0().modify(|_, w| w.stop().stop());
    }

    /// Waits until no other master is using the bus.
    ///
    /// Only the first START needs this: `I2CBSY` stays set while this master owns
    /// the bus, so waiting again before a repeated START would wait forever.
    fn wait_idle(&self) {
        while self.i2c.stat1().read().i2cbsy().is_busy() {}
    }
    /// Sends START and the address with `read` as its direction bit, then waits
    /// for the device to answer.
    ///
    /// On a bus this master already holds, the START is a repeated one — hence no
    /// [`wait_idle`](Self::wait_idle) here. Returns with `ADDSEND` still raised:
    /// a short read has to write `ACKEN` before
    /// [`clear_addsend`](Self::clear_addsend) opens the data phase. Failures
    /// release the bus, so callers can propagate with `?`.
    fn start(&mut self, address: u8, read: bool) -> Result<(), Error> {
        self.i2c.ctl0().modify(|_, w| w.start().start());

        while self.i2c.stat0().read().sbsend().is_no_start() {
            if let Some(err) = self.take_error(NoAcknowledgeSource::Unknown) {
                self.stop();
                return Err(err);
            }
        }
        // Writing DATA is the second half of clearing SBSEND; the loop above
        // did the first by reading STAT0.
        self.i2c
            .data()
            .write(|w| w.trb().bits(address << 1 | read as u8));

        while self.i2c.stat0().read().addsend().is_not_match() {
            if let Some(err) = self.take_error(NoAcknowledgeSource::Address) {
                self.stop();
                return Err(err);
            }
        }
        Ok(())
    }
    /// Clears `ADDSEND`, which releases the bus into the data phase.
    ///
    /// Goes down on a read of `STAT0` then a read of `STAT1`, in that order;
    /// skipping either leaves the transfer stalled.
    fn clear_addsend(&mut self) {
        self.i2c.stat0().read();
        self.i2c.stat1().read();
    }
    /// Waits for a received byte to reach `DATA`, releasing the bus on error.
    fn wait_rbne(&mut self) -> Result<(), Error> {
        while self.i2c.stat0().read().rbne().is_empty() {
            if let Some(err) = self.take_error(NoAcknowledgeSource::Data) {
                self.stop();
                return Err(err);
            }
        }
        Ok(())
    }
    /// Waits for the byte in the shift register to finish, releasing the bus on
    /// error.
    ///
    /// While receiving, `BTC` means a byte arrived behind the one still unread in
    /// `DATA`, and the peripheral holds SCL low until software catches up.
    fn wait_btc(&mut self) -> Result<(), Error> {
        while self.i2c.stat0().read().btc().is_not_finished() {
            if let Some(err) = self.take_error(NoAcknowledgeSource::Data) {
                self.stop();
                return Err(err);
            }
        }
        Ok(())
    }
    /// Takes the received byte out of `DATA`, which also clears `RBNE`.
    fn read_data(&mut self) -> u8 {
        self.i2c.data().read().trb().bits()
    }

    /// Returns the first pending error, if any, clearing its flag.
    ///
    /// `source` says which step is running: one `AERR` covers both a dead address
    /// and an unacknowledged data byte.
    ///
    /// `STAT0` flags are `rc_w0`, so clearing goes through `modify` — a `write`
    /// would zero the flags it does not name and wipe them all.
    fn take_error(&mut self, source: NoAcknowledgeSource) -> Option<Error> {
        let stat = self.i2c.stat0().read();
        if stat.berr().is_error() {
            self.i2c.stat0().modify(|_, w| w.berr().no_error());
            Some(Error::Bus)
        } else if stat.lostarb().is_lost() {
            self.i2c.stat0().modify(|_, w| w.lostarb().no_lost());
            Some(Error::ArbitrationLoss)
        } else if stat.aerr().is_error() {
            self.i2c.stat0().modify(|_, w| w.aerr().no_error());
            Some(Error::NoAcknowledge(source))
        } else if stat.ouerr().is_overrun() {
            self.i2c.stat0().modify(|_, w| w.ouerr().no_overrun());
            Some(Error::Overrun)
        } else if stat.pecerr().is_error() {
            self.i2c.stat0().modify(|_, w| w.pecerr().no_error());
            Some(Error::Pec)
        } else if stat.smbto().is_timeout() {
            self.i2c.stat0().modify(|_, w| w.smbto().no_timeout());
            Some(Error::SmbusTimeout)
        } else if stat.smbalt().is_alert() {
            self.i2c.stat0().modify(|_, w| w.smbalt().no_alert());
            Some(Error::SmbusAlert)
        } else {
            None
        }
    }

    /// Writes `bytes` to the device at `address`, framed by START and STOP.
    ///
    /// `address` is the plain 7-bit value from the datasheet; the direction bit
    /// is appended here. Blocks until the last byte is out and acknowledged, so
    /// the bus is idle on return. An unanswered address is
    /// [`Error::NoAcknowledge`] with [`NoAcknowledgeSource::Address`], which is
    /// how a bus scan finds devices.
    ///
    /// # Panics
    ///
    /// If `address` does not fit in seven bits (10-bit addressing is not
    /// supported). Checked before the bus is touched.
    pub fn write(&mut self, address: u8, bytes: &[u8]) -> Result<(), Error> {
        assert!(address <= 0x7F, "I2C address must be 7-bit");

        self.wait_idle();
        self.write_phase(address, bytes)?;
        self.stop();
        Ok(())
    }

    /// Addresses the device for writing and clocks `bytes` out, leaving the bus
    /// held: [`write`](Self::write) ends it with STOP,
    /// [`write_read`](Self::write_read) with a repeated START.
    fn write_phase(&mut self, address: u8, bytes: &[u8]) -> Result<(), Error> {
        self.start(address, false)?;
        self.clear_addsend();
        self.write_bytes(bytes)?;

        // With nothing sent there is no transfer to complete, and BTC never comes.
        if !bytes.is_empty() {
            self.wait_btc()?;
        }
        Ok(())
    }

    /// Clocks `bytes` out of an already addressed device.
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), Error> {
        for byte in bytes {
            while self.i2c.stat0().read().tbe().is_not_empty() {
                if let Some(err) = self.take_error(NoAcknowledgeSource::Data) {
                    self.stop();
                    return Err(err);
                }
            }
            self.i2c.data().write(|w| w.trb().bits(*byte));
        }
        Ok(())
    }
    /// Reads `bytes.len()` bytes from the device at `address`, framed by START
    /// and STOP.
    ///
    /// A read ends by withholding the acknowledge of the last byte, and `ACKEN`
    /// governs the byte already on the wire — one ahead of the one being read.
    /// Hence separate sequences for one and two bytes; longer reads leave a byte
    /// unread so the peripheral stretches SCL while `ACKEN` is cleared (the
    /// manual's "Solution B" — "Solution A" has to react within the last byte's
    /// transfer, which an interrupt can break).
    ///
    /// An empty `bytes` is a no-op: a zero-byte transfer has no acknowledge to
    /// end on.
    ///
    /// # Panics
    ///
    /// If `address` does not fit in seven bits.
    pub fn read(&mut self, address: u8, bytes: &mut [u8]) -> Result<(), Error> {
        assert!(address <= 0x7F, "I2C address must be 7-bit");
        if bytes.is_empty() {
            return Ok(());
        }

        self.wait_idle();
        self.read_phase(address, bytes)
    }

    /// Addresses the device for reading and fills `bytes`, ending with STOP.
    fn read_phase(&mut self, address: u8, bytes: &mut [u8]) -> Result<(), Error> {
        let n = bytes.len();
        self.read_slots(address, n, bytes.iter_mut())
    }

    /// The read phase over `n` byte slots, wherever they live: [`read`](Self::read)
    /// passes one slice, [`transaction`] spreads one run of received bytes over
    /// several buffers. `n` has to be known upfront — it decides where the NAK
    /// falls.
    ///
    /// [`transaction`]: embedded_hal::i2c::I2c::transaction
    fn read_slots<'a>(
        &mut self,
        address: u8,
        n: usize,
        mut slots: impl Iterator<Item = &'a mut u8>,
    ) -> Result<(), Error> {
        // Every `next()` below is matched against a slot counted in `n`.
        match n {
            0 => Ok(()),
            1 => {
                self.start(address, true)?;
                // The byte starts arriving the moment ADDSEND goes down, so the
                // NAK has to stand before that.
                self.i2c.ctl0().modify(|_, w| w.acken().nak());
                self.clear_addsend();
                self.stop();

                self.wait_rbne()?;
                if let Some(slot) = slots.next() {
                    *slot = self.read_data();
                }
                Ok(())
            }
            2 => {
                // POAP moves the acknowledge one byte along, so the NAK lands on
                // the second byte. Must be set before START.
                self.i2c.ctl0().modify(|_, w| w.poap().next());
                let result = self.read_two(address, slots);
                self.i2c.ctl0().modify(|_, w| w.poap().current());
                result
            }
            _ => {
                self.i2c.ctl0().modify(|_, w| w.acken().ack());
                self.start(address, true)?;
                self.clear_addsend();

                // Up to the last three bytes: take each one as it lands.
                for _ in 0..n - 3 {
                    self.wait_rbne()?;
                    if let Some(slot) = slots.next() {
                        *slot = self.read_data();
                    }
                }

                // Byte N-2 is left in DATA on purpose: once N-1 arrives behind it
                // the peripheral holds SCL, so this ACKEN cannot be late.
                self.wait_btc()?;
                self.i2c.ctl0().modify(|_, w| w.acken().nak());
                if let Some(slot) = slots.next() {
                    *slot = self.read_data();
                }

                // Reading N-2 let the last byte in; SCL is stretched again, and
                // STOP goes out before it is read.
                self.wait_btc()?;
                self.stop();
                for slot in slots {
                    *slot = self.read_data();
                }
                Ok(())
            }
        }
    }

    /// The two-byte read, split out so `POAP` is restored on every path.
    fn read_two<'a>(
        &mut self,
        address: u8,
        slots: impl Iterator<Item = &'a mut u8>,
    ) -> Result<(), Error> {
        self.start(address, true)?;
        self.i2c.ctl0().modify(|_, w| w.acken().nak());
        self.clear_addsend();

        // Neither byte is read until both have arrived, and STOP goes out before
        // either leaves DATA.
        self.wait_btc()?;
        self.stop();
        for slot in slots {
            *slot = self.read_data();
        }
        Ok(())
    }

    /// Writes `write`, then reads into `read`, joined by a repeated START.
    ///
    /// This is how a register-addressed device is read: the write phase moves its
    /// pointer, the read phase takes what the pointer names. A
    /// [`write`](Self::write) then a [`read`](Self::read) is a different
    /// operation — the STOP between them lets another master move the pointer,
    /// and the wrong register reads back as valid data.
    ///
    /// An empty `read` degenerates to [`write`](Self::write), an empty `write`
    /// to [`read`](Self::read).
    ///
    /// # Panics
    ///
    /// If `address` does not fit in seven bits.
    pub fn write_read(&mut self, address: u8, write: &[u8], read: &mut [u8]) -> Result<(), Error> {
        assert!(address <= 0x7F, "I2C address must be 7-bit");

        self.wait_idle();
        self.write_phase(address, write)?;

        if read.is_empty() {
            self.stop();
            return Ok(());
        }
        // No STOP in between: the read phase's START is a repeated one.
        self.read_phase(address, read)
    }
}

impl<I2CX, SDA, SCL> ErrorType for I2c<I2CX, SDA, SCL>
where
    I2CX: Instance,
{
    type Error = Error;
}

/// Whether two operations run in the same direction, and so belong to one
/// addressed phase.
fn same_direction(a: &Operation<'_>, b: &Operation<'_>) -> bool {
    matches!(
        (a, b),
        (Operation::Read(_), Operation::Read(_)) | (Operation::Write(_), Operation::Write(_))
    )
}

impl<I2CX, SDA, SCL> embedded_hal::i2c::I2c for I2c<I2CX, SDA, SCL>
where
    I2CX: Instance,
{
    /// Per the trait's contract: neighbouring operations of the same direction
    /// join into one addressed phase, a change of direction inserts a repeated
    /// START, and only the end carries a STOP.
    ///
    /// # Panics
    ///
    /// If `address` does not fit in seven bits, or if a `Read` is followed by
    /// further operations — ending a read early needs a repeated START in place
    /// of its STOP, which the read sequences here do not express. `[Write.., Read]`
    /// works.
    fn transaction(&mut self, address: u8, operations: &mut [Operation<'_>]) -> Result<(), Error> {
        assert!(address <= 0x7F, "I2C address must be 7-bit");
        if operations.is_empty() {
            return Ok(());
        }
        let phases = operations.chunk_by(same_direction).count();

        self.wait_idle();
        for (i, phase) in operations.chunk_by_mut(same_direction).enumerate() {
            let last = i + 1 == phases;
            match phase.first() {
                Some(Operation::Write(_)) => {
                    self.start(address, false)?;
                    self.clear_addsend();
                    let mut sent = false;
                    for op in phase.iter() {
                        if let Operation::Write(bytes) = op {
                            self.write_bytes(bytes)?;
                            sent |= !bytes.is_empty();
                        }
                    }
                    if sent {
                        self.wait_btc()?;
                    }
                    if last {
                        self.stop();
                    }
                }
                Some(Operation::Read(_)) => {
                    assert!(last, "I2C read must be the last operation of a transaction");
                    let n = phase
                        .iter()
                        .map(|op| match op {
                            Operation::Read(buf) => buf.len(),
                            Operation::Write(_) => 0,
                        })
                        .sum();
                    let slots = phase
                        .iter_mut()
                        .filter_map(|op| match op {
                            Operation::Read(buf) => Some(buf),
                            Operation::Write(_) => None,
                        })
                        .flat_map(|buf| buf.iter_mut());
                    self.read_slots(address, n, slots)?;
                }
                None => {}
            }
        }
        Ok(())
    }

    fn read(&mut self, address: u8, bytes: &mut [u8]) -> Result<(), Error> {
        self.read(address, bytes)
    }
    fn write(&mut self, address: u8, bytes: &[u8]) -> Result<(), Error> {
        self.write(address, bytes)
    }
    fn write_read(&mut self, address: u8, bytes: &[u8], buffer: &mut [u8]) -> Result<(), Error> {
        self.write_read(address, bytes, buffer)
    }
}
