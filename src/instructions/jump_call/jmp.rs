use crate::cpu::CPU;

impl CPU {
    pub fn jmp_absolute(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 3;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let addr = self.fetch_word(cycles);

        self.pc = addr;
        true
    }

    pub fn jmp_indirect(&mut self, cycles: &mut u32) -> bool {
        let cycles_required: u32 = 5;

        if *cycles < cycles_required - 1 {
            return false;
        }

        let ptr_addr = self.fetch_word(cycles);
        let addr = self.read_word(ptr_addr, cycles);

        self.pc = addr;
        true
    }
}
