//! The emulator: the CPU, the chips, the machines and the host's side,
//! for the program to run (`main.rs`), or a web page (a web build leaves out
//! the window, and has its files in memory).

pub mod cpu;

/// The chips the machines are made of, besides the CPU.
pub mod chips {
    pub mod acia6551;
    pub mod ay38910;
    pub mod pokey;
    pub mod riot6532;
    pub mod tia_sound;
    pub mod via6522;
    pub mod ym2151;

    #[cfg(test)]
    mod tests {
        mod acia6551;
        mod ay38910;
        mod pokey;
        mod riot6532;
        mod via6522;
        mod ym2151;
    }
}

/// The command line: what's to run, and how, as the program and Android's
/// game screen are given it.
#[cfg(not(target_arch = "wasm32"))]
pub mod cli;

/// The host's side: what drives a machine, a frame at a time, and all that
/// any machine shares: its window, sound and controls, its terminal, its
/// notices, its save states.
pub mod host {
    pub mod clock;
    pub mod controls;
    pub mod display;
    #[cfg_attr(target_arch = "wasm32", path = "files_web.rs")]
    pub mod files;
    pub mod font;
    pub mod machine;
    pub mod md5;
    pub mod modem;
    pub mod notice;
    #[cfg(not(target_arch = "wasm32"))]
    pub mod pads;
    pub mod page;
    pub mod run;
    pub mod save_state;
    pub mod sound;
    pub mod stats;
    pub mod terminal;
    #[cfg(not(target_arch = "wasm32"))]
    pub mod window;
    pub mod zip;

    #[cfg(test)]
    mod tests {
        mod clock;
        mod controls;
        mod files;
        mod machine;
        mod md5;
        mod modem;
        mod notice;
        mod page;
        mod run;
        mod stats;
        #[cfg(not(target_arch = "wasm32"))]
        mod window;
        mod zip;
    }
}

/// The machines the CPU runs in.
pub mod machines {
    pub mod apple2;
    pub mod atari2600;
    pub mod atari7800;
    pub mod basic;
    pub mod facts;
    pub mod light_gun;
    pub mod nes;
    pub mod region;
    pub mod skink;
    pub mod x16;
}

/// What a web page calls.
#[cfg(target_arch = "wasm32")]
pub mod web;
