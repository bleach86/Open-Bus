use crate::cpu::CPU;

impl CPU {
    pub fn plp(&mut self, cycles: &mut u32) {
        let status = self.pop_byte(cycles);
        self.status.from_byte(status);
        *cycles = cycles.wrapping_add(2);
    }
}
