# gd32e230-hal

[English](#english) · [Русский](#русский)

---

## English

A hardware abstraction layer (HAL) for the **GD32E230K8U6** microcontroller
(Cortex-M23 core), written in Rust from scratch on top of the
[`gd32e2`](https://crates.io/crates/gd32e2) PAC.

> ⚠️ **Work in progress.** The HAL is written incrementally and by hand — to
> genuinely understand both the hardware and Rust's type system. The API is
> unstable; the crate currently lives as a module inside the project (`src/hal/`)
> and will later be extracted into a standalone library.

### Philosophy

There is no full-featured HAL for the GD32E230 in the Rust ecosystem, so the raw
PAC (`gd32e2` — direct register access) is used as the base, and a safe, ergonomic
layer is built on top. Principles:

- **Errors at compile time, not on the board.** A pin's mode is encoded in its type
  (typestate): `Pin<Input>` physically has no `set_high` method, and the ownership
  system won't let you reconfigure a single pin twice.
- **Zero-cost.** The abstractions compile down to the same register writes as
  hand-written PAC code: no runtime overhead, `Pin<Output>` is 2 bytes.
- **`#![no_std]`, no heap.**

### What's implemented

**GPIO** (`src/hal/gpio.rs`):

- Typestate pins `Pin<MODE>` with modes `Input` / `Output` / `Analog`.
- Handing out pins: `split_gpioa` / `split_gpiob` consume the port singleton and
  return a set of individual pins — each pin's uniqueness is guaranteed by the compiler.
- Mode transitions: `into_output` / `into_input` / `into_analog`.
- Output: `set_high` / `set_low` — atomically via the `BOP` / `BC` registers.
- Input: `is_high` / `is_low` — via `ISTAT`.

```rust
let dp = gd32e230::Peripherals::take().unwrap();
let parts = split_gpioa(dp.gpioa);
let mut led = parts.pa5.into_output();
led.set_high();
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

- [ ] `GpioExt` trait + associated types — idiomatic `dp.gpioa.split()`.
- [ ] `into_alternate` — alternate pin functions (AF0..7, `AFSEL` register).
- [ ] `embedded-hal` 1.0 trait impls (`OutputPin` / `InputPin`) for compatibility
      with portable drivers.
- [ ] `OMODE` / `OSPD` / `PUD` configuration (push-pull/open-drain, speed, pulls).
- [ ] Peripherals: USART, timers / PWM, ADC, SPI, I²C.
- [ ] Extract the HAL into a standalone library crate.

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
> понимания и железа, и системы типов Rust. API нестабилен, крейт пока живёт как
> модуль внутри проекта (`src/hal/`), позже будет вынесен в отдельную библиотеку.

### Философия

Полноценного HAL для GD32E230 в экосистеме Rust нет, поэтому за основу взят «сырой»
PAC (`gd32e2` — прямой доступ к регистрам), а поверх строится безопасный и
эргономичный слой. Принципы:

- **Ошибки — на этапе компиляции, а не на плате.** Режим ноги закодирован в её типе
  (typestate): у `Pin<Input>` физически нет метода `set_high`, а перенастроить одну
  ногу дважды не даст система владения.
- **Zero-cost.** Абстракции компилируются в те же записи в регистры, что и ручной
  PAC-код: никакого оверхеда в рантайме, `Pin<Output>` весит 2 байта.
- **`#![no_std]`, без кучи.**

### Что уже есть

**GPIO** (`src/hal/gpio.rs`):

- Typestate-пины `Pin<MODE>` с режимами `Input` / `Output` / `Analog`.
- Раздача ног: `split_gpioa` / `split_gpiob` поглощают синглтон порта и возвращают
  набор отдельных пинов — уникальность каждой ноги гарантирована компилятором.
- Переходы режимов: `into_output` / `into_input` / `into_analog`.
- Выход: `set_high` / `set_low` — атомарно через регистры `BOP` / `BC`.
- Вход: `is_high` / `is_low` — через `ISTAT`.

```rust
let dp = gd32e230::Peripherals::take().unwrap();
let parts = split_gpioa(dp.gpioa);
let mut led = parts.pa5.into_output();
led.set_high();
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

- [ ] Трейт `GpioExt` + ассоциированные типы — идиоматичный `dp.gpioa.split()`.
- [ ] `into_alternate` — альтернативные функции ног (AF0..7, регистр `AFSEL`).
- [ ] Реализация трейтов `embedded-hal` 1.0 (`OutputPin` / `InputPin`) для
      совместимости с портируемыми драйверами.
- [ ] Настройка `OMODE` / `OSPD` / `PUD` (push-pull/open-drain, скорость, подтяжки).
- [ ] Периферия: USART, таймеры / PWM, ADC, SPI, I²C.
- [ ] Вынос HAL в отдельный крейт-библиотеку.

### Регистры

`gd32e2` сгенерирован из патченных SVD; имена полей стоит сверять по
`docs/GD32E23x_User_Manual.pdf` (см. `docs/README.md` — PDF лежат локально, в git не
входят).
