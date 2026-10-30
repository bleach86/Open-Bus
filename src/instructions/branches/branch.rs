use crate::cpu::CPU;

impl CPU {
    /// A branch: the offset read; taken, a cycle to add it (reading the
    /// next opcode, unused), and another when it crosses a page (reading
    /// at the address before the high byte is fixed).
    pub fn branch(&mut self, taken: bool, cycles: &mut u32) {
        let offset = self.fetch_byte(cycles) as i8;
        if !taken {
            return;
        }
        self.read_byte(self.pc, cycles);
        let target = self.pc.wrapping_add(offset as u16);
        if target & 0xFF00 != self.pc & 0xFF00 {
            self.read_byte(self.pc & 0xFF00 | target & 0x00FF, cycles);
        }
        self.pc = target;
    }
}
