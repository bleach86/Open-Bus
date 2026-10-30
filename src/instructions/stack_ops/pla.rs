use crate::cpu::CPU;

impl CPU {
    pub fn pla(&mut self, cycles: &mut u32) {
        self.a = self.pop_byte(cycles);
        *cycles = cycles.wrapping_add(2);
        self.status.zero = self.a == 0;
        self.status.negative = (self.a & 0x80) != 0;
    }
}
