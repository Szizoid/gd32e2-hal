//! GPIO modes and the `embedded-hal` digital traits.
//!
//! PA1 is a push-pull output that toggles; PA4 is an open-drain output; PA0 is
//! an input with a pull-up whose level is logged. PA1 is then locked and keeps
//! toggling, showing a `Locked` pin still drives.
//!
//! Covers: `into_input`/`into_push_pull_output`/`into_open_drain_output`/
//! `into_analog`, `set_pull`, `set_speed`, `lock`, and the `OutputPin` /
//! `StatefulOutputPin` / `InputPin` impls.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use embedded_hal::digital::{InputPin, OutputPin, StatefulOutputPin};
use panic_halt as _;

use gd32e2_hal::gpio::{GpioExt, Pull, Speed};
use gd32e2_hal::pac;
use gd32e2_hal::rcu::{CFGR, PllFreq, RcuExt};

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    // Nothing here reads the frequencies, but the clock tree still has to be
    // frozen: `split` needs the GPIO port clocked.
    let _clocks = CFGR::default()
        .sysclk(PllFreq::Mhz48)
        .freeze(&mut rcu, &mut dp.fmc);

    let gpioa = dp.gpioa.split(&mut rcu);

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

    defmt::info!("GPIO test start");

    // A few toggles while the pin is still freely reconfigurable.
    for _ in 0..3 {
        led.toggle().unwrap();
        let lit = led.is_set_high().unwrap();
        let pressed = button.is_low().unwrap(); // active-low with the pull-up
        defmt::info!("led={} button_pressed={}", lit, pressed);
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
