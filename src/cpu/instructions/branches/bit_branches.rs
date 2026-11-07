use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    // The Rockwell and WDC 65C02s' bit instructions, on a zero page byte.

    /// RMB: bit `bit` reset; read, read again, written.
    pub fn rmb(&mut self, bit: u8) {
        let addr = self.fetch_byte() as u16;
        let value = self.read_byte(addr);
        self.modify_cycle(addr, value);
        self.write_byte(addr, value & !(1 << bit));
    }

    /// SMB: bit `bit` set.
    pub fn smb(&mut self, bit: u8) {
        let addr = self.fetch_byte() as u16;
        let value = self.read_byte(addr);
        self.modify_cycle(addr, value);
        self.write_byte(addr, value | (1 << bit));
    }

    /// BBR and BBS: a branch on bit `bit` reset, or set; the byte read,
    /// read again, then the branch's offset. Taken, as a branch is, but to
    /// another page, the next instruction is read again, not the target
    /// with the old high byte.
    fn branch_on_bit(&mut self, bit: u8, set: bool) {
        let addr = self.fetch_byte() as u16;
        let value = self.read_byte(addr);
        self.read_byte(addr);
        let offset = self.fetch_byte() as i8;
        if (value & (1 << bit) != 0) == set {
            let next = self.pc;
            self.read_byte(next); // the next opcode, read and thrown away
            self.pc = (next as i32 + offset as i32) as u16;
            if (self.pc & 0xFF00) != (next & 0xFF00) {
                self.read_byte(next);
            }
        }
    }

    pub fn bbr(&mut self, bit: u8) {
        self.branch_on_bit(bit, false);
    }

    pub fn bbs(&mut self, bit: u8) {
        self.branch_on_bit(bit, true);
    }
}
