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
        // NMOS quirk: the high byte is fetched from the same page, so a
        // pointer at $xxFF wraps to $xx00 rather than crossing the page.
        let low = self.read_byte(ptr_addr, cycles) as u16;
        let high_addr = (ptr_addr & 0xFF00) | (ptr_addr.wrapping_add(1) & 0x00FF);
        let high = self.read_byte(high_addr, cycles) as u16;
        let addr = (high << 8) | low;

        self.pc = addr;
        true
    }
}
