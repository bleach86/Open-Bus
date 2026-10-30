use crate::cpu::CPU;

impl CPU {
    pub fn rti(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 6;
        if *cycles < cycles_required - 1 {
            return false;
        }

        self.read_next_ignored(cycles);
        self.read_byte(self.sp_to_addr(), cycles); // the stack as it stands, read while S moves
        let status_byte = self.pop_byte(cycles);
        self.status.from_byte(status_byte);

        let ret_pc = self.pop_word(cycles);
        self.pc = ret_pc;
        true
    }
}
