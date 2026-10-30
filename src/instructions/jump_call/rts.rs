use crate::cpu::CPU;

impl CPU {
    pub fn rts(&mut self, cycles: &mut u32) -> bool {
        let cycles_required = 6;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let ret_addr = self.pop_word(cycles);

        self.pc = ret_addr.wrapping_add(1);
        *cycles = cycles.wrapping_sub(3);
        true
    }
}
