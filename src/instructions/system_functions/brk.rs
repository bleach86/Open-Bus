use crate::cpu::CPU;

impl CPU {
    /// BRK: the byte after it read (and skipped), the return address and
    /// the status with B set pushed, I set, then on through $FFFE.
    pub fn brk(&mut self, cycles: &mut u32) {
        self.fetch_byte(cycles);
        self.push_word(self.pc, cycles);
        self.status.break_command = true;
        let status = self.status.to_byte();
        self.push_byte(status, cycles);
        // As for an IRQ: the handler isn't interrupted.
        self.status.interrupt_disable = true;
        self.pc = self.read_word(0xFFFE, cycles);
    }
}

#[cfg(test)]
mod tests {
    use crate::cpu::CPU;

    /// BRK pushes PC+2 and the status with B set, sets I, and goes through
    /// $FFFE; the CPU carries on from there.
    #[test]
    fn brk_sets_interrupt_disable() {
        let mut cpu = CPU::new();
        cpu.pc = 0x0201; // past the BRK opcode, as fetched
        cpu.sp = 0xFF;
        cpu.status.interrupt_disable = false;
        let mut cycles = u32::MAX;
        cpu.brk(&mut cycles);
        assert!(cpu.status.interrupt_disable);
        assert_eq!(cpu.memory.read(0x01FD) & 0x10, 0x10, "B set on the stack");
        assert_eq!(cpu.memory.read(0x01FD) & 0x04, 0, "I as it was, on the stack");
        assert_eq!(u16::from_le_bytes([cpu.memory.read(0x01FE), cpu.memory.read(0x01FF)]), 0x0202);
    }
}
