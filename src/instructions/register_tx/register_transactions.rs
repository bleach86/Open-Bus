use crate::bus::Bus;
use crate::cpu::{CPU, Registers};

impl<B: Bus> CPU<B> {
    pub fn transfer_register(&mut self, source: Registers, destination: Registers) {
        let value = match source {
            Registers::A => self.a,
            Registers::X => self.x,
            Registers::Y => self.y,
        };

        match destination {
            Registers::A => self.a = value,
            Registers::X => self.x = value,
            Registers::Y => self.y = value,
        }

        let dest_value = match destination {
            Registers::A => self.a,
            Registers::X => self.x,
            Registers::Y => self.y,
        };

        self.status.zero = dest_value == 0;
        self.status.negative = (dest_value & 0x80) != 0;

        self.read_byte(self.pc); // the next byte, read and thrown away
    }
}
