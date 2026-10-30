use crate::cpu::CPU;

impl CPU {
    pub fn rti(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 6;
        if *cycles < cycles_required - 1 {
            return false;
        }

        let status_byte = self.pop_byte(cycles);
        self.status.from_byte(status_byte);

        let ret_pc = self.pop_word(cycles);
        self.pc = ret_pc;

        *cycles = cycles.wrapping_sub(1);
        true
    }
}
