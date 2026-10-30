use crate::cpu::CPU;

impl CPU {
    pub fn pha(&mut self, cycles: &mut u32) {
        self.push_byte(self.a, cycles);
        *cycles = cycles.wrapping_add(1);
    }
}
