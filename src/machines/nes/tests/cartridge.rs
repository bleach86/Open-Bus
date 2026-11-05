//! The cartridge: an iNES header's sizes, mirroring, battery, trainer and
//! mapper, NES 2.0's and an old header's junk too; NROM's 16K mirrored,
//! its CHR RAM; the nametables' wiring; the small boards' banks, where
//! their registers are, and their bus conflicts.

#[cfg(test)]
mod tests {
    use crate::machines::nes::cartridge::{Board, Cartridge, Mapper, Mirroring};

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
    fn mmc2_has_an_8k_bank_and_the_last_three_fixed() {
        let mut cart = board(9, 8, 8);
        assert_eq!(cart.cpu_read(0x8020), Some(0), "from the first");
        cart.cpu_write(0xA000, 0xF5, 0); // bank 5 of 8K: the third 16K's second half
        assert_eq!(cart.cpu_read(0x8020), Some(2), "four bits");
        assert_eq!(cart.cpu_read(0xA020), Some(6), "the third last 8K, the seventh 16K's second half");
        assert_eq!((cart.cpu_read(0xC020), cart.cpu_read(0xE020)), (Some(7), Some(7)), "the last two");

        // $AFFF is the same register, $9FFF isn't one.
        cart.cpu_write(0xAFFF, 2, 0);
        cart.cpu_write(0x9FFF, 7, 0);
        assert_eq!(cart.cpu_read(0x8020), Some(1));

        cart.cpu_write(0xF000, 0, 0);
        assert_eq!(cart.mirroring, Mirroring::Vertical);
        cart.cpu_write(0xFFFF, 1, 0);
        assert_eq!(cart.mirroring, Mirroring::Horizontal);
    }

    /// An MMC2 with its 4K banks: at $0000 for tile $FD 8K bank 0 and for
    /// $FE bank 1; at $1000, 2 and 3.
    fn mmc2() -> Cartridge {
        let mut cart = board(9, 8, 8);
        for (register, bank) in [(0xB000, 0), (0xC000, 2), (0xD000, 4), (0xE000, 6)] {
            cart.cpu_write(register, bank, 0);
        }
        cart
    }

    #[test]
    fn mmc2s_tiles_fd_and_fe_switch_a_table_after_theyre_read() {
        let mut cart = mmc2();
        assert_eq!(banks(&mut cart).2..=banks(&mut cart).3, 0xC1..=0xC3, "tile $FE's, from the first");

        // The tile's read from the old bank, what follows from the new.
        assert_eq!(cart.chr_read(0x0FD8), 0xC1, "tile $FD's read switched itself");
        assert_eq!(cart.chr_read(0x0000), 0xC0);
        assert_eq!(cart.chr_read(0x1000), 0xC3, "the other table switched");
        assert_eq!(cart.chr_read(0x0FE8), 0xC0, "tile $FE's read switched itself");
        assert_eq!(cart.chr_read(0x0000), 0xC1);

        assert_eq!(cart.chr_read(0x1FD8), 0xC3);
        assert_eq!(cart.chr_read(0x1000), 0xC2);
        assert_eq!(cart.chr_read(0x0000), 0xC1, "the other table switched");
        assert_eq!(cart.chr_read(0x1FE8), 0xC2);
        assert_eq!(cart.chr_read(0x1000), 0xC3);
    }

    #[test]
    fn mmc2s_left_table_switches_on_one_row_and_its_right_on_all() {
        let mut cart = mmc2();
        for row in 1..8 {
            cart.chr_read(0x0FD8 + row);
            assert_eq!(cart.chr_read(0x0000), 0xC1, "the left table, on row {row}");
        }
        for row in 0..8 {
            cart.chr_read(0x0FD0 + row);
            assert_eq!(cart.chr_read(0x0000), 0xC1, "the left table, on the first plane");
        }
        for row in 0..8 {
            cart.chr_read(0x1FD8 + row);
            assert_eq!(cart.chr_read(0x1000), 0xC2, "the right table, on row {row}");
            cart.chr_read(0x1FE8 + row);
            assert_eq!(cart.chr_read(0x1000), 0xC3, "the right table, on row {row}");
        }
        cart.chr_read(0x1FD0);
        assert_eq!(cart.chr_read(0x1000), 0xC3, "the right table, on the first plane");
    }

    /// MMC3's CHR registers set: R0 to R5, each its value.
    fn mmc3_banks(cart: &mut Cartridge, inverted: bool, banks: [u8; 6]) {
        let invert = if inverted { 0x80 } else { 0 };
        for (register, bank) in banks.into_iter().enumerate() {
            cart.cpu_write(0x8000, invert | register as u8, 0);
            cart.cpu_write(0x8001, bank, 0);
        }
    }

    /// Which page each of the four nametables is.
    fn pages(cart: &Cartridge) -> [usize; 4] {
        [0x2000, 0x2400, 0x2800, 0x2C00].map(|addr| cart.nametable_page(addr))
    }

    #[test]
    fn txsroms_nametables_are_the_chr_banks_top_bits() {
        let mut cart = board(118, 8, 8);

        // R0 and R1, 2K each: horizontal, either way round, or one screen.
        mmc3_banks(&mut cart, false, [0x00, 0x80, 0, 0, 0, 0]);
        assert_eq!(pages(&cart), [0, 0, 1, 1]);
        mmc3_banks(&mut cart, false, [0x80, 0x00, 0, 0, 0, 0]);
        assert_eq!(pages(&cart), [1, 1, 0, 0]);
        mmc3_banks(&mut cart, false, [0x80, 0x80, 0, 0, 0, 0]);
        assert_eq!(pages(&cart), [1, 1, 1, 1]);

        // R2 to R5's bits are ignored while they're at $1000 up.
        mmc3_banks(&mut cart, false, [0, 0, 0x80, 0x80, 0x80, 0x80]);
        assert_eq!(pages(&cart), [0, 0, 0, 0]);

        // Inverted, they're at $0000: 1K each, and R0 and R1's ignored.
        mmc3_banks(&mut cart, true, [0x80, 0x80, 0x00, 0x80, 0x80, 0x00]);
        assert_eq!(pages(&cart), [0, 1, 1, 0], "each 1K its own");
        mmc3_banks(&mut cart, true, [0, 0, 0x00, 0x80, 0x00, 0x80]);
        assert_eq!(pages(&cart), [0, 1, 0, 1]);

        // The nametables' mirror at $3000, and $A000 doing nothing.
        assert_eq!(cart.nametable_page(0x3400), 1);
        cart.cpu_write(0xA000, 1, 0);
        assert_eq!(pages(&cart), [0, 1, 0, 1], "$A000 did something");
    }

    #[test]
    fn a_plain_mmc3s_top_bits_are_no_nametable() {
        let mut cart = board(4, 8, 8);
        cart.cpu_write(0xA000, 0, 0);
        mmc3_banks(&mut cart, false, [0x80, 0x00, 0, 0, 0, 0]);
        assert_eq!(pages(&cart), [0, 1, 0, 1], "vertical, by $A000");
    }

    #[test]
    fn tqroms_banks_choose_its_chr_rom_or_its_ram() {
        let mut cart = board(119, 8, 4);
        // R0 ROM at $0000; R2 RAM at $1000, and R3 another bank of it.
        mmc3_banks(&mut cart, false, [0x08, 0, 0x40, 0x41, 0, 0]);
        assert_eq!(cart.chr_read(0x0000), 0xC1, "ROM's 1K bank 8, its second 8K");

        cart.chr_write(0x1000, 0x5A);
        cart.chr_write(0x1400, 0xA5);
        assert_eq!((cart.chr_read(0x1000), cart.chr_read(0x1400)), (0x5A, 0xA5), "the RAM's banks");

        // A write to ROM doing nothing, and the RAM's 8K wrapping.
        cart.chr_write(0x0000, 0x11);
        assert_eq!(cart.chr_read(0x0000), 0xC1, "the ROM written");
        mmc3_banks(&mut cart, false, [0x08, 0, 0x48, 0x41, 0, 0]);
        assert_eq!(cart.chr_read(0x1000), 0x5A, "RAM bank 8 not 0 again");

        // Swapped to ROM, the RAM's kept.
        mmc3_banks(&mut cart, false, [0x08, 0, 0x08, 0x41, 0, 0]);
        assert_eq!(cart.chr_read(0x1000), 0xC1);
        mmc3_banks(&mut cart, false, [0x08, 0, 0x40, 0x41, 0, 0]);
        assert_eq!(cart.chr_read(0x1000), 0x5A);

        // RAM in a 2K bank too, and bit 7 nothing.
        mmc3_banks(&mut cart, false, [0xC0, 0, 0x40, 0x41, 0, 0]);
        cart.chr_write(0x0400, 0x77);
        assert_eq!(cart.chr_read(0x0400), 0x77);
    }

    #[test]
    fn a_tqrom_of_no_chr_rom_is_all_ram() {
        let mut cart = board(119, 8, 0);
        cart.chr_write(0x0000, 0x12);
        assert_eq!(cart.chr_read(0x0000), 0x12);
    }

    #[test]
    fn a_state_has_its_board_from_the_cartridge_its_loaded_into() {
        let mut loaded = board(0, 1, 1);
        let mut running = board(119, 8, 4);
        loaded.take_roms(&mut running);
        assert_eq!(loaded.board, Board::Tqrom);
    }

    /// A RAMBO-1's register `number` set, with bank select's `mode` bits.
    fn rambo_bank(cart: &mut Cartridge, mode: u8, number: u8, bank: u8) {
        cart.cpu_write(0x8000, mode | number, 0);
        cart.cpu_write(0x8001, bank, 0);
    }

    #[test]
    fn rambo_1s_third_prg_bank_leaves_only_the_last_fixed() {
        let mut cart = board(64, 8, 8);
        // In 8K: R6 2, R7 4, RF 6; the 16K banks' numbers halve them.
        for (number, bank) in [(6, 2), (7, 4), (15, 6)] {
            rambo_bank(&mut cart, 0, number, bank);
        }
        let prg = |cart: &mut Cartridge| [0x8020, 0xA020, 0xC020, 0xE020].map(|addr| cart.cpu_read(addr).unwrap());
        assert_eq!(prg(&mut cart), [1, 2, 3, 7], "R6, R7, RF, the last");
        cart.cpu_write(0x8000, 0x40, 0);
        assert_eq!(prg(&mut cart), [3, 2, 1, 7], "RF, R7, R6, the last");
        assert_eq!(cart.cpu_read(0x6000), None, "PRG RAM");
    }

    #[test]
    fn rambo_1s_chr_is_2k_and_1k_or_all_1k() {
        let mut cart = board(64, 8, 8);
        // In 1K: each 8K bank's value at every 8 of them.
        for (number, bank) in [(0, 0x08), (1, 0x10), (2, 0x18), (3, 0x20), (4, 0x28), (5, 0x30), (8, 0x38), (9, 0x00)] {
            rambo_bank(&mut cart, 0, number, bank);
        }
        let chr = |cart: &mut Cartridge| [0x0000, 0x0400, 0x0800, 0x0C00, 0x1000, 0x1400, 0x1800, 0x1C00].map(|addr| cart.chr_read(addr) & 7);

        cart.cpu_write(0x8000, 0x00, 0);
        assert_eq!(chr(&mut cart), [1, 1, 2, 2, 3, 4, 5, 6], "R0 and R1 2K");
        cart.cpu_write(0x8000, 0x20, 0);
        assert_eq!(chr(&mut cart), [1, 7, 2, 0, 3, 4, 5, 6], "R0, R8, R1, R9");
        cart.cpu_write(0x8000, 0x80, 0);
        assert_eq!(chr(&mut cart), [3, 4, 5, 6, 1, 1, 2, 2], "inverted");
        cart.cpu_write(0x8000, 0xA0, 0);
        assert_eq!(chr(&mut cart), [3, 4, 5, 6, 1, 7, 2, 0], "inverted, all 1K");

        // 2K's low bit is A10's, not the register's.
        rambo_bank(&mut cart, 0, 0, 0x09);
        assert_eq!(cart.chr_read(0x0000), 0xC1);
    }

    #[test]
    fn rambo_1s_mirroring_is_mmc3s() {
        let mut cart = board(64, 2, 1);
        cart.cpu_write(0xA000, 0, 0);
        assert_eq!(cart.mirroring, Mirroring::Vertical);
        cart.cpu_write(0xA000, 1, 0);
        assert_eq!(cart.mirroring, Mirroring::Horizontal);
    }

    #[test]
    fn rambo_1_counts_4_cpu_cycles_a_count() {
        let mut cart = board(64, 2, 1);
        cart.cpu_write(0xC000, 2, 0);
        cart.cpu_write(0xC001, 1, 0); // counting cycles, and reloaded
        cart.cpu_write(0xE001, 0, 0);

        // At 4 reloaded with 2 ORed with 1; at 8, 2; at 12, 1; at 16, 0;
        // the line held a cycle on.
        for cycle in 1..=16 {
            cart.cycle();
            assert!(!cart.irq(), "held at {cycle}");
        }
        cart.cycle();
        assert!(cart.irq(), "not held");

        // Enabling doesn't acknowledge it; $E000 does, and disables.
        cart.cpu_write(0xE001, 0, 0);
        assert!(cart.irq(), "acknowledged by $E001");
        cart.cpu_write(0xE000, 0, 0);
        assert!(!cart.irq(), "not acknowledged");
        for _ in 0..100 {
            cart.cycle();
        }
        assert!(!cart.irq(), "held, disabled");
    }

    #[test]
    fn rambo_1_counts_a12s_rises_and_holds_the_line_4_cycles_on() {
        let mut cart = board(64, 2, 1);
        let rise = |cart: &mut Cartridge, dot: u64| {
            cart.ppu_bus(0x0000, dot);
            cart.ppu_bus(0x1000, dot + 20);
        };
        cart.cpu_write(0xC000, 1, 0);
        cart.cpu_write(0xC001, 0, 0); // counting lines, and reloaded
        cart.cpu_write(0xE001, 0, 0);
        rise(&mut cart, 0); // reloaded, 1
        for _ in 0..4 {
            cart.cycle();
        }
        assert!(!cart.irq(), "held at 1");
        rise(&mut cart, 100); // 0
        for _ in 0..3 {
            cart.cycle();
        }
        assert!(!cart.irq(), "held three on");
        cart.cycle();
        assert!(cart.irq(), "not held four on");

        // A rise too soon after a fall isn't one.
        cart.cpu_write(0xE000, 0, 0);
        cart.cpu_write(0xE001, 0, 0);
        cart.ppu_bus(0x0000, 200);
        cart.ppu_bus(0x1000, 203);
        for _ in 0..4 {
            cart.cycle();
        }
        assert!(!cart.irq(), "counted a short fall");

        // Counting cycles, A12's ignored.
        cart.cpu_write(0xC000, 200, 0);
        cart.cpu_write(0xC001, 1, 0);
        rise(&mut cart, 300);
        for _ in 0..3 {
            cart.cycle();
        }
        assert!(!cart.irq(), "counted A12's rise, counting cycles");
    }

    /// FME-7's command `command` given `value`.
    fn fme7(cart: &mut Cartridge, command: u8, value: u8) {
        cart.cpu_write(0x8000, command, 0);
        cart.cpu_write(0xA000, value, 0);
    }

    #[test]
    fn fme7_banks_its_prg_8k_at_a_time_and_its_chr_1k() {
        let mut cart = board(69, 8, 8);
        // In 8K, doubled from the 16K banks' numbers.
        fme7(&mut cart, 9, 2);
        fme7(&mut cart, 0xA, 6);
        fme7(&mut cart, 0xB, 10);
        let prg = [0x8020, 0xA020, 0xC020, 0xE020].map(|addr| cart.cpu_read(addr).unwrap());
        assert_eq!(prg, [1, 3, 5, 7], "commands 9 to B, the last fixed");

        // Each 1K its own, in 8K's eighths.
        for slot in 0..8 {
            fme7(&mut cart, slot, slot * 8);
        }
        let chr = [0x0000, 0x0400, 0x0800, 0x0C00, 0x1000, 0x1400, 0x1800, 0x1C00].map(|addr| cart.chr_read(addr));
        assert_eq!(chr, [0xC0, 0xC1, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7]);

        // The command's taken from anywhere up to $9FFF, the parameter $BFFF.
        cart.cpu_write(0x9FFF, 9, 0);
        cart.cpu_write(0xBFFF, 4, 0);
        assert_eq!(cart.cpu_read(0x8020), Some(2));

        // The 5B's sound registers do nothing to the banks.
        cart.cpu_write(0xC000, 9, 0);
        cart.cpu_write(0xE000, 0, 0);
        assert_eq!(cart.cpu_read(0x8020), Some(2));
    }

    #[test]
    fn fme7_has_rom_or_ram_at_6000() {
        let mut cart = board(69, 8, 1);
        assert_eq!(cart.cpu_read(0x6020), Some(0), "ROM from the first");
        fme7(&mut cart, 8, 6);
        assert_eq!(cart.cpu_read(0x6020), Some(3), "a bank of ROM");
        cart.cpu_write(0x6020, 0x55, 0);
        assert_eq!(cart.cpu_read(0x6020), Some(3), "ROM written");

        fme7(&mut cart, 8, 0xC0);
        cart.cpu_write(0x6020, 0x55, 0);
        assert_eq!(cart.cpu_read(0x6020), Some(0x55), "RAM, enabled");

        fme7(&mut cart, 8, 0x40);
        assert_eq!(cart.cpu_read(0x6020), None, "RAM selected, not enabled: open bus");
        cart.cpu_write(0x6020, 0x66, 0);
        fme7(&mut cart, 8, 0xC0);
        assert_eq!(cart.cpu_read(0x6020), Some(0x55), "RAM written, disabled");

        // Its enable bit set, bit 6 clear: ROM.
        fme7(&mut cart, 8, 0x80 | 6);
        assert_eq!(cart.cpu_read(0x6020), Some(3));
    }

    #[test]
    fn fme7s_nametables() {
        let mut cart = board(69, 2, 1);
        for (value, mirroring) in [(0, Mirroring::Vertical), (1, Mirroring::Horizontal), (2, Mirroring::SingleLower), (3, Mirroring::SingleUpper)] {
            fme7(&mut cart, 0xC, value);
            assert_eq!(cart.mirroring, mirroring, "{value}");
        }
    }

    #[test]
    fn fme7s_irq_counts_every_cycle_and_fires_going_past_0() {
        let mut cart = board(69, 2, 1);
        fme7(&mut cart, 0xE, 2);
        fme7(&mut cart, 0xF, 0);
        for _ in 0..10 {
            cart.cycle();
        }
        assert!(!cart.irq(), "counted, not enabled to");

        // 2, 1, 0, then past it.
        fme7(&mut cart, 0xD, 0x81);
        for cycle in 1..=2 {
            cart.cycle();
            assert!(!cart.irq(), "held at {cycle}");
        }
        cart.cycle();
        assert!(cart.irq(), "not held going past 0");

        // Held, the counter going on, till acknowledged; then $FFFF.
        cart.cycle();
        assert!(cart.irq(), "let go by itself");
        fme7(&mut cart, 0xD, 0x81);
        assert!(!cart.irq(), "not acknowledged");
        for _ in 0..0xFFFD {
            cart.cycle();
        }
        assert!(!cart.irq(), "held early");
        for _ in 0..3 {
            cart.cycle();
        }
        assert!(cart.irq(), "not held wrapping round");

        // Counting without the line, and the line without counting.
        fme7(&mut cart, 0xE, 0);
        fme7(&mut cart, 0xF, 0);
        fme7(&mut cart, 0xD, 0x80);
        cart.cycle();
        assert!(!cart.irq(), "the line not enabled");
        fme7(&mut cart, 0xE, 0);
        fme7(&mut cart, 0xF, 0);
        fme7(&mut cart, 0xD, 0x01);
        cart.cycle();
        assert!(!cart.irq(), "the count not enabled");
    }

    #[test]
    fn quattros_banks_are_in_a_64k_block() {
        let mut cart = board(232, 16, 0);
        assert_eq!(banks(&mut cart), (0, 3, 0, 0), "from the first, its block's last fixed");
        cart.cpu_write(0xC000, 0xFD, 0);
        assert_eq!(banks(&mut cart).0, 1, "two bits of bank");

        for (block, value) in [(0, 0x00), (1, 0x08), (2, 0x10), (3, 0x18)] {
            cart.cpu_write(0x8000, value, 0);
            assert_eq!(banks(&mut cart), (block * 4 + 1, block * 4 + 3, 0, 0), "block {block}");
        }
        cart.cpu_write(0xBFFF, 0xE7 | 0x08, 0);
        assert_eq!(banks(&mut cart).1, 7, "only bits 3 and 4, up to $BFFF");

        // $C000 up is the bank only, not the block.
        cart.cpu_write(0xFFFF, 0x10, 0);
        assert_eq!(banks(&mut cart), (4, 7, 0, 0));

        cart.chr_write(0x0123, 0x45);
        assert_eq!(cart.chr_read(0x0123), 0x45, "its CHR RAM");
    }

    #[test]
    fn the_aladdins_quattros_have_the_blocks_bits_swapped() {
        let mut image = image(16, 0, [0x80, 0xE8, 0x10, 0, 0, 0, 0, 0, 0, 0]);
        for page in image[16..].chunks_mut(0x100) {
            page[..16].fill(0xFF);
        }
        let mut cart = Cartridge::new(&image).unwrap();
        for (written, block) in [(0x00, 0), (0x08, 2), (0x10, 1), (0x18, 3)] {
            cart.cpu_write(0x8000, written, 0);
            assert_eq!(banks(&mut cart).1, block * 4 + 3, "written {written:02X}");
        }
    }

    /// A multicart of mapper `number`: 256K of PRG and of CHR, each bank's
    /// every byte its number, its PRG RAM's writes allowed.
    fn multicart(number: u8) -> Cartridge {
        let mut cart = board(number, 16, 32);
        cart.cpu_write(0xA001, 0x80, 0);
        cart
    }

    /// Where the MMC3's 8K bank `bank` at $8000 lands: in 16K banks.
    fn multicart_prg(cart: &mut Cartridge, bank: u8) -> u8 {
        cart.cpu_write(0x8000, 6, 0);
        cart.cpu_write(0x8001, bank, 0);
        cart.cpu_read(0x8020).unwrap()
    }

    #[test]
    fn mapper_37s_register_picks_as_its_table_says() {
        let mut cart = multicart(37);
        // The MMC3's 8K banks 2 and 10, 16K banks 1 and 5 of the 16.
        for (value, low, high) in [
            (0, 1, 1), // the first 64K, whatever it asks for
            (1, 1, 1),
            (2, 1, 1),
            (3, 5, 5), // the second 64K
            (4, 9, 13), // the second 128K, the MMC3's A16 through
            (5, 9, 13),
            (6, 9, 13),
            (7, 13, 13), // its last 64K
        ] {
            cart.cpu_write(0x6000, value, 0);
            assert_eq!(multicart_prg(&mut cart, 2), low, "{value}: bank 2");
            assert_eq!(multicart_prg(&mut cart, 10), high, "{value}: bank 10");
        }

        // Its CHR's 128K by bit 2, the first 8K bank of each.
        cart.cpu_write(0x6000, 0, 0);
        assert_eq!(cart.chr_read(0x0000), 0xC0);
        cart.cpu_write(0x6000, 3, 0);
        assert_eq!(cart.chr_read(0x0000), 0xC0, "value 3 is the first 128K's");
        cart.cpu_write(0x6000, 4, 0);
        assert_eq!(cart.chr_read(0x0000), 0xC0 | 16, "value 4 the second's");
    }

    #[test]
    fn mapper_47s_register_picks_a_128k_block() {
        let mut cart = multicart(47);
        cart.cpu_write(0x7FFF, 1, 0);
        assert_eq!(multicart_prg(&mut cart, 2), 9, "bank 2 of the second 128K");
        assert_eq!(cart.chr_read(0x0000), 0xC0 | 16);
        assert_eq!(cart.cpu_read(0xE020), Some(15), "the last fixed, of its block");

        cart.cpu_write(0x6000, 0xFE, 0);
        assert_eq!(multicart_prg(&mut cart, 2), 1, "bit 0 only");
        assert_eq!(cart.cpu_read(0xE020), Some(7), "the first block's last fixed");
    }

    #[test]
    fn a_multicarts_register_is_as_its_ram_would_be() {
        for number in [37, 47] {
            let mut cart = board(number, 16, 32);
            cart.cpu_write(0xA001, 0x00, 0);
            cart.cpu_write(0x6000, 1 | 4, 0);
            assert!(multicart_prg(&mut cart, 2) < 8, "{number}: written, the MMC3's RAM disabled");

            cart.cpu_write(0xA001, 0xC0, 0);
            cart.cpu_write(0x6000, 1 | 4, 0);
            assert!(multicart_prg(&mut cart, 2) < 8, "{number}: written, protected");

            cart.cpu_write(0xA001, 0x80, 0);
            cart.cpu_write(0x6000, 1 | 4, 0);
            assert!(multicart_prg(&mut cart, 2) >= 8, "{number}: not written");
            assert_eq!(cart.cpu_read(0x6000), None, "{number}: read");

            cart.reset();
            assert!(multicart_prg(&mut cart, 2) < 8, "{number}: not back to the menu's");
        }
    }

    #[test]
    fn a_plain_mmc3s_ram_is_kept_over_a_reset() {
        let mut cart = board(4, 8, 8);
        cart.cpu_write(0xA001, 0x80, 0);
        cart.cpu_write(0x6000, 0x55, 0);
        cart.reset();
        assert_eq!(cart.cpu_read(0x6000), Some(0x55));
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
