//! Bandai's FCG boards: their banks and nametables, where each has its
//! registers, the IRQ counter (the LZ93D50's from a latch, the FCG's set
//! directly), PRG's fifth bit on 512K; and the EEPROMs, a byte written and
//! read back a bit at a time as a game does it, the 24C02's and the 24C01's.

#[cfg(test)]
mod tests {
    use crate::machines::nes::cartridge::{Cartridge, Mirroring};

    /// A Bandai cartridge, NES 2.0's, `number` with `submapper`: `prg` 16K
    /// banks each its number in every byte, and 256K of CHR, each 1K its
    /// number, unless it's Datach's, with CHR RAM; a battery, if `battery`.
    fn bandai(number: u8, submapper: u8, prg: u8, battery: bool) -> Cartridge {
        let chr = if number == 157 { 0 } else { 32 };
        let mut image = b"NES\x1A".to_vec();
        image.extend([
            prg,
            chr,
            number << 4 | (battery as u8) << 1,
            number & 0xF0 | 0x08,
            submapper << 4,
        ]);
        image.extend([0, 0, if number == 157 { 0x07 } else { 0 }, 0, 0, 0, 0]);
        for bank in 0..prg {
            image.extend([bank; 0x4000]);
        }
        for page in 0..chr as usize * 8 {
            image.extend([page as u8; 0x400]);
        }
        Cartridge::new(&image).unwrap()
    }

    fn prg(cart: &mut Cartridge) -> [u8; 2] {
        [0x8000, 0xC000].map(|addr| cart.cpu_read(addr).unwrap())
    }

    #[test]
    fn its_16k_bank_the_last_fixed_its_1k_chr_banks_and_nametables() {
        let mut cart = bandai(16, 5, 16, false);
        assert_eq!(cart.mapper_name(), "MAPPER 16 BANDAI LZ93D50");
        assert_eq!(prg(&mut cart), [0, 15]);
        cart.cpu_write(0x8008, 0x13, 0); // its low four bits
        assert_eq!(prg(&mut cart), [3, 15]);
        for bank in 0..8 {
            cart.cpu_write(0x8000 + bank, 0x40 + bank as u8, 0);
        }
        let pages: Vec<u8> = (0..8).map(|bank| cart.chr_read(bank * 0x400)).collect();
        assert_eq!(pages, [0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47]);
        for (value, mirroring) in [
            (0, Mirroring::Vertical),
            (1, Mirroring::Horizontal),
            (2, Mirroring::SingleLower),
            (3, Mirroring::SingleUpper),
        ] {
            cart.cpu_write(0x8009, value, 0);
            assert_eq!(cart.mirroring, mirroring);
        }
        // Mirrored all the way up, by the address's low four bits.
        cart.cpu_write(0xFFF8, 5, 0);
        assert_eq!(prg(&mut cart), [5, 15]);
    }

    #[test]
    fn each_board_has_its_registers_where_it_has_them() {
        // The FCG-1 and -2's at $6000, not $8000.
        let mut fcg = bandai(16, 4, 16, false);
        fcg.cpu_write(0x8008, 3, 0);
        assert_eq!(prg(&mut fcg), [0, 15]);
        fcg.cpu_write(0x6008, 3, 0);
        assert_eq!(prg(&mut fcg), [3, 15]);
        // The LZ93D50's at $8000, not $6000.
        let mut lz = bandai(16, 5, 16, false);
        lz.cpu_write(0x6008, 3, 0);
        assert_eq!(prg(&mut lz), [0, 15]);
        // An old header's, not saying which: both.
        let mut image = b"NES\x1A\x10\x20\x00\x10".to_vec();
        image.extend([0; 8]);
        for bank in 0..16 {
            image.extend([bank; 0x4000]);
        }
        image.extend(vec![0; 0x40000]);
        let mut either = Cartridge::new(&image).unwrap();
        assert_eq!(either.mapper_name(), "MAPPER 16 BANDAI FCG OR LZ93D50");
        either.cpu_write(0x6008, 3, 0);
        assert_eq!(prg(&mut either), [3, 15]);
        either.cpu_write(0x8008, 4, 0);
        assert_eq!(prg(&mut either), [4, 15]);
    }

    /// The IRQ line held after `cycles` CPU cycles, not before.
    fn held_after(cart: &mut Cartridge, cycles: usize) {
        for cycle in 0..cycles {
            assert!(!cart.irq(), "held after {cycle} cycles");
            cart.cycle();
        }
        assert!(cart.irq(), "not held after {cycles}");
    }

    #[test]
    fn the_lz93d50s_counter_counts_down_from_its_latch() {
        let mut cart = bandai(16, 5, 16, false);
        cart.cpu_write(0x800B, 0x10, 0);
        cart.cpu_write(0x800C, 0x00, 0);
        cart.cycle();
        assert!(!cart.irq(), "not enabled");
        cart.cpu_write(0x800A, 1, 0); // enabled, the latch copied
        held_after(&mut cart, 0x11);
        cart.cpu_write(0x800A, 1, 0);
        assert!(!cart.irq(), "acknowledged");
        held_after(&mut cart, 0x11);
    }

    #[test]
    fn the_fcgs_counter_is_set_directly() {
        let mut cart = bandai(16, 4, 16, false);
        cart.cpu_write(0x600A, 0, 0);
        cart.cpu_write(0x600B, 0x08, 0);
        cart.cpu_write(0x600C, 0x00, 0);
        cart.cpu_write(0x600A, 1, 0); // enabled, the counter as it is
        held_after(&mut cart, 9);
    }

    #[test]
    fn with_512k_the_chr_registers_bit_0s_are_prgs_fifth_bit() {
        let mut cart = bandai(16, 5, 32, false);
        cart.cpu_write(0x8008, 2, 0);
        assert_eq!(prg(&mut cart), [2, 15]);
        cart.cpu_write(0x8005, 1, 0);
        assert_eq!(prg(&mut cart), [18, 31], "the second 256K");
    }

    /// The EEPROM's clock and data lines, as $D sets them.
    fn lines(cart: &mut Cartridge, clock: u8, data: u8) {
        cart.cpu_write(0x800D, clock << 5 | data << 6, 0);
    }

    fn start(cart: &mut Cartridge) {
        lines(cart, 0, 1);
        lines(cart, 1, 1);
        lines(cart, 1, 0);
        lines(cart, 0, 0);
    }

    fn stop(cart: &mut Cartridge) {
        lines(cart, 0, 0);
        lines(cart, 1, 0);
        lines(cart, 1, 1);
    }

    /// A bit sent, or read: what the EEPROM says as the clock rises, read
    /// in bit 4 at $6000.
    fn bit(cart: &mut Cartridge, bit: u8) -> u8 {
        lines(cart, 0, bit);
        lines(cart, 1, bit);
        let said = cart.register_read(0x6000, 0) >> 4 & 1;
        lines(cart, 0, bit);
        said
    }

    /// A byte sent, its bits in `order`, and the EEPROM's acknowledgement.
    fn send(cart: &mut Cartridge, byte: u8, msb_first: bool) -> u8 {
        for n in 0..8 {
            let at = if msb_first { 7 - n } else { n };
            bit(cart, byte >> at & 1);
        }
        bit(cart, 1)
    }

    fn receive(cart: &mut Cartridge, msb_first: bool) -> u8 {
        (0..8).fold(0, |byte, n| {
            let at = if msb_first { 7 - n } else { n };
            byte | bit(cart, 1) << at
        })
    }

    #[test]
    fn a_24c02s_byte_written_and_read_back() {
        let mut cart = bandai(16, 5, 16, true);
        assert_eq!(cart.prg_ram.len(), 256, "its bytes, saved as a battery's");
        assert!(cart.battery);
        // Written: the chip, the address, the byte, each acknowledged.
        start(&mut cart);
        assert_eq!(send(&mut cart, 0xA0, true), 0, "the chip acknowledges");
        assert_eq!(send(&mut cart, 0x42, true), 0, "the address");
        assert_eq!(send(&mut cart, 0x5A, true), 0, "the byte");
        stop(&mut cart);
        assert_eq!(cart.prg_ram[0x42], 0x5A);
        assert!(cart.written, "to be saved");
        // Read: the address set, then from there.
        start(&mut cart);
        send(&mut cart, 0xA0, true);
        send(&mut cart, 0x42, true);
        start(&mut cart);
        assert_eq!(send(&mut cart, 0xA1, true), 0);
        assert_eq!(receive(&mut cart, true), 0x5A);
        bit(&mut cart, 1); // not acknowledged: that's all
        stop(&mut cart);
        // Another chip's address isn't acknowledged.
        start(&mut cart);
        assert_eq!(send(&mut cart, 0x50, true), 1);
        stop(&mut cart);
    }

    #[test]
    fn a_24c01s_byte_written_and_read_back() {
        let mut cart = bandai(159, 0, 16, true);
        assert_eq!(cart.mapper_name(), "MAPPER 159 BANDAI LZ93D50");
        assert_eq!(cart.prg_ram.len(), 128);
        // Its 7 address bits, least first, then 0 to write.
        start(&mut cart);
        assert_eq!(send(&mut cart, 0x23, false), 0, "address and write");
        assert_eq!(send(&mut cart, 0xC5, false), 0, "the byte");
        stop(&mut cart);
        assert_eq!(cart.prg_ram[0x23], 0xC5);
        start(&mut cart);
        assert_eq!(send(&mut cart, 0x23 | 0x80, false), 0, "address and read");
        assert_eq!(receive(&mut cart, false), 0xC5);
        stop(&mut cart);
    }

    #[test]
    fn without_an_eeprom_bit_4_is_low() {
        let mut cart = bandai(16, 4, 16, false);
        assert!(cart.prg_ram.is_empty() && !cart.battery);
        assert_eq!(cart.register_read(0x6000, 0xFF), 0xE7);
    }

    #[test]
    fn famicom_jump_iis_ram_and_its_512k() {
        // As an old header has it: mapper 16, 512K of PRG, CHR RAM.
        let mut image = b"NES\x1A\x20\x00\x02\x10".to_vec();
        image.extend([0; 8]);
        for bank in 0..32 {
            image.extend([bank; 0x4000]);
        }
        let mut cart = Cartridge::new(&image).unwrap();
        assert_eq!(cart.mapper_name(), "MAPPER 153 BANDAI LZ93D50 WITH RAM");
        assert_eq!(cart.prg_ram.len(), 0x2000);
        // Its RAM, there only once $D's bit 5 says so.
        cart.cpu_write(0x6123, 0x5A, 0);
        assert_eq!(cart.register_read(0x6123, 0xEE), 0xEE, "not yet");
        cart.cpu_write(0x800D, 0x20, 0);
        cart.cpu_write(0x6123, 0x5A, 0);
        assert_eq!(cart.register_read(0x6123, 0xEE), 0x5A);
        assert!(cart.written, "to be saved");
        // The CHR registers' bit 0s, PRG's fifth bit; its CHR RAM unbanked.
        cart.cpu_write(0x8008, 3, 0);
        cart.cpu_write(0x8000, 1, 0);
        assert_eq!(prg(&mut cart), [19, 31]);
        cart.chr_write(0x0400, 0x77);
        assert_eq!(cart.chr_read(0x0400), 0x77);
    }

    #[test]
    fn datachs_base_and_its_chr_ram() {
        let mut cart = bandai(157, 0, 16, false);
        assert_eq!(cart.mapper_name(), "MAPPER 157 DATACH");
        assert_eq!(
            cart.prg_ram.len(),
            256,
            "its base's 24C02, no 24C01 told of"
        );
        cart.cpu_write(0x8000, 5, 0);
        cart.chr_write(0x0010, 0x77);
        assert_eq!(cart.chr_read(0x0010), 0x77, "CHR RAM, not banked");
        start(&mut cart);
        assert_eq!(send(&mut cart, 0xA0, true), 0);
        send(&mut cart, 0x01, true);
        send(&mut cart, 0x99, true);
        stop(&mut cart);
        assert_eq!(cart.prg_ram[1], 0x99);
    }
}
