//! The cartridge: an iNES header's sizes, mirroring, battery, trainer and
//! mapper, NES 2.0's and an old header's junk too; NROM's 16K mirrored,
//! its CHR RAM; the nametables' wiring; the small boards' banks, where
//! their registers are, and their bus conflicts.

#[cfg(test)]
mod tests {
    use crate::machines::nes::cartridge::{Cartridge, Mapper, Mirroring};

    /// An iNES image: its flags 6 to 15, then `prg` 16K banks, each its
    /// number in every byte, and `chr` 8K banks of $C0 and its number.
    fn image(prg: u8, chr: u8, flags: [u8; 10]) -> Vec<u8> {
        let mut image = b"NES\x1A".to_vec();
        image.extend([prg, chr]);
        image.extend(flags);
        if flags[0] & 0x04 != 0 {
            image.extend([0xEE; 512]); // a trainer
        }
        for bank in 0..prg {
            image.extend([bank; 0x4000]);
        }
        for bank in 0..chr {
            image.extend([0xC0 | bank; 0x2000]);
        }
        image
    }

    #[test]
    fn an_ines_header_is_read() {
        let mut cart = Cartridge::new(&image(1, 1, [0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0])).unwrap();
        assert_eq!((cart.mapper.clone(), cart.mirroring, cart.battery), (Mapper::Nrom, Mirroring::Vertical, false));
        assert_eq!((cart.cpu_read(0x8000), cart.cpu_read(0xC000)), (Some(0), Some(0)), "16K at $8000, and again at $C000");
        assert_eq!(cart.chr_read(0x1FFF), 0xC0, "its CHR ROM");
        cart.chr_write(0x0000, 0x12);
        assert_eq!(cart.chr_read(0x0000), 0xC0, "its CHR ROM written");

        // 32K; a battery; a trainer, skipped; horizontal.
        let mut cart = Cartridge::new(&image(2, 1, [0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0])).unwrap();
        assert_eq!((cart.cpu_read(0x8000), cart.cpu_read(0xC000)), (Some(0), Some(1)), "32K, past the trainer");
        assert_eq!((cart.mirroring, cart.battery), (Mirroring::Horizontal, true));
        cart.cpu_write(0x6123, 0x77, 0);
        assert_eq!(cart.cpu_read(0x6123), Some(0x77), "its PRG RAM");
        assert_eq!(cart.cpu_read(0x5000), None, "nothing at $5000");

        // No CHR ROM: 8K of CHR RAM. Four screens.
        let mut cart = Cartridge::new(&image(1, 0, [0x08, 0, 0, 0, 0, 0, 0, 0, 0, 0])).unwrap();
        cart.chr_write(0x1234, 0x56);
        assert_eq!((cart.chr_read(0x1234), cart.mirroring), (0x56, Mirroring::FourScreen), "its CHR RAM");
    }

    #[test]
    fn its_mapper_is_from_flags_6_and_7_or_none_with_junk() {
        let mapper = |flags| Cartridge::new(&image(1, 1, flags)).map(|cart| cart.mapper).map_err(|e| e.to_string());
        assert_eq!(mapper([0x10, 0x40, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap_err(), "mapper 65, which isn't here yet");
        // NES 2.0: byte 8's low nibble its bits 8 to 11.
        assert_eq!(mapper([0x00, 0x08, 0x01, 0, 0, 0, 0, 0, 0, 0]).unwrap_err(), "mapper 256, which isn't here yet");
        // An old header, a ripper's name in its last bytes: flags 7 junk too.
        assert_eq!(mapper([0x00, 0x44, 0, 0, 0, 0, b'D', b'i', b's', b'k']).unwrap(), Mapper::Nrom);
        assert!(Cartridge::new(b"not an image").is_err());
        assert!(Cartridge::new(&image(2, 1, [0; 10])[..0x5000]).is_err(), "cut short");
    }

    #[test]
    fn the_nametables_are_wired_as_the_board_is() {
        let pages = |mirroring: Mirroring| [0x2000, 0x2400, 0x2800, 0x2C00, 0x3000].map(|addr| mirroring.page(addr));
        assert_eq!(pages(Mirroring::Horizontal), [0, 0, 1, 1, 0]);
        assert_eq!(pages(Mirroring::Vertical), [0, 1, 0, 1, 0]);
        assert_eq!(pages(Mirroring::SingleLower), [0; 5]);
        assert_eq!(pages(Mirroring::SingleUpper), [1; 5]);
        assert_eq!(pages(Mirroring::FourScreen), [0, 1, 2, 3, 0]);
    }

    /// A board of mapper `number`, `prg` 16K banks and `chr` 8K: each PRG
    /// bank's every byte its number, but for $FF in each 256 bytes' first
    /// 16, for writes with no conflict; its flags 6 and 7 from the number.
    fn board(number: u8, prg: u8, chr: u8) -> Cartridge {
        let mut image = image(prg, chr, [number << 4, number & 0xF0, 0, 0, 0, 0, 0, 0, 0, 0]);
        for page in image[16..16 + prg as usize * 0x4000].chunks_mut(0x100) {
            page[..16].fill(0xFF);
        }
        Cartridge::new(&image).unwrap()
    }

    /// What's at $8020 and $C020 and in the CHR at $0000 and $1000.
    fn banks(cart: &mut Cartridge) -> (u8, u8, u8, u8) {
        (cart.cpu_read(0x8020).unwrap(), cart.cpu_read(0xC020).unwrap(), cart.chr_read(0), cart.chr_read(0x1000))
    }

    #[test]
    fn color_dreams_has_a_32k_bank_and_an_8k_of_chr() {
        let mut cart = board(11, 8, 4);
        assert_eq!(banks(&mut cart), (0, 1, 0xC0, 0xC0), "from the first");
        cart.cpu_write(0xFF00, 0x32, 0);
        assert_eq!(banks(&mut cart), (4, 5, 0xC3, 0xC3), "bits 0 and 1, and 4 up");

        // A plain header: no conflicts, even written over a 0 in the ROM.
        cart.cpu_write(0x8020, 0x01, 0);
        assert_eq!(banks(&mut cart), (2, 3, 0xC0, 0xC0), "taken as it was written");

        // NES 2.0's submapper 2: they're there.
        let mut flags = [0xB0, 0x08, 0x20, 0, 0, 0, 0, 0, 0, 0];
        let mut cart = Cartridge::new(&image(8, 4, flags)).unwrap();
        cart.cpu_write(0x8020, 0x11, 0);
        assert_eq!(banks(&mut cart), (0, 1, 0xC0, 0xC0), "over a 0, nothing's left of it");
        flags[2] = 0x10;
        let mut cart = Cartridge::new(&image(8, 4, flags)).unwrap();
        cart.cpu_write(0x8020, 0x11, 0);
        assert_eq!(banks(&mut cart), (2, 3, 0xC1, 0xC1), "submapper 1 has none");
    }

    #[test]
    fn mapper_144s_rom_always_wins_d0() {
        let mut cart = board(144, 8, 4);
        cart.cpu_write(0x8000, 0x10, 0);
        assert_eq!(banks(&mut cart), (2, 3, 0xC1, 0xC1), "over $FF, bit 0 the ROM's though 0 was written");

        // Over 2, all 1s written: only that bit, D0 the ROM's 0.
        cart.cpu_write(0x8020, 0xFF, 0);
        assert_eq!(banks(&mut cart), (4, 5, 0xC0, 0xC0));
    }

    #[test]
    fn gxrom_has_its_banks_the_other_way_round_with_conflicts() {
        let mut cart = board(66, 8, 4);
        assert_eq!(banks(&mut cart), (0, 1, 0xC0, 0xC0), "from the first");
        cart.cpu_write(0x8000, 0x13, 0);
        assert_eq!(banks(&mut cart), (2, 3, 0xC3, 0xC3), "bits 4 and 5, and 0 and 1");
        cart.cpu_write(0x8000, 0x30, 0);
        assert_eq!(banks(&mut cart), (6, 7, 0xC0, 0xC0), "over $FF, as written");
        cart.cpu_write(0x8020, 0x11, 0);
        assert_eq!(cart.chr_read(0), 0xC0 | (6 & 1), "over 6, only what both hold");
        assert_eq!(cart.cpu_read(0x8020), Some(0), "over 6, the PRG bits gone");
    }

    #[test]
    fn bnrom_has_a_32k_bank_with_conflicts_and_nina_001_its_registers_over_its_ram() {
        // Up to 8K of CHR, or none: BNROM.
        for chr in [0, 1] {
            let mut cart = board(34, 8, chr);
            assert!(matches!(cart.mapper, Mapper::Bnrom(_)), "{:?}", cart.mapper);
            cart.cpu_write(0x8000, 3, 0);
            assert_eq!(cart.cpu_read(0x8020), Some(6), "bank 3");
            cart.cpu_write(0x8020, 1, 0);
            assert_eq!(cart.cpu_read(0x8020), Some(0), "over 6, only what both hold");
        }

        // More: NINA-001, a 32K bank and two 4K of CHR.
        let mut cart = board(34, 4, 4);
        assert!(matches!(cart.mapper, Mapper::Nina001(_)), "{:?}", cart.mapper);
        cart.cpu_write(0x7FFD, 1, 0);
        cart.cpu_write(0x7FFE, 3, 0); // bank 3 of 4K: the second 8K's second half
        cart.cpu_write(0x7FFF, 4, 0); // bank 4: the third 8K's first half
        assert_eq!(banks(&mut cart), (2, 3, 0xC1, 0xC2));
        assert_eq!(cart.cpu_read(0x7FFE), Some(3), "the RAM under it written too");
        cart.cpu_write(0x7FFC, 1, 0);
        assert_eq!(banks(&mut cart).0, 2, "$7FFC a register");
        cart.cpu_write(0x8000, 0, 0);
        assert_eq!(banks(&mut cart).0, 2, "$8000 a register");
    }

    #[test]
    fn a_header_can_say_which_of_mapper_34s_boards_it_is() {
        let flags = |submapper: u8| [0x20, 0x28, submapper << 4, 0, 0, 0, 0, 0, 0, 0];
        let nina = Cartridge::new(&image(4, 1, flags(1))).unwrap();
        assert!(matches!(nina.mapper, Mapper::Nina001(_)), "{:?}", nina.mapper);
        let bnrom = Cartridge::new(&image(4, 4, flags(2))).unwrap();
        assert!(matches!(bnrom.mapper, Mapper::Bnrom(_)), "{:?}", bnrom.mapper);
    }

    #[test]
    fn camerica_has_a_16k_bank_written_at_c000_up() {
        let mut cart = board(71, 8, 0);
        assert_eq!(banks(&mut cart), (0, 7, 0, 0), "the last fixed");
        cart.cpu_write(0xC000, 3, 0);
        assert_eq!(banks(&mut cart), (3, 7, 0, 0));
        cart.cpu_write(0xFFFF, 0xF5, 0);
        assert_eq!(banks(&mut cart).0, 5, "up to $FFFF, four bits");
        cart.chr_write(0x0123, 0x45);
        assert_eq!(cart.chr_read(0x0123), 0x45, "its CHR RAM");

        // Micro Machines' write to $8000 isn't a bank, or the nametables.
        let wired = cart.mirroring;
        cart.cpu_write(0x8000, 0x10, 0);
        assert_eq!((banks(&mut cart).0, cart.mirroring), (5, wired));

        // Fire Hawk's, at $9000, are.
        cart.cpu_write(0x9000, 0x10, 0);
        assert_eq!(cart.mirroring, Mirroring::SingleUpper);
        cart.cpu_write(0x9FFF, 0x00, 0);
        assert_eq!(cart.mirroring, Mirroring::SingleLower);
    }

    #[test]
    fn nina_03s_register_is_at_4100_and_every_other_page_to_5fff() {
        let mut cart = board(79, 4, 4);
        assert_eq!(banks(&mut cart), (0, 1, 0xC0, 0xC0), "from the first");
        cart.cpu_write(0x4100, 0x0A, 0);
        assert_eq!(banks(&mut cart), (2, 3, 0xC2, 0xC2), "bit 3, and 0 to 2");
        for at in [0x4020, 0x4200, 0x41FF - 0x100, 0x6100, 0x8100, 0xC100] {
            cart.cpu_write(at, 0x01, 0);
            assert_eq!(banks(&mut cart), (2, 3, 0xC2, 0xC2), "a register at {at:04X}");
        }
        cart.cpu_write(0x5FFF, 0x03, 0);
        assert_eq!(banks(&mut cart), (0, 1, 0xC3, 0xC3), "$5F00 is another");
    }
}
