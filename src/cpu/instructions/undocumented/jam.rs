use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    /// JAM (KIL): the 6502 stops. It reads the next byte, then only $FFFF
    /// and $FFFE, a cycle at a time, until it's reset: nothing else gets it
    /// going, not IRQ or NMI.
    pub fn jam(&mut self) {
        self.read_byte(self.pc); // the next byte, read and thrown away
        self.jammed = Some(0);
    }

    /// A cycle jammed: $FFFF read, then $FFFE twice, then $FFFF from then on.
    pub(crate) fn jammed_cycle(&mut self) {
        let cycles = self.jammed.unwrap_or(0);
        let addr = if matches!(cycles, 1 | 2) { 0xFFFE } else { 0xFFFF };
        self.read_byte(addr);
        self.jammed = Some((cycles + 1).min(3));
    }
}
