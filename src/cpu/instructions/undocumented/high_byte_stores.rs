use crate::cpu::CPU;
use crate::cpu::bus::Bus;

impl<B: Bus> CPU<B> {
    /// The stores of a register ANDed with its address's high byte plus 1:
    /// SHA, SHX, SHY and TAS. The address's indexed as abs,X or abs,Y's is,
    /// read before the carry; but if the index carries into the high byte,
    /// the high byte is what's stored, not the carried one.
    fn store_and_high(&mut self, base_addr: u16, index: u8, value: u8) {
        let addr = base_addr.wrapping_add(index as u16);
        self.read_before_carry(base_addr, addr);
        let value = value & ((base_addr >> 8) as u8).wrapping_add(1);
        let addr = if (base_addr ^ addr) & 0xFF00 != 0 { ((value as u16) << 8) | (addr & 0x00FF) } else { addr };
        self.write_byte(addr, value);
    }

    /// SHA: A AND X, so stored.
    pub fn sha_absolute_y(&mut self) {
        let base_addr = self.fetch_word();
        self.store_and_high(base_addr, self.y, self.a & self.x);
    }

    pub fn sha_indirect_y(&mut self) {
        let zp_addr = self.fetch_byte();
        let lo = self.read_byte(zp_addr as u16) as u16;
        let hi = self.read_byte(zp_addr.wrapping_add(1) as u16) as u16;
        self.store_and_high((hi << 8) | lo, self.y, self.a & self.x);
    }

    /// SHX: X, so stored.
    pub fn shx_absolute_y(&mut self) {
        let base_addr = self.fetch_word();
        self.store_and_high(base_addr, self.y, self.x);
    }

    /// SHY: Y, so stored.
    pub fn shy_absolute_x(&mut self) {
        let base_addr = self.fetch_word();
        self.store_and_high(base_addr, self.x, self.y);
    }

    /// TAS: S = A AND X, then S so stored.
    pub fn tas_absolute_y(&mut self) {
        let base_addr = self.fetch_word();
        self.sp = self.a & self.x;
        self.store_and_high(base_addr, self.y, self.sp);
    }

    /// LAS: the byte AND S into A, X and S.
    pub fn las_absolute_y(&mut self) {
        let addr = self.fetch_addr_absolute_indexed(self.y);
        let value = self.read_byte(addr) & self.sp;
        self.a = value;
        self.x = value;
        self.sp = value;
        self.lda_set_status();
    }
}
