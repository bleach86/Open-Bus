use crate::cpu::CPU;

impl CPU {
    pub fn tsx(&mut self, cycles: &mut u32) {
        self.x = self.sp;
        self.status.zero = self.x == 0;
        self.status.negative = (self.x & 0x80) != 0;

        *cycles = cycles.wrapping_add(1);
    }
}
