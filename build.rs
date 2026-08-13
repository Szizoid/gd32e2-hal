//! Turns the selected chip feature into the two things the build needs from it:
//! a `memory.x` for the linker, and the `cfg` flags the source gates on.
//!
//! A feature is named after the part with an `x` in every field the code cannot
//! see: temperature grade always, package wherever it bonds the same pads. The
//! 32-pin parts are the exception that needs its package spelled out — a QFN32
//! carries VSS on its thermal pad and spends the two freed pins on PB2 and PB8.

use std::env;
use std::fs;
use std::path::PathBuf;

/// Every part of the series: feature name, flash code, pad set, flash and SRAM in
/// KiB.
///
/// The flash code also gives the SRAM size (4 -> 4K, 6 -> 6K, 8 -> 8K), but both
/// are spelled out rather than derived — a future part that breaks the pattern
/// should be a new row here, not a special case in code.
const CHIPS: &[Chip] = &[
    Chip::new("gd32e230f4xx", Flash::X4, Pads::Pins20, Timers::Four, 16, 4),
    Chip::new("gd32e230f6xx", Flash::X6, Pads::Pins20, Timers::Four, 32, 6),
    Chip::new("gd32e230f8xx", Flash::X8, Pads::Pins20, Timers::Four, 64, 8),
    Chip::new("gd32e230e8xx", Flash::X8, Pads::Pins24, Timers::Four, 64, 8),
    Chip::new("gd32e230g4xx", Flash::X4, Pads::Pins28, Timers::Four, 16, 4),
    Chip::new("gd32e230g6xx", Flash::X6, Pads::Pins28, Timers::Four, 32, 6),
    Chip::new("gd32e230g8xx", Flash::X8, Pads::Pins28, Timers::Five, 64, 8),
    Chip::new("gd32e230k4tx", Flash::X4, Pads::Lqfp32, Timers::Four, 16, 4),
    Chip::new("gd32e230k4ux", Flash::X4, Pads::Qfn32, Timers::Four, 16, 4),
    Chip::new("gd32e230k6tx", Flash::X6, Pads::Lqfp32, Timers::Four, 32, 6),
    Chip::new("gd32e230k6ux", Flash::X6, Pads::Qfn32, Timers::Four, 32, 6),
    Chip::new("gd32e230k8tx", Flash::X8, Pads::Lqfp32, Timers::Five, 64, 8),
    Chip::new("gd32e230k8ux", Flash::X8, Pads::Qfn32, Timers::Five, 64, 8),
    Chip::new("gd32e230c4xx", Flash::X4, Pads::Pins48, Timers::Four, 16, 4),
    Chip::new("gd32e230c6xx", Flash::X6, Pads::Pins48, Timers::Four, 32, 6),
    Chip::new("gd32e230c8xx", Flash::X8, Pads::Pins48, Timers::Five, 64, 8),
];

/// Where the two memories live on every part of the series.
const FLASH_ORIGIN: &str = "0x08000000";
const RAM_ORIGIN: &str = "0x20000000";

/// Flash code from the part number, which is what the AF map differs by
/// (datasheet Table 2-13/2-14 footnotes).
#[derive(Clone, Copy)]
enum Flash {
    X4,
    X6,
    X8,
}

impl Flash {
    const ALL: &'static [Flash] = &[Flash::X4, Flash::X6, Flash::X8];

    /// Name of the `chip_*` cfg flag standing for this die.
    const fn flag(self) -> &'static str {
        match self {
            Flash::X4 => "chip_x4",
            Flash::X6 => "chip_x6",
            Flash::X8 => "chip_x8",
        }
    }
}

/// The pad sets of the series, ascending — each one contains every set before it
/// (datasheet figures 2-2 … 2-9).
///
/// QFN32 sits between LQFP32 and the 48-pin parts, bonding the same pads plus PB2
/// and PB8. Because the sets nest, the flags handed to the source are "at least
/// this pad set" and a pin needs exactly one gate.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Pads {
    Pins20,
    Pins24,
    Pins28,
    Lqfp32,
    Qfn32,
    Pins48,
}

impl Pads {
    /// Ascending, so a part gets every flag up to and including its own set.
    const ALL: &'static [Pads] = &[
        Pads::Pins20,
        Pads::Pins24,
        Pads::Pins28,
        Pads::Lqfp32,
        Pads::Qfn32,
        Pads::Pins48,
    ];

    /// Name of the `pads_ge_*` cfg flag standing for this set.
    const fn flag(self) -> &'static str {
        match self {
            Pads::Pins20 => "pads_ge_20",
            Pads::Pins24 => "pads_ge_24",
            Pads::Pins28 => "pads_ge_28",
            Pads::Lqfp32 => "pads_ge_lqfp32",
            Pads::Qfn32 => "pads_ge_qfn32",
            Pads::Pins48 => "pads_ge_48",
        }
    }
}

/// How many general-purpose timers the part carries (datasheet Table 1-1). The
/// fifth one is `TIMER14`, and which parts have it does NOT follow from the flash
/// code: the 20- and 24-pin x8 parts stop at four.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Timers {
    Four,
    Five,
}

struct Chip {
    feature: &'static str,
    flash: Flash,
    /// The largest pad set this part bonds.
    pads: Pads,
    timers: Timers,
    flash_kb: u32,
    sram_kb: u32,
}

impl Chip {
    const fn new(
        feature: &'static str,
        flash: Flash,
        pads: Pads,
        timers: Timers,
        flash_kb: u32,
        sram_kb: u32,
    ) -> Self {
        Self {
            feature,
            flash,
            pads,
            timers,
            flash_kb,
            sram_kb,
        }
    }

    /// Whether Cargo enabled this part's feature.
    fn enabled(&self) -> bool {
        env::var_os(format!("CARGO_FEATURE_{}", self.feature.to_uppercase())).is_some()
    }
}

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    for flash in Flash::ALL {
        println!("cargo::rustc-check-cfg=cfg({})", flash.flag());
    }
    for pads in Pads::ALL {
        println!("cargo::rustc-check-cfg=cfg({})", pads.flag());
    }
    println!("cargo::rustc-check-cfg=cfg(has_timer14)");

    let selected: Vec<&Chip> = CHIPS.iter().filter(|chip| chip.enabled()).collect();
    let chip = match selected.as_slice() {
        [chip] => chip,
        // A build script cannot see a `compile_error!`, and by the time one fires
        // the linker script is already missing, so the check lives here too.
        [] => panic!("select a chip: enable exactly one of the gd32e230* features"),
        several => panic!(
            "the gd32e230* features are mutually exclusive, but {} are enabled: {}",
            several.len(),
            several
                .iter()
                .map(|chip| chip.feature)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };

    println!("cargo::rustc-cfg={}", chip.flash.flag());
    for pads in Pads::ALL.iter().filter(|set| **set <= chip.pads) {
        println!("cargo::rustc-cfg={}", pads.flag());
    }
    if chip.timers == Timers::Five {
        println!("cargo::rustc-cfg=has_timer14");
    }

    // The linker resolves `INCLUDE memory.x` against the current directory first
    // and the search paths after, so a `memory.x` in the project root still wins
    // — that is the escape hatch for a board this table does not describe.
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    fs::write(
        out_dir.join("memory.x"),
        format!(
            "/* Generated by build.rs from the `{}` feature. */\n\
             MEMORY\n\
             {{\n\
             \x20   FLASH : ORIGIN = {}, LENGTH = {}K\n\
             \x20   RAM   : ORIGIN = {}, LENGTH = {}K\n\
             }}\n",
            chip.feature, FLASH_ORIGIN, chip.flash_kb, RAM_ORIGIN, chip.sram_kb
        ),
    )
    .expect("write memory.x");
    println!("cargo::rustc-link-search={}", out_dir.display());
}
