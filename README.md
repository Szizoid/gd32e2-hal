# gd32e230-hal

[English](#english) · [Русский](#русский)

---

## English

A hardware abstraction layer (HAL) for the **GD32E230K8U6** microcontroller
(Cortex-M23 core), written in Rust from scratch on top of the
[`gd32e2`](https://crates.io/crates/gd32e2) PAC.

> ⚠️ **Work in progress.** The HAL is written incrementally and by hand — to
> genuinely understand both the hardware and Rust's type system. The API is
> unstable. The package is a library crate (`src/lib.rs` → `gpio`, `rcu`, `time`,
> `usart` modules) plus a small on-hardware test bench binary (`src/main.rs`); the
> HAL will later be extracted into a standalone library. `main.rs` has been flashed
> and verified on real hardware (RCU PLL, GPIO output, USART0 TX+RX echo).

### Philosophy

There is no full-featured HAL for the GD32E230 in the Rust ecosystem, so the raw
PAC (`gd32e2` — direct register access) is used as the base, and a safe, ergonomic
layer is built on top. Principles:

- **Errors at compile time, not on the board.** A pin's identity and mode are
  encoded in its type: `Pin<'A', 5, Input>` physically has no `set_high` method, an
  invalid alternate function (`into_alternate::<3>()` on a pin that lacks AF3) fails
  to compile, and the ownership system won't let you reconfigure a single pin twice
  or use a port before its clock is enabled.
- **Zero-cost.** The abstractions compile down to the same register writes as
  hand-written PAC code: no runtime overhead. Pin identity lives entirely in the type
  (const generics), so `Pin` is zero-sized.
- **`#![no_std]`, no heap.**

### What's implemented

**GPIO** (`src/gpio.rs`):

- Const-generic pins `Pin<const P: char, const N: u8, MODE>` — the port (`'A'`/`'B'`/`'F'`)
  and pin number live in the type; `Pin` is a ZST.
- Modes as typestate: `Input`, `Analog`, `Alternate<const AF: u8>` (the AF number stays in
  the type), and `Output<PushPull>` / `Output<OpenDrain>` (the output type is in the type too).
- Handing out pins: `dp.gpioa.split(&mut rcu)` (trait `GpioExt`) consumes the port
  singleton, **enables its clock**, and returns the individual pins — each pin's
  uniqueness is guaranteed by the compiler, and you cannot obtain pins without a clock.
  Ports A, B and F are wired up (port C has no bonded pins on this package, QFN32, so it's
  intentionally left out).
- Mode transitions: `into_input` / `into_output` (push-pull default) /
  `into_push_pull_output` / `into_open_drain_output` / `into_analog`.
- Alternate functions with **compile-time validation**: `into_alternate::<AF>()` — the
  AF number is a const generic checked against a per-pin map (`ValidAf`); an invalid
  number does not compile. The map is transcribed from the datasheet (Table 2-13/2-14).
- `embedded-hal` 1.0: `OutputPin` / `InputPin` (`Error = Infallible`) — portable drivers
  work against these pins. `Output<OpenDrain>` also implements `InputPin` (reading back an
  open-drain line, e.g. for I2C clock-stretching). `StatefulOutputPin` (`is_set_high` /
  `is_set_low`, reading `OCTL`) with `toggle()` overridden to flip the pin through the
  atomic `TG` register instead of the crate's default read-then-write implementation.
- Per-pin configuration: `set_pull` (`PUD`), `set_speed` (`OSPD`).
- `PA13`/`PA14` (`SWDIO`/`SWCLK`) get a dedicated `Debugger` typestate instead of `Input` —
  they're genuinely in SWD mode out of reset, not floating input, and the type says so.
  `into_*`/setters are compile-time unreachable on `Debugger` (gated by a marker trait,
  `Active`); the only way out is `unsafe fn activate() -> Pin<P, N, Input>`, a pure type
  relabel (no register write) — the caller is on the hook for actually reconfiguring the
  pin with a follow-up `into_*()`.
- Full configuration lock: `lock()` runs the `LOCK` register's LKK write sequence and
  returns `Pin<P, N, Locked<MODE>>` — a terminal typestate (no `unlock`: per the manual
  the register stays locked until an MCU reset, so a software "unlock" would just lie
  about the hardware state). `Locked<MODE>` doesn't implement the `Active` marker, so
  `into_*`/`set_pull`/`set_speed` become compile-time unreachable, while `OutputPin` /
  `InputPin` / `StatefulOutputPin` still apply — generically, via `impl<...> Trait for
  Pin<P, N, Locked<MODE>> where Pin<P, N, MODE>: Trait`, so a locked pin keeps exactly
  the read/write capabilities its unlocked `MODE` already had, with no per-mode
  duplication. Gated to ports `A`/`B` by a `HasLock` marker trait — port `F` has no
  physical `LOCK` register.

**RCU** (`src/rcu.rs`):

- `dp.rcu.constrain()` (trait `RcuExt`) wraps the raw peripheral in a managed `Rcu`.
- Peripheral clock gating: a generic `Enable` trait (`enable` / `disable`), implemented
  per-peripheral by the `bus!` macro (one line: peripheral type → register → bit) instead of
  hand-written methods on `Rcu`. Wired into `split`, so a port is guaranteed clocked before use.
- System clock tree: a `CFGR` builder (`CFGR::default()`) configures `sysclk` / `hclk` /
  `pclk1` / `pclk2`, and `.freeze(&mut rcu, &mut dp.fmc)` writes the registers and returns a
  frozen `Clocks` (the actual resulting frequencies). PLL from IRC8M is supported
  (`PllFreq`, 8–72 MHz in 4 MHz steps); bus prescalers use `AhbPrescaler` / `ApbPrescaler`.
  All three are **enums, not raw `u32`** — an unreachable frequency or divider simply can't
  be requested, it's a compile error rather than a silently-rounded value. Flash wait states
  (`FMC.ws().wscnt`) are set from the resulting `hclk` *before* switching the system clock
  source. `HXTAL` is out of scope for now (no crystal on this board).
- Peripheral reset: a generic `Reset` trait (`reset`), wired up alongside `Enable` by the
  same `bus!` macro line (`AHBRST`/`APB1RST`/`APB2RST`) — pulses a peripheral back to its
  post-reset defaults.
- Typed frequencies (`src/time.rs`): `Hertz` / `KiloHertz` / `MegaHertz` / `Bps` /
  `MilliSeconds` / `MicroSeconds` newtypes, a `U32Ext` extension trait (`.hz()` / `.mhz()`
  / ...), `From` conversions between units, and `Mul`/`Div` arithmetic. `Clocks`'s four
  getters return `Hertz` instead of a raw `u32`.
- `CK_OUT`: `rcu.ck_out(src, div)` routes an internal clock node out onto `PA8`/`PA9` —
  the only way to verify a real frequency with a scope or logic analyzer, since this board
  has no debug probe. `CkOutSrc` covers the full `CKOUTSEL` mux (`None` / `Irc14m` /
  `Lsi40k` / `Lxtal` / `Sysclk` / `Irc8m` / `Hxtal` / `Pll(PllDiv)` — the PLL branch's own
  pre-mux divider is carried inside the enum variant, so it can't be set for any other
  source); `CkOutDiv` is the post-mux `1..128` divider that applies to every source.
- `Usart0Sel` (`CFGR.usart0_sel`, part of the same `CFGR`/`freeze()`/`Clocks` flow, not a
  one-off `Rcu` method like `ck_out` — the choice has to be *remembered* for `usart.rs`'s
  baud-rate math, not just fired once) lets `USART0` run off `CK_SYS`/`CK_LXTAL`/`CK_IRC8M`
  instead of `pclk2`; the resolved frequency lands in `Clocks::usart0()`, which
  `usart.rs`'s `BusClocks` impl reads instead of hardcoding `pclk2`. `Lxtal` is exposed
  even though this board has no 32.768 kHz crystal — selecting it without starting
  `LXTAL` elsewhere hangs USART0's TX/RX forever; the HAL can't know what's soldered on a
  given board, so that footgun is left to the caller on purpose.

**USART** (`src/usart.rs`) — blocking + non-blocking TX/RX, minimal scope done:

- `TxPin<USART>` / `RxPin<USART>` marker traits (generic over the peripheral type, not
  a const — the same trait covers both `Usart0` and `Usart1`), filled in by a
  `usart_pins!` table macro from the pin/AF combinations already verified in `gpio.rs`'s
  `pin_af!`. A pin of the wrong type or AF for a given `USARTX` simply doesn't satisfy
  the bound — caught at compile time, not by a silently-dead UART line.
  `Usart1` turned out to be a straight re-export of the `usart0` PAC module
  (`pub use self::usart0 as usart1;`), so both peripherals share one `RegisterBlock`
  type — no manual pointer-cast trick needed (unlike `Gpioa`/`Gpiob`, which are
  layout-compatible but distinct types).
- `BusClocks` — another type-level binding: `Usart0` runs off `pclk2` (APB2), `Usart1`
  off `pclk1` (APB1); `USARTX::clock(&clocks)` returns the right one, so the baud-rate
  calculation can't accidentally use the wrong bus frequency.
- `Usart<USARTX, TX, RX, WORD = Byte>` owns the peripheral and both pins (kept, not
  dropped — see `.release()` below). The 4th parameter defaults to `Byte`, so every
  existing 3-parameter `Usart<USARTX, TX, RX>` reference keeps compiling unchanged.
  `Usart::new(&mut rcu, usart, tx_pin, rx_pin, clocks, config)` enables the clock,
  resets the peripheral, computes `BAUD`, and turns on `UEN`/`TEN`/`REN` (shared logic
  factored into a private `configure()`, reused by the 9-bit constructor below).
  - **`Oversampling::{X16, X8}`** — at ×16 the whole 16-bit `BAUD` register turns out to
    equal `round(pclk / baud)` directly (`intdiv`/`fradiv` are just its upper 12 / lower
    4 bits). At ×8 the register format changes (only 3 fraction bits are usable,
    `BRR[3]` must stay `0`), so the value is split (`intdiv = w >> 3`, `fradiv8 = w &
    0x7`) and reassembled; `OVSMOD` in `CTL0` has to flip too, or the peripheral keeps
    sampling at ×16 regardless of what `BAUD` says.
  - **`FrameFormat::{N8, E8, O8, E7, O7}`** — the single source of truth for `WL`/
    `PCEN`/`PM` together (not a separate `Parity` enum alongside it: two independent
    knobs over the same three register fields would risk contradicting each other, the
    same trap already avoided with `BusClocks`). `E8`/`O8` set `WL=1` (9-bit frame) so
    the parity bit lands in bit 8, outside the `u8` range — `write_byte`/`read_byte`
    don't need to change at all, truncating to `u8` already discards it. `E7`/`O7` use
    `WL=0`: parity replaces bit 7 *inside* the `u8`, so only 7 real data bits remain and
    `received_byte()` has to mask `& 0x7F` on read (not on write — the peripheral
    overwrites the MSB with the computed parity regardless of what was written there).
    There's no separate "`N7`" — without parity, `WL=0` always yields the full 8 bits,
    i.e. `N8`; a 7th-bit ceiling only appears once parity eats into the word.
  - **`UsartConfig`** bundles `baud`/`oversampling`/`frame_format` behind fluent setters
    and `impl Default` (`115_200`/`X16`/`N8`), matching the reference HALs' `Config`
    pattern — Rust has no default function arguments. Unlike `CFGR`'s `Option<T>` fields
    (`None` = "leave the register alone"), `UsartConfig`'s fields are bare values: every
    `new()` writes all three regardless, so "don't touch" isn't meaningful here.
  - **`usart::baud`** — 15 named `u32` constants (`B110`..`B921600`, the standard POSIX
    set), purely for readability. `baud` stays a plain `u32`, not an enum: unlike
    `PllFreq`, the achievable range isn't a small hardware-fixed set, so an enum would
    only block legitimate non-standard rates without preventing anything actually
    impossible.
- **Pure 9-bit words, no parity** (`WL=1, PCEN=0`) — `Usart<USARTX, TX, RX, Word>` via
  `Usart::new_word(&mut rcu, usart, tx_pin, rx_pin, clocks, UsartConfig9::default())`
  (a config type with no `frame_format` field — nothing meaningful to put there for this
  path), with `write_word(&self, word: u16)` / `read_word(&self) -> Result<u16,
  ErrorKind>` (masked `& 0x1FF`, the 9 significant bits). `Byte`/`Word` are zero-sized
  marker types: `write_word`/`read_word` simply don't exist on `Usart<..., Byte>`, and
  `write_byte`/`read_byte` don't exist on `Usart<..., Word>` — verified by temporarily
  compiling a wrong-width call (`E0599: no method named write_word found`), not just
  reasoned about. Chosen over an unguarded pair of `u16` methods on the plain 3-parameter
  `Usart` specifically because the set of frame formats is small and known up front
  (like `PllFreq`), unlike `baud`/`pclk`, which are runtime-continuum values a marker
  type couldn't meaningfully gate.
- `write_byte(&self, byte: u8)` / `read_byte(&self) -> Result<u8, ErrorKind>` — busy-wait
  on `TBE`/`RBNE` then hit `TDATA`/`RDATA`, checking for RX errors (see below) before
  trusting the byte.
- `embedded_hal_nb::serial::{ErrorType, Read<u8>, Write<u8>}` — non-blocking
  counterparts, scoped to `Usart<..., Byte>` only (they'd be meaningless on `Word`): the
  same flag check, but `if !flag { return Err(nb::Error::WouldBlock) }` instead of a
  busy-wait loop. `Write::flush()` waits on a separate flag, `TC` (transmission complete
  — the shift register has physically emptied, not the same as `TBE`, which just means
  "buffer free for the next byte"). No inherent `flush(&self)` was added alongside it —
  same inherent-vs-trait method-resolution trap already hit in GPIO, so the trait
  `flush()` is the only entry point.
- **RX error handling**: uses `embedded_hal_nb::serial::ErrorKind` directly as
  `ErrorType::Error` — no HAL-specific error enum. `ErrorKind` already covers
  `Overrun`/`FrameFormat`/`Parity`/`Noise` (plus `Other`) and implements
  `serial::Error` itself (`kind()` is the identity), so wrapping it in a local type
  would only have duplicated it. Cleared via a **separate** `USART_INTC` register
  (write `1` to `OREC`/`NEC`/`FEC`/`PEC`), not the "read STAT then read DATA" pattern
  common on STM32. Clearing the error flag alone does **not** clear `RBNE` (different
  bits in different registers) — the private `take_error()` helper additionally does a
  dummy read of `RDATA` on error, purely to reset `RBNE`, or the next receive would spin
  forever on a stale flag. (`ErrorKind::FrameFormat`, the error variant, and this
  module's `FrameFormat`, the config type, share a name by coincidence only — different
  namespaces, no actual collision.)
- `release(self) -> (USARTX, TX, RX)` — disables `UEN` then hands back ownership of the
  peripheral and both pins as a plain tuple (no attempt to "return" them into `GpioExt`'s
  `Parts` struct — that's not how the typestate model works; once split out, a pin is
  just a value you hold wherever makes sense in your own code, same as any other HAL in
  this ecosystem). Generic over `WORD` — works the same for `Byte` and `Word`. A fresh
  `Usart::new()`/`new_word()` already does a full `enable`+`reset`, so `release()`
  doesn't duplicate that.

```rust
use embedded_hal::digital::OutputPin;
use gd32e230_hal::gpio::GpioExt;
use gd32e230_hal::rcu::{RcuExt, CFGR, PllFreq};
use gd32e230_hal::usart::{Usart, UsartConfig};

let mut dp = gd32e230::Peripherals::take().unwrap();
let mut rcu = dp.rcu.constrain();
let clocks = CFGR::default()
    .sysclk(PllFreq::Mhz48)              // PLL from IRC8M -> 48 MHz sysclk
    .freeze(&mut rcu, &mut dp.fmc);
let parts = dp.gpioa.split(&mut rcu);        // enables the GPIOA clock
let mut led = parts.pa5.into_output();
led.set_high().unwrap();

let tx_pin = parts.pa9.into_alternate::<1>();    // USART0_TX; ::<3>() would not compile
let rx_pin = parts.pa10.into_alternate::<1>();   // USART0_RX
let usart0 = Usart::new(&mut rcu, dp.usart0, tx_pin, rx_pin, clocks, UsartConfig::default());
if let Ok(byte) = usart0.read_byte() {
    usart0.write_byte(byte);                      // verified on hardware: echoes back
}
```

### Project constraints

- **PAC-only base, no third-party HAL.**
- **No debug probe** — flashing only via the UART bootloader (GD32 All-In-One
  Programmer); all output over USART0 (115200 8N1). No RTT / `defmt` / semihosting.
- Target `thumbv8m.base-none-eabi`, flash 64K / RAM 8K.

### Building

```sh
cargo build --release
cargo bin            # -> firmware.bin (needs cargo-binutils + llvm-tools)
```

Flash to `0x08000000`, read the log in a terminal @ 115200 8N1.

> On Windows without Visual Studio the host is switched to the GNU toolchain (see
> `rust-toolchain.toml`) so the MSVC linker isn't required.

### Roadmap

- [x] `GpioExt` trait — idiomatic `dp.gpioa.split()`.
- [x] `into_alternate` — alternate pin functions (`AFSEL` register).
- [x] `embedded-hal` 1.0 trait impls (`OutputPin` / `InputPin`).
- [x] `OMODE` / `OSPD` / `PUD` configuration (push-pull/open-drain, speed, pulls).
- [x] Output type as typestate (`Output<PushPull>` / `Output<OpenDrain>`).
- [x] Const-generic pins + compile-time alternate-function validation.
- [x] RCU: peripheral clock gating, enforced at `split`.
- [x] `StatefulOutputPin` (`toggle` via the atomic `TG` register / `is_set_*`).
- [x] Port F (`PF0`/`PF1`); port C skipped (not bonded on this package).
- [x] `Debugger` typestate for `PA13`/`PA14` (SWD pins), gated by a marker trait.
- [x] RCU: clock tree — PLL from IRC8M, AHB/APB prescalers, flash wait states, typed
      `CFGR` API (`PllFreq` / `AhbPrescaler` / `ApbPrescaler` enums, not raw `u32`).
- [x] RCU: `Reset` trait for peripherals (`AHBRST`/`APB1RST`/`APB2RST`).
- [x] Typed frequencies (`src/time.rs`, `Hertz` and friends) integrated into `Clocks`.
- [x] RCU: `CK_OUT` (internal clock signal on `PA8`/`PA9`).
- [x] GPIO `LOCK` (config freeze via `Locked<MODE>` typestate).
- [x] USART: blocking + `embedded-hal-nb` TX/RX (`Usart<USARTX, TX, RX>`, `TxPin`/
      `RxPin`, `BusClocks`), RX error handling (`Overrun`/`Noise`/`Framing`/`Parity`),
      `.release()` — verified on hardware via an echo loop.
- [x] USART: ×8-oversampling, `USART0SEL` (alternate clock source via `CFGR`/`Clocks`),
      `FrameFormat::{N8, E8, O8, E7, O7}` parity/word-length config, `UsartConfig`
      (fluent + `Default`) and named `usart::baud` constants — build-verified.
- [x] USART: pure 9-bit words, no parity — `Usart<USARTX, TX, RX, Word>` typestate
      (`Byte`/`Word` marker, defaulted 4th type parameter), `new_word`/`write_word`/
      `read_word`; own `Error` enum dropped in favor of
      `embedded_hal_nb::serial::ErrorKind` directly — build-verified, not yet reflashed.
- [ ] USART: hardware flow control (`CTS`/`RTS`) — deferred, low priority.
- [ ] RCU: `HXTAL` clock source (needs an external crystal on the board).
- [ ] Peripherals: timers / PWM, ADC, SPI, I²C.
- [ ] Extract the HAL into a standalone library crate.
- [ ] Support for other GD32E230x package/pin-count variants (future, low priority).

### Registers

`gd32e2` is generated from patched SVDs; field names should be verified against
`docs/GD32E23x_User_Manual.pdf` (see `docs/README.md` — the PDFs are kept locally and
are not committed to git).

---

## Русский

HAL (hardware abstraction layer) для микроконтроллера **GD32E230K8U6** (ядро
Cortex-M23), написанный на Rust с нуля поверх PAC-крейта
[`gd32e2`](https://crates.io/crates/gd32e2).

> ⚠️ **Работа в процессе.** HAL пишется постепенно и вручную — ради глубокого
> понимания и железа, и системы типов Rust. API нестабилен. Пакет — это
> библиотечный крейт (`src/lib.rs` → модули `gpio`, `rcu`, `time`, `usart`) плюс
> небольшой бинарь-стенд для проверки на железе (`src/main.rs`); позже HAL будет
> вынесен в отдельную библиотеку. `main.rs` уже прошит и проверен на реальном железе
> (RCU PLL, GPIO output, USART0 TX+RX echo).

### Философия

Полноценного HAL для GD32E230 в экосистеме Rust нет, поэтому за основу взят «сырой»
PAC (`gd32e2` — прямой доступ к регистрам), а поверх строится безопасный и
эргономичный слой. Принципы:

- **Ошибки — на этапе компиляции, а не на плате.** Идентичность и режим ноги закодированы
  в её типе: у `Pin<'A', 5, Input>` физически нет метода `set_high`, неверная
  альтернативная функция (`into_alternate::<3>()` на ноге без AF3) не компилируется, а
  перенастроить одну ногу дважды или использовать порт до включения его такта не даст
  система владения.
- **Zero-cost.** Абстракции компилируются в те же записи в регистры, что и ручной
  PAC-код: никакого оверхеда в рантайме. Идентичность ноги целиком в типе (const
  generics), поэтому `Pin` — нулевого размера.
- **`#![no_std]`, без кучи.**

### Что уже есть

**GPIO** (`src/gpio.rs`):

- Const-generic пины `Pin<const P: char, const N: u8, MODE>` — порт (`'A'`/`'B'`/`'F'`) и
  номер ноги живут в типе; `Pin` — ZST.
- Режимы как typestate: `Input`, `Analog`, `Alternate<const AF: u8>` (номер AF остаётся в
  типе), и `Output<PushPull>` / `Output<OpenDrain>` (тип выхода тоже в типе).
- Раздача ног: `dp.gpioa.split(&mut rcu)` (трейт `GpioExt`) поглощает синглтон порта,
  **включает его такт** и возвращает отдельные пины — уникальность каждой ноги гарантирована
  компилятором, а получить пины без такта нельзя. Заведены порты A, B и F (порт C на этом
  корпусе, QFN32, физически не разведён — сознательно пропущен).
- Переходы режимов: `into_input` / `into_output` (push-pull по умолчанию) /
  `into_push_pull_output` / `into_open_drain_output` / `into_analog`.
- Альтернативные функции с **проверкой на компиляции**: `into_alternate::<AF>()` — номер AF
  это const-параметр, сверяемый с per-pin картой (`ValidAf`); неверный номер не
  компилируется. Карта перенесена из datasheet (Table 2-13/2-14).
- `embedded-hal` 1.0: `OutputPin` / `InputPin` (`Error = Infallible`) — портируемые драйверы
  работают с этими пинами. `Output<OpenDrain>` тоже реализует `InputPin` (чтение состояния
  open-drain-линии, например для I2C clock-stretching). `StatefulOutputPin` (`is_set_high`/
  `is_set_low`, чтение `OCTL`) с переопределённым `toggle()` — переключает ногу через
  атомарный регистр `TG` вместо read-then-write реализации по умолчанию из крейта.
- Пер-пиновая настройка: `set_pull` (`PUD`), `set_speed` (`OSPD`).
- `PA13`/`PA14` (`SWDIO`/`SWCLK`) получили отдельный typestate `Debugger` вместо `Input` —
  по факту после сброса они реально в режиме SWD, а не floating input, и тип это честно
  отражает. `into_*`/сеттеры для `Debugger` недостижимы на этапе компиляции (гейт через
  трейт-маркер `Active`); единственный выход — `unsafe fn activate() -> Pin<P, N, Input>`,
  чистая смена типа без записи в регистры — реальную перенастройку ноги (`into_*()` следом)
  берёт на себя вызывающий код.
- Полная заморозка конфигурации: `lock()` выполняет LKK-последовательность записи в
  регистр `LOCK` и возвращает `Pin<P, N, Locked<MODE>>` — терминальный typestate (без
  `unlock`: по мануалу регистр остаётся залоченным до сброса чипа, программный «разлок»
  был бы ложью о состоянии железа). `Locked<MODE>` не реализует маркер `Active`, поэтому
  `into_*`/`set_pull`/`set_speed` недостижимы на этапе компиляции, а вот `OutputPin`/
  `InputPin`/`StatefulOutputPin` продолжают работать — обобщённо, через `impl<...> Trait
  for Pin<P, N, Locked<MODE>> where Pin<P, N, MODE>: Trait`, так что залоченный пин
  сохраняет ровно те возможности чтения/записи, что были у нелоченного `MODE`, без
  дублирования под каждый режим. Доступно только на портах `A`/`B` через трейт-маркер
  `HasLock` — у порта `F` физически нет регистра `LOCK`.

**RCU** (`src/rcu.rs`):

- `dp.rcu.constrain()` (трейт `RcuExt`) оборачивает сырой периферал в управляемый `Rcu`.
- Гейтинг тактов периферии: генерик-трейт `Enable` (`enable`/`disable`), реализуемый под
  каждую периферию макросом `bus!` (одна строка: тип периферии → регистр → бит) вместо
  ручных методов на `Rcu`. Вплетён в `split` — порт гарантированно затактован перед
  использованием.
- Дерево тактов: билдер `CFGR` (`CFGR::default()`) настраивает `sysclk`/`hclk`/`pclk1`/
  `pclk2`, а `.freeze(&mut rcu, &mut dp.fmc)` пишет регистры и возвращает замороженный
  `Clocks` (реальные получившиеся частоты). PLL от IRC8M поддержан (`PllFreq`, `8–72 МГц`
  с шагом `4 МГц`); прескейлеры шин — через `AhbPrescaler`/`ApbPrescaler`. Все три —
  **энумы, а не голый `u32`** — недостижимую частоту/делитель просто нельзя запросить, это
  ошибка компиляции, а не тихое округление. Wait state'ы flash (`FMC.ws().wscnt`)
  выставляются от получившегося `hclk` ДО переключения источника системного такта. `HXTAL`
  пока вне скоупа (кварц на плате не запаян).
- Сброс периферии: генерик-трейт `Reset` (`reset`), вплетён рядом с `Enable` той же строкой
  макроса `bus!` (`AHBRST`/`APB1RST`/`APB2RST`) — возвращает периферию в дефолт после сброса.
- Типизированные частоты (`src/time.rs`): tuple-структы `Hertz`/`KiloHertz`/`MegaHertz`/
  `Bps`/`MilliSeconds`/`MicroSeconds`, extension-трейт `U32Ext` (`.hz()`/`.mhz()`/...),
  `From`-конверсии между единицами и арифметика `Mul`/`Div`. Четыре геттера `Clocks`
  возвращают `Hertz` вместо голого `u32`.
- `CK_OUT`: `rcu.ck_out(src, div)` выводит внутренний тактовый узел на `PA8`/`PA9` —
  единственный способ вживую проверить реальную частоту осциллографом или логическим
  анализатором, раз на плате нет отладочного зонда. `CkOutSrc` покрывает весь
  мультиплексор `CKOUTSEL` (`None`/`Irc14m`/`Lsi40k`/`Lxtal`/`Sysclk`/`Irc8m`/`Hxtal`/
  `Pll(PllDiv)` — собственный делитель ветки PLL до мультиплексора приклеен внутрь варианта
  enum'а, так что его нельзя выставить для любого другого источника); `CkOutDiv` — общий
  делитель `1..128` после мультиплексора, действует на любой источник.
- `Usart0Sel` (`CFGR.usart0_sel`, часть того же потока `CFGR`/`freeze()`/`Clocks`, а не
  разовый метод на `Rcu`, как `ck_out` — выбор нужно *запомнить* для расчёта `baud` в
  `usart.rs`, а не просто применить один раз) позволяет тактовать `USART0` от `CK_SYS`/
  `CK_LXTAL`/`CK_IRC8M` вместо `pclk2`; получившаяся частота оседает в `Clocks::usart0()`,
  который читает `BusClocks`-реализация в `usart.rs` вместо жёсткого `pclk2`. `Lxtal`
  доступен в API, хотя на этой плате нет кварца `32.768 кГц` — выбор `Lxtal` без запуска
  `LXTAL` где-то ещё навсегда подвесит TX/RX USART0; HAL не может знать, что разведено на
  конкретной плате, поэтому эта опасность сознательно оставлена на совести вызывающего.

**USART** (`src/usart.rs`) — блокирующий + неблокирующий TX/RX, минимальный скоуп готов:

- `TxPin<USART>`/`RxPin<USART>` — trait-marker'ы, генерик по типу периферии (не по
  константе — один трейт покрывает и `Usart0`, и `Usart1`), заполнены макросом
  `usart_pins!` по таблице из уже выверенных комбинаций пин/AF в `pin_af!` (`gpio.rs`).
  Неверный пин или AF для конкретного `USARTX` просто не удовлетворяет биндингу —
  ловится на компиляции, а не тихо мёртвой линией UART.
  `Usart1` оказался прямым реэкспортом PAC-модуля `usart0`
  (`pub use self::usart0 as usart1;`) — обе периферии делят один тип `RegisterBlock`,
  ручной каст указателя не понадобился (в отличие от `Gpioa`/`Gpiob` — те совпадают по
  раскладке, но остаются разными типами).
- `BusClocks` — ещё один биндинг на уровне типов: `Usart0` тактуется от `pclk2` (APB2),
  `Usart1` — от `pclk1` (APB1); `USARTX::clock(&clocks)` возвращает нужный, так что
  расчёт `baud` не может случайно взять не ту шину.
- `Usart<USARTX, TX, RX, WORD = Byte>` владеет периферией и обоими пинами (хранит, не
  роняет — см. `.release()` ниже). 4-й параметр по умолчанию `Byte`, так что весь код
  с 3-параметровым `Usart<USARTX, TX, RX>` продолжает компилироваться без правок.
  `Usart::new(&mut rcu, usart, tx_pin, rx_pin, clocks, config)` включает такт,
  сбрасывает периферию, считает `BAUD` и включает `UEN`/`TEN`/`REN` (общая логика
  вынесена в приватную `configure()`, переиспользуется 9-битным конструктором ниже).
  - **`Oversampling::{X16, X8}`** — при ×16 весь 16-битный регистр `BAUD` оказывается
    равен `round(pclk / baud)` напрямую (`intdiv`/`fradiv` — просто его верхние 12 /
    нижние 4 бита). При ×8 формат регистра другой (реально используются только 3 бита
    дробной части, `BRR[3]` обязан быть `0`), поэтому значение раскладывается
    (`intdiv = w >> 3`, `fradiv8 = w & 0x7`) и собирается заново; `OVSMOD` в `CTL0`
    тоже нужно переключить — иначе периферия продолжит сэмплить по ×16 независимо от
    того, что записано в `BAUD`.
  - **`FrameFormat::{N8, E8, O8, E7, O7}`** — единственный источник правды сразу для
    `WL`/`PCEN`/`PM` (не отдельный `Parity` рядом: два независимых поля за одними и
    теми же тремя битами регистра рисковали бы разойтись — та же ловушка, что уже
    обходили с `BusClocks`). `E8`/`O8` выставляют `WL=1` (9-битный фрейм), так что бит
    чётности попадает в 9-й бит, за пределы `u8` — `write_byte`/`read_byte` вообще не
    пришлось менять, обрезание до `u8` и так его отбрасывает. `E7`/`O7` используют
    `WL=0`: чётность замещает бит 7 *внутри* `u8`, реальных данных остаётся только 7,
    поэтому `received_byte()` маскирует `& 0x7F` на приёме (не на передаче — периферия
    сама затирает старший бит вычисленной чётностью, что бы там ни было записано).
    Отдельного «`N7`» не существует — без чётности `WL=0` всегда даёт полные 8 бит,
    то есть `N8`; потолок в 7 бит появляется только когда чётность откусывает место у
    данных.
  - **`UsartConfig`** объединяет `baud`/`oversampling`/`frame_format` за fluent-сеттерами
    и `impl Default` (`115_200`/`X16`/`N8`) — по образцу `Config` из референсных HAL,
    раз в Rust нет дефолтных аргументов функций. В отличие от полей `CFGR` (`Option<T>`,
    `None` = «не трогать регистр»), поля `UsartConfig` — голые значения: каждый `new()`
    в любом случае пишет все три, «не трогать» тут смысла не имеет.
  - **`usart::baud`** — 15 именованных констант `u32` (`B110`..`B921600`, стандартный
    POSIX-набор), чисто для читаемости. `baud` остался голым `u32`, не `enum`: в
    отличие от `PllFreq`, достижимый диапазон не является маленьким фиксированным
    железом множеством, так что `enum` только запретил бы легитимные нестандартные
    скорости, не защитив ни от чего реально невозможного.
- **Чистый 9-битный режим, без чётности** (`WL=1, PCEN=0`) — `Usart<USARTX, TX, RX,
  Word>` через `Usart::new_word(&mut rcu, usart, tx_pin, rx_pin, clocks,
  UsartConfig9::default())` (конфиг без поля `frame_format` — для этого пути там
  нечего указывать осмысленного), с `write_word(&self, word: u16)`/`read_word(&self)
  -> Result<u16, ErrorKind>` (маска `& 0x1FF`, 9 значащих бит). `Byte`/`Word` —
  ZST-маркеры: `write_word`/`read_word` просто не существуют на `Usart<..., Byte>`, а
  `write_byte`/`read_byte` — на `Usart<..., Word>`; проверено не рассуждением, а
  живой компиляцией заведомо неверного вызова (`E0599: no method named write_word
  found`). Выбрано вместо незащищённой пары `u16`-методов на обычном 3-параметровом
  `Usart` именно потому, что множество форматов слова конечно и известно заранее (как
  у `PllFreq`), в отличие от `baud`/`pclk` — те рантайм-континуум, маркерным типом их
  осмысленно не огородить.
- `write_byte(&self, byte: u8)`/`read_byte(&self) -> Result<u8, ErrorKind>` — busy-wait
  на `TBE`/`RBNE`, потом запись/чтение `TDATA`/`RDATA`, с проверкой ошибок приёма (см.
  ниже) перед тем как доверять байту.
- `embedded_hal_nb::serial::{ErrorType, Read<u8>, Write<u8>}` — неблокирующие аналоги,
  сужены до `Usart<..., Byte>` (на `Word` были бы бессмысленны): та же проверка флага,
  но `if !flag { return Err(nb::Error::WouldBlock) }` вместо busy-wait. `Write::flush()`
  ждёт отдельный флаг `TC` (transmission complete — сдвиговый регистр физически
  опустел, не то же самое, что `TBE`, который лишь говорит «буфер освободился под
  следующий байт»). Инхерентный `flush(&self)` рядом НЕ заводили — та же ловушка
  разрешения инхерентных/трейтовых методов, что уже была в GPIO, так что трейтовый
  `flush()` остался единственной точкой входа.
- **Обработка ошибок приёма**: напрямую `embedded_hal_nb::serial::ErrorKind` как
  `ErrorType::Error` — без собственного enum ошибок. `ErrorKind` уже покрывает
  `Overrun`/`FrameFormat`/`Parity`/`Noise` (плюс `Other`) и сам реализует
  `serial::Error` (`kind()` — тождество), так что обёртка своим типом только бы его
  задублировала. Сбрасывается через **отдельный** регистр `USART_INTC` (запись `1` в
  `OREC`/`NEC`/`FEC`/`PEC`), не через паттерн «прочитать STAT, потом DATA», как часто
  бывает на STM32. Сброс флага ошибки сам по себе `RBNE` **не** снимает (разные биты в
  разных регистрах) — приватный хелпер `take_error()` при ошибке дополнительно делает
  пустое чтение `RDATA`, чисто ради сброса `RBNE`, иначе следующий приём завис бы на
  стухшем флаге. (`ErrorKind::FrameFormat` — вариант чужого enum'а «ошибка кадра» — и
  наш тип `FrameFormat` — конфигурация формата слова — совпадают по имени чисто
  случайно, разные namespace'ы, реального пересечения нет.)
- `release(self) -> (USARTX, TX, RX)` — выключает `UEN` и отдаёт периферию и оба пина
  обратно простым кортежем (без попытки «вернуть» их в `Parts` из `GpioExt` — так
  typestate-модель не работает: once пин вынесен из `Parts`, это просто значение,
  которое ты держишь там, где удобно, как и в любом другом HAL этой экосистемы).
  Generic по `WORD` — одна реализация на `Byte` и `Word`. Свежий
  `Usart::new()`/`new_word()` и так делает полный `enable`+`reset`, так что `release()`
  это не дублирует.

```rust
use embedded_hal::digital::OutputPin;
use gd32e230_hal::gpio::GpioExt;
use gd32e230_hal::rcu::{RcuExt, CFGR, PllFreq};
use gd32e230_hal::usart::{Usart, UsartConfig};

let mut dp = gd32e230::Peripherals::take().unwrap();
let mut rcu = dp.rcu.constrain();
let clocks = CFGR::default()
    .sysclk(PllFreq::Mhz48)              // PLL от IRC8M -> 48 МГц sysclk
    .freeze(&mut rcu, &mut dp.fmc);
let parts = dp.gpioa.split(&mut rcu);        // включает такт GPIOA
let mut led = parts.pa5.into_output();
led.set_high().unwrap();

let tx_pin = parts.pa9.into_alternate::<1>();    // USART0_TX; ::<3>() не скомпилируется
let rx_pin = parts.pa10.into_alternate::<1>();   // USART0_RX
let usart0 = Usart::new(&mut rcu, dp.usart0, tx_pin, rx_pin, clocks, UsartConfig::default());
if let Ok(byte) = usart0.read_byte() {
    usart0.write_byte(byte);                      // проверено на железе: приходит эхом
}
```

### Ограничения проекта

- **PAC-only база, без сторонних HAL.**
- **Нет отладочного зонда** — прошивка только через UART-бутлоадер (GD32 All-In-One
  Programmer), вывод через USART0 (115200 8N1). Никаких RTT / `defmt` / semihosting.
- Target `thumbv8m.base-none-eabi`, флеш 64K / ОЗУ 8K.

### Сборка

```sh
cargo build --release
cargo bin            # -> firmware.bin (нужны cargo-binutils + llvm-tools)
```

Прошивать на `0x08000000`, читать лог в терминале @ 115200 8N1.

> На Windows без Visual Studio host переключён на GNU-toolchain (см.
> `rust-toolchain.toml`), чтобы не требовался MSVC-линкер.

### Roadmap

- [x] Трейт `GpioExt` — идиоматичный `dp.gpioa.split()`.
- [x] `into_alternate` — альтернативные функции ног (регистр `AFSEL`).
- [x] Реализация трейтов `embedded-hal` 1.0 (`OutputPin` / `InputPin`).
- [x] Настройка `OMODE` / `OSPD` / `PUD` (push-pull/open-drain, скорость, подтяжки).
- [x] Тип выхода как typestate (`Output<PushPull>` / `Output<OpenDrain>`).
- [x] Const-generic пины + проверка альтернативных функций на компиляции.
- [x] RCU: гейтинг тактов периферии, обязательный на `split`.
- [x] `StatefulOutputPin` (`toggle` через атомарный регистр `TG` / `is_set_*`).
- [x] Порт F (`PF0`/`PF1`); порт C пропущен (не разведён на этом корпусе).
- [x] Typestate `Debugger` для `PA13`/`PA14` (ноги SWD), гейт через трейт-маркер.
- [x] RCU: дерево тактов — PLL от IRC8M, прескейлеры AHB/APB, flash wait states,
      типизированный API `CFGR` (энумы `PllFreq`/`AhbPrescaler`/`ApbPrescaler`, не голый `u32`).
- [x] RCU: трейт `Reset` для периферии (`AHBRST`/`APB1RST`/`APB2RST`).
- [x] Типизированные частоты (`src/time.rs`, `Hertz` и семейство) интегрированы в `Clocks`.
- [x] RCU: `CK_OUT` (вывод внутреннего тактового сигнала на `PA8`/`PA9`).
- [x] GPIO `LOCK` (заморозка конфигурации через typestate `Locked<MODE>`).
- [x] USART: блокирующий + `embedded-hal-nb` TX/RX (`Usart<USARTX, TX, RX>`,
      `TxPin`/`RxPin`, `BusClocks`), обработка ошибок приёма (`Overrun`/`Noise`/
      `Framing`/`Parity`), `.release()` — проверено на железе echo-лупом.
- [x] USART: ×8-oversampling, `USART0SEL` (альтернативный источник такта через
      `CFGR`/`Clocks`), конфигурация формата слова `FrameFormat::{N8, E8, O8, E7,
      O7}`, `UsartConfig` (fluent + `Default`) и именованные константы `usart::baud` —
      проверено сборкой.
- [x] USART: чистый 9-битный режим без чётности — typestate `Usart<USARTX, TX, RX,
      Word>` (маркеры `Byte`/`Word`, 4-й параметр типа с дефолтом), `new_word`/
      `write_word`/`read_word`; собственный `Error` заменён на
      `embedded_hal_nb::serial::ErrorKind` напрямую — проверено сборкой, на железе
      ещё не перепрошито.
- [ ] USART: аппаратное управление потоком (`CTS`/`RTS`) — отложено, низкий приоритет.
- [ ] RCU: источник `HXTAL` (нужен внешний кварц на плате).
- [ ] Периферия: таймеры / PWM, ADC, SPI, I²C.
- [ ] Вынос HAL в отдельный крейт-библиотеку.
- [ ] Поддержка других вариантов корпуса/пинаута GD32E230x (будущее, низкий приоритет).

### Регистры

`gd32e2` сгенерирован из патченных SVD; имена полей стоит сверять по
`docs/GD32E23x_User_Manual.pdf` (см. `docs/README.md` — PDF лежат локально, в git не
входят).
