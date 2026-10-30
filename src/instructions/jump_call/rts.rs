use crate::cpu::CPU;

impl CPU {
    pub fn rts(&mut self, cycles: &mut u32) {
        let ret_addr = self.pop_word(cycles);

        self.pc = ret_addr.wrapping_add(1);
        *cycles = cycles.wrapping_add(3);
    }
}
