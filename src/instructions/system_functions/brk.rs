use crate::bus::Bus;
use crate::cpu::CPU;

impl<B: Bus> CPU<B> {
    pub fn brk(&mut self) {
        self.fetch_byte(); // the byte after BRK, read and passed over
        self.push_word(self.pc);

        self.status.break_command = true;
        let status_byte = self.status.to_byte();
        self.push_byte(status_byte);
        // As for an IRQ: the handler isn't interrupted.
        self.status.interrupt_disable = true;

        self.pc = self.read_word(0xFFFE);
    }
}
