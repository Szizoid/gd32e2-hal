# gd32e2-hal

[English](#english) · [Русский](#русский)

---

## English

A hardware abstraction layer for the **GD32E230K8U6** (Cortex-M23), written in
Rust from scratch on top of the [`gd32e2`](https://crates.io/crates/gd32e2) PAC.

> ⚠️ **Work in progress.** Written by hand, incrementally; the API is unstable.
> The package is a library (`src/lib.rs` → `adc`, `crc`, `dma`, `gpio`, `i2c`,
> `prelude`, `rcu`, `spi`, `time`, `timer`, `usart`, `watchdog`) plus on-hardware
> binaries in `examples/`. All 19 examples have been flashed and verified on the
> board — RCU, GPIO, USART (8/9-bit and parity), SPI0/SPI1, ADC, a one-shot DMA
> transfer, TIMER, blocking delays, PWM, input capture, I²C, CRC, both watchdogs,
> RTT. SPI1 was verified over the SWD pins with a USART log, the only pins it has
> on a 32-pin part; `spi1-word` as it stands is written for a 48-pin one.

### Principles

- **Errors at compile time, not on the board.** Pin identity and mode live in the
  type: `Pin<'A', 5, Input>` has no `set_high`, an invalid AF number doesn't
  compile, and ownership prevents reconfiguring a pin twice or using a port
  before its clock is on.
- **A method that changes hardware takes `&mut self`.** `&self` is for reads that
  leave the peripheral as it was, so the borrow checker rejects two concurrent
  users of one peripheral in safe code.
- **Zero-cost.** The same register writes as hand-written PAC code; `Pin` is a ZST.
- **`#![no_std]`, no heap.**

### Chip variants

One feature names the part, and exactly one must be enabled — zero or several is
an error rather than a silently truncated pin map. **There is no default**: which
part is on a board is not something this crate can assume. A feature is the part
number with an `x` in each field the code cannot see: the letter is the bonded pin
count, the digit the flash code, and the trailing `x` is the temperature grade.

| feature | pins | flash | SRAM |
| --- | --- | --- | --- |
| `gd32e230f4xx` / `f6xx` / `f8xx` | 20 | 16K / 32K / 64K | 4K / 6K / 8K |
| `gd32e230e8xx` | 24 | 64K | 8K |
| `gd32e230g4xx` / `g6xx` / `g8xx` | 28 | 16K / 32K / 64K | 4K / 6K / 8K |
| `gd32e230k4tx` / `k6tx` / `k8tx` | 32, LQFP | 16K / 32K / 64K | 4K / 6K / 8K |
| `gd32e230k4ux` / `k6ux` / `k8ux` | 32, QFN | 16K / 32K / 64K | 4K / 6K / 8K |
| `gd32e230c4xx` / `c6xx` / `c8xx` | 48 | 16K / 32K / 64K | 4K / 6K / 8K |

The 32-pin parts are the one place the package matters: a QFN32 carries VSS on its
thermal pad and gives the two freed pins to `PB2` and `PB8`, an LQFP32 does not.
Elsewhere the packages of one pin count bond the same pads, hence the `x`.

Development targets the `GD32E230K8U6`, i.e. `gd32e230k8ux`; the examples get it
from the crate's own `[dev-dependencies]` entry. The published documentation is built
for `gd32e230c8xx`, the largest part — every other one is a subset of it, so the page
shows pins and peripherals a smaller part does not have, and its AF map holds for x8.
`build.rs` turns the choice into the `memory.x` the linker needs and into the cfg
flags the source gates on: the AF map differs by flash code — the same pin at the
same AF number reaching a different peripheral (`PA2` AF1 is `USART0_TX` on x4,
`USART1_TX` on x8), datasheet Table 2-13/2-14 notes (1) x4, (2) x6/x8, (3) x8 —
while the bonded pads decide which pins exist. x8 is a superset of x6; where a row
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

**RCU** (`src/rcu.rs`) — `dp.rcu.constrain()` (`RcuExt`) hands out an
`UnfrozenRcu`, whose only method is `freeze(&mut fmc, config)`; it consumes that
value and returns the `Rcu` every driver takes, with the resulting `Clocks`
inside (`rcu.clocks()`). The tree is therefore frozen exactly once, and no driver
can be built before it is. Per-peripheral `Enable`
and `Reset` traits from the `bus_en!`/`bus_rst!` macros (separate, since not every
peripheral has a reset bit — DMA has none), called from every driver's
constructor, so nothing can be used unclocked. `ClockConfig::default()` is
the reset state — undivided buses, `SysClk::Irc8m`, USART0 on APB2, `AdcSel::Off` —
and every field reaches its registers whether it was named or not. PLL from IRC8M
(`PllFreq`, 8–72 MHz) and bus prescalers (`AhbPsc` / `ApbPsc`) are typed enums, so an
unreachable frequency is a compile error, not a silent rounding; flash wait states
are set from the new `hclk` before the source switch. `Usart0Sel` and `AdcSel` /
`AdcPsc` resolve into `Clocks`. `rcu.ck_out(src, div)` routes an internal clock
node onto `PA8`/`PA9` (`CkOutSrc` / `CkOutDiv`) for measurement.
`rcu.enable_irc40k()` starts the internal 40 kHz oscillator and waits for it to
stabilise; its frequency is fixed, so nothing is recorded in `Clocks`.
`rcu.reset_flag(ResetFlag::…)` reports what brought the chip up; several flags
can stand at once, and they accumulate until `rcu.clear_reset_flags()` drops all
seven — `RSTFC` has no per-flag granularity.
`pclk1_tim` / `pclk2_tim` carry the timer branch: `hclk` at an undivided APB,
twice the bus clock otherwise. Frequencies are `fugit` aliases from `src/time.rs`
(`Hertz`), re-exported. `HXTAL` is out of scope — no crystal on this board.

**FMC** (`src/fmc.rs`) — `dp.fmc.constrain()` (`FmcExt`), no clock gating: the
flash controller is always clocked. Owns the peripheral and the wait states,
which `UnfrozenRcu::freeze` writes through a borrow. `with_unlocked(|f| ...)`
unlocks `CTL` for the body of the call and locks it again; `UnlockedFmc` cannot
outlive that call, so an unlocked flash is not expressible. It carries
`erase_page(Page)`, `mass_erase()` and `program(Page, index, word)`. `Page` is an
enum over the pages of the part being built for (16 / 32 / 64), its discriminant
the page address; `index` counts the 256 words of one page, so an address outside
the flash or off a word boundary cannot be formed. Errors come from `FMC_STAT`
through `take_error`. `listen` / `unlisten` for `Event::End` and `Event::Error`
are on `UnlockedFmc` as well — `ENDIE` and `ERRIE` sit in `CTL` — while
`is_listening` and `clear_interrupt` are on `Fmc`, where a handler can reach
them. Programming is 32-bit, `PGW` left at its reset value; there is no slice
variant and option bytes are not covered.

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
waiting. `read_ready` / `write_ready` / `flush_ready` poll `RBNE` / `TBE` / `TC`.
Trait layers on the
same width: `embedded-hal-nb` `Read<u8>` / `Write<u8>` and `embedded-io`
`Read` / `Write` / `ReadReady` / `WriteReady`, the latter supplying
`read_exact` / `write_all` / `write_fmt` as defaults. `flush`, `read_ready` and
`write_ready` exist in both layers; the inherent ones win method-call syntax on an
owned or `&` receiver, the trait ones on `&mut` — reach the other one by path.
`listen` / `unlisten` / `is_listening` toggle `RBNEIE` (`Event::Rbne`), `TBEIE`
(`Event::Tbe`), `ERRIE` (`Event::Error`) and `PERRIE` (`Event::ParityError`);
all four share one NVIC line, so a handler checks `is_listening` before trusting
any flag. None needs a separate clear — `Rbne` clears on `read_byte`, `Tbe` on
`write_byte`, both error events on `take_error` — and `Tbe` is set at idle, so a
handler listening for it must `unlisten` once nothing is left to send. `Error`
covers framing, noise and overrun, and in hardware its enable is ANDed with the
DMA request line: without DMA reception it never fires. Overrun additionally
arrives through `RBNEIE`. Unmasking the line in the NVIC is the caller's.
`ParityError` is verified on hardware by `examples/usart-parity-error.rs` — two
USARTs crossed over a `PA9`-`PA3` jumper. `Error` is unverified.
Raw 9-bit words via `new_word` / `write_word` /
`write_words` / `read_word` / `read_words` on the `Word` typestate
(`UsartConfig9`). RX errors are `usart::Error` (`Overrun` / `Noise` / `Framing` /
`Parity`), taken and cleared by `take_error` through `USART_INTC`, with
`ErrorKind` from the
`serial::Error` and `embedded_io::Error` impls. `release()` returns the
peripheral and both pins.

**ADC** (`src/adc.rs`) — `dp.adc.constrain(rcu)` through `AdcExt` runs
the manual's calibration sequence (`ADCON`, a
14-`CK_ADC`-cycle delay converted to core cycles, `RSTCLB`/`CLB`).
`read<PIN: Channel>(&pin, SampTime) -> u16` performs a single blocking,
software-triggered conversion. `Channel` is implemented only
for `Pin<P, N, Analog>`, so a pin must actually have gone through
`into_analog()`. Internal channels: `read_vref() -> i32` returns the real `VDDA`
in mV (derived from the factory `VREFINT_CAL` in flash, falling back to the
typical ~1.2 V VREFINT if that calibration is blank), and
`read_temperature() -> Option<i32>` returns tenths of °C — `None` when `CK_ADC`
is too fast to satisfy the sensor's minimum sampling time. `start(&pin, SampTime)`
and `result()` are the two halves of `read` taken apart, for driving conversions
from an interrupt: `listen` / `unlisten` (`Event::Eoc`) toggle `EOCIE`, and `EOC`
clears on reading `RDATA`, so `result()` is both the fetch and the acknowledge —
there is no separate clear. Unmasking the line in the NVIC is the caller's. Scan
mode needs DMA and is deferred.

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
`transfer_bytes` / `transfer_bytes_in_place` and the `_words` variants.
`transfer_bytes` takes buffers of equal length and panics otherwise: the bus
trades a word for a word, so what drives the wire is the caller's to choose, and
`spi::fill` names the usual levels (`LOW` / `HIGH`, `_WORD` for 16 bits).
`SpiBus::read` sends `fill::LOW`, `SpiBus::write` discards what arrives.
`write_byte` and `read_byte` (`_word` on `Word`) are the two halves of an
exchange: neither waits for the other side, so a handler moves one word per
entry. State is read through `read_ready` (`RBNE`), `write_ready` (`TBE`) and
`take_error`. Errors are
`spi::Error` (`Overrun` / `ModeFault` / `Crc` / `Framing`), `Crc` mapping to
`ErrorKind::Other`. `listen` / `unlisten` / `is_listening` toggle `RBNEIE`
(`Event::Rbne`), `TBEIE` (`Event::Tbe`) and `ERRIE` (`Event::Error`, which covers
every `STAT` error flag — they share the one enable). There is no
`clear_interrupt`: `read_byte`, `write_byte` and `take_error` clear the flags.
Interrupts are verified on hardware (`examples/spi0-interrupt.rs`, a `PB5`-`PB4`
loopback): `Tbe` starts the run, `Rbne` paces it — one byte in flight, the
receive buffer being one word deep. `release()` returns the peripheral and
pins. Hardware NSS and
CRC are deliberately not implemented. SPI1 exists on x8 parts only, and below 48
pins its sole bonded pins are `PB1` plus `PA13`/`PA14` — the SWD pair. Hence
`examples/spi1-word.rs` is `required-features = ["gd32e230c8xx"]`.

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
timers. `dp.timer5.constrain(rcu)` through `TimerExt` clocks and resets
the peripheral and records its own `CK_TIMERx`
through `Instance`, which binds each timer to its bus. `start(psc, car)` consumes
the stopped `Timer` and returns a running `CountDownTimer`, so `wait()` doesn't
exist on a timer that was never started; `stop()` goes back. `start` sets `UPS`
before loading both dividers out of their shadow registers with `UPG`, so that
load does not itself raise `UPIF` — only a real rollover does, and the first
`wait()` measures a full interval. `wait()` blocks for
one rollover and leaves the timer running; `cnt()`, `car()` and `psc()` are read
back from the hardware, not from a remembered copy. `start_interval(5.secs())`
takes a `fugit` duration in any scale — the scale is a const generic, so `millis`
and `micros` need no conversion at the call site — and derives the dividers
against this timer's clock, in `u64` and saturating. `listen` / `unlisten`
(`Event::Update`) toggle `UPIE`; `clear_interrupt` clears `UPIF`, which hardware
never does on its own. Unmasking the line in the NVIC is the caller's. Register
access is confined to `Instance`, so no `Deref` is needed. `into_delay()` gives a third type,
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
timers with a `CCHP` register (`TIMER0`, `TIMER15`, `TIMER16`, plus `TIMER14`), whose
outputs stay silent until `POEN` is raised. `TIMER14` exists only on the 28-pin parts
and larger at 64K flash, and every impl of it is gated on that — the flash code alone
does not decide it.

`into_capture(psc)` gives a fifth type, `Capture`, whose counter free runs at the
full `u16` range — only the prescaler is a choice. `channel(pin, edge)` returns a
`CaptureChannel`, bound by the same `ChannelPin<TIMERX, C>` map, with operations
in `CaptureOps<C>`; `ChannelEnable<C>` carries `CHxEN`, `CHxIE` and `CHxIF`,
shared by both roles.
`Edge` is `Rising` or `Falling` — both edges at once is reserved on this part.
`read()` returns `nb::Result<u16, Error>`: `WouldBlock` until an edge is latched,
`Error::Overcapture` when one overwrote a timestamp not yet read.
`interval(from, to)` converts a span into a duration of any scale, wrapping with
the counter; `select_edge` changes the edge on a live channel. `into_timer()` /
`release()` leave the role on `Pwm` and `Capture` alike.

Interrupts: `listen` / `unlisten` / `is_listening` / `is_pending` /
`clear_interrupt` take an `Event` on whichever role owns the counter —
`CountDownTimer`, `Pwm` and `Capture` alike, `Event::Update` being the rollover.
Channels carry the same set without an argument, having exactly one event each,
and the type says which it is: a compare match on `PwmChannel`, a capture on
`CaptureChannel`. Only `PwmChannel` needs `clear_interrupt` — a capture is
acknowledged by `read()`, a compare match yields nothing to consume. All of a
timer's events share one NVIC line, so a handler pairs `is_listening` with
`is_pending`; the flag alone will not do, since an untouched channel compares
against zero after reset and raises its flag once per rollover. Unmasking the
line is the caller's. `examples/capture-interrupt.rs` measures intervals past one
counter cycle by counting rollovers alongside the timestamps. Complementary
outputs, break inputs and dead time are not implemented.

**Prelude** (`src/prelude.rs`) — split per peripheral: `prelude::gpio` (`GpioExt`,
`OutputPin` / `InputPin` / `StatefulOutputPin`), `prelude::rcu`, `prelude::dma`,
`prelude::fmc`, `prelude::adc`, `prelude::spi`, `prelude::i2c`, `prelude::timer` (`TimerExt`, `DelayNs`,
`SetDutyCycle`, `block!`), `prelude::watchdog` (`FwdgtExt`, `WwdgtExt`),
`prelude::time` (the `fugit` suffixes `500.millis()`,
`100.kHz()`) and `prelude::usart`, which has `io` and `nb` for the two serial flavours —
one or the other, since their `read`/`write` land on the same type and two same-named
traits in scope make the call ambiguous (`E0034`). `use gd32e2_hal::prelude::*;` takes
everything with `usart::io`; import a submodule instead to narrow it. Traits are
re-exported as `_`, so the methods arrive without the names; types are not included.

**I²C** (`src/i2c.rs`) — master, blocking, 7-bit addressing, both peripherals.
`I2c::new(rcu, i2c, sda, scl, mode)`
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
frequency panics in the constructor. `listen` / `unlisten` / `is_listening` take
`Event::Protocol` (`EVIE`), `Event::Byte` (`BUFIE`, and `EVIE` with it, which in
hardware it needs) or `Event::Error` (`ERRIE`). `unlisten(Event::Byte)` keeps the
phase events and drops `TBE`/`RBNE`, which is how a handler ends a write;
`unlisten(Event::Protocol)` takes both down. `take_error` is the acknowledge for
`Event::Error`.
The phase flags (`sbsend` / `addsend` / `tbe` / `btc` / `rbne` / `is_busy`,
plus blocking `wait_idle`) and the single steps a transaction is made of
(`write_start` / `write_addr` / `write_byte` / `read_byte` / `write_stop` /
`clear_addsend` / `set_acken` / `set_poap`) are public, so a handler can be
written by hand; the module docs carry the order of the phases and the
acknowledge rules, which the four read lengths differ in.
`start_write` / `start_read` take the peripheral and a `'static` buffer and
return a `WriteTransfer` / `ReadTransfer`, whose `on_interrupt` — called from
both vectors of that peripheral — advances the state machine; `is_done` reports
completion and `release` gives back the peripheral, the buffer and the outcome.
The outcome sits beside the pair rather than wrapping it: a failed transfer still
returns the peripheral. `Drop` releases the bus with a STOP. There is no
interrupt-driven `write_read`. 10-bit addressing, SMBus, slave mode and DMA are
not implemented. `examples/i2c.rs` scans the bus and reads 1, 2, 3 and 4
bytes from the first device that answers, sending nothing but a register index;
`examples/i2c-registers.rs` writes two bytes and reads them back. Both were run
at 50 kHz against an RP2040 in I²C target mode; `examples/i2c-interrupt.rs` runs
the same bench through the transfers. Fast and fast plus are implemented but
unverified, as are the interrupts and the transfers.

**CRC** (`src/crc.rs`) — hardware CRC, verified on hardware. `Crc<PS>` is generic
over the polynomial width (`B32`/`B16`/`B8`/`B7`), fixed by which constructor
built it: `new_32bit` / `new_16bit` / `new_8bit` / `new_7bit(rcu, crc, poly,
CrcConfig)` set `POLY`, `PS` and `REV_I`/`REV_O` in one write; `CrcConfig` carries
the last two and defaults to no reversal either way. `write_*bit` feeds one word
at the bus width matching `PS`; `DATA` combines it with whatever result is
already there rather than replacing it. `read` / `read_*bit()` return the
running result; `reset_with(seed)`
sets `IDATA` and pulses `RST`, so the next write starts from `seed` instead of
whatever was left over. `set_fdata` / `fdata` reach the unrelated scratch byte.

**Watchdogs** (`src/watchdog/`) — both verified on hardware. Neither can be
stopped once started, so each is a pair of types with no way back.

**FWDGT**: `dp.fwdgt.constrain(rcu)` through `FwdgtExt` starts IRC40K, which
clocks the counter; no
bus clock is involved. Starting is
irreversible in hardware, so it is irreversible in the type: `start(psc, rld)`
and `start_timeout(2.secs())` consume `Fwdgt` and return `FwdgtRunning`, whose
only method is `feed()`. `start_timeout` picks the smallest `FwdgtPsc` that
spans the request, truncating, and saturates at 26 s; `start` panics on an `rld`
past 12 bits. The manual asks for 7 or more IRC40K cycles between two reloads,
which is not enforced. Window mode (`WND`) is not implemented.

**WWDGT**: clocked from `PCLK1 / 4096 / WwdgtPsc`, spanning tens of
milliseconds. `dp.wwdgt.constrain(rcu)` through `WwdgtExt` clocks and resets the
peripheral;
`start(psc, cnt, win)` consumes it into `WwdgtRunning` with `feed()`. Period and
window are given in counter ticks, 0 to 63 — the register's top bit is added by
the HAL, so a value meaning "reset now" cannot be passed in; `start` panics if
either exceeds 6 bits or if the window outlasts the period. Feeding before the
window opens resets the chip just as a missed deadline does.
`set_period(cnt, win)` takes effect at the next feed, the counter being left
alone because writing it would itself count as a feed. `listen()` enables the
early wakeup interrupt one tick before the reset and has no `unlisten` — `EWIE`
ignores a written zero; `is_listening` / `is_pending` / `clear_interrupt` round
it out, and the flag is set by hardware whether or not the interrupt is enabled.
The flag also re-arms while the counter sits at `0x40`, so a handler that only
clears it is entered again until the reset lands.

### Usage

```rust
use gd32e2_hal::gpio::GpioExt;
use gd32e2_hal::pac;
use gd32e2_hal::rcu::{ClockConfig, PllFreq, RcuExt, SysClk};
use gd32e2_hal::usart::{Usart, UsartConfig};

let dp = pac::Peripherals::take().unwrap();
let mut fmc = dp.fmc.constrain();
let config = ClockConfig::default()
    .sysclk(SysClk::Pll(PllFreq::Mhz48));    // PLL from IRC8M -> 48 MHz
let mut rcu = dp.rcu.constrain().freeze(&mut fmc, config);
let clocks = rcu.clocks();
let parts = dp.gpioa.split(&mut rcu);        // enables the GPIOA clock

let mut led = parts.pa5.into_output();
led.set_high();

let tx = parts.pa9.into_alternate::<1>();    // USART0_TX; ::<3>() wouldn't compile
let rx = parts.pa10.into_alternate::<1>();
let mut usart0 = Usart::new(&mut rcu, dp.usart0, tx, rx, UsartConfig::default());
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
cargo lib                      # library only, alias for build --features gd32e230c8xx
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

**Peripherals not covered at all**

- [ ] `EXTI` and `SYSCFG` — external events on pins, the wakeup source for sleep
      modes, and the `EXTI` source select that lives in `SYSCFG`. The same module
      carries the `PA11`/`PA12` remap onto the `PA9`/`PA10` pads below 32 pins
      and the second DMA channel map.
- [ ] `PMU` — sleep / deep-sleep / standby, the wakeup pin, LDO. Waking needs
      `EXTI` first.
- [ ] `embedded-storage` (`ReadNorFlash` / `NorFlash`) over the FMC.
- [ ] `RTC` — the calendar. Runs off IRC40K without a crystal, at IRC40K accuracy.
- [ ] `CMP` — the analog comparator; its `CMP_OUT` is already in the AF map.

**Finishing what is there**

- [ ] Interrupts for DMA — every other peripheral has theirs; the
      `Event` / `listen` shape is settled, so this is mostly mechanical.
- [ ] DMA: circular mode, `M2M`, and `embedded-dma` for the buffers.
- [ ] ADC: scan mode across several channels, which needs DMA to keep the
      intermediate results.
- [ ] Timers: complementary outputs, break and dead time; `TRGO` on `TIMER5`, so
      it can trigger DMA.
- [ ] I²C: 10-bit addressing, SMBus, slave, DMA.
- [ ] SPI: half-duplex / single-wire modes (`BDEN`/`BDOEN`/`RO`); hardware NSS,
      CRC, TI mode and slave are low priority.
- [ ] USART: hardware flow control (`CTS`/`RTS`); a non-blocking API for 9-bit
      words.
- [ ] FWDGT: window mode (`WND`); `FWDGT_HOLD` and `WWDGT_HOLD` in the DBG
      module, so neither watchdog runs while a debugger holds the core.
- [ ] GPIO: port C (`PC13`–`PC15`, 48-pin parts only) — needs its own `Parts`;
      port F alternate functions (no `AFSEL` register — needs its own study).
- [ ] RCU: `HXTAL` and `LXTAL` (neither crystal is fitted on this board).

**Infrastructure**

- [ ] Async: `embedded-hal-async` / `embedded-io-async`, starting with a time
      driver over one of the timers, which is what Embassy needs from a HAL.
- [ ] Tests on the host and CI — there are none; the crate does not build for the
      host, so this needs mocks.
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
> библиотека (`src/lib.rs` → `adc`, `crc`, `dma`, `gpio`, `i2c`, `prelude`, `rcu`,
> `spi`, `time`, `timer`, `usart`, `watchdog`) плюс тестовые бинарники на железо в `examples/`.
> Все 19 примеров прошиты и проверены на плате — RCU, GPIO, USART (8/9-бит и
> чётность), SPI0/SPI1, ADC, разовая передача по DMA, TIMER, блокирующие задержки,
> PWM, input capture, I²C, CRC, оба сторожевых таймера, RTT. SPI1 проверялся на ногах SWD с логом по USART — других
> ног у него на 32-выводном чипе нет; `spi1-word` в нынешнем виде написан под
> 48-выводный.

### Принципы

- **Ошибки на компиляции, а не на плате.** Идентичность и режим ноги живут в
  типе: у `Pin<'A', 5, Input>` нет `set_high`, неверный номер AF не
  компилируется, а система владения не даст перенастроить ногу дважды или
  использовать порт до включения такта.
- **Метод, меняющий железо, берёт `&mut self`.** `&self` — у чтений, оставляющих
  периферию прежней, поэтому borrow checker отсекает двух одновременных
  пользователей одной периферии в safe-коде.
- **Zero-cost.** Те же записи в регистры, что и ручной PAC-код; `Pin` — ZST.
- **`#![no_std]`, без кучи.**

### Варианты чипа

Партномер задаётся одной фичей, и ровно одна обязана быть включена — ноль или
несколько дают ошибку, а не молча урезанную карту пинов. **Дефолта нет**: какой
чип стоит на плате, крейт знать не может. Фича — это партномер с `x` в тех полях,
которых код не видит: буква — число разваренных ног, цифра — код флеша, последний
`x` — температурный диапазон.

| фича | ноги | флеш | SRAM |
| --- | --- | --- | --- |
| `gd32e230f4xx` / `f6xx` / `f8xx` | 20 | 16K / 32K / 64K | 4K / 6K / 8K |
| `gd32e230e8xx` | 24 | 64K | 8K |
| `gd32e230g4xx` / `g6xx` / `g8xx` | 28 | 16K / 32K / 64K | 4K / 6K / 8K |
| `gd32e230k4tx` / `k6tx` / `k8tx` | 32, LQFP | 16K / 32K / 64K | 4K / 6K / 8K |
| `gd32e230k4ux` / `k6ux` / `k8ux` | 32, QFN | 16K / 32K / 64K | 4K / 6K / 8K |
| `gd32e230c4xx` / `c6xx` / `c8xx` | 48 | 16K / 32K / 64K | 4K / 6K / 8K |

Корпус значим ровно у 32-выводных: QFN32 держит VSS на брюшке и отдаёт два
освободившихся вывода под `PB2` и `PB8`, LQFP32 — нет. У остальных корпуса одного
числа ног разваривают одни и те же площадки, отсюда `x`.

Разработка идёт на `GD32E230K8U6`, то есть `gd32e230k8ux`; примеры получают фичу из
`[dev-dependencies]` самого крейта. Опубликованная дока собрана для `gd32e230c8xx` —
самой крупной детали: остальные её подмножества, поэтому на странице видны ноги и
периферия, которых у мелкой детали нет, а карта AF там верна для x8.
Из выбора `build.rs` делает
`memory.x` для линкера и cfg-флаги, которыми гейтится код: карта AF зависит от кода
флеша — одна и та же нога на одном номере AF ведёт к разной периферии (`PA2` AF1 —
`USART0_TX` у x4 и `USART1_TX` у x8), сноски datasheet Table 2-13/2-14: (1) x4,
(2) x6/x8, (3) x8 — а разваренные площадки решают, какие ноги существуют вообще. x8 —
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

**RCU** (`src/rcu.rs`) — `dp.rcu.constrain()` (`RcuExt`) отдаёт `UnfrozenRcu`, у
которого единственный метод `freeze(&mut fmc, config)`; он съедает это значение и
возвращает `Rcu`, который берут все драйверы, с полученным `Clocks` внутри
(`rcu.clocks()`). Дерево замораживается ровно один раз, и ни один драйвер не
собрать до этого. Трейты `Enable` и
`Reset` на каждую периферию из макросов `bus_en!`/`bus_rst!` (порознь, потому что
не у каждой периферии есть бит сброса — у DMA его нет), зовутся из конструктора
каждого драйвера, так что без такта не поработать. `ClockConfig::default()` —
это состояние после сброса (шины без делителей, `SysClk::Irc8m`, USART0 на APB2,
`AdcSel::Off`), и каждое поле доезжает до регистров, названо оно или нет. PLL от IRC8M
(`PllFreq`, 8–72 МГц) и прескейлеры шин (`AhbPsc` / `ApbPsc`) — типизированные
энумы, поэтому недостижимая частота даёт ошибку компиляции, а не тихое
округление; wait state'ы flash выставляются от нового `hclk` до переключения
источника. `Usart0Sel` и `AdcSel` / `AdcPsc` оседают в `Clocks`.
`rcu.ck_out(src, div)` выводит внутренний тактовый узел на `PA8`/`PA9`
(`CkOutSrc` / `CkOutDiv`) для замера. `rcu.enable_irc40k()` запускает внутренний
генератор 40 кГц и ждёт стабилизации; частота фиксирована, в `Clocks` не
попадает. `rcu.reset_flag(ResetFlag::…)` сообщает, что подняло чип; флагов может
стоять несколько сразу, и они накапливаются, пока `rcu.clear_reset_flags()` не
погасит все семь — выборочно `RSTFC` не умеет. `pclk1_tim` / `pclk2_tim` несут тактовую ветку таймеров: `hclk` при
неделённой APB, удвоенная частота шины иначе. Частоты — псевдонимы `fugit` из
`src/time.rs` (`Hertz`), крейт реэкспортируется. `HXTAL` вне скоупа — кварц на
плате не запаян.

**FMC** (`src/fmc.rs`) — `dp.fmc.constrain()` (`FmcExt`), без включения такта:
контроллер флеша тактуется всегда. Владеет периферией и wait state'ами, которые
пишет `UnfrozenRcu::freeze` по заимствованию. `with_unlocked(|f| ...)` снимает
замок с `CTL` на тело вызова и ставит обратно; `UnlockedFmc` вызов не переживает,
поэтому отпертый флеш в API не выражается. На нём — `erase_page(Page)`,
`mass_erase()` и `program(Page, index, word)`. `Page` — enum по страницам того
партномера, под который идёт сборка (16 / 32 / 64), дискриминант равен адресу
страницы; `index` считает 256 слов страницы, поэтому адрес вне флеша или не по
границе слова не собрать. Ошибки — из `FMC_STAT` через `take_error`.
`listen` / `unlisten` для `Event::End` и `Event::Error` — тоже на `UnlockedFmc`
(`ENDIE` и `ERRIE` лежат в `CTL`), а `is_listening` и `clear_interrupt` — на
`Fmc`, откуда до них дотянется обработчик. Программирование 32-битное, `PGW`
остаётся в сбросовом значении; варианта по срезу нет, option bytes не покрыты.

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
пришедшее. `read_ready` / `write_ready` / `flush_ready` опрашивают `RBNE` / `TBE`
/ `TC`. Трейтовые слои
на той же ширине: `embedded-hal-nb` `Read<u8>` / `Write<u8>` и `embedded-io`
`Read` / `Write` / `ReadReady` / `WriteReady`; второй даёт `read_exact` /
`write_all` / `write_fmt` дефолтами. `flush`, `read_ready` и `write_ready` есть в
обоих слоях — точечную запись на владении или `&` выигрывает инхерентный, на
`&mut` — трейтовый; другой доступен по пути.
`listen` / `unlisten` / `is_listening` переключают `RBNEIE` (`Event::Rbne`),
`TBEIE` (`Event::Tbe`), `ERRIE` (`Event::Error`) и `PERRIE`
(`Event::ParityError`); все четыре висят на одной линии NVIC, поэтому обработчик
сверяет `is_listening`, прежде чем доверять флагу. Отдельного сброса не нужно
никому: `Rbne` гасится `read_byte`, `Tbe` — `write_byte`, оба события ошибок —
`take_error`. `Tbe` взведён в покое, поэтому слушающий его обработчик обязан
звать `unlisten`, когда слать больше нечего. `Error` покрывает framing, noise и
overrun, и в железе его разрешение объединено по И с линией запроса DMA: без
DMA-приёма он не сработает. Overrun приходит ещё и через `RBNEIE`.
Размаскирование линии в NVIC — за вызывающим. `ParityError` проверен на железе
примером `examples/usart-parity-error.rs` — два USART крест-накрест по перемычке
`PA9`-`PA3`. `Error` не проверен.
Сырой 9-битный режим — `new_word` / `write_word` / `write_words` / `read_word` /
`read_words` на typestate `Word` (`UsartConfig9`). Ошибки приёма — `usart::Error`
(`Overrun` / `Noise` / `Framing` / `Parity`), забираются и сбрасываются
`take_error` через `USART_INTC`,
`ErrorKind` дают impl'ы `serial::Error` и `embedded_io::Error`. `release()`
возвращает периферию и оба пина.

**ADC** (`src/adc.rs`) — `dp.adc.constrain(rcu)` через `AdcExt`
выполняет процедуру калибровки из мануала (`ADCON`, задержка 14 тактов `CK_ADC` в пересчёте на такты
ядра, `RSTCLB`/`CLB`). `read<PIN: Channel>(&pin, SampTime) -> u16` — одиночное
блокирующее преобразование по софт-триггеру. `Channel` реализован только для
`Pin<P, N, Analog>`, поэтому пин обязан реально пройти через `into_analog()`.
Внутренние каналы: `read_vref() -> i32` возвращает реальное `VDDA` в мВ
(вычисляется по заводскому `VREFINT_CAL` из flash, а если эта калибровка пустая —
по типовому VREFINT ≈ 1.2 В), а
`read_temperature() -> Option<i32>` — десятые доли °C; `None`, когда `CK_ADC`
слишком высока для минимального времени сэмплирования датчика. `start(&pin, SampTime)`
и `result()` — те же две половины `read`, разнятые для запуска конверсий из
прерывания: `listen` / `unlisten` (`Event::Eoc`) переключают `EOCIE`, а `EOC`
гасится чтением `RDATA`, поэтому `result()` — одновременно и забор значения, и
подтверждение; отдельного сброса нет. Размаскирование линии в NVIC — за
вызывающим. Scan-режим требует DMA и отложен.

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
`transfer_bytes` / `transfer_bytes_in_place` и те же в `_words`-варианте.
`transfer_bytes` требует буферы равной длины и паникует иначе: шина меняет слово
на слово, поэтому чем гнать провод — выбор вызывающего, а `spi::fill` называет
обычные уровни (`LOW` / `HIGH`, `_WORD` для 16 бит). `SpiBus::read` шлёт
`fill::LOW`, `SpiBus::write` выбрасывает принятое. `write_byte` и `read_byte` (`_word` на `Word`) — половинки обмена: встречной
стороны не ждут, поэтому обработчик двигает одно слово за вход. Состояние
читают `read_ready` (`RBNE`), `write_ready` (`TBE`) и `take_error`. Ошибки —
`spi::Error` (`Overrun` / `ModeFault` / `Crc` / `Framing`), `Crc` ложится на
`ErrorKind::Other`. `listen` / `unlisten` / `is_listening` переключают `RBNEIE`
(`Event::Rbne`), `TBEIE` (`Event::Tbe`) и `ERRIE` (`Event::Error` — покрывает все
флаги ошибок `STAT`, у них одно разрешение на всех). `clear_interrupt` нет: флаги
гасят `read_byte`, `write_byte` и `take_error`. Прерывания проверены на железе
(`examples/spi0-interrupt.rs`, петля `PB5`-`PB4`): `Tbe` запускает обмен, `Rbne`
его пасует — в полёте один байт, приёмный буфер глубиной в слово.
`release()` возвращает периферию и пины. Аппаратный NSS и CRC
сознательно не реализованы. SPI1 есть только на x8, и ниже 48 выводов из его ног
разварены лишь `PB1` плюс `PA13`/`PA14` — пара SWD. Поэтому у
`examples/spi1-word.rs` стоит `required-features = ["gd32e230c8xx"]`.

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
таймеров. `dp.timer5.constrain(rcu)` через `TimerExt` тактирует и
сбрасывает периферию и запоминает свой `CK_TIMERx` через
`Instance`, который привязывает каждый таймер к его шине. `start(psc, car)`
забирает остановленный `Timer` и отдаёт запущенный `CountDownTimer`, поэтому
`wait()` не существует у незапущенного таймера; `stop()` возвращает обратно.
`start` поднимает `UPS` перед тем, как загрузить делители из теневых регистров
через `UPG`, поэтому сама загрузка `UPIF` не взводит — только настоящее
переполнение, и первый `wait()` отсчитывает полный интервал.
`wait()` блокирует до одного переполнения и оставляет таймер бежать; `cnt()`,
`car()` и `psc()` читаются из железа, а не из запомненной копии.
`start_interval(5.secs())` принимает длительность `fugit` в любой шкале — шкала
приезжает const-генериком, поэтому `millis` и `micros` не требуют конверсии на
месте вызова — и выводит делители от собственного такта таймера, в `u64` и с
насыщением. `listen` / `unlisten` (`Event::Update`) переключают `UPIE`;
`clear_interrupt` гасит `UPIF`, который железо само не снимает никогда.
Размаскирование линии в NVIC — за вызывающим. Доступ к регистрам заперт в
`Instance`, поэтому `Deref` не нужен.
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
с регистром `CCHP` (`TIMER0`, `TIMER15`, `TIMER16` и `TIMER14`), выходы которых
молчат, пока не поднят `POEN`. `TIMER14` есть только на чипах от 28 выводов с 64K
флеша, и все его impl'ы гейтятся этим — код флеша сам по себе его не решает.

`into_capture(psc)` даёт пятый тип, `Capture`, счётчик которого свободно бежит на
полном диапазоне `u16` — выбором остаётся только делитель. `channel(pin, edge)`
возвращает `CaptureChannel`, гейтится той же картой `ChannelPin<TIMERX, C>`,
операции лежат в `CaptureOps<C>`; `CHxEN`, `CHxIE` и `CHxIF` вынесены в
`ChannelEnable<C>`, общий для обеих ролей. `Edge` — `Rising` или `Falling`,
захват по обоим фронтам на этом
чипе зарезервирован. `read()` возвращает `nb::Result<u16, Error>`: `WouldBlock`,
пока фронт не защёлкнут, и `Error::Overcapture`, когда фронт затёр непрочитанную
отметку. `interval(from, to)` переводит промежуток в длительность любой шкалы,
заворачиваясь вместе со счётчиком; `select_edge` меняет фронт на живом канале.
`into_timer()` / `release()` выводят из роли и `Pwm`, и `Capture`.

Прерывания: `listen` / `unlisten` / `is_listening` / `is_pending` /
`clear_interrupt` принимают `Event` у любой роли, владеющей счётчиком, —
`CountDownTimer`, `Pwm` и `Capture` одинаково; `Event::Update` — переполнение.
У каналов тот же набор без аргумента: событие у канала ровно одно, и какое
именно — сказано типом (совпадение у `PwmChannel`, захват у `CaptureChannel`).
`clear_interrupt` нужен только `PwmChannel`: захват подтверждается вызовом
`read()`, а у совпадения потреблять нечего. Все события таймера сидят на одной
линии NVIC, поэтому обработчик сверяет `is_listening` вместе с `is_pending` —
одного флага мало, нетронутый канал сравнивает с нулём после сброса и взводит
флаг раз в оборот. Размаскирование линии — за вызывающим.
`examples/capture-interrupt.rs` меряет промежутки длиннее одного оборота, считая
переполнения рядом с отметками. Комплементарные выходы, break и dead time не
реализованы.

**Прелюдия** (`src/prelude.rs`) — разбита по периферии: `prelude::gpio` (`GpioExt`,
`OutputPin` / `InputPin` / `StatefulOutputPin`), `prelude::rcu`, `prelude::dma`,
`prelude::fmc`, `prelude::adc`, `prelude::spi`, `prelude::i2c`, `prelude::timer` (`TimerExt`, `DelayNs`,
`SetDutyCycle`, `block!`), `prelude::watchdog` (`FwdgtExt`, `WwdgtExt`),
`prelude::time` (суффиксы `fugit`: `500.millis()`,
`100.kHz()`) и `prelude::usart` с подмодулями `io` и `nb` — один или другой, потому что
их `read`/`write` живут на одном типе, а два одноимённых трейта в области видимости
делают вызов неоднозначным (`E0034`). `use gd32e2_hal::prelude::*;` берёт всё вместе с
`usart::io`; чтобы сузить — импортировать подмодуль. Трейты реэкспортированы под `_`:
методы приезжают, имена нет. Типы в прелюдию не входят.

**I²C** (`src/i2c.rs`) — мастер, блокирующий, 7-битная адресация, обе периферии.
`I2c::new(rcu, i2c, sda, scl, mode)`
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
`listen` / `unlisten` / `is_listening` принимают `Event::Protocol` (`EVIE`),
`Event::Byte` (`BUFIE`, вместе с ним и `EVIE`, без которого он в железе не
работает) или `Event::Error` (`ERRIE`). `unlisten(Event::Byte)` оставляет
фазовые события и снимает `TBE`/`RBNE` — так обработчик заканчивает запись;
`unlisten(Event::Protocol)` гасит оба. `take_error` — подтверждение для
`Event::Error`. Флаги фаз (`sbsend` / `addsend`
/ `tbe` / `btc` / `rbne` / `is_busy` плюс блокирующий `wait_idle`) и одиночные
шаги транзакции (`write_start` / `write_addr` / `write_byte` / `read_byte` /
`write_stop` / `clear_addsend` / `set_acken` / `set_poap`) публичны — обработчик
можно написать руками; порядок фаз и правила подтверждения, которыми различаются
четыре длины чтения, описаны в доке модуля. `start_write` / `start_read`
забирают периферию и `'static`-буфер и отдают `WriteTransfer` / `ReadTransfer`;
их `on_interrupt`, вызываемый из обоих векторов этой периферии, двигает автомат,
`is_done` сообщает о завершении, `release` возвращает периферию, буфер и исход.
Исход идёт рядом с парой, а не оборачивает её: провалившаяся передача обязана
вернуть периферию. `Drop` отпускает шину STOP'ом. `write_read` по прерываниям
нет. 10-битная адресация, SMBus, slave и DMA не реализованы. `examples/i2c.rs`
сканирует шину и читает у первого ответившего 1, 2, 3 и 4 байта, отправляя
только индекс регистра; `examples/i2c-registers.rs` пишет два байта и читает их
обратно. Оба прогнаны на 50 кГц против RP2040 в режиме I²C target;
`examples/i2c-interrupt.rs` гоняет тот же стенд через передачи. Fast и fast plus
реализованы, но не проверены, как и прерывания с передачами.

**CRC** (`src/crc.rs`) — аппаратный CRC, проверен на железе. `Crc<PS>` — дженерик
по ширине полинома (`B32`/`B16`/`B8`/`B7`), фиксируется тем, какой конструктор
построил значение: `new_32bit` / `new_16bit` / `new_8bit` /
`new_7bit(rcu, crc, poly, CrcConfig)` одним заходом пишут `POLY`, `PS` и
`REV_I`/`REV_O`; `CrcConfig` несёт последние два, по умолчанию — без реверса в
обе стороны. `write_*bit` скармливает одно слово на ширине шины, совпадающей с
`PS`; `DATA` комбинирует его с уже накопленным результатом, а не заменяет.
`read` / `read_*bit()` возвращают текущий результат; `reset_with(seed)`
выставляет `IDATA` и дёргает `RST`, поэтому следующая запись стартует с `seed`,
а не с того, что осталось. `set_fdata` / `fdata` — доступ к несвязанному
scratch-байту.

**Сторожевые таймеры** (`src/watchdog/`) — оба проверены на железе. Ни один
нельзя остановить после запуска, поэтому каждый — пара типов без пути назад.

**FWDGT**: `dp.fwdgt.constrain(rcu)` через `FwdgtExt` запускает IRC40K, от
которого счётчик и тактуется; шинного такта у него нет.
Пуск необратим в железе, поэтому необратим и в типе: `start(psc, rld)` и
`start_timeout(2.secs())` съедают `Fwdgt` и возвращают `FwdgtRunning`, у
которого есть единственный метод `feed()`. `start_timeout` берёт наименьший
`FwdgtPsc`, покрывающий запрос, с усечением, и насыщается на 26 с; `start`
паникует на `rld` шире 12 бит. Требование мануала о 7 и более тактах IRC40K
между двумя перезагрузками не проверяется. Оконный режим (`WND`) не реализован.

**WWDGT**: тактуется от `PCLK1 / 4096 / WwdgtPsc`, диапазон — десятки
миллисекунд. `dp.wwdgt.constrain(rcu)` через `WwdgtExt` включает такт и
сбрасывает периферию,
`start(psc, cnt, win)` съедает её и возвращает `WwdgtRunning` с `feed()`. Период
и окно задаются в тиках счётчика, от 0 до 63 — старший бит регистра HAL
добавляет сам, поэтому значение со смыслом «сбросить сейчас» передать нельзя;
`start` паникует, если любое из них шире 6 бит или окно длиннее периода.
Кормление до открытия окна сбрасывает чип так же, как пропущенный дедлайн.
`set_period(cnt, win)` вступает в силу со следующим кормлением, счётчик при этом
не трогается — запись в него сама считалась бы кормлением. `listen()` разрешает
прерывание раннего пробуждения за тик до сброса и не имеет пары `unlisten`:
`EWIE` игнорирует запись нуля; рядом `is_listening` / `is_pending` /
`clear_interrupt`, причём флаг взводится независимо от разрешения прерывания.
Пока счётчик стоит на `0x40`, флаг взводится снова, поэтому обработчик, который
только гасит его, входит повторно вплоть до сброса.

### Пример

```rust
use gd32e2_hal::gpio::GpioExt;
use gd32e2_hal::pac;
use gd32e2_hal::rcu::{ClockConfig, PllFreq, RcuExt, SysClk};
use gd32e2_hal::usart::{Usart, UsartConfig};

let dp = pac::Peripherals::take().unwrap();
let mut fmc = dp.fmc.constrain();
let config = ClockConfig::default()
    .sysclk(SysClk::Pll(PllFreq::Mhz48));    // PLL от IRC8M -> 48 МГц
let mut rcu = dp.rcu.constrain().freeze(&mut fmc, config);
let clocks = rcu.clocks();
let parts = dp.gpioa.split(&mut rcu);        // включает такт GPIOA

let mut led = parts.pa5.into_output();
led.set_high();

let tx = parts.pa9.into_alternate::<1>();    // USART0_TX; ::<3>() не скомпилируется
let rx = parts.pa10.into_alternate::<1>();
let mut usart0 = Usart::new(&mut rcu, dp.usart0, tx, rx, UsartConfig::default());
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
cargo lib                      # только библиотека, алиас для build --features gd32e230c8xx
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

**Периферия, которой нет вовсе**

- [ ] `EXTI` и `SYSCFG` — внешние события на ногах, источник пробуждения для
      режимов сна и выбор линии `EXTI`, который живёт в `SYSCFG`. Там же ремап
      `PA11`/`PA12` на площадки `PA9`/`PA10` ниже 32 выводов и вторая карта
      каналов DMA.
- [ ] `PMU` — sleep / deep-sleep / standby, wakeup-нога, LDO. Пробуждение
      требует сначала `EXTI`.
- [ ] `embedded-storage` (`ReadNorFlash` / `NorFlash`) поверх FMC.
- [ ] `RTC` — календарь. Без кварца пойдёт от IRC40K, с его же точностью.
- [ ] `CMP` — аналоговый компаратор; его `CMP_OUT` уже есть в карте AF.

**Доводка того, что есть**

- [ ] Прерывания у DMA — у остальных периферий они уже есть; форма
      `Event` / `listen` устоялась, так что это почти механика.
- [ ] DMA: циклический режим, `M2M` и `embedded-dma` для буферов.
- [ ] ADC: scan по нескольким каналам — нужен DMA, иначе промежуточные
      результаты теряются.
- [ ] Таймеры: комплементарные выходы, break, dead time; `TRGO` у `TIMER5`,
      чтобы он мог запускать DMA.
- [ ] I²C: 10-битная адресация, SMBus, slave, DMA.
- [ ] SPI: half-duplex / однопроводные режимы (`BDEN`/`BDOEN`/`RO`); аппаратный
      NSS, CRC, TI mode и slave — низкий приоритет.
- [ ] USART: аппаратное управление потоком (`CTS`/`RTS`); неблокирующий API для
      9-битных слов.
- [ ] FWDGT: оконный режим (`WND`); `FWDGT_HOLD` и `WWDGT_HOLD` в модуле DBG,
      чтобы ни один сторож не считал, пока отладчик держит ядро.
- [ ] GPIO: порт C (`PC13`–`PC15`, только 48-выводные) — нужен свой `Parts`;
      альтернативные функции порта F (регистра `AFSEL` нет — нужен отдельный
      разбор).
- [ ] RCU: `HXTAL` и `LXTAL` (ни один кварц на плате не запаян).

**Инфраструктура**

- [ ] Async: `embedded-hal-async` / `embedded-io-async`, начиная с драйвера
      времени поверх одного из таймеров — именно его Embassy ждёт от HAL.
- [ ] Тесты на хосте и CI — их нет вовсе; крейт под хост не собирается, поэтому
      нужны моки.
- [ ] Вынос HAL в полностью отдельный крейт/репозиторий (не просто `examples/` —
      разделение самой библиотеки).

### Лицензия

Двойное лицензирование, на выбор пользователя:

- Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT ([LICENSE-MIT](LICENSE-MIT))

Вклад, присланный в проект, лицензируется так же, без дополнительных условий,
если явно не оговорено иное (Apache-2.0, раздел 5).
