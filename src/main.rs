mod cpu;
#[cfg(test)]
mod dormann;
mod instructions;
mod ram;

use crate::cpu::CPU;
use crate::ram::Memory;

fn main() {
    let mut cpu = CPU::new();
    cpu.reset();
}
