use crate::cpu::CPU;

impl CPU {
    pub fn nop(&mut self, cycles: &mut u32) {
        *cycles = cycles.wrapping_add(1);
    }
}
