mod cpu;
#[cfg(test)]
mod dormann;
#[cfg(test)]
mod harte;
mod instructions;
#[cfg(test)]
mod interrupt_test;
mod ram;

use crate::cpu::CPU;
use crate::ram::Memory;

fn main() {
    let mut cpu = CPU::new();

    // The monitor's ROM: the top 8K, its vectors included.
    let rom = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/roms/monitor.bin"))
        .expect("roms/monitor.bin: run make in roms/");
    cpu.memory.data[0x10000 - rom.len()..].copy_from_slice(&rom);

    cpu.reset();
    cpu.execute_forever();
}
