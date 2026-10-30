use crate::cpu::CPU;

impl CPU {
    pub fn nop(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 2;

        if *cycles < cycles_required - 1 {
            return false;
        }

        *cycles = cycles.wrapping_sub(1);

        true
    }
}
