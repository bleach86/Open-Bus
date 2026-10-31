mod apple2;
#[cfg(test)]
mod apple2_test;
mod bus;
#[cfg(test)]
mod bus_test;
#[cfg(test)]
mod clock_test;
mod cpu;
#[cfg(test)]
mod dormann;
#[cfg(test)]
mod harte;
mod instructions;
#[cfg(test)]
mod interrupt_test;
mod ram;
mod terminal;

use crate::apple2::Apple2;
use crate::cpu::CPU;
use crate::ram::Memory;
use crate::terminal::Terminal;

/// The machine to run: `cpu_emu` for this one, with BASIC; `cpu_emu apple2
/// ROM` for an Apple II+, ROM a dump of its ROM (its last 12K, $D000 up).
fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("apple2") => apple2(args.get(2).expect("cpu_emu apple2 ROM: a dump of the II+'s ROM")),
        _ => basic(),
    }
}

fn apple2(rom: &str) {
    let rom = std::fs::read(rom).unwrap_or_else(|e| panic!("{rom}: {e}"));
    assert!(rom.len() >= 0x3000, "the ROM is 12K, $D000 to $FFFF");
    let mut apple = Apple2::new(rom[rom.len() - 0x3000..].to_vec());
    apple.terminal = Some(Terminal::new());

    let mut cpu = CPU::with_bus(apple);
    cpu.clock_hz = apple2::CLOCK_HZ;
    print!("\x1B[2J"); // the screen cleared, for the Apple's
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
