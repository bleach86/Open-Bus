//! Namco's 163: its banks, the nametable RAM in its pattern tables and CHR
//! ROM in its nametables, its RAM's protection, its own 128 bytes, its IRQ
//! and its sound. The 175's RAM and the 340's nametables; and which a board
//! the database doesn't name is guessed to be.

#[cfg(test)]
mod tests {
    use crate::machines::nes::cartridge::{Cartridge, Mirroring};
    use crate::machines::nes::namco163::CHIP_RAM;

    /// A Namco's cartridge, NES 2.0's, `number` with `submapper`: 128K of
    /// PRG, each 8K its number in every byte; 128K of CHR, each 1K its
    /// number; 8K of RAM, battery backed; mirrored vertically.
    fn namco(number: u8, submapper: u8) -> Cartridge {
        let mut image = b"NES\x1A\x08\x10".to_vec();
        image.extend([
            number << 4 | 0x03,
            number & 0xF0 | 0x08,
            submapper << 4,
            0,
            0x70,
            0,
            0,
            0,
            0,
            0,
        ]);
        for bank in 0..16 {
            image.extend([bank; 0x2000]);
        }
        for page in 0..128 {
            image.extend([page; 0x400]);
        }
        Cartridge::new(&image).unwrap()
    }

    fn prg(cart: &mut Cartridge) -> [u8; 4] {
        [0x8000, 0xA000, 0xC000, 0xE000].map(|addr| cart.cpu_read(addr).unwrap())
    }

    /// A byte of its own RAM written, by $F800's address and $4800.
    fn poke(cart: &mut Cartridge, at: u8, value: u8) {
        cart.cpu_write(0xF800, at, 0);
        cart.cpu_write(0x4800, value, 0);
    }

    #[test]
    fn its_8k_banks_the_last_fixed() {
        let mut cart = namco(19, 0);
        assert_eq!(prg(&mut cart), [0, 0, 0, 15]);
        cart.cpu_write(0xE000, 3, 0);
        cart.cpu_write(0xE800, 5, 0);
        cart.cpu_write(0xF000, 0x47, 0); // the top bits aren't the bank's
        assert_eq!(prg(&mut cart), [3, 5, 7, 15]);
        assert_eq!(cart.mapper_name(), "MAPPER 19 NAMCO 163");
    }

    #[test]
    fn its_pattern_tables_1k_banks_and_the_nametable_ram_at_e0_up() {
        let mut cart = namco(19, 0);
        for (bank, value) in [9u8, 8, 7, 6, 5, 4, 3, 2].into_iter().enumerate() {
            cart.cpu_write(0x8000 + bank as u16 * 0x800, value, 0);
        }
        let pages: Vec<u8> = (0..8).map(|bank| cart.chr_read(bank * 0x400)).collect();
        assert_eq!(pages, [9, 8, 7, 6, 5, 4, 3, 2]);
        // $E0 up, the nametable RAM, its bit 0 which 1K.
        cart.cpu_write(0x8800, 0xE1, 0);
        cart.cpu_write(0xA000, 0xE0, 0);
        assert_eq!(cart.chr_nametable(0x0400), Some(1));
        assert_eq!(cart.chr_nametable(0x1000), Some(0));
        assert_eq!(cart.chr_nametable(0x0000), None);
        // $E800's bit 6, the low four CHR ROM after all; bit 7, the high.
        cart.cpu_write(0xE800, 0x40, 0);
        assert_eq!(cart.chr_nametable(0x0400), None);
        assert_eq!(cart.chr_read(0x0400), 0xE1 & 0x7F, "CHR ROM, its 128K over");
        assert_eq!(cart.chr_nametable(0x1000), Some(0));
        cart.cpu_write(0xE800, 0x80, 0);
        assert_eq!(cart.chr_nametable(0x1000), None);
    }

    #[test]
    fn its_nametables_the_ram_or_chr_rom() {
        let mut cart = namco(19, 0);
        // Till they're written, as the board's wired: vertically.
        assert_eq!(
            [0x2000, 0x2400, 0x2800, 0x2C00].map(|addr| cart.nametable_page(addr)),
            [0, 1, 0, 1]
        );
        cart.cpu_write(0xC000, 0xE1, 0);
        cart.cpu_write(0xC800, 0x05, 0);
        cart.cpu_write(0xD000, 0xE0, 0);
        assert_eq!(cart.nametable_page(0x2000), 1);
        assert_eq!(cart.nametable_read(0x2000), None, "the RAM's, the PPU's");
        assert_eq!(cart.nametable_read(0x2410), Some(5), "CHR ROM's");
        assert!(cart.nametable_write(0x2410, 0x99), "not written, but taken");
        assert!(!cart.nametable_write(0x2800, 0x99), "the RAM's written");
        assert!(cart.watches_fetches(), "CHR ROM in a nametable, fetched");
    }

    #[test]
    fn its_ram_written_only_as_f800_lets_it() {
        let mut cart = namco(19, 0);
        assert_eq!(
            cart.prg_ram.len(),
            0x2000 + CHIP_RAM,
            "its 128 bytes after its RAM"
        );
        let wrote = |cart: &mut Cartridge, addr: u16| {
            cart.cpu_write(addr, 0x5A, 0);
            let took = cart.cpu_read(addr) == Some(0x5A);
            cart.prg_ram.fill(0);
            took
        };
        assert!(!wrote(&mut cart, 0x6000), "bit 6 clear: none");
        cart.cpu_write(0xF800, 0x40, 0);
        assert!(
            (0..4).all(|k| wrote(&mut cart, 0x6000 + k * 0x800)),
            "bit 6 set: all"
        );
        cart.cpu_write(0xF800, 0x45, 0); // the first 2K, and the third, not
        let each = (0..4)
            .map(|k| wrote(&mut cart, 0x6000 + k * 0x800))
            .collect::<Vec<_>>();
        assert_eq!(each, [false, true, false, true]);
    }

    #[test]
    fn its_own_128_bytes_at_4800_the_address_moving_on() {
        let mut cart = namco(19, 0);
        cart.cpu_write(0xF800, 0x80 | 0x7E, 0);
        for value in [0x11, 0x22, 0x33] {
            cart.cpu_write(0x4800, value, 0);
        }
        assert_eq!(cart.prg_ram[0x2000 + 0x7E..], [0x11, 0x22]);
        assert_eq!(cart.prg_ram[0x2000], 0x33, "past $7F, round to 0");
        cart.cpu_write(0xF800, 0x80 | 0x7F, 0);
        let read: Vec<u8> = (0..2).map(|_| cart.register_read(0x4800, 0xFF)).collect();
        assert_eq!(read, [0x22, 0x33]);
        cart.cpu_write(0xF800, 0x7F, 0);
        let read: Vec<u8> = (0..2).map(|_| cart.register_read(0x4800, 0xFF)).collect();
        assert_eq!(read, [0x22, 0x22], "without bit 7, staying put");
    }

    #[test]
    fn its_irq_counts_cycles_up_to_7fff_and_holds_the_line_till_written() {
        let mut cart = namco(19, 0);
        cart.cpu_write(0x5000, 0xF0, 0);
        cart.cpu_write(0x5800, 0x7F, 0); // not enabled
        cart.cycle();
        assert_eq!(cart.register_read(0x5000, 0), 0xF0, "not counting");
        cart.cpu_write(0x5800, 0xFF, 0);
        for cycle in 0..15 {
            assert!(!cart.irq(), "held after {cycle} cycles");
            cart.cycle();
        }
        assert!(cart.irq());
        assert_eq!(
            [cart.register_read(0x5000, 0), cart.register_read(0x5800, 0)],
            [0xFF, 0xFF]
        );
        cart.cycle();
        assert_eq!(cart.register_read(0x5000, 0), 0xFF, "stopped at the top");
        cart.cpu_write(0x5000, 0xFF, 0);
        assert!(!cart.irq(), "written, let go, and not held again there");
    }

    #[test]
    fn its_sound_each_channel_in_turn_every_15_cycles() {
        let mut cart = namco(19, 0);
        // A wave of 4 samples, all 15, at $00; channel 7 playing it at full
        // volume, two channels on ($7F's bits 4 to 6, 1); channel 6 at 0.
        for at in 0..2 {
            poke(&mut cart, at, 0xFF);
        }
        poke(&mut cart, 0x78, 0x40); // its frequency's low byte
        poke(&mut cart, 0x7C, 0xFC); // a length of 4
        poke(&mut cart, 0x7E, 0x00); // at $00
        poke(&mut cart, 0x7F, 0x1F); // two channels, volume 15
        let run = |cart: &mut Cartridge, cycles| (0..cycles).for_each(|_| cart.cycle());
        run(&mut cart, 15);
        // Channel 7, (15 - 8) * 15, shared with channel 6's nothing.
        assert_eq!(cart.output(), (105 / 2) as f32 * 0.004);
        run(&mut cart, 15);
        assert_eq!(cart.output(), (105 / 2) as f32 * 0.004, "6's, still 0");
        // Its phase moved on, in its own RAM.
        assert_eq!(cart.prg_ram[0x2000 + 0x79], 0x40);
        cart.cpu_write(0xE000, 0x40, 0); // silenced
        poke(&mut cart, 0x7F, 0x0F); // one channel
        run(&mut cart, 30);
        assert_eq!(cart.output(), (105 / 2) as f32 * 0.004, "as it was");
        cart.cpu_write(0xE000, 0, 0);
        run(&mut cart, 15);
        assert_eq!(cart.output(), 105.0 * 0.004, "on, channel 7 alone");
    }

    #[test]
    fn the_175s_ram_and_wiring() {
        let mut cart = namco(210, 1);
        assert_eq!(cart.mapper_name(), "MAPPER 210 NAMCO 175");
        cart.cpu_write(0x6000, 0x5A, 0);
        assert_eq!(cart.cpu_read(0x6000), Some(0), "not yet writable");
        cart.cpu_write(0xC000, 0x01, 0);
        cart.cpu_write(0x6000, 0x5A, 0);
        assert_eq!(cart.cpu_read(0x6000), Some(0x5A));
        cart.cpu_write(0xE000, 0xC0, 0);
        assert_eq!(cart.mirroring, Mirroring::Vertical, "as it's wired");
        cart.cpu_write(0x8000, 0xE0, 0);
        assert_eq!(cart.chr_nametable(0x0000), None, "no nametable RAM there");
    }

    #[test]
    fn the_340s_nametables_and_no_ram() {
        let mut cart = namco(210, 2);
        assert_eq!(cart.mapper_name(), "MAPPER 210 NAMCO 340");
        for (value, mirroring) in [
            (0x00, Mirroring::SingleLower),
            (0x40, Mirroring::Vertical),
            (0x80, Mirroring::Horizontal),
            (0xC0, Mirroring::SingleUpper),
        ] {
            cart.cpu_write(0xE000, value | 2, 0);
            assert_eq!(cart.mirroring, mirroring);
        }
        assert_eq!(prg(&mut cart)[0], 2);
        cart.cpu_write(0xC000, 0x01, 0);
        cart.cpu_write(0x6000, 0x5A, 0);
        assert_eq!(cart.cpu_read(0x6000), None, "no RAM");
    }

    #[test]
    fn a_board_not_named_is_guessed_from_what_its_game_writes() {
        // 210's, not said which: $C000 alone makes it a 175.
        let mut cart = namco(210, 0);
        cart.cpu_write(0xC000, 0x01, 0);
        assert_eq!(cart.mapper_name(), "MAPPER 210 NAMCO 175");
        // $E000's top bit, a 340.
        let mut cart = namco(210, 0);
        cart.cpu_write(0xE000, 0x80, 0);
        assert_eq!(cart.mapper_name(), "MAPPER 210 NAMCO 340");
        // 19's, the database not naming its board: a 163 till $E000's top
        // bit says a 340; but not once it's written RAM, which a 340 hasn't.
        let mut cart = namco(19, 0);
        cart.cpu_write(0xE000, 0x80, 0);
        assert_eq!(cart.mapper_name(), "MAPPER 19 NAMCO 340");
        let mut cart = namco(19, 0);
        cart.cpu_write(0x6000, 0, 0);
        cart.cpu_write(0xE000, 0x80, 0);
        assert_eq!(cart.mapper_name(), "MAPPER 19 NAMCO 163");
    }
}
