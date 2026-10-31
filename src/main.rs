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

use crate::cpu::CPU;
use crate::ram::Memory;
use crate::terminal::Terminal;

fn main() {
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
