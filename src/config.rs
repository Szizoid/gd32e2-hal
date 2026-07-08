//! Все настраиваемые параметры прошивки. Производные значения считаются из
//! исходных на этапе компиляции — меняй только исходные.

/// Тактовая частота ядра и APB2 (встроенный IRC8M).
pub const SYSCLK_HZ: u32 = 8_000_000;

// --- АЦП ---

/// Канал рабочего сигнала: PA5 / ADC_IN5.
pub const CH_WORK: u8 = 5;
/// Канал опорного сигнала: PA7 / ADC_IN7.
pub const CH_REF: u8 = 7;

// При смене каналов правь также gpio.rs (аналоговый режим ножки) и adc.rs
// (время выборки spt5/spt7) — PAC задаёт их отдельными методами.

/// Пауза стабилизации АЦП после включения, в тактах ядра.
pub const ADC_STAB_DELAY: u32 = 8_000;

/// Опорное напряжение АЦП, мВ (соответствует коду 4095).
pub const VREF_MV: u32 = 3300;
/// Максимальный код 12-битного АЦП.
pub const ADC_FULL_SCALE: u32 = 4095;

// --- USART ---

/// Скорость UART, бод.
pub const BAUD: u32 = 115_200;

// USARTDIV (16·USARTDIV = f_ck/baud) как целая часть (INTDIV) + дробная в 1/16 (FRADIV).
const USART_DIV: u32 = SYSCLK_HZ / BAUD;

pub const USART_INTDIV: u16 = (USART_DIV >> 4) as u16;
pub const USART_FRADIV: u8 = (USART_DIV & 0xF) as u8;

// --- ШИМ ИК-источника (TIMER2_CH3 → PB1) ---

// high = MOSFET открыт = ИК включён. Меандр 50%. Ножка задаётся в gpio.rs (AF1).

/// Частота ШИМ, Гц.
pub const PWM_FREQ_HZ: u32 = 3;

/// Предделитель таймера (в регистр пишется PSC = делитель − 1). Тик = SYSCLK/(PSC+1).
pub const PWM_PSC: u16 = 249;

/// Период счёта − 1 (авто-перезагрузка).
pub const PWM_ARR: u16 = (SYSCLK_HZ / ((PWM_PSC as u32 + 1) * PWM_FREQ_HZ) - 1) as u16;

/// Значение сравнения (скважность). Выход высокий, пока CNT < PWM_CCR.
pub const PWM_CCR: u16 = ((PWM_ARR as u32 + 1) / 2) as u16;

// --- Прочее ---

/// Размер буфера строки лога.
pub const LINE_LEN: usize = 28;
