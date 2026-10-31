use crate::cpu::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    /// An IRQ or NMI, taken between instructions: as BRK, but the PC pushed
    /// is the next instruction's and the status is pushed with B clear.
    pub fn interrupt(&mut self, vector: u16) {
        self.read_byte(self.pc); // the next instruction, read and thrown away,
        self.read_byte(self.pc); // twice
        self.push_word(self.pc);

        let status_byte = self.status.to_byte() & !0x10;
        self.push_byte(status_byte);
        self.status.interrupt_disable = true;

        self.pc = self.read_word(vector);
    }
}
