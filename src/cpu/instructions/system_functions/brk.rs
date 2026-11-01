use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    pub fn brk(&mut self) {
        self.fetch_byte(); // the byte after BRK, read and passed over
        self.push_word(self.pc);

        self.status.break_command = true;
        let status_byte = self.status.to_byte();
        self.push_byte(status_byte);
        // As for an IRQ: the handler isn't interrupted; on a 65C02, it's
        // binary.
        self.status.interrupt_disable = true;
        if self.cmos() {
            self.status.decimal_mode = false;
        }

        // An NMI come by now: BRK's push made, B set, but NMI's handler.
        let vector = self.hijacked(0xFFFE);
        self.pc = self.read_word(vector);
    }
}
