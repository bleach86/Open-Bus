//! The cartridge: an iNES header's sizes, mirroring, battery, trainer and
//! mapper, NES 2.0's and an old header's junk too; a game the database
//! knows as it has it; NROM's 16K mirrored,
//! its CHR RAM; the nametables' wiring; the small boards' banks, where
//! their registers are, and their bus conflicts.

#[cfg(test)]
mod tests {
    use crate::machines::nes::cartridge::{Board, Cartridge, Mapper, Mirroring};
    use crate::machines::nes::database::Known;

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
        assert_eq!(
            (cart.mapper.clone(), cart.mirroring, cart.battery),
            (Mapper::Nrom, Mirroring::Vertical, false)
        );
        assert_eq!(
            (cart.cpu_read(0x8000), cart.cpu_read(0xC000)),
            (Some(0), Some(0)),
            "16K at $8000, and again at $C000"
        );
        assert_eq!(cart.chr_read(0x1FFF), 0xC0, "its CHR ROM");
        cart.chr_write(0x0000, 0x12);
        assert_eq!(cart.chr_read(0x0000), 0xC0, "its CHR ROM written");

        // 32K; a battery; a trainer, skipped; horizontal.
        let mut cart = Cartridge::new(&image(2, 1, [0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0])).unwrap();
        assert_eq!(
            (cart.cpu_read(0x8000), cart.cpu_read(0xC000)),
            (Some(0), Some(1)),
            "32K, past the trainer"
        );
        assert_eq!(
            (cart.mirroring, cart.battery),
            (Mirroring::Horizontal, true)
        );
        cart.cpu_write(0x6123, 0x77, 0);
        assert_eq!(cart.cpu_read(0x6123), Some(0x77), "its PRG RAM");
        assert_eq!(cart.cpu_read(0x5000), None, "nothing at $5000");

        // No CHR ROM: 8K of CHR RAM. Four screens.
        let mut cart = Cartridge::new(&image(1, 0, [0x08, 0, 0, 0, 0, 0, 0, 0, 0, 0])).unwrap();
        cart.chr_write(0x1234, 0x56);
        assert_eq!(
            (cart.chr_read(0x1234), cart.mirroring),
            (0x56, Mirroring::FourScreen),
            "its CHR RAM"
        );
    }

    #[test]
    fn its_mapper_is_from_flags_6_and_7_or_none_with_junk() {
        let mapper = |flags| {
            Cartridge::new(&image(1, 1, flags))
                .map(|cart| cart.mapper)
                .map_err(|e| e.to_string())
        };
        assert_eq!(
            mapper([0x10, 0x40, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap_err(),
            "mapper 65, which isn't here yet"
        );
        // NES 2.0: byte 8's low nibble its bits 8 to 11.
        assert_eq!(
            mapper([0x00, 0x08, 0x01, 0, 0, 0, 0, 0, 0, 0]).unwrap_err(),
            "mapper 256, which isn't here yet"
        );
        // An old header, a ripper's name in its last bytes: flags 7 junk too.
        assert_eq!(
            mapper([0x00, 0x44, 0, 0, 0, 0, b'D', b'i', b's', b'k']).unwrap(),
            Mapper::Nrom
        );
        // Junk that looks like NES 2.0, but its ROMs as that has them
        // wouldn't fit in the image: an old header's junk all the same.
        assert_eq!(
            mapper([0x00, 0x48, 0, 0x4C, 0, 0, b'N', b'i', b'0', b'3']).unwrap(),
            Mapper::Nrom
        );
        assert!(Cartridge::new(b"not an image").is_err());
        assert!(
            Cartridge::new(&image(2, 1, [0; 10])[..0x5000]).is_err(),
            "cut short"
        );
    }

    /// A game a database knows: NROM, 32K and 8K, vertical, no RAM, checked.
    static KNOWN: Known = Known {
        mapper: 0,
        submapper: Some(0),
        board: "",
        prg_rom: 0x8000,
        chr_rom: 0x2000,
        chr_ram: 0,
        prg_ram: 0,
        battery: false,
        mirroring: Some(Mirroring::Vertical),
        famicom: false,
        zapper: false,
    };

    #[test]
    fn a_known_game_is_as_the_database_has_it() {
        // Its header says CNROM, 16K, horizontal, a battery.
        let mut cnrom = image(2, 1, [0x32, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        cnrom[4] = 1;
        let cart = Cartridge::known_as(&cnrom, |_| Some(&KNOWN)).unwrap();
        assert_eq!(
            (
                cart.mapper,
                cart.prg_rom.len(),
                cart.mirroring,
                cart.prg_ram.len()
            ),
            (Mapper::Nrom, 0x8000, Mirroring::Vertical, 0),
            "checked, it has no RAM"
        );
        assert!(!cart.battery, "checked, its battery as the database has it");
        let unchecked = |_| {
            static UNCHECKED: Known = Known {
                submapper: None,
                battery: true,
                mirroring: None,
                ..KNOWN
            };
            Some(&UNCHECKED)
        };
        let cart = Cartridge::known_as(&image(2, 1, [0; 10]), unchecked).unwrap();
        assert_eq!(
            (cart.prg_ram.len(), cart.battery, cart.mirroring),
            (0x2000, true, Mirroring::Horizontal),
            "unchecked, the header's RAM and mirroring, a battery added"
        );
        let no_battery = |_| {
            static NO_BATTERY: Known = Known {
                submapper: None,
                ..KNOWN
            };
            Some(&NO_BATTERY)
        };
        let cart = Cartridge::known_as(&cnrom, no_battery).unwrap();
        assert!(cart.battery, "unchecked, the header's battery kept");
    }

    #[test]
    fn a_known_game_on_a_board_not_here_is_as_its_header_has_it() {
        let karaoke = |_| {
            static KARAOKE: Known = Known {
                mapper: 188,
                ..KNOWN
            };
            Some(&KARAOKE)
        };
        let cart =
            Cartridge::known_as(&image(2, 1, [0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0]), karaoke).unwrap();
        assert_eq!(
            (cart.mapper, cart.mirroring),
            (Mapper::Nrom, Mirroring::Vertical)
        );
        let neither =
            Cartridge::known_as(&image(2, 1, [0x10, 0x40, 0, 0, 0, 0, 0, 0, 0, 0]), karaoke);
        assert_eq!(
            neither.err().as_deref(),
            Some("mapper 188, which isn't here yet"),
            "the database's board named"
        );
    }

    #[test]
    fn the_nametables_are_wired_as_the_board_is() {
        let pages = |mirroring: Mirroring| {
            [0x2000, 0x2400, 0x2800, 0x2C00, 0x3000].map(|addr| mirroring.page(addr))
        };
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
        let mut image = image(
            prg,
            chr,
            [number << 4, number & 0xF0, 0, 0, 0, 0, 0, 0, 0, 0],
        );
        for page in image[16..16 + prg as usize * 0x4000].chunks_mut(0x100) {
            page[..16].fill(0xFF);
        }
        Cartridge::new(&image).unwrap()
    }

    /// As `board`, but NES 2.0's, with `submapper`.
    fn board_of(number: u8, submapper: u8, prg: u8, chr: u8) -> Cartridge {
        let flags = [
            number << 4,
            number & 0xF0 | 0x08,
            submapper << 4,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ];
        let mut image = image(prg, chr, flags);
        for page in image[16..16 + prg as usize * 0x4000].chunks_mut(0x100) {
            page[..16].fill(0xFF);
        }
        Cartridge::new(&image).unwrap()
    }

    /// What's at $8020 and $C020 and in the CHR at $0000 and $1000.
    fn banks(cart: &mut Cartridge) -> (u8, u8, u8, u8) {
        (
            cart.cpu_read(0x8020).unwrap(),
            cart.cpu_read(0xC020).unwrap(),
            cart.chr_read(0),
            cart.chr_read(0x1000),
        )
    }

    #[test]
    fn color_dreams_has_a_32k_bank_and_an_8k_of_chr() {
        let mut cart = board(11, 8, 4);
        assert_eq!(banks(&mut cart), (0, 1, 0xC0, 0xC0), "from the first");
        cart.cpu_write(0xFF00, 0x32, 0);
        assert_eq!(
            banks(&mut cart),
            (4, 5, 0xC3, 0xC3),
            "bits 0 and 1, and 4 up"
        );

        // A plain header: no conflicts, even written over a 0 in the ROM.
        cart.cpu_write(0x8020, 0x01, 0);
        assert_eq!(
            banks(&mut cart),
            (2, 3, 0xC0, 0xC0),
            "taken as it was written"
        );

        // NES 2.0's submapper 2: they're there.
        let mut flags = [0xB0, 0x08, 0x20, 0, 0, 0, 0, 0, 0, 0];
        let mut cart = Cartridge::new(&image(8, 4, flags)).unwrap();
        cart.cpu_write(0x8020, 0x11, 0);
        assert_eq!(
            banks(&mut cart),
            (0, 1, 0xC0, 0xC0),
            "over a 0, nothing's left of it"
        );
        flags[2] = 0x10;
        let mut cart = Cartridge::new(&image(8, 4, flags)).unwrap();
        cart.cpu_write(0x8020, 0x11, 0);
        assert_eq!(banks(&mut cart), (2, 3, 0xC1, 0xC1), "submapper 1 has none");
    }

    #[test]
    fn mapper_144s_rom_always_wins_d0() {
        let mut cart = board(144, 8, 4);
        cart.cpu_write(0x8000, 0x10, 0);
        assert_eq!(
            banks(&mut cart),
            (2, 3, 0xC1, 0xC1),
            "over $FF, bit 0 the ROM's though 0 was written"
        );

        // Over 2, all 1s written: only that bit, D0 the ROM's 0.
        cart.cpu_write(0x8020, 0xFF, 0);
        assert_eq!(banks(&mut cart), (4, 5, 0xC0, 0xC0));
    }

    #[test]
    fn gxrom_has_its_banks_the_other_way_round_and_no_conflicts() {
        let mut cart = board(66, 8, 4);
        assert_eq!(banks(&mut cart), (0, 1, 0xC0, 0xC0), "from the first");
        cart.cpu_write(0x8000, 0x13, 0);
        assert_eq!(
            banks(&mut cart),
            (2, 3, 0xC3, 0xC3),
            "bits 4 and 5, and 0 and 1"
        );
        cart.cpu_write(0x8000, 0x30, 0);
        assert_eq!(banks(&mut cart), (6, 7, 0xC0, 0xC0), "over $FF, as written");
        // Over bank 6, whose bytes are all 6: as written, as Mesen has it.
        cart.cpu_write(0x8020, 0x11, 0);
        assert_eq!(cart.chr_read(0), 0xC1, "the CHR's bank as written");
        assert_eq!(banks(&mut cart), (2, 3, 0xC1, 0xC1), "the PRG's too");
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
            assert_eq!(
                cart.cpu_read(0x8020),
                Some(0),
                "over 6, only what both hold"
            );
        }

        // More: NINA-001, a 32K bank and two 4K of CHR.
        let mut cart = board(34, 4, 4);
        assert!(
            matches!(cart.mapper, Mapper::Nina001(_)),
            "{:?}",
            cart.mapper
        );
        cart.cpu_write(0x7FFD, 1, 0);
        cart.cpu_write(0x7FFE, 3, 0); // bank 3 of 4K: the second 8K's second half
        cart.cpu_write(0x7FFF, 4, 0); // bank 4: the third 8K's first half
        assert_eq!(banks(&mut cart), (2, 3, 0xC1, 0xC2));
        assert_eq!(
            cart.cpu_read(0x7FFE),
            Some(3),
            "the RAM under it written too"
        );
        cart.cpu_write(0x7FFC, 1, 0);
        assert_eq!(banks(&mut cart).0, 2, "$7FFC a register");
        cart.cpu_write(0x8000, 0, 0);
        assert_eq!(banks(&mut cart).0, 2, "$8000 a register");
    }

    #[test]
    fn a_header_can_say_which_of_mapper_34s_boards_it_is() {
        let flags = |submapper: u8| [0x20, 0x28, submapper << 4, 0, 0, 0, 0, 0, 0, 0];
        let nina = Cartridge::new(&image(4, 1, flags(1))).unwrap();
        assert!(
            matches!(nina.mapper, Mapper::Nina001(_)),
            "{:?}",
            nina.mapper
        );
        let bnrom = Cartridge::new(&image(4, 4, flags(2))).unwrap();
        assert!(
            matches!(bnrom.mapper, Mapper::Bnrom(_)),
            "{:?}",
            bnrom.mapper
        );
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
        assert_eq!(
            cart.cpu_read(0xA020),
            Some(6),
            "the third last 8K, the seventh 16K's second half"
        );
        assert_eq!(
            (cart.cpu_read(0xC020), cart.cpu_read(0xE020)),
            (Some(7), Some(7)),
            "the last two"
        );

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
        assert_eq!(
            banks(&mut cart).2..=banks(&mut cart).3,
            0xC1..=0xC3,
            "tile $FE's, from the first"
        );

        // The tile's read from the old bank, what follows from the new.
        assert_eq!(
            cart.chr_read(0x0FD8),
            0xC1,
            "tile $FD's read switched itself"
        );
        assert_eq!(cart.chr_read(0x0000), 0xC0);
        assert_eq!(cart.chr_read(0x1000), 0xC3, "the other table switched");
        assert_eq!(
            cart.chr_read(0x0FE8),
            0xC0,
            "tile $FE's read switched itself"
        );
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
            assert_eq!(
                cart.chr_read(0x0000),
                0xC1,
                "the left table, on the first plane"
            );
        }
        for row in 0..8 {
            cart.chr_read(0x1FD8 + row);
            assert_eq!(cart.chr_read(0x1000), 0xC2, "the right table, on row {row}");
            cart.chr_read(0x1FE8 + row);
            assert_eq!(cart.chr_read(0x1000), 0xC3, "the right table, on row {row}");
        }
        cart.chr_read(0x1FD0);
        assert_eq!(
            cart.chr_read(0x1000),
            0xC3,
            "the right table, on the first plane"
        );
    }

    #[test]
    fn mmc4_has_a_16k_bank_and_its_left_table_switches_on_any_row() {
        let mut cart = board(10, 8, 8);
        for (register, bank) in [(0xB000, 0), (0xC000, 2), (0xD000, 4), (0xE000, 6)] {
            cart.cpu_write(register, bank, 0);
        }
        cart.cpu_write(0xA000, 3, 0);
        assert_eq!(
            [0x8020, 0xBF20, 0xC020, 0xFF20].map(|addr| cart.cpu_read(addr).unwrap()),
            [3, 3, 7, 7],
            "a 16K bank, the last fixed"
        );
        for row in 0..8 {
            cart.chr_read(0x0FD8 + row);
            assert_eq!(cart.chr_read(0x0000), 0xC0, "tile $FD, row {row}");
            cart.chr_read(0x0FE8 + row);
            assert_eq!(cart.chr_read(0x0000), 0xC1, "tile $FE, row {row}");
        }
        cart.cpu_write(0x6000, 0x5A, 0);
        assert_eq!(cart.cpu_read(0x6000), Some(0x5A), "its PRG RAM");
    }

    #[test]
    fn un1rom_has_its_bank_in_bits_2_to_4_and_unrom_180_at_c000() {
        let mut cart = board(94, 8, 0);
        cart.cpu_write(0x8000, 3 << 2, 0);
        assert_eq!(
            (cart.cpu_read(0x8020), cart.cpu_read(0xC020)),
            (Some(3), Some(7))
        );
        let mut cart = board(180, 8, 0);
        cart.cpu_write(0x8000, 2, 0);
        assert_eq!(
            (cart.cpu_read(0x8020), cart.cpu_read(0xC020)),
            (Some(0), Some(2)),
            "the first fixed, the bank after it"
        );
    }

    #[test]
    fn cprom_has_16k_of_chr_ram_a_4k_bank_at_1000() {
        let mut cart = board(13, 2, 0);
        cart.chr_write(0x0000, 0x33);
        for bank in 1..4 {
            cart.cpu_write(0x8000, bank, 0);
            cart.chr_write(0x1000, 0x10 * bank);
        }
        for bank in 1..4 {
            cart.cpu_write(0x8000, bank, 0);
            assert_eq!(cart.chr_read(0x1000), 0x10 * bank, "bank {bank}");
            assert_eq!(cart.chr_read(0x0000), 0x33, "$0000 fixed");
        }
        cart.cpu_write(0x8000, 0, 0);
        assert_eq!(cart.chr_read(0x1000), 0x33, "bank 0, $0000's");
    }

    #[test]
    fn cnrom_185s_chr_is_there_only_as_its_written_says() {
        let mut cart = board(185, 2, 1);
        assert_eq!(cart.chr_read(0x0012), 0xC0, "there at first");
        cart.cpu_write(0x8000, 0x00, 0);
        assert_eq!(cart.chr_read(0x0012), 0x13, "open bus, D0 pulled up");
        cart.cpu_write(0x8000, 0x13, 0);
        assert_eq!(cart.chr_read(0x0012), 0x13, "nor with $13");
        cart.cpu_write(0x8000, 0x21, 0);
        assert_eq!(cart.chr_read(0x0012), 0xC0);
        cart.cpu_write(0x8020, 0x21, 0);
        assert_eq!(cart.chr_read(0x0012), 0x13, "the ROM's 0 there winning");
        // NES 2.0's submapper 5: there with bits 0 and 1 1. Written at
        // $C000, the ROM's 1 there.
        let mut cart =
            Cartridge::new(&image(2, 1, [0x90, 0xB8, 0x50, 0, 0, 0, 0, 0, 0, 0])).unwrap();
        assert_eq!(cart.chr_read(0x0012), 0xC0, "there at first");
        cart.cpu_write(0xC000, 0x02, 0);
        assert_eq!(cart.chr_read(0x0012), 0x13);
        cart.cpu_write(0xC000, 0x05, 0);
        assert_eq!(cart.chr_read(0x0012), 0xC0);
    }

    #[test]
    fn bandais_have_a_16k_bank_and_an_8k_of_chr_and_152_its_one_screen() {
        let mut cart = board(70, 8, 16);
        cart.cpu_write(0x8000, 0xB5, 0);
        assert_eq!(banks(&mut cart), (3, 7, 0xC5, 0xC5));
        assert_eq!(cart.mirroring, Mirroring::Horizontal, "70's wired");
        let mut cart = board(152, 8, 16);
        cart.cpu_write(0x8000, 0xB5, 0);
        assert_eq!(
            (banks(&mut cart), cart.mirroring),
            ((3, 7, 0xC5, 0xC5), Mirroring::SingleUpper)
        );
        cart.cpu_write(0x8000, 0x35, 0);
        assert_eq!(cart.mirroring, Mirroring::SingleLower);
    }

    #[test]
    fn sunsoft_2s_boards() {
        // Mito Koumon's: the CHR bank's bit 3 in bit 7, the one screen in bit 3.
        let mut cart = board(89, 8, 16);
        cart.cpu_write(0x8000, 0x9D, 0);
        assert_eq!(
            (banks(&mut cart), cart.mirroring),
            ((1, 7, 0xCD, 0xCD), Mirroring::SingleUpper)
        );
        // Fantasy Zone's: its CHR RAM there only with bit 0 set.
        let mut cart = board(93, 8, 0);
        cart.cpu_write(0x8000, 0x31, 0);
        cart.chr_write(0x0123, 0x5A);
        assert_eq!(
            (cart.cpu_read(0x8020), cart.chr_read(0x0123)),
            (Some(3), 0x5A)
        );
        cart.cpu_write(0x8000, 0x30, 0);
        assert_eq!(cart.chr_read(0x0123), 0x23, "open bus");
    }

    #[test]
    fn jf_17s_banks_are_taken_as_bits_7_and_6_rise() {
        let mut cart = board(72, 8, 16);
        cart.cpu_write(0x8000, 0xC3, 0);
        assert_eq!(banks(&mut cart), (3, 7, 0xC3, 0xC3), "both rising");
        cart.cpu_write(0x8000, 0xC5, 0);
        assert_eq!(banks(&mut cart), (3, 7, 0xC3, 0xC3), "neither rising");
        cart.cpu_write(0x8000, 0x05, 0);
        cart.cpu_write(0x8000, 0x85, 0);
        assert_eq!(
            banks(&mut cart),
            (5, 7, 0xC3, 0xC3),
            "bit 7 rising, its PRG"
        );
        cart.cpu_write(0x8020, 0xC4, 0);
        assert_eq!(banks(&mut cart).2, 0xC3, "the ROM's 0 there winning");
    }

    #[test]
    fn jf_19s_bank_is_at_c000_the_first_fixed() {
        let mut cart = board(92, 8, 16);
        assert_eq!(banks(&mut cart).1, 7, "the last at first");
        cart.cpu_write(0x8000, 0x83, 0);
        assert_eq!(banks(&mut cart), (0, 3, 0xC0, 0xC0));
    }

    #[test]
    fn jalecos_registers_at_6000() {
        // JF-13: a 32K bank, its CHR's bit 2 in bit 6; nothing at $7000 up
        // or $8000 up.
        let mut cart = board(86, 8, 16);
        cart.cpu_write(0x6000, 0x62, 0);
        assert_eq!(banks(&mut cart), (4, 5, 0xC6, 0xC6));
        cart.cpu_write(0x7000, 0x00, 0);
        cart.cpu_write(0x8000, 0x00, 0);
        assert_eq!(banks(&mut cart), (4, 5, 0xC6, 0xC6));
        // 87: its CHR's bits the other way round.
        let mut cart = board(87, 2, 4);
        cart.cpu_write(0x6000, 0x01, 0);
        assert_eq!(cart.chr_read(0), 0xC2);
        cart.cpu_write(0x7FFF, 0x02, 0);
        assert_eq!(cart.chr_read(0), 0xC1);
        // JF-11 and JF-14: a 32K bank and an 8K of CHR.
        let mut cart = board(140, 8, 16);
        cart.cpu_write(0x6000, 0x35, 0);
        assert_eq!(banks(&mut cart), (6, 7, 0xC5, 0xC5));
    }

    #[test]
    fn jf_16s_bit_3_its_one_screen_but_holy_divers_its_mirroring() {
        let mut cart = board(78, 8, 16);
        cart.cpu_write(0x8000, 0x5B, 0);
        assert_eq!(
            (banks(&mut cart), cart.mirroring),
            ((3, 7, 0xC5, 0xC5), Mirroring::SingleUpper)
        );
        cart.cpu_write(0x8000, 0x53, 0);
        assert_eq!(cart.mirroring, Mirroring::SingleLower);
        let mut cart = board_of(78, 3, 8, 16);
        cart.cpu_write(0x8000, 0x08, 0);
        assert_eq!(cart.mirroring, Mirroring::Vertical);
        cart.cpu_write(0x8000, 0x00, 0);
        assert_eq!(cart.mirroring, Mirroring::Horizontal);
    }

    #[test]
    fn tam_s1s_bank_is_at_c000_and_its_top_bits_its_nametables() {
        let mut cart = board(97, 8, 2);
        assert_eq!(
            banks(&mut cart).0..=banks(&mut cart).1,
            7..=7,
            "the last, both"
        );
        for (value, mirroring) in [
            (0x03, Mirroring::SingleLower),
            (0x43, Mirroring::Horizontal),
            (0x83, Mirroring::Vertical),
            (0xC3, Mirroring::SingleUpper),
        ] {
            cart.cpu_write(0x8000, value, 0);
            assert_eq!(
                (cart.cpu_read(0x8020), cart.cpu_read(0xC020), cart.mirroring),
                (Some(7), Some(3), mirroring)
            );
        }
    }

    #[test]
    fn sunsoft_1_has_two_4k_banks_of_chr_written_at_6000() {
        let mut cart = board(184, 2, 4);
        cart.cpu_write(0x6000, 0x53, 0);
        assert_eq!((cart.chr_read(0), cart.chr_read(0x1000)), (0xC1, 0xC2));
    }

    #[test]
    fn irem_77s_chr_is_2k_of_rom_then_6k_of_ram() {
        let mut cart = board(77, 8, 2);
        assert_eq!(cart.mirroring, Mirroring::FourScreen);
        cart.chr_write(0x0800, 0x77);
        cart.chr_write(0x1FFF, 0x88);
        cart.cpu_write(0x8000, 0x52, 0);
        assert_eq!(
            (
                cart.cpu_read(0x8020),
                cart.chr_read(0),
                cart.chr_read(0x0800)
            ),
            (Some(4), 0xC1, 0x77),
            "its 32K and 2K banks, its RAM as it was"
        );
        cart.chr_write(0x0000, 0x00);
        assert_eq!((cart.chr_read(0), cart.chr_read(0x1FFF)), (0xC1, 0x88));
    }

    #[test]
    fn caltrons_banks_are_in_the_address_and_its_inner_bits_only_sometimes() {
        let mut cart = board(41, 16, 16);
        cart.cpu_write(0x602D, 0, 0);
        cart.cpu_write(0x8000, 0x03, 0);
        assert_eq!(
            (cart.cpu_read(0x8020), cart.chr_read(0), cart.mirroring),
            (Some(10), 0xC7, Mirroring::Horizontal),
            "its 32K bank 5, CHR 4 + 3"
        );
        cart.cpu_write(0x6000, 0, 0);
        cart.cpu_write(0x8000, 0x02, 0);
        assert_eq!(
            (cart.chr_read(0), cart.mirroring),
            (0xC3, Mirroring::Vertical),
            "bank 0: not written"
        );
        cart.reset();
        assert_eq!((cart.cpu_read(0x8020), cart.chr_read(0)), (Some(0), 0xC0));
    }

    #[test]
    fn the_maxi_15s_registers_are_read_or_written_and_its_first_once() {
        // Its ROM's $FF where its registers are, for nothing written to be
        // lost to bus conflicts.
        let mut image = image(8, 16, [0xA0, 0xE0, 0, 0, 0, 0, 0, 0, 0, 0]);
        for bank in image[16..16 + 8 * 0x4000].chunks_mut(0x4000) {
            bank[0x3F80..=0x3F9F].fill(0xFF);
            bank[0x3FE8..=0x3FF8].fill(0xFF);
        }
        let mut cart = Cartridge::new(&image).unwrap();
        cart.cpu_write(0xFF80, 0x43, 0);
        assert_eq!(
            (cart.cpu_read(0x8020), cart.chr_read(0), cart.mirroring),
            (Some(4), 0xC8, Mirroring::Vertical),
            "as NINA-03, its 32K bank 2, CHR 8"
        );
        cart.cpu_read(0xFFE8);
        assert_eq!(
            (cart.cpu_read(0x8020), cart.chr_read(0)),
            (Some(6), 0xCF),
            "read, the ROM's $FF its second"
        );
        cart.cpu_write(0xFF80, 0xC0, 0);
        assert_eq!(cart.mirroring, Mirroring::Vertical, "its first not again");
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
        assert_eq!(
            cart.chr_read(0x0000),
            0xC1,
            "ROM's 1K bank 8, its second 8K"
        );

        cart.chr_write(0x1000, 0x5A);
        cart.chr_write(0x1400, 0xA5);
        assert_eq!(
            (cart.chr_read(0x1000), cart.chr_read(0x1400)),
            (0x5A, 0xA5),
            "the RAM's banks"
        );

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
        let prg = |cart: &mut Cartridge| {
            [0x8020, 0xA020, 0xC020, 0xE020].map(|addr| cart.cpu_read(addr).unwrap())
        };
        assert_eq!(prg(&mut cart), [1, 2, 3, 7], "R6, R7, RF, the last");
        cart.cpu_write(0x8000, 0x40, 0);
        assert_eq!(prg(&mut cart), [3, 2, 1, 7], "RF, R7, R6, the last");
        assert_eq!(cart.cpu_read(0x6000), None, "PRG RAM");
    }

    #[test]
    fn rambo_1s_chr_is_2k_and_1k_or_all_1k() {
        let mut cart = board(64, 8, 8);
        // In 1K: each 8K bank's value at every 8 of them.
        for (number, bank) in [
            (0, 0x08),
            (1, 0x10),
            (2, 0x18),
            (3, 0x20),
            (4, 0x28),
            (5, 0x30),
            (8, 0x38),
            (9, 0x00),
        ] {
            rambo_bank(&mut cart, 0, number, bank);
        }
        let chr = |cart: &mut Cartridge| {
            [
                0x0000, 0x0400, 0x0800, 0x0C00, 0x1000, 0x1400, 0x1800, 0x1C00,
            ]
            .map(|addr| cart.chr_read(addr) & 7)
        };

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
        let chr = [
            0x0000, 0x0400, 0x0800, 0x0C00, 0x1000, 0x1400, 0x1800, 0x1C00,
        ]
        .map(|addr| cart.chr_read(addr));
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
        assert_eq!(
            cart.cpu_read(0x6020),
            None,
            "RAM selected, not enabled: open bus"
        );
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
        for (value, mirroring) in [
            (0, Mirroring::Vertical),
            (1, Mirroring::Horizontal),
            (2, Mirroring::SingleLower),
            (3, Mirroring::SingleUpper),
        ] {
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
        assert_eq!(
            banks(&mut cart),
            (0, 3, 0, 0),
            "from the first, its block's last fixed"
        );
        cart.cpu_write(0xC000, 0xFD, 0);
        assert_eq!(banks(&mut cart).0, 1, "two bits of bank");

        for (block, value) in [(0, 0x00), (1, 0x08), (2, 0x10), (3, 0x18)] {
            cart.cpu_write(0x8000, value, 0);
            assert_eq!(
                banks(&mut cart),
                (block * 4 + 1, block * 4 + 3, 0, 0),
                "block {block}"
            );
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
            (3, 5, 5),  // the second 64K
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
        assert_eq!(
            cart.cpu_read(0xE020),
            Some(15),
            "the last fixed, of its block"
        );

        cart.cpu_write(0x6000, 0xFE, 0);
        assert_eq!(multicart_prg(&mut cart, 2), 1, "bit 0 only");
        assert_eq!(
            cart.cpu_read(0xE020),
            Some(7),
            "the first block's last fixed"
        );
    }

    #[test]
    fn a_multicarts_register_is_as_its_ram_would_be() {
        for number in [37, 47] {
            let mut cart = board(number, 16, 32);
            cart.cpu_write(0xA001, 0x00, 0);
            cart.cpu_write(0x6000, 1 | 4, 0);
            assert!(
                multicart_prg(&mut cart, 2) < 8,
                "{number}: written, the MMC3's RAM disabled"
            );

            cart.cpu_write(0xA001, 0xC0, 0);
            cart.cpu_write(0x6000, 1 | 4, 0);
            assert!(
                multicart_prg(&mut cart, 2) < 8,
                "{number}: written, protected"
            );

            cart.cpu_write(0xA001, 0x80, 0);
            cart.cpu_write(0x6000, 1 | 4, 0);
            assert!(multicart_prg(&mut cart, 2) >= 8, "{number}: not written");
            assert_eq!(cart.cpu_read(0x6000), None, "{number}: read");

            cart.reset();
            assert!(
                multicart_prg(&mut cart, 2) < 8,
                "{number}: not back to the menu's"
            );
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

    /// Action 52's write: chip, 16K bank, one-bank, CHR's high and low bits,
    /// the mirroring bit, as the address and data hold them.
    /// What's at $8020, $A020, $C020 and $E020: each its 16K bank's number.
    fn prg_8k(cart: &mut Cartridge) -> [u8; 4] {
        [0x8020, 0xA020, 0xC020, 0xE020].map(|addr| cart.cpu_read(addr).unwrap())
    }

    #[test]
    fn namco_108_has_bank_select_and_data_and_nothing_else() {
        let mut cart = board(206, 8, 8);
        // Bank select's PRG mode and CHR inversion aren't there.
        cart.cpu_write(0x8000, 0xC6, 0);
        cart.cpu_write(0x8001, 4, 0);
        // Every write from $8000 up is one or the other, by its A0.
        cart.cpu_write(0xA000, 7, 0);
        cart.cpu_write(0xE001, 7, 0);
        assert_eq!(
            prg_8k(&mut cart),
            [2, 3, 7, 7],
            "R6 and R7, the last two fixed"
        );
        cart.cpu_write(0xC000, 0, 0);
        cart.cpu_write(0xC001, 8, 0);
        assert_eq!(
            (cart.chr_read(0), cart.chr_read(0x1000)),
            (0xC1, 0xC0),
            "R0 at $0000, not inverted"
        );
        // No mirroring of its own, and no IRQ to enable.
        assert_eq!(cart.mirroring, Mirroring::Horizontal);
        for line in 0..4u64 {
            cart.ppu_bus(0x0000, line * 341);
            cart.ppu_bus(0x1000, line * 341 + 260);
        }
        assert!(!cart.irq());
    }

    #[test]
    fn namcot_3446s_chr_is_four_2k_banks() {
        let mut cart = board(76, 8, 8);
        mmc3_banks(&mut cart, false, [0, 0, 4, 9, 2, 31]);
        assert_eq!(
            [0, 0x0800, 0x1000, 0x1800].map(|addr| cart.chr_read(addr)),
            [0xC1, 0xC2, 0xC0, 0xC7],
            "R2 to R5, 2K each"
        );
    }

    #[test]
    fn namcot_3443s_left_table_is_the_first_64k_and_its_right_the_second() {
        let mut cart = board(88, 8, 16);
        mmc3_banks(&mut cart, false, [0x48, 0, 8, 0, 0, 0]);
        assert_eq!(
            (cart.chr_read(0), cart.chr_read(0x1000)),
            (0xC1, 0xC9),
            "R0's top bit gone, R2's set"
        );
    }

    #[test]
    fn namcot_3425s_nametables_are_r0_and_r1s_bit_5() {
        let mut cart = board(95, 8, 4);
        mmc3_banks(&mut cart, false, [0x20, 0, 0, 0, 0, 0]);
        assert_eq!(pages(&cart), [1, 1, 0, 0]);
        mmc3_banks(&mut cart, false, [0, 0x20, 0, 0, 0, 0]);
        assert_eq!(pages(&cart), [0, 0, 1, 1]);
    }

    #[test]
    fn namcot_3453s_every_write_says_its_one_screen() {
        let mut cart = board(154, 8, 16);
        cart.cpu_write(0x8000, 0x42, 0);
        assert_eq!(pages(&cart), [1, 1, 1, 1], "bit 6 set");
        cart.cpu_write(0x8001, 8, 0);
        assert_eq!(pages(&cart), [0, 0, 0, 0], "and clear");
        assert_eq!(
            cart.chr_read(0x1000),
            0xC9,
            "the right table from the second 64K"
        );
    }

    fn active(
        cart: &mut Cartridge,
        chip: u16,
        bank: u16,
        one: u16,
        high: u16,
        low: u8,
        mirroring: u16,
    ) {
        let addr = 0x8000 | mirroring << 13 | chip << 11 | bank << 6 | one << 5 | high;
        cart.cpu_write(addr, 0xFC | low, 0);
    }

    /// A cartridge of Action 52's shape: 1.5M of PRG, its 16K banks' first
    /// bytes each its number, $FF for 96, past a byte; 512K of CHR.
    fn action_52() -> Cartridge {
        let mut header = b"NES\x1A".to_vec();
        header.extend([96, 64, 0x41, 0xE0, 0, 0, 0, 0, 0, 0, 0, 0]);
        for bank in 0..96u8 {
            header.extend([bank; 0x4000]);
        }
        for bank in 0..64u8 {
            header.extend([bank; 0x2000]);
        }
        Cartridge::new(&header).unwrap()
    }

    #[test]
    fn action_52s_chips_are_where_they_are_in_its_image() {
        let mut cart = action_52();
        assert_eq!(
            (cart.cpu_read(0x8000), cart.cpu_read(0xC000)),
            (Some(0), Some(1)),
            "from the first, 32K"
        );

        // In 32K mode, even and odd; the second chip is 32 16Ks on.
        active(&mut cart, 1, 5, 0, 0, 0, 0);
        assert_eq!(
            (cart.cpu_read(0x8000), cart.cpu_read(0xC000)),
            (Some(32 + 4), Some(32 + 5))
        );

        // One bank at both.
        active(&mut cart, 1, 5, 1, 0, 0, 0);
        assert_eq!(
            (cart.cpu_read(0x8000), cart.cpu_read(0xC000)),
            (Some(32 + 5), Some(32 + 5))
        );

        // The fourth chip's straight after the second, in the image.
        active(&mut cart, 3, 31, 1, 0, 0, 0);
        assert_eq!(cart.cpu_read(0x8000), Some(64 + 31));

        // The third isn't there, and the first's back.
        active(&mut cart, 2, 3, 0, 0, 0, 0);
        assert_eq!(
            (cart.cpu_read(0x8000), cart.cpu_read(0xC000)),
            (None, None),
            "open bus"
        );
        active(&mut cart, 0, 3, 1, 0, 0, 0);
        assert_eq!(cart.cpu_read(0x8000), Some(3));
    }

    #[test]
    fn action_52s_chr_is_from_the_address_and_the_data() {
        let mut cart = action_52();
        active(&mut cart, 0, 0, 0, 0b1010, 0b11, 0);
        assert_eq!(
            cart.chr_read(0),
            0b1010_11,
            "the address's four, the data's two"
        );
        assert_eq!(cart.chr_read(0x1FFF), 0b1010_11, "8K of it");
    }

    #[test]
    fn action_52s_mirroring_is_a13() {
        let mut cart = action_52();
        active(&mut cart, 0, 0, 0, 0, 0, 1);
        assert_eq!(cart.mirroring, Mirroring::Horizontal);
        active(&mut cart, 0, 0, 0, 0, 0, 0);
        assert_eq!(cart.mirroring, Mirroring::Vertical);
    }

    #[test]
    fn reset_takes_action_52_back_to_its_menu() {
        let mut cart = action_52();
        active(&mut cart, 3, 31, 1, 15, 3, 1);
        cart.reset();
        assert_eq!(
            (
                cart.cpu_read(0x8000),
                cart.cpu_read(0xC000),
                cart.chr_read(0)
            ),
            (Some(0), Some(1), 0)
        );
    }

    #[test]
    fn nina_03s_register_is_at_4100_and_every_other_page_to_5fff() {
        let mut cart = board(79, 4, 4);
        assert_eq!(banks(&mut cart), (0, 1, 0xC0, 0xC0), "from the first");
        cart.cpu_write(0x4100, 0x0A, 0);
        assert_eq!(banks(&mut cart), (2, 3, 0xC2, 0xC2), "bit 3, and 0 to 2");
        for at in [0x4020, 0x4200, 0x41FF - 0x100, 0x6100, 0x8100, 0xC100] {
            cart.cpu_write(at, 0x01, 0);
            assert_eq!(
                banks(&mut cart),
                (2, 3, 0xC2, 0xC2),
                "a register at {at:04X}"
            );
        }
        cart.cpu_write(0x5FFF, 0x03, 0);
        assert_eq!(banks(&mut cart), (0, 1, 0xC3, 0xC3), "$5F00 is another");
    }

    #[test]
    fn a_games_facts_are_a_line_its_problem_if_it_wont_load() {
        let folder = std::env::temp_dir().join(format!("cpu_emu_{}_facts", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let (good, bad) = (folder.join("Good (J).nes"), folder.join("Bad.nes"));
        std::fs::write(&good, image(2, 1, [0; 10])).unwrap();
        // Mapper 188, Bandai's karaoke, not worth its microphone.
        std::fs::write(&bad, image(2, 1, [0xC0, 0xB0, 0, 0, 0, 0, 0, 0, 0, 0])).unwrap();
        let crc = crate::host::zip::crc32(&image(2, 1, [0; 10])[16..]);
        assert_eq!(
            crate::machines::nes::facts(&good),
            format!("{crc:08X}\tMAPPER 0 NROM\t1\t0\t\t{}", good.display()),
            "Japan's alone, by its name, a Famicom's"
        );
        assert_eq!(
            crate::machines::nes::facts(&bad),
            format!(
                "\t\t0\t0\tmapper 188, which isn't here yet\t{}",
                bad.display()
            )
        );
        assert!(crate::machines::nes::facts(&folder.join("gone.nes")).contains("\t\t0\t0\t"));
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn whats_chosen_of_a_game_wins_over_what_its_known_to_be() {
        use crate::machines::nes::{Chosen, build, region::Region};
        let folder = std::env::temp_dir().join(format!("cpu_emu_{}_chosen", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let game = folder.join("Game (J).nes");
        std::fs::write(&game, image(2, 1, [0; 10])).unwrap();
        let rom = game.to_str().unwrap();
        let known = build(rom, 0, Chosen::default()).unwrap();
        assert!(known.memory.famicom && known.memory.zapper.is_none());
        assert_eq!(known.memory.region, Region::Ntsc);
        let chosen = Chosen {
            region: Some(Region::Pal),
            famicom: Some(false),
            zapper: Some(true),
        };
        let nes = build(rom, 0, chosen).unwrap().memory;
        assert!(!nes.famicom, "Japan's alone, but chosen a NES");
        assert!(nes.zapper.is_some(), "a Zapper chosen");
        assert_eq!(nes.region, Region::Pal);
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
