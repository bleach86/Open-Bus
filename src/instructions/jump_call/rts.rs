use crate::cpu::CPU;

impl CPU {
    pub fn rts(&mut self, cycles: &mut u32) -> bool {
        let cycles_required = 6;

        if *cycles < cycles_required - 1 {
            return false;
        }

        self.read_next_ignored(cycles);
        self.read_byte(self.sp_to_addr(), cycles); // the stack as it stands, read while S moves
        let ret_addr = self.pop_word(cycles);
        self.read_byte(ret_addr, cycles); // read while one is added to it

        self.pc = ret_addr.wrapping_add(1);
        true
    }
}
