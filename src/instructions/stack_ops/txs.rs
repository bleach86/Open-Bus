use crate::cpu::CPU;

impl CPU {
    pub fn txs(&mut self, cycles: &mut u32) {
        self.sp = self.x;

        *cycles = cycles.wrapping_add(1);
    }
}
