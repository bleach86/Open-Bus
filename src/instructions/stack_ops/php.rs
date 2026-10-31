use crate::cpu::CPU;

impl CPU {
    pub fn php(&mut self) {
        // PHP always pushes with the B flag (bit 4) set.
        let status = self.status.to_byte() | 0x10;

        self.read_byte(self.pc); // the next byte, read and thrown away
        self.push_byte(status);
    }
}
