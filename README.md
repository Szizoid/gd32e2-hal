# gd32e2-hal

[English](#english) · [Русский](#русский)

---

## English

A hardware abstraction layer for the **GD32E230K8U6** (Cortex-M23), written in
Rust from scratch on top of the [`gd32e2`](https://crates.io/crates/gd32e2) PAC.

> ⚠️ **Work in progress.** Written by hand, incrementally; the API is unstable.
> The package is a library (`src/lib.rs` → `adc`, `dma`, `gpio`, `i2c`, `prelude`,
> `rcu`, `spi`, `time`, `timer`, `usart`) plus on-hardware binaries in `examples/`.
> All 16 examples have been flashed and verified on the board — RCU, GPIO, USART
> (8/9-bit and parity), SPI0/SPI1, ADC, a one-shot DMA transfer, TIMER, blocking
> delays, PWM, input capture, I²C, RTT.

### Principles

- **Errors at compile time, not on the board.** Pin identity and mode live in the
  type: `Pin<'A', 5, Input>` has no `set_high`, an invalid AF number doesn't
  compile, and ownership prevents reconfiguring a pin twice or using a port
  before its clock is on.
- **Zero-cost.** The same register writes as hand-written PAC code; `Pin` is a ZST.
- **`#![no_std]`, no heap.**

### Chip variants

One feature names the part, and exactly one must be enabled — zero or several is
an error rather than a silently truncated pin map. **There is no default**: which
part is on a board is not something this crate can assume. The letter is the
bonded pin count, the digit the flash code. Package and temperature suffixes are
not in the name: `K8U6` and `K8T6` are one die in two packages.

| feature | pins | flash | SRAM |
| --- | --- | --- | --- |
| `gd32e230f4` / `f6` / `f8` | 20 | 16K / 32K / 64K | 4K / 6K / 8K |
| `gd32e230e8` | 24 | 64K | 8K |
| `gd32e230g4` / `g6` / `g8` | 28 | 16K / 32K / 64K | 4K / 6K / 8K |
| `gd32e230k4` / `k6` / `k8` | 32 | 16K / 32K / 64K | 4K / 6K / 8K |
| `gd32e230c4` / `c6` / `c8` | 48 | 16K / 32K / 64K | 4K / 6K / 8K |

Development targets the `GD32E230K8U6`, i.e. `gd32e230k8`; the examples get it
from the crate's own `[dev-dependencies]` entry.
`build.rs` turns the choice into the `memory.x` the linker needs and into the cfg
flags the source gates on: the AF map differs by flash code — the same pin at the
same AF number reaching a different peripheral (`PA2` AF1 is `USART0_TX` on x4,
`USART1_TX` on x8), datasheet Table 2-13/2-14 notes (1) x4, (2) x6/x8, (3) x8 —
while the pin count decides which pins exist. x8 is a superset of x6; where a row
is footnoted only (1) and (3), x6 has no function on that AF at all.

An orthogonal feature, **`defmt`**, derives `defmt::Format` on the public enums
and error types. Off by default; also enables `embedded-hal/defmt-03`.

### What's implemented

**GPIO** (`src/gpio.rs`) — const-generic `Pin<P, N, MODE>`, a ZST. Modes as
typestate: `Input`, `Analog`, `Output<PushPull>` / `Output<OpenDrain>`,
`Alternate<AF, OTYPE = PushPull>`, `Debugger` (`PA13`/`PA14`, left through the
`activate_into_*()` family), `Locked<MODE>` (terminal — no `unlock` in hardware
either). `dp.gpioa.split(&mut rcu)` (`GpioExt`) enables the port clock and hands
out pins. Transitions: `into_input` / `into_output` / `into_push_pull_output` /
`into_open_drain_output` / `into_analog` / `into_alternate::<AF>()` /
`into_alternate_open_drain::<AF>()`; the AF number is checked at compile time
against a per-pin `ValidAf` map, and the output type in `Alternate` is what I²C
binds to. Plus `set_pull`, `set_speed`, `lock()` (ports A/B — port F has no
`LOCK`). Pin state is inherent and `Result`-free: `set_high` / `set_low` /
`toggle` (atomic `TG`) / `is_set_high` / `is_set_low`, and `is_high` / `is_low`
on inputs and open-drain outputs; `embedded-hal` 1.0 `OutputPin` / `InputPin` /
`StatefulOutputPin` sit on the same private helpers. `erase()` gives
`ErasedPin<MODE>` — port and number as fields, so pins of any port share a type
and fit an array, with `port()` / `number()` added and the mode still in the
type. Gated by `Active`, one way only. Ports A, B and F. Which pins a port yields
follows the package: `Parts` holds only bonded pads, so `PB9` exists on a 48-pin
part and not on a 32-pin one, and reaching for it is a compile error instead of a
pin that reads nothing. Port C (`PC13`–`PC15`, 48-pin only) is not implemented
yet.

**RCU** (`src/rcu.rs`) — `dp.rcu.constrain()` (`RcuExt`). Per-peripheral `Enable`
and `Reset` traits from the `bus_en!`/`bus_rst!` macros (separate, since not every
peripheral has a reset bit — DMA has none), called from every driver's
constructor, so nothing can be used unclocked. Clock tree: a `ClockConfig` builder →
`.freeze(&mut rcu, &mut dp.fmc)` → a frozen `Clocks`. `ClockConfig::default()` is
the reset state — undivided buses, `SysClk::Irc8m`, USART0 on APB2, `AdcSel::Off` —
and every field reaches its registers whether it was named or not. PLL from IRC8M
(`PllFreq`, 8–72 MHz) and bus prescalers (`AhbPsc` / `ApbPsc`) are typed enums, so an
unreachable frequency is a compile error, not a silent rounding; flash wait states
are set from the new `hclk` before the source switch. `Usart0Sel` and `AdcSel` /
`AdcPsc` resolve into `Clocks`. `rcu.ck_out(src, div)` routes an internal clock
node onto `PA8`/`PA9` (`CkOutSrc` / `CkOutDiv`) for measurement.
`pclk1_tim` / `pclk2_tim` carry the timer branch: `hclk` at an undivided APB,
twice the bus clock otherwise. Frequencies are `fugit` aliases from `src/time.rs`
(`Hertz`), re-exported. `HXTAL` is out of scope — no crystal on this board.

**USART** (`src/usart.rs`) — `Usart<USARTX, TX, RX, WORD = Byte>` owns the
peripheral and both pins. `TxPin` / `RxPin` markers (from `usart_pins!`) reject a
wrong pin or AF at compile time; `BusClocks` picks the bus frequency per
instance, so the baud divisor cannot use the wrong one. `UsartConfig` (fluent,
`Default` = 115200 / ×16 / `N8`) carries `baud` as a `time::Bps` (`115_200.bps()`,
or the standard rates named in `usart::baud`),
`Oversampling::{X16, X8}` and `FrameFormat::{N8, E8, O8, E7, O7}`
— one source of truth for `WL`/`PCEN`/`PM`. Inherent blocking API on the `Byte`
width: `write_byte` / `write_bytes` / `read_byte` / `read_bytes`, plus `flush`
(waits for `TC`, not `TBE`); transmission has no error conditions and returns no
`Result`. `read_bytes` blocks for the first byte, then takes what is already
waiting. `read_ready` / `write_ready` poll `RBNE` / `TBE`. Trait layers on the
same width: `embedded-hal-nb` `Read<u8>` / `Write<u8>` and `embedded-io`
`Read` / `Write` / `ReadReady` / `WriteReady`, the latter supplying
`read_exact` / `write_all` / `write_fmt` as defaults. `flush`, `read_ready` and
`write_ready` exist in both layers; the inherent ones win method-call syntax, the
trait ones are reached by path. Raw 9-bit words via `new_word` / `write_word` /
`write_words` / `read_word` / `read_words` on the `Word` typestate
(`UsartConfig9`). RX errors are `usart::Error` (`Overrun` / `Noise` / `Framing` /
`Parity`), cleared through `USART_INTC`, with `ErrorKind` from the
`serial::Error` and `embedded_io::Error` impls. `release()` returns the
peripheral and both pins.

**ADC** (`src/adc.rs`) — `Adc::new(rcu, adc, clocks)`, or `dp.adc.constrain(...)`
through `AdcExt`, runs the manual's calibration sequence (`ADCON`, a
14-`CK_ADC`-cycle delay converted to core cycles, `RSTCLB`/`CLB`).
`read<PIN: Channel>(&pin, SampTime) -> u16` performs a single blocking,
software-triggered conversion. `Channel` is implemented only
for `Pin<P, N, Analog>`, so a pin must actually have gone through
`into_analog()`. Internal channels: `read_vref() -> i32` returns the real `VDDA`
in mV (derived from the factory `VREFINT_CAL` in flash, falling back to the
typical ~1.2 V VREFINT if that calibration is blank), and
`read_temperature() -> Option<i32>` returns tenths of °C — `None` when `CK_ADC`
is too fast to satisfy the sensor's minimum sampling time. Scan mode needs DMA
and is deferred.

**SPI** (`src/spi.rs`) — SPI0 **and** SPI1: master, full-duplex, blocking, 8- or
16-bit, software NSS. `Spi::new(...)` / `Spi::new_word(...)` take a `SpiConfig`
(`SpiConfig::new(psc)`; no `Default`, since an SCK divider has no universal
value, and `.mode()` / `.bit_order()` override the Mode 0 / MSB-first defaults).
Word width is a typestate: `transfer_word` and `SpiBus<u16>` don't exist on
`Spi<..., Byte>`, `transfer_byte` and `SpiBus<u8>` don't exist on
`Spi<..., Word>`. `BitOrder` is a runtime value — it changes wire serialization,
not a signature. An `Instance` trait abstracts the peripheral at the operation
level, so one generic `Spi<>` serves both despite their distinct `RegisterBlock`
types and bit-level divergence (`FF16` in `CTL0` on SPI0 vs `DZ` in `CTL1` on
SPI1, `BYTEN` derived from the width). Buffer operations are inherent —
`transfer_bytes` / `transfer_bytes_in_place` / `read_bytes` / `write_bytes` and
the `_words` variants — with the `SpiBus` impls delegating to them. Errors are
`spi::Error` (`Overrun` / `ModeFault` / `Crc` / `Framing`), `Crc` mapping to
`ErrorKind::Other`. `release()` returns the peripheral and pins. Hardware NSS and
CRC are deliberately not implemented.

**DMA** (`src/dma.rs`) — one-shot transfers, verified on hardware.
`dp.dma.split(&mut rcu)` (`DmaExt`) hands out `Channel<0>`…`Channel<4>`, each a
unique ZST token, so a channel can't drive two transfers at once. `write_to` /
`read_from` take the channel, the peripheral **and** the buffer by value and
return a `Transfer`; `wait()` is the only way back, so a buffer the hardware is
still filling cannot be read. Buffers are `&'static [W]` / `&'static mut [W]`.
`DmaSrc<N>` / `DmaDst<N>` encode the request map (Table 8-3), so a peripheral
paired with the wrong channel doesn't compile instead of silently never being
requested, and the associated `Word` derives `PWIDTH`/`MWIDTH` from the
peripheral's typestate. The shared `DmaPeriph<N>` supertrait also gates the
request line (`DENT`/`DENR`, `DMATEN`/`DMAREN`, ADC `DMA`), so the drivers know
nothing about DMA. `remaining()` and `is_error()` inspect a running transfer.
Dropping a `Transfer` stops the channel and loses the three owned parts.
Circular mode and `M2M` are deferred.

**TIMER** (`src/timer.rs`) — the counter core, verified on hardware. All seven
timers. `Timer::new(rcu, timer, clocks)`, or `dp.timer5.constrain(...)` through
`TimerExt`, clocks and resets the peripheral and records its own `CK_TIMERx`
through `Instance`, which binds each timer to its bus. `start(psc, car)` consumes
the stopped `Timer` and returns a running `CountDownTimer`, so `wait()` doesn't
exist on a timer that was never started; `stop()` goes back. `start` loads both
dividers out of their shadow registers with `UPG` and consumes the update event
that raises, so the first `wait()` measures a full interval. `wait()` blocks for
one rollover and leaves the timer running; `cnt()`, `car()` and `psc()` are read
back from the hardware, not from a remembered copy. `start_interval(5.secs())`
takes a `fugit` duration in any scale — the scale is a const generic, so `millis`
and `micros` need no conversion at the call site — and derives the dividers
against this timer's clock, in `u64` and saturating. Register access is confined
to `Instance`, so no `Deref` is needed. `into_delay()` gives a third type,
`Delay`, which promises no interval of its own: `delay(interval)` sets up the
dividers, blocks and stops the counter again, so a period set elsewhere cannot be
overwritten. `embedded-hal`'s `DelayNs` sits on it; resolution is one timer tick.
`elapsed()` converts the counter into a duration of any scale.

`into_pwm(psc, car)` / `into_pwm_interval(interval)` give a fourth type, `Pwm`,
which owns the period and hands out channels. `channel(pin)` returns a
`PwmChannel`: the channel number comes from the pin through
`ChannelPin<TIMERX, C>`, implemented only for the routes the silicon has, and the
operations from `PwmOps<C>`, implemented only for channels a timer actually has
(`TIMER5` none, `TIMER13` one). The pin comes back from `release()`. Each channel
carries its own handle taken through the `unsafe` `Instance::steal()` and touches
only its own compare register; several pins reaching one channel yield several
channels writing it. `enable()` / `disable()`, `set_duty(cv)` and `max_duty()`
are inherent, `embedded-hal`'s `SetDutyCycle` sits on top. `set_period` /
`set_period_interval` change the frequency for every channel at once, and duties
keep their tick value, not their share. `enable_output()` exists only on the
timers with a `CCHP` register (`TIMER0`, `TIMER14`, `TIMER15`, `TIMER16`), whose
outputs stay silent until `POEN` is raised.

`into_capture(psc)` gives a fifth type, `Capture`, whose counter free runs at the
full `u16` range — only the prescaler is a choice. `channel(pin, edge)` returns a
`CaptureChannel`, bound by the same `ChannelPin<TIMERX, C>` map, with operations
in `CaptureOps<C>`; `ChannelEnable<C>` carries `CHxEN`, shared by both roles.
`Edge` is `Rising` or `Falling` — both edges at once is reserved on this part.
`read()` returns `nb::Result<u16, Error>`: `WouldBlock` until an edge is latched,
`Error::Overcapture` when one overwrote a timestamp not yet read.
`interval(from, to)` converts a span into a duration of any scale, wrapping with
the counter; `select_edge` changes the edge on a live channel. `into_timer()` /
`release()` leave the role on `Pwm` and `Capture` alike. Complementary outputs,
break inputs, dead time and interrupts are not implemented.

**Prelude** (`src/prelude.rs`) — split per peripheral: `prelude::gpio` (`GpioExt`,
`OutputPin` / `InputPin` / `StatefulOutputPin`), `prelude::rcu`, `prelude::dma`,
`prelude::adc`, `prelude::spi`, `prelude::i2c`, `prelude::timer` (`TimerExt`, `DelayNs`,
`SetDutyCycle`, `block!`), `prelude::time` (the `fugit` suffixes `500.millis()`,
`100.kHz()`) and `prelude::usart`, which has `io` and `nb` for the two serial flavours —
one or the other, since their `read`/`write` land on the same type and two same-named
traits in scope make the call ambiguous (`E0034`). `use gd32e2_hal::prelude::*;` takes
everything with `usart::io`; import a submodule instead to narrow it. Traits are
re-exported as `_`, so the methods arrive without the names; types are not included.

**I²C** (`src/i2c.rs`) — master, blocking, 7-bit addressing, both peripherals.
`I2c::new(rcu, i2c, sda, scl, &clocks, mode)`
takes `I2cMode::{standard, fast, fast_plus}`, each carrying its SCL frequency and
the fast modes their `DutyCycle`; `CLKC`, `RISETIME` and `I2CCLK` are derived
from `pclk1`, which is why `&Clocks` is needed. Both pins must be
`Alternate<AF, OpenDrain>`, enforced by the `SdaPin` / `SclPin` markers.
`write` / `read` / `write_read` are inherent, the last joined by a repeated
START. Reads follow the manual's "Solution B", which stretches SCL instead of
racing software against the last byte. `embedded_hal::i2c::I2c` sits on top;
its `transaction` merges adjacent operations of one direction and panics if a
`Read` is not last. Errors are `i2c::Error`, one variant per `STAT0` flag, with
`NoAcknowledge` carrying the source. A too-slow `pclk1` or an unreachable
frequency panics in the constructor. 10-bit addressing, SMBus, slave mode and
DMA are not implemented. `examples/i2c.rs` scans the bus and reads 1, 2, 3 and 4
bytes from the first device that answers, sending nothing but a register index;
`examples/i2c-registers.rs` writes two bytes and reads them back. Both were run
at 50 kHz against an RP2040 in I²C target mode; fast and fast plus have not been
on hardware yet.

### Usage

```rust
use gd32e2_hal::gpio::GpioExt;
use gd32e2_hal::pac;
use gd32e2_hal::rcu::{ClockConfig, PllFreq, RcuExt, SysClk};
use gd32e2_hal::usart::{Usart, UsartConfig};

let mut dp = pac::Peripherals::take().unwrap();
let mut rcu = dp.rcu.constrain();
let clocks = ClockConfig::default()
    .sysclk(SysClk::Pll(PllFreq::Mhz48))     // PLL from IRC8M -> 48 MHz
    .freeze(&mut rcu, &mut dp.fmc);
let parts = dp.gpioa.split(&mut rcu);        // enables the GPIOA clock

let led = parts.pa5.into_output();
led.set_high();

let tx = parts.pa9.into_alternate::<1>();    // USART0_TX; ::<3>() wouldn't compile
let rx = parts.pa10.into_alternate::<1>();
let usart0 = Usart::new(&mut rcu, dp.usart0, tx, rx, clocks, UsartConfig::default());
if let Ok(byte) = usart0.read_byte() {
    usart0.write_byte(byte);                 // verified on hardware: echoes back
}
```

### Constraints

- **PAC-only base, no third-party HAL.**
- **Flashing and debugging over SWD** (ST-Link V2 + `probe-rs`, `PA13`/`PA14`).
  Runtime output goes over RTT on the same probe (`defmt` + `defmt-rtt`); no
  USB-serial adapter, and `PA9`/`PA10` are used only by the USART examples.
- Target `thumbv8m.base-none-eabi`; flash 64K, RAM 8K.
- `gd32e2` is generated from patched SVDs — verify field names against
  `docs/GD32E23x_User_Manual.pdf` (PDFs are kept locally, not committed).

### Building

`build.rs` writes the `memory.x` the linker needs from the selected chip feature,
so there is nothing to copy first. A `memory.x` in the project root still wins —
the linker looks there before the search paths — which is the way out for a board
this table does not describe.

```sh
cargo lib                      # library only, alias for build --features gd32e230k8
cargo be usart-echo            # compile-check one example, needs no probe
cargo bre usart-echo           # same, release profile
```

The library alone needs the part named, since nothing supplies a default and
`[dev-dependencies]` does not apply to it; an example gets one either way.

To flash, with an ST-Link on `PA13`/`PA14`:

```sh
cargo re usart-echo   # build + flash over SWD, then stay attached
```

`re` is `cargo run --release --example`; `.cargo/config.toml` points the
target's `runner` at `probe-rs run --chip GD32E230K8`, which flashes the ELF
directly (no `objcopy`, no `.bin`) and then stays attached, printing the RTT log
until Ctrl-C. The log level is fixed at compile time by `DEFMT_LOG` in
`.cargo/config.toml`. The chip keeps running without the probe, so the freed
probe can read any register live:

```sh
probe-rs reset --chip GD32E230K8              # run the firmware from reset
probe-rs read  --chip GD32E230K8 b32 0x48000014 1   # e.g. GPIOA_OCTL
```

The default feature targets the `GD32E230K8U6` (x8); for another variant:

```sh
cargo build --release --no-default-features --features gd32e230x4
```

> `rust-toolchain.toml` pins the channel and installs the target. On Windows
> without Visual Studio the host toolchain has to be GNU, otherwise build
> scripts fail for want of the MSVC linker:
> `rustup default stable-x86_64-pc-windows-gnu`.

### Roadmap

- [ ] DMA: circular mode and `M2M`.
- [ ] Timers: complementary outputs, break and dead time.
- [ ] I²C: fast and fast plus on hardware, 10-bit addressing, SMBus, slave, DMA.
- [ ] Interrupt-driven operation (NVIC infrastructure — also affects USART/SPI).
- [ ] SPI: half-duplex / single-wire modes (`BDEN`/`BDOEN`/`RO`).
- [ ] SPI: hardware NSS, CRC, TI mode, slave — low priority.
- [ ] USART: hardware flow control (`CTS`/`RTS`).
- [ ] RCU: `HXTAL` (needs an external crystal on the board).
- [ ] GPIO: port F alternate functions (no `AFSEL` register — needs its own study).
- [ ] Package / pin-count variants — a second axis, independent of x4/x6/x8:
      which pins are bonded out at all (`PC13`–`PC15`, `PF6`/`PF7` exist on
      QFN48 but not on this QFN32).
- [ ] `embedded-dma` for the DMA buffers.
- [ ] Extract the HAL into its own standalone crate/repo (not just `examples/` —
      splitting the library out entirely).

### License

Dual-licensed, at your option:

- Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT ([LICENSE-MIT](LICENSE-MIT))

Any contribution submitted for inclusion in this work is licensed the same way,
without additional terms, unless stated otherwise (Apache-2.0, section 5).

---

## Русский

HAL для микроконтроллера **GD32E230K8U6** (Cortex-M23), написанный на Rust с
нуля поверх PAC-крейта [`gd32e2`](https://crates.io/crates/gd32e2).

> ⚠️ **Работа в процессе.** Пишется вручную и постепенно; API нестабилен. Пакет —
> библиотека (`src/lib.rs` → `adc`, `dma`, `gpio`, `i2c`, `prelude`, `rcu`, `spi`, `time`,
> `timer`, `usart`) плюс тестовые бинарники на железо в `examples/`. Все 16
> примеров прошиты и проверены на плате — RCU, GPIO, USART (8/9-бит и чётность),
> SPI0/SPI1, ADC, разовая передача по DMA, TIMER, блокирующие задержки, PWM,
> input capture, I²C, RTT.

### Принципы

- **Ошибки на компиляции, а не на плате.** Идентичность и режим ноги живут в
  типе: у `Pin<'A', 5, Input>` нет `set_high`, неверный номер AF не
  компилируется, а система владения не даст перенастроить ногу дважды или
  использовать порт до включения такта.
- **Zero-cost.** Те же записи в регистры, что и ручной PAC-код; `Pin` — ZST.
- **`#![no_std]`, без кучи.**

### Варианты чипа

Партномер задаётся одной фичей, и ровно одна обязана быть включена — ноль или
несколько дают ошибку, а не молча урезанную карту пинов. **Дефолта нет**: какой
чип стоит на плате, крейт знать не может. Буква — число разваренных ног, цифра —
код флеша. Корпус и температура в имя не входят: `K8U6` и `K8T6` — один кристалл
в двух корпусах.

| фича | ноги | флеш | SRAM |
| --- | --- | --- | --- |
| `gd32e230f4` / `f6` / `f8` | 20 | 16K / 32K / 64K | 4K / 6K / 8K |
| `gd32e230e8` | 24 | 64K | 8K |
| `gd32e230g4` / `g6` / `g8` | 28 | 16K / 32K / 64K | 4K / 6K / 8K |
| `gd32e230k4` / `k6` / `k8` | 32 | 16K / 32K / 64K | 4K / 6K / 8K |
| `gd32e230c4` / `c6` / `c8` | 48 | 16K / 32K / 64K | 4K / 6K / 8K |

Разработка идёт на `GD32E230K8U6`, то есть `gd32e230k8`; примеры получают фичу из
`[dev-dependencies]` самого крейта. Из выбора `build.rs` делает
`memory.x` для линкера и cfg-флаги, которыми гейтится код: карта AF зависит от кода
флеша — одна и та же нога на одном номере AF ведёт к разной периферии (`PA2` AF1 —
`USART0_TX` у x4 и `USART1_TX` у x8), сноски datasheet Table 2-13/2-14: (1) x4,
(2) x6/x8, (3) x8 — а число ног решает, какие ноги существуют вообще. x8 —
надмножество x6; там, где строка помечена только (1) и (3), у x6 функции на этом AF
нет вовсе.

Ортогональная фича **`defmt`** вешает `defmt::Format` на публичные энумы и типы
ошибок. По умолчанию выключена; заодно включает `embedded-hal/defmt-03`.

### Что уже есть

**GPIO** (`src/gpio.rs`) — const-generic `Pin<P, N, MODE>`, ZST. Режимы как
typestate: `Input`, `Analog`, `Output<PushPull>` / `Output<OpenDrain>`,
`Alternate<AF, OTYPE = PushPull>`, `Debugger` (`PA13`/`PA14`, выход через
семейство `activate_into_*()`), `Locked<MODE>` (терминальный — `unlock` нет и в
железе). `dp.gpioa.split(&mut rcu)` (`GpioExt`) включает такт порта и раздаёт
пины. Переходы: `into_input` / `into_output` / `into_push_pull_output` /
`into_open_drain_output` / `into_analog` / `into_alternate::<AF>()` /
`into_alternate_open_drain::<AF>()`; номер AF сверяется на компиляции с per-pin
картой `ValidAf`, а тип выхода в `Alternate` — то, чем гейтится I²C. Плюс
`set_pull`, `set_speed`, `lock()` (порты A/B — у порта F нет `LOCK`). Состояние
ноги — инхерентное и без `Result`: `set_high` / `set_low` / `toggle` (атомарный
`TG`) / `is_set_high` / `is_set_low`, а `is_high` / `is_low` — у входов и выходов
open-drain; `embedded-hal` 1.0 `OutputPin` / `InputPin` / `StatefulOutputPin`
стоят на тех же приватных хелперах. `erase()` даёт `ErasedPin<MODE>` — порт и
номер полями, поэтому ноги любого порта складываются в массив; добавляются
`port()` / `number()`, режим остаётся в типе. Гейтится `Active`, обратного пути
нет. Порты A, B и F. Какие ноги отдаёт порт, зависит от корпуса: в `Parts` лежат
только разваренные площадки, поэтому `PB9` есть на 48-выводном чипе и отсутствует
на 32-выводном, а обращение к нему — ошибка компиляции, а не нога, которая ничего
не читает. Порт C (`PC13`–`PC15`, только 48 выводов) пока не реализован.

**RCU** (`src/rcu.rs`) — `dp.rcu.constrain()` (`RcuExt`). Трейты `Enable` и
`Reset` на каждую периферию из макросов `bus_en!`/`bus_rst!` (порознь, потому что
не у каждой периферии есть бит сброса — у DMA его нет), зовутся из конструктора
каждого драйвера, так что без такта не поработать. Дерево тактов: билдер `ClockConfig` →
`.freeze(&mut rcu, &mut dp.fmc)` → замороженный `Clocks`. `ClockConfig::default()` —
это состояние после сброса (шины без делителей, `SysClk::Irc8m`, USART0 на APB2,
`AdcSel::Off`), и каждое поле доезжает до регистров, названо оно или нет. PLL от IRC8M
(`PllFreq`, 8–72 МГц) и прескейлеры шин (`AhbPsc` / `ApbPsc`) — типизированные
энумы, поэтому недостижимая частота даёт ошибку компиляции, а не тихое
округление; wait state'ы flash выставляются от нового `hclk` до переключения
источника. `Usart0Sel` и `AdcSel` / `AdcPsc` оседают в `Clocks`.
`rcu.ck_out(src, div)` выводит внутренний тактовый узел на `PA8`/`PA9`
(`CkOutSrc` / `CkOutDiv`) для замера. `pclk1_tim` / `pclk2_tim` несут тактовую
ветку таймеров: `hclk` при неделённой APB, удвоенная частота шины иначе. Частоты
— псевдонимы `fugit` из `src/time.rs` (`Hertz`), крейт реэкспортируется. `HXTAL`
вне скоупа — кварц на плате не запаян.

**USART** (`src/usart.rs`) — `Usart<USARTX, TX, RX, WORD = Byte>` владеет
периферией и обоими пинами. Маркеры `TxPin` / `RxPin` (из `usart_pins!`)
отсекают неверный пин или AF на компиляции; `BusClocks` выбирает частоту шины под
конкретный инстанс, поэтому делитель `baud` не возьмёт не ту. `UsartConfig`
(fluent, `Default` = 115200 / ×16 / `N8`) несёт `baud` величиной `time::Bps`
(`115_200.bps()` либо стандартные значения по именам в `usart::baud`),
`Oversampling::{X16, X8}` и `FrameFormat::{N8, E8, O8, E7, O7}` —
единственный источник правды для `WL`/`PCEN`/`PM`. Инхерентный блокирующий API на
ширине `Byte`: `write_byte` / `write_bytes` / `read_byte` / `read_bytes` плюс
`flush` (ждёт `TC`, а не `TBE`); у передачи нет условий ошибки, `Result` она не
возвращает. `read_bytes` блокируется до первого байта, дальше забирает только уже
пришедшее. `read_ready` / `write_ready` опрашивают `RBNE` / `TBE`. Трейтовые слои
на той же ширине: `embedded-hal-nb` `Read<u8>` / `Write<u8>` и `embedded-io`
`Read` / `Write` / `ReadReady` / `WriteReady`; второй даёт `read_exact` /
`write_all` / `write_fmt` дефолтами. `flush`, `read_ready` и `write_ready` есть в
обоих слоях — точечную запись выигрывает инхерентный, трейтовый доступен по пути.
Сырой 9-битный режим — `new_word` / `write_word` / `write_words` / `read_word` /
`read_words` на typestate `Word` (`UsartConfig9`). Ошибки приёма — `usart::Error`
(`Overrun` / `Noise` / `Framing` / `Parity`), сбрасываются через `USART_INTC`,
`ErrorKind` дают impl'ы `serial::Error` и `embedded_io::Error`. `release()`
возвращает периферию и оба пина.

**ADC** (`src/adc.rs`) — `Adc::new(rcu, adc, clocks)`, либо `dp.adc.constrain(...)`
через `AdcExt`, выполняет процедуру калибровки из мануала (`ADCON`, задержка 14 тактов `CK_ADC` в пересчёте на такты
ядра, `RSTCLB`/`CLB`). `read<PIN: Channel>(&pin, SampTime) -> u16` — одиночное
блокирующее преобразование по софт-триггеру. `Channel` реализован только для
`Pin<P, N, Analog>`, поэтому пин обязан реально пройти через `into_analog()`.
Внутренние каналы: `read_vref() -> i32` возвращает реальное `VDDA` в мВ
(вычисляется по заводскому `VREFINT_CAL` из flash, а если эта калибровка пустая —
по типовому VREFINT ≈ 1.2 В), а
`read_temperature() -> Option<i32>` — десятые доли °C; `None`, когда `CK_ADC`
слишком высока для минимального времени сэмплирования датчика. Scan-режим
требует DMA и отложен.

**SPI** (`src/spi.rs`) — SPI0 **и** SPI1: master, full-duplex, блокирующий, 8 или
16 бит, программный NSS. `Spi::new(...)` / `Spi::new_word(...)` принимают
`SpiConfig` (`SpiConfig::new(psc)`; `Default` нет, поскольку у делителя SCK нет
универсального значения, а `.mode()` и `.bit_order()` переопределяют дефолты
Mode 0 / MSB-first). Ширина слова — typestate: `transfer_word` и `SpiBus<u16>` не
существуют на `Spi<..., Byte>`, а `transfer_byte` и `SpiBus<u8>` — на
`Spi<..., Word>`. `BitOrder` — рантайм-значение: меняет сериализацию на проводе,
но не сигнатуры. Трейт `Instance` абстрагирует периферию на уровне операций,
поэтому один generic `Spi<>` обслуживает оба инстанса, несмотря на разные типы
`RegisterBlock` и расхождение на уровне битов (`FF16` в `CTL0` у SPI0 против `DZ`
в `CTL1` у SPI1, `BYTEN` выводится из ширины). Буферные операции инхерентные —
`transfer_bytes` / `transfer_bytes_in_place` / `read_bytes` / `write_bytes` и те
же в `_words`-варианте, — impl'ы `SpiBus` делегируют к ним. Ошибки —
`spi::Error` (`Overrun` / `ModeFault` / `Crc` / `Framing`), `Crc` ложится на
`ErrorKind::Other`. `release()` возвращает периферию и пины. Аппаратный NSS и CRC
сознательно не реализованы.

**DMA** (`src/dma.rs`) — разовые передачи, проверены на железе.
`dp.dma.split(&mut rcu)` (`DmaExt`) раздаёт `Channel<0>`…`Channel<4>` — каждый
уникальный ZST-токен, поэтому один канал не может вести две передачи сразу.
`write_to` / `read_from` забирают по значению канал, периферию **и** буфер и
отдают `Transfer`; вернуть все три можно только через `wait()`, поэтому буфер,
который железо ещё заполняет, прочитать нельзя. Буферы — `&'static [W]` /
`&'static mut [W]`. `DmaSrc<N>` / `DmaDst<N>` кодируют карту запросов (Table
8-3), поэтому периферия в паре с неверным каналом не компилируется, а не
простаивает молча, а ассоциированный `Word` выводит `PWIDTH`/`MWIDTH` из
typestate периферии. Общий супертрейт `DmaPeriph<N>` заодно держит линию запроса
(`DENT`/`DENR`, `DMATEN`/`DMAREN`, ADC `DMA`), так что сами драйверы про DMA
ничего не знают. `remaining()` и `is_error()` показывают состояние идущей
передачи. Дроп `Transfer` останавливает канал и теряет все три владения.
Циклический режим и `M2M` отложены.

**TIMER** (`src/timer.rs`) — ядро счётчика, проверено на железе. Все семь
таймеров. `Timer::new(rcu, timer, clocks)`, либо `dp.timer5.constrain(...)` через
`TimerExt`, тактирует и сбрасывает периферию и запоминает свой `CK_TIMERx` через
`Instance`, который привязывает каждый таймер к его шине. `start(psc, car)`
забирает остановленный `Timer` и отдаёт запущенный `CountDownTimer`, поэтому
`wait()` не существует у незапущенного таймера; `stop()` возвращает обратно.
`start` загружает делители из теневых регистров через `UPG` и гасит порождённое
им событие обновления, так что первый `wait()` отсчитывает полный интервал.
`wait()` блокирует до одного переполнения и оставляет таймер бежать; `cnt()`,
`car()` и `psc()` читаются из железа, а не из запомненной копии.
`start_interval(5.secs())` принимает длительность `fugit` в любой шкале — шкала
приезжает const-генериком, поэтому `millis` и `micros` не требуют конверсии на
месте вызова — и выводит делители от собственного такта таймера, в `u64` и с
насыщением. Доступ к регистрам заперт в `Instance`, поэтому `Deref` не нужен.
`into_delay()` даёт третий тип, `Delay`, который не обещает своего интервала:
`delay(interval)` настраивает делители, блокирует и снова останавливает счётчик,
поэтому задержкой нельзя затереть период, заданный где-то ещё. На нём реализован
`DelayNs` из `embedded-hal`; разрешение — один тик таймера. `elapsed()` переводит
счётчик в длительность любой шкалы.

`into_pwm(psc, car)` / `into_pwm_interval(interval)` дают четвёртый тип, `Pwm`,
который владеет периодом и раздаёт каналы. `channel(pin)` возвращает
`PwmChannel`: номер канала приезжает из ноги через `ChannelPin<TIMERX, C>`,
реализованный только для существующей в железе разводки, а операции — из
`PwmOps<C>`, реализованного только для каналов, которые у таймера есть (у
`TIMER5` их нет, у `TIMER13` один). Нога возвращается из `release()`. Каждый
канал держит собственный хэндл, взятый через `unsafe` `Instance::steal()`, и
трогает только свой регистр сравнения; несколько ног на один канал дают несколько
каналов, пишущих в него. `enable()` / `disable()`, `set_duty(cv)` и `max_duty()`
инхерентные, сверху лежит `SetDutyCycle` из `embedded-hal`. `set_period` /
`set_period_interval` меняют частоту сразу всем каналам, скважность при этом
сохраняется в тиках, а не в долях. `enable_output()` существует только у таймеров
с регистром `CCHP` (`TIMER0`, `TIMER14`, `TIMER15`, `TIMER16`), выходы которых
молчат, пока не поднят `POEN`.

`into_capture(psc)` даёт пятый тип, `Capture`, счётчик которого свободно бежит на
полном диапазоне `u16` — выбором остаётся только делитель. `channel(pin, edge)`
возвращает `CaptureChannel`, гейтится той же картой `ChannelPin<TIMERX, C>`,
операции лежат в `CaptureOps<C>`; `CHxEN` вынесен в `ChannelEnable<C>`, общий для
обеих ролей. `Edge` — `Rising` или `Falling`, захват по обоим фронтам на этом
чипе зарезервирован. `read()` возвращает `nb::Result<u16, Error>`: `WouldBlock`,
пока фронт не защёлкнут, и `Error::Overcapture`, когда фронт затёр непрочитанную
отметку. `interval(from, to)` переводит промежуток в длительность любой шкалы,
заворачиваясь вместе со счётчиком; `select_edge` меняет фронт на живом канале.
`into_timer()` / `release()` выводят из роли и `Pwm`, и `Capture`.
Комплементарные выходы, break, dead time и прерывания не реализованы.

**Прелюдия** (`src/prelude.rs`) — разбита по периферии: `prelude::gpio` (`GpioExt`,
`OutputPin` / `InputPin` / `StatefulOutputPin`), `prelude::rcu`, `prelude::dma`,
`prelude::adc`, `prelude::spi`, `prelude::i2c`, `prelude::timer` (`TimerExt`, `DelayNs`,
`SetDutyCycle`, `block!`), `prelude::time` (суффиксы `fugit`: `500.millis()`,
`100.kHz()`) и `prelude::usart` с подмодулями `io` и `nb` — один или другой, потому что
их `read`/`write` живут на одном типе, а два одноимённых трейта в области видимости
делают вызов неоднозначным (`E0034`). `use gd32e2_hal::prelude::*;` берёт всё вместе с
`usart::io`; чтобы сузить — импортировать подмодуль. Трейты реэкспортированы под `_`:
методы приезжают, имена нет. Типы в прелюдию не входят.

**I²C** (`src/i2c.rs`) — мастер, блокирующий, 7-битная адресация, обе периферии.
`I2c::new(rcu, i2c, sda, scl, &clocks, mode)`
принимает `I2cMode::{standard, fast, fast_plus}`, каждый несёт свою частоту SCL, а
быстрые режимы — ещё и `DutyCycle`; `CLKC`, `RISETIME` и `I2CCLK` считаются от
`pclk1`, поэтому нужен `&Clocks`. Обе ноги обязаны быть
`Alternate<AF, OpenDrain>` — это держат маркеры `SdaPin` / `SclPin`. Инхерентные
`write` / `read` / `write_read`, последний склеен repeated START'ом. Приём идёт по
«Solution B» из мануала: SCL растягивается вместо гонки софта с последним байтом.
Сверху лежит `embedded_hal::i2c::I2c`, его `transaction` склеивает соседние
операции одного направления и паникует, если `Read` не последняя. Ошибки —
`i2c::Error`, по варианту на каждый флаг `STAT0`, `NoAcknowledge` несёт источник.
Слишком медленный `pclk1` или недостижимая частота — паника в конструкторе.
10-битная адресация, SMBus, slave и DMA не реализованы. `examples/i2c.rs`
сканирует шину и читает у первого ответившего 1, 2, 3 и 4 байта, отправляя
только индекс регистра; `examples/i2c-registers.rs` пишет два байта и читает их
обратно. Оба прогнаны на 50 кГц против RP2040 в режиме I²C target; fast и fast
plus на железе пока не были.

### Пример

```rust
use gd32e2_hal::gpio::GpioExt;
use gd32e2_hal::pac;
use gd32e2_hal::rcu::{ClockConfig, PllFreq, RcuExt, SysClk};
use gd32e2_hal::usart::{Usart, UsartConfig};

let mut dp = pac::Peripherals::take().unwrap();
let mut rcu = dp.rcu.constrain();
let clocks = ClockConfig::default()
    .sysclk(SysClk::Pll(PllFreq::Mhz48))     // PLL от IRC8M -> 48 МГц
    .freeze(&mut rcu, &mut dp.fmc);
let parts = dp.gpioa.split(&mut rcu);        // включает такт GPIOA

let led = parts.pa5.into_output();
led.set_high();

let tx = parts.pa9.into_alternate::<1>();    // USART0_TX; ::<3>() не скомпилируется
let rx = parts.pa10.into_alternate::<1>();
let usart0 = Usart::new(&mut rcu, dp.usart0, tx, rx, clocks, UsartConfig::default());
if let Ok(byte) = usart0.read_byte() {
    usart0.write_byte(byte);                 // проверено на железе: приходит эхом
}
```

### Ограничения

- **PAC-only база, без сторонних HAL.**
- **Прошивка и отладка по SWD** (ST-Link V2 + `probe-rs`, `PA13`/`PA14`). Вывод
  из прошивки идёт по RTT через тот же зонд (`defmt` + `defmt-rtt`);
  USB-переходник не нужен, `PA9`/`PA10` заняты только в примерах на USART.
- Target `thumbv8m.base-none-eabi`; флеш 64K, ОЗУ 8K.
- `gd32e2` сгенерирован из патченных SVD — имена полей сверять по
  `docs/GD32E23x_User_Manual.pdf` (PDF лежат локально, в git не входят).

### Сборка

`memory.x` для линкера пишет `build.rs` из выбранной фичи чипа, копировать перед
сборкой нечего. Файл `memory.x` в корне проекта по-прежнему главнее — линкер
смотрит туда раньше путей поиска, и это выход для платы, которой в таблице нет.

```sh
cargo lib                      # только библиотека, алиас для build --features gd32e230k8
cargo be usart-echo            # проверить сборку одного примера, зонд не нужен
cargo bre usart-echo           # то же самое, release
```

Библиотеке отдельно партномер нужно назвать: дефолта нет, а `[dev-dependencies]`
к ней не применяются. Примеру фича достаётся и так.

Чтобы прошить, с ST-Link на `PA13`/`PA14`:

```sh
cargo re usart-echo   # сборка + прошивка по SWD, дальше остаётся подключённым
```

`re` — это `cargo run --release --example`; в `.cargo/config.toml` у цели прописан
`runner = "probe-rs run --chip GD32E230K8"`, который заливает ELF напрямую (без
`objcopy` и без `.bin`), а дальше остаётся подключённым и печатает RTT-лог до
Ctrl-C. Уровень лога фиксируется на компиляции переменной `DEFMT_LOG` из того же
`.cargo/config.toml`. Чип работает и без зонда, а освободившийся зонд читает
любой регистр вживую:

```sh
probe-rs reset --chip GD32E230K8              # запустить прошивку со сброса
probe-rs read  --chip GD32E230K8 b32 0x48000014 1   # например, GPIOA_OCTL
```

Дефолтная фича собирает под `GD32E230K8U6` (x8); для другого варианта:

```sh
cargo build --release --no-default-features --features gd32e230x4
```

> `rust-toolchain.toml` фиксирует канал и ставит нужный таргет. На Windows без
> Visual Studio host-toolchain должен быть GNU, иначе build-скрипты падают без
> MSVC-линкера: `rustup default stable-x86_64-pc-windows-gnu`.

### Roadmap

- [ ] DMA: циклический режим и `M2M`.
- [ ] Таймеры: комплементарные выходы, break, dead time.
- [ ] I²C: fast и fast plus на железе, 10-битная адресация, SMBus, slave, DMA.
- [ ] Работа на прерываниях (инфраструктура NVIC — затронет и USART/SPI).
- [ ] SPI: half-duplex / однопроводные режимы (`BDEN`/`BDOEN`/`RO`).
- [ ] SPI: аппаратный NSS, CRC, TI mode, slave — низкий приоритет.
- [ ] USART: аппаратное управление потоком (`CTS`/`RTS`).
- [ ] RCU: `HXTAL` (нужен внешний кварц на плате).
- [ ] GPIO: альтернативные функции порта F (регистра `AFSEL` нет — нужен
      отдельный разбор).
- [ ] Варианты корпуса / числа ног — вторая ось, независимая от x4/x6/x8: какие
      ноги вообще разварены (`PC13`–`PC15`, `PF6`/`PF7` есть на QFN48, но не на
      нашем QFN32).
- [ ] `embedded-dma` для буферов DMA.
- [ ] Вынос HAL в полностью отдельный крейт/репозиторий (не просто `examples/` —
      разделение самой библиотеки).

### Лицензия

Двойное лицензирование, на выбор пользователя:

- Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT ([LICENSE-MIT](LICENSE-MIT))

Вклад, присланный в проект, лицензируется так же, без дополнительных условий,
если явно не оговорено иное (Apache-2.0, раздел 5).
