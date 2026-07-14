# gd32e230-hal

[English](#english) · [Русский](#русский)

---

## English

A hardware abstraction layer (HAL) for the **GD32E230K8U6** microcontroller
(Cortex-M23 core), written in Rust from scratch on top of the
[`gd32e2`](https://crates.io/crates/gd32e2) PAC.

> ⚠️ **Work in progress.** The HAL is written incrementally and by hand — to
> genuinely understand both the hardware and Rust's type system. The API is
> unstable. The package is a library crate (`src/lib.rs` → `gpio`, `rcu` modules)
> plus a small on-hardware test bench binary (`src/main.rs`); the HAL will later be
> extracted into a standalone library.

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

```rust
use embedded_hal::digital::OutputPin;
use gd32e230_hal::gpio::GpioExt;
use gd32e230_hal::rcu::{RcuExt, CFGR, PllFreq};

let mut dp = gd32e230::Peripherals::take().unwrap();
let mut rcu = dp.rcu.constrain();
let _clocks = CFGR::default()
    .sysclk(PllFreq::Mhz48)              // PLL from IRC8M -> 48 MHz sysclk
    .freeze(&mut rcu, &mut dp.fmc);
let parts = dp.gpioa.split(&mut rcu);        // enables the GPIOA clock
let mut led = parts.pa5.into_output();
led.set_high().unwrap();

let _tx = parts.pa9.into_alternate::<1>();   // USART0_TX; ::<3>() would not compile
```

### Project constraints

- **PAC-only base, no third-party HAL.**
- **No debug probe** — flashing only via the UART bootloader (GD32 All-In-One
  Programmer); all output over USART0 (115200 8N1). No RTT / `defmt` / semihosting.
- Target `thumbv8m.base-none-eabi`, flash 48K / RAM 8K.

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
- [ ] RCU: `HXTAL` clock source (needs an external crystal on the board).
- [ ] Peripherals: USART, timers / PWM, ADC, SPI, I²C.
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
> библиотечный крейт (`src/lib.rs` → модули `gpio`, `rcu`) плюс небольшой бинарь-стенд
> для проверки на железе (`src/main.rs`); позже HAL будет вынесен в отдельную библиотеку.

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

```rust
use embedded_hal::digital::OutputPin;
use gd32e230_hal::gpio::GpioExt;
use gd32e230_hal::rcu::{RcuExt, CFGR, PllFreq};

let mut dp = gd32e230::Peripherals::take().unwrap();
let mut rcu = dp.rcu.constrain();
let _clocks = CFGR::default()
    .sysclk(PllFreq::Mhz48)              // PLL от IRC8M -> 48 МГц sysclk
    .freeze(&mut rcu, &mut dp.fmc);
let parts = dp.gpioa.split(&mut rcu);        // включает такт GPIOA
let mut led = parts.pa5.into_output();
led.set_high().unwrap();

let _tx = parts.pa9.into_alternate::<1>();   // USART0_TX; ::<3>() не скомпилируется
```

### Ограничения проекта

- **PAC-only база, без сторонних HAL.**
- **Нет отладочного зонда** — прошивка только через UART-бутлоадер (GD32 All-In-One
  Programmer), вывод через USART0 (115200 8N1). Никаких RTT / `defmt` / semihosting.
- Target `thumbv8m.base-none-eabi`, флеш 48K / ОЗУ 8K.

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
- [ ] RCU: источник `HXTAL` (нужен внешний кварц на плате).
- [ ] Периферия: USART, таймеры / PWM, ADC, SPI, I²C.
- [ ] Вынос HAL в отдельный крейт-библиотеку.
- [ ] Поддержка других вариантов корпуса/пинаута GD32E230x (будущее, низкий приоритет).

### Регистры

`gd32e2` сгенерирован из патченных SVD; имена полей стоит сверять по
`docs/GD32E23x_User_Manual.pdf` (см. `docs/README.md` — PDF лежат локально, в git не
входят).
