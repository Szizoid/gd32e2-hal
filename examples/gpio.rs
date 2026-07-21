//! GPIO modes and the `embedded-hal` digital traits.
//!
//! PA1 is a push-pull output that toggles; PA4 is an open-drain output; PA0 is
//! an input with a pull-up whose level is reported over USART0 (PA9/PA10) at
//! 115200 8N1. PA1 is then locked and keeps toggling, showing a `Locked` pin
//! still drives.
//!
//! Covers: `into_input`/`into_push_pull_output`/`into_open_drain_output`/
//! `into_analog`, `set_pull`, `set_speed`, `lock`, and the `OutputPin` /
//! `StatefulOutputPin` / `InputPin` impls.

#![no_std]
#![no_main]

use core::fmt::Write as _;

use cortex_m_rt::entry;
use embedded_hal::digital::{InputPin, OutputPin, StatefulOutputPin};
use panic_halt as _;

use gd32e2_hal::gpio::{GpioExt, Pull, Speed};
use gd32e2_hal::pac;
use gd32e2_hal::rcu::{CFGR, PllFreq, RcuExt};
use gd32e2_hal::usart::{Usart, UsartConfig};

struct Serial<W>(W);

impl<W: embedded_hal_nb::serial::Write<u8>> core::fmt::Write for Serial<W> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for &b in s.as_bytes() {
            let _ = nb::block!(self.0.write(b));
        }
        Ok(())
    }
}

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    let clocks = CFGR::default()
        .sysclk(PllFreq::Mhz48)
        .freeze(&mut rcu, &mut dp.fmc);

    let gpioa = dp.gpioa.split(&mut rcu);

    let tx = gpioa.pa9.into_alternate::<1>();
    let rx = gpioa.pa10.into_alternate::<1>();
    let usart0 = Usart::new(&mut rcu, dp.usart0, tx, rx, clocks, UsartConfig::default());
    let mut log = Serial(usart0);

    // Input with a pull-up.
    let mut button = gpioa.pa0.into_input();
    button.set_pull(Pull::Up);

    // Push-pull output at high slew rate.
    let mut led = gpioa.pa1.into_push_pull_output();
    led.set_speed(Speed::Mhz50);

    // Open-drain output (e.g. a shared line); high = released.
    let mut od = gpioa.pa4.into_open_drain_output();
    od.set_high().unwrap();

    // Analog just shows the transition here — reading it is the ADC example.
    let _ain = gpioa.pa2.into_analog();

    let _ = writeln!(log, "GPIO test start");

    // A few toggles while the pin is still freely reconfigurable.
    for _ in 0..3 {
        led.toggle().unwrap();
        let lit = led.is_set_high().unwrap();
        let pressed = button.is_low().unwrap(); // active-low with the pull-up
        let _ = writeln!(log, "led={} button_pressed={}", lit as u8, pressed as u8);
    }

    // Lock PA1: the configuration is frozen until reset, but a Locked output
    // still drives — `toggle` keeps working.
    let mut led = led.lock();

    loop {
        led.toggle().unwrap();
        for _ in 0..1_000_000 {
            cortex_m::asm::nop();
        }
    }
}
