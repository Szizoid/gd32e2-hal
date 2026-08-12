//! GPIO modes and pin state.
//!
//! PA1 is a push-pull output that toggles; PA4 is an open-drain output; PA0 is
//! an input with a pull-up whose level is logged. PA1 is then locked and keeps
//! toggling, showing a `Locked` pin still drives.
//!
//! PA5, PA6 and PB0 are erased and walked over in a loop — a chase pattern
//! across two ports, which is what erasure buys: their types differ until
//! `erase()`, and no array can hold them before it.
//!
//! Covers: `into_input`/`into_push_pull_output`/`into_open_drain_output`/
//! `into_analog`, `set_pull`, `set_speed`, `lock`, `erase`, and the inherent
//! `set_high`/`toggle`/`is_set_high`/`is_low` accessors on both plain and erased
//! pins. The same pins also implement `OutputPin`/`StatefulOutputPin`/`InputPin`
//! for portable drivers; those return `Result` and are reached by importing the
//! trait.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_halt as _;

use gd32e2_hal::gpio::{Pull, Speed};
use gd32e2_hal::pac;
use gd32e2_hal::prelude::*;
use gd32e2_hal::rcu::{ClockConfig, PllFreq, SysClk};

#[entry]
fn main() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut rcu = dp.rcu.constrain();
    // Nothing here reads the frequencies, but the clock tree still has to be
    // frozen: `split` needs the GPIO port clocked.
    let _clocks = ClockConfig::default()
        .sysclk(SysClk::Pll(PllFreq::Mhz48))
        .freeze(&mut rcu, &mut dp.fmc);

    let gpioa = dp.gpioa.split(&mut rcu);
    let gpiob = dp.gpiob.split(&mut rcu);

    // Input with a pull-up.
    let button = gpioa.pa0.into_input();
    button.set_pull(Pull::Up);

    // Push-pull output at high slew rate.
    let led = gpioa.pa1.into_push_pull_output();
    led.set_speed(Speed::Mhz50);

    // Open-drain output (e.g. a shared line); high = released.
    let od = gpioa.pa4.into_open_drain_output();
    od.set_high();

    // Analog just shows the transition here — reading it is the ADC example.
    let _ain = gpioa.pa2.into_analog();

    defmt::info!("GPIO test start");

    // A few toggles while the pin is still freely reconfigurable.
    for _ in 0..3 {
        led.toggle();
        let lit = led.is_set_high();
        let pressed = button.is_low(); // active-low with the pull-up
        defmt::info!("led={} button_pressed={}", lit, pressed);
    }

    // Three outputs of two different ports, each a distinct type until erased.
    // After `erase()` they share one, which is what lets them into an array.
    let chase = [
        gpioa.pa5.into_push_pull_output().erase(),
        gpioa.pa6.into_push_pull_output().erase(),
        gpiob.pb0.into_push_pull_output().erase(),
    ];
    for pin in &chase {
        pin.set_low();
    }

    // Lock PA1: the configuration is frozen until reset, but a Locked output
    // still drives — `toggle` keeps working.
    let led = led.lock();

    let mut lit = 0;
    loop {
        led.toggle();

        chase[lit].set_low();
        lit = (lit + 1) % chase.len();
        chase[lit].set_high();
        let pin = &chase[lit];
        defmt::info!(
            "lit P{}{} driven={}",
            pin.port(),
            pin.number(),
            pin.is_set_high()
        );

        for _ in 0..1_000_000 {
            cortex_m::asm::nop();
        }
    }
}
