use crate::cpu::CPU;

impl CPU {
    pub fn iny(&mut self, cycles: &mut u32) {
        self.y = self.y.wrapping_add(1);
        self.status.zero = self.y == 0;
        self.status.negative = (self.y & 0x80) != 0;

        *cycles = cycles.wrapping_add(1);
    }
}
