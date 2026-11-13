use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    /// An IRQ or NMI, taken between instructions: as BRK, but the PC pushed
    /// is the next instruction's and the status is pushed with B clear. An
    /// NMI come by the time its status is pushed takes an IRQ's vector over,
    /// on the 6502: the IRQ, still held, taken after.
    pub fn interrupt(&mut self, vector: u16) {
        self.read_byte(self.pc); // the next instruction, read and thrown away,
        self.read_byte(self.pc); // twice
        self.push_word(self.pc);

        let status_byte = self.status.to_byte() & !0x10;
        self.push_byte(status_byte);
        self.status.interrupt_disable = true;
        if self.cmos() {
            self.status.decimal_mode = false; // a 65C02's handler, binary
        }

        let vector = self.hijacked(vector);
        self.pc = self.read_vector(vector);
    }

    /// The vector at `vector` read, VPB low as it is.
    pub(crate) fn read_vector(&mut self, vector: u16) -> u16 {
        self.memory.vector_pull(true);
        let address = self.read_word(vector);
        self.memory.vector_pull(false);
        address
    }

    /// The vector an IRQ's or BRK's sequence takes, `vector`: or on the
    /// 6502, an NMI come by its fifth cycle, NMI's, the NMI taken by it.
    pub(crate) fn hijacked(&mut self, vector: u16) -> u16 {
        if vector == 0xFFFE && self.nmi && !self.cmos() {
            self.nmi = false;
            return 0xFFFA;
        }
        vector
    }
}
