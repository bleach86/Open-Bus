use crate::cpu::CPU;

impl CPU {
    pub fn jsr_absolute(&mut self, cycles: &mut u32) -> bool {
        let cycles_required = 6;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let sub_addr = self.fetch_word(cycles);
        let return_addr = self.pc.wrapping_sub(1);
        self.push_word(return_addr, cycles);

        self.pc = sub_addr;
        *cycles = cycles.wrapping_sub(1);
        true
    }
}
