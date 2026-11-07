use crate::cpu::bus::Bus;
use crate::cpu::{CPU, Variant};

/// What ANE and LXA OR into A first: it varies from chip to chip, and with
/// its temperature; $EE is the usual.
const MAGIC: u8 = 0xEE;

impl<B: Bus> CPU<B> {
    /// ANC: AND, then C as N, as if shifted out.
    pub fn anc_immediate(&mut self) {
        let value = self.fetch_byte();
        self.a &= value;
        self.and_set_flags();
        self.status.carry = self.status.negative;
    }

    /// ALR: AND, then LSR A.
    pub fn alr_immediate(&mut self) {
        let value = self.fetch_byte();
        let and = self.a & value;
        self.a = and >> 1;
        self.status.carry = (and & 0x01) != 0; // the bit shifted out
        self.lda_set_status();
    }

    /// ARR: AND, then ROR A, but with the adder's flags: C bit 6 of the
    /// result, V bits 6 and 5 differing. In decimal mode the adder also
    /// corrects each nibble, as ADC would, from the AND's: N the carry
    /// rotated in, V from the AND's bit 6 and the result's.
    pub fn arr_immediate(&mut self) {
        let value = self.fetch_byte();
        let and = self.a & value;
        let carry_in = u8::from(self.status.carry);
        self.a = (and >> 1) | (carry_in << 7);
        self.status.zero = self.a == 0;
        self.status.negative = carry_in != 0;
        if !self.decimal() {
            self.status.carry = (self.a & 0x40) != 0;
            self.status.overflow = ((self.a >> 6) ^ (self.a >> 5)) & 1 != 0;
            return;
        }
        self.status.overflow = ((and ^ self.a) & 0x40) != 0;
        let (low, high) = (and & 0x0F, and >> 4);
        if low + (low & 1) > 5 {
            self.a = (self.a & 0xF0) | (self.a.wrapping_add(6) & 0x0F);
        }
        self.status.carry = high + (high & 1) > 5;
        if self.status.carry {
            self.a = self.a.wrapping_add(0x60);
        }
    }

    /// SBX: X = (A AND X) - the byte, with no borrow in; C and the flags
    /// CMP's. Never decimal.
    pub fn sbx_immediate(&mut self) {
        let value = self.fetch_byte();
        let and = self.a & self.x;
        self.x = and.wrapping_sub(value);
        self.status.carry = and >= value;
        self.status.zero = self.x == 0;
        self.status.negative = (self.x & 0x80) != 0;
    }

    /// ANE (XAA): A = (A OR MAGIC) AND X AND the byte.
    pub fn ane_immediate(&mut self) {
        let value = self.fetch_byte();
        self.a = (self.a | MAGIC) & self.x & value;
        self.lda_set_status();
    }

    /// LXA: A and X = (A OR MAGIC) AND the byte; on the NES's 2A03, as
    /// blargg's NES has it, $FF ORed in, the byte itself.
    pub fn lxa_immediate(&mut self) {
        let value = self.fetch_byte();
        let magic = if self.variant == Variant::Ricoh2A03 {
            0xFF
        } else {
            MAGIC
        };
        self.a = (self.a | magic) & value;
        self.x = self.a;
        self.lda_set_status();
    }
}
