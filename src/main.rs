mod apple2;
#[cfg(test)]
mod apple2_test;
mod bus;
#[cfg(test)]
mod bus_test;
#[cfg(test)]
mod clock_test;
mod cpu;
mod display;
#[cfg(test)]
mod dormann;
#[cfg(test)]
mod harte;
mod instructions;
#[cfg(test)]
mod interrupt_test;
mod ram;
mod terminal;
mod window;

use crate::apple2::Apple2;
use crate::cpu::CPU;
use crate::ram::Memory;
use crate::terminal::Terminal;
use crate::window::Window;

/// The machine to run: `cpu_emu` for this one, with BASIC; `cpu_emu apple2
/// ROM` for an Apple II+, ROM a dump of its ROM (its last 12K, $D000 up),
/// in a window, or with `--text` its text screen in the terminal.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("apple2") => {
            let rom = args.get(2).expect("cpu_emu apple2 ROM [--text]: a dump of the II+'s ROM");
            apple2(rom, args.iter().any(|arg| arg == "--text"));
        }
        _ => basic(),
    }
}

fn apple2(rom: &str, text: bool) {
    let rom = std::fs::read(rom).unwrap_or_else(|e| panic!("{rom}: {e}"));
    assert!(rom.len() >= 0x3000, "the ROM is 12K, $D000 to $FFFF");
    let mut apple = Apple2::new(rom[rom.len() - 0x3000..].to_vec());
    if text {
        apple.terminal = Some(Terminal::new());
        print!("\x1B[2J"); // the screen cleared, for the Apple's
    } else {
        let (width, height) = (apple2::video::WIDTH, apple2::video::HEIGHT);
        apple.display = Some(Box::new(Window::new("Apple ][", width, height, 3)));
    }

    let mut cpu = CPU::with_bus(apple);
    cpu.clock_hz = apple2::CLOCK_HZ;
    cpu.reset();
    cpu.execute_forever();
}

fn basic() {
    let mut cpu = CPU::new();

    // BASIC's ROM: the top 16K, its vectors included; RAM below it.
    let rom = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/roms/basic.bin"))
        .expect("roms/basic.bin: run tools/fetch-msbasic");
    let rom_start = 0x10000 - rom.len();
    cpu.memory.data[rom_start..].copy_from_slice(&rom);
    cpu.memory.rom_start = Some(rom_start as u16);
    cpu.memory.terminal = Some(Terminal::new());

    cpu.reset();
    cpu.execute_forever();
}
