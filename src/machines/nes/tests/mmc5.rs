//! The MMC5: its PRG modes and RAM, its CHR's modes and two sets, its
//! nametables and ExRAM, extended attributes, the split, its multiplier and
//! its scanline IRQ, found as it finds them, from the PPU's reads.

#[cfg(test)]
mod tests {
    use crate::machines::nes::cartridge::Cartridge;
    use crate::machines::nes::ppu::Ppu;
    use crate::machines::nes::region::Region;

    /// A 512K, 64K-CHR cartridge of mapper 5: each 8K of PRG its number,
    /// each 1K of CHR its number, $80 up for the second half of each 8K.
    fn mmc5() -> Cartridge {
        let (prg, chr) = (32, 8);
        let mut image = b"NES\x1A".to_vec();
        image.extend([prg, chr, 0x50, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        for bank in 0..prg as usize * 2 {
            image.extend([bank as u8; 0x2000]);
        }
        for bank in 0..chr as usize * 8 {
            image.extend([bank as u8; 0x400]);
        }
        Cartridge::new(&image).unwrap()
    }

    fn prg(cart: &mut Cartridge) -> [u8; 4] {
        [0x8000, 0xA000, 0xC000, 0xE000].map(|addr| cart.cpu_read(addr).unwrap())
    }

    /// Each register of $5113 to $5117, from the second, set to ROM.
    fn prg_banks(cart: &mut Cartridge, banks: [u8; 4]) {
        for (register, bank) in banks.into_iter().enumerate() {
            cart.cpu_write(0x5114 + register as u16, 0x80 | bank, 0);
        }
    }

    #[test]
    fn it_powers_on_in_mode_3_with_the_last_bank_at_e000() {
        let mut cart = mmc5();
        assert_eq!(cart.cpu_read(0xE000), Some(63));
        prg_banks(&mut cart, [1, 2, 3, 4]);
        assert_eq!(prg(&mut cart), [1, 2, 3, 4], "four 8K banks, with no mode written");
    }

    #[test]
    fn its_prg_modes_have_their_windows() {
        let mut cart = mmc5();
        prg_banks(&mut cart, [9, 10, 21, 42]);

        cart.cpu_write(0x5100, 2, 0);
        assert_eq!(prg(&mut cart), [10, 11, 21, 42], "16K of $5115, 8K of $5116, 8K of $5117");

        cart.cpu_write(0x5100, 1, 0);
        assert_eq!(prg(&mut cart), [10, 11, 42, 43], "16K of $5115, 16K of $5117");

        cart.cpu_write(0x5100, 0, 0);
        assert_eq!(prg(&mut cart), [40, 41, 42, 43], "32K of $5117, its low bits ignored");
    }

    #[test]
    fn its_prg_ram_is_banked_at_6000_and_up_and_protected() {
        let mut cart = mmc5();
        cart.cpu_write(0x5113, 2, 0);
        cart.cpu_write(0x6000, 0x11, 0);
        assert_eq!(cart.cpu_read(0x6000), Some(0), "written, protected");

        cart.cpu_write(0x5102, 2, 0);
        cart.cpu_write(0x6000, 0x11, 0);
        assert_eq!(cart.cpu_read(0x6000), Some(0), "written, half unprotected");

        cart.cpu_write(0x5103, 1, 0);
        cart.cpu_write(0x6000, 0x11, 0);
        assert_eq!(cart.cpu_read(0x6000), Some(0x11));

        // Another bank at $6000; and the first, as RAM at $8000: Uncharted
        // Waters reads back where it didn't write, Bandit Kings writes there.
        cart.cpu_write(0x5113, 3, 0);
        assert_eq!(cart.cpu_read(0x6000), Some(0));
        cart.cpu_write(0x5114, 2, 0);
        assert_eq!(cart.cpu_read(0x8000), Some(0x11), "RAM at $8000");
        cart.cpu_write(0x8001, 0x22, 0);
        cart.cpu_write(0x5113, 2, 0);
        assert_eq!(cart.cpu_read(0x6001), Some(0x22), "written at $8000");
    }

    #[test]
    fn e000_is_never_ram() {
        let mut cart = mmc5();
        cart.cpu_write(0x5117, 0x05, 0);
        assert_eq!(cart.cpu_read(0xE000), Some(5));
    }

    fn chr(cart: &mut Cartridge) -> [u8; 8] {
        [0x0000, 0x0400, 0x0800, 0x0C00, 0x1000, 0x1400, 0x1800, 0x1C00].map(|addr| cart.chr_read(addr))
    }

    #[test]
    fn its_chr_banks_are_of_the_size_chosen() {
        let mut cart = mmc5();
        for register in 0..8 {
            cart.cpu_write(0x5120 + register, 2 * register as u8 + 2, 0);
        }
        cart.cpu_write(0x5101, 3, 0);
        assert_eq!(chr(&mut cart), [2, 4, 6, 8, 10, 12, 14, 16], "1K: its registers");

        cart.cpu_write(0x5101, 2, 0);
        assert_eq!(chr(&mut cart), [8, 9, 16, 17, 24, 25, 32, 33], "2K: $5121, 3, 5, 7, by 2K banks");

        cart.cpu_write(0x5101, 1, 0);
        assert_eq!(chr(&mut cart), [32, 33, 34, 35, 0, 1, 2, 3], "4K: $5123, $5127, by 4K banks, wrapped");

        cart.cpu_write(0x5101, 0, 0);
        assert_eq!(chr(&mut cart)[0], 16 * 8 % 64, "8K: $5127, by 8K banks, wrapped");
    }

    /// The MMC5 told rendering's on, with `tall` sprites; and the frame
    /// begun, as the PPU's reads begin one.
    fn in_frame(cart: &mut Cartridge, tall: bool) {
        cart.ppu_register_written(0x2000, if tall { 0x20 } else { 0 });
        cart.ppu_register_written(0x2001, 0x18);
        line_start(cart);
    }

    /// What a line's start is to it: three reads of one nametable address,
    /// and the read after them.
    fn line_start(cart: &mut Cartridge) {
        for _ in 0..3 {
            cart.ppu_read(0x2000);
        }
        cart.ppu_read(0x23C0);
    }

    #[test]
    fn tall_sprites_have_the_first_set_and_the_background_the_second() {
        let mut cart = mmc5();
        cart.cpu_write(0x5101, 3, 0);
        for register in 0..12 {
            cart.cpu_write(0x5120 + register, 10 + register as u8, 0);
        }
        in_frame(&mut cart, true);
        assert_eq!(cart.chr_read(0x0000), 18, "the background's $5128");
        assert_eq!(cart.chr_read(0x1000), 18, "and again at $1000");
        cart.sprite_fetch = true;
        assert_eq!(cart.chr_read(0x0000), 10, "a sprite's $5120");
        assert_eq!(cart.chr_read(0x1C00), 17, "a sprite's $5127");
        cart.sprite_fetch = false;

        // 8 by 8: the first set alone.
        let mut cart = mmc5();
        cart.cpu_write(0x5101, 3, 0);
        cart.cpu_write(0x5120, 10, 0);
        cart.cpu_write(0x5128, 18, 0);
        in_frame(&mut cart, false);
        assert_eq!(cart.chr_read(0x0000), 10, "8 by 8, the second set used");
    }

    #[test]
    fn ppudata_has_the_set_last_written_with_tall_sprites_on() {
        let mut cart = mmc5();
        cart.cpu_write(0x5101, 3, 0);
        cart.ppu_register_written(0x2000, 0x20);
        cart.ppu_register_written(0x2001, 0x18);
        cart.cpu_write(0x5120, 10, 0);
        cart.cpu_write(0x5128, 18, 0);
        assert_eq!(cart.chr_read(0x0000), 18, "the last written, the second");
        cart.cpu_write(0x5120, 10, 0);
        assert_eq!(cart.chr_read(0x0000), 10, "the last written, the first");
    }

    #[test]
    fn its_nametables_are_the_ppus_exram_or_a_fill() {
        let mut cart = mmc5();
        cart.cpu_write(0x5104, 2, 0);
        cart.cpu_write(0x5C10, 0x77, 0);
        cart.cpu_write(0x5104, 0, 0);
        cart.cpu_write(0x5106, 0x42, 0);
        cart.cpu_write(0x5107, 2, 0);
        cart.cpu_write(0x5105, 0b11_10_01_00, 0);

        assert_eq!((cart.nametable_page(0x2000), cart.nametable_read(0x2000)), (0, None), "the PPU's first");
        assert_eq!((cart.nametable_page(0x2400), cart.nametable_read(0x2400)), (1, None), "the PPU's second");
        assert_eq!(cart.nametable_read(0x2810), Some(0x77), "ExRAM");
        assert_eq!(cart.nametable_read(0x2C10), Some(0x42), "the fill's tile");
        assert_eq!(cart.nametable_read(0x2FC0), Some(0xAA), "the fill's palette, each quarter");

        assert!(cart.nametable_write(0x2820, 0x12), "ExRAM written");
        assert_eq!(cart.nametable_read(0x2820), Some(0x12));
        assert!(!cart.nametable_write(0x2000, 0x34), "the PPU's taken for ExRAM");

        // ExRAM in its RAM modes is no nametable.
        cart.cpu_write(0x5104, 2, 0);
        assert_eq!(cart.nametable_read(0x2810), Some(0), "read as zeros");
    }

    #[test]
    fn exrams_modes_say_who_reads_and_writes_it() {
        let mut cart = mmc5();
        cart.cpu_write(0x5104, 2, 0);
        cart.cpu_write(0x5C00, 0x11, 0);
        assert_eq!(cart.cpu_read(0x5C00), Some(0x11), "mode 2, RAM");

        cart.cpu_write(0x5104, 3, 0);
        cart.cpu_write(0x5C00, 0x22, 0);
        assert_eq!(cart.cpu_read(0x5C00), Some(0x11), "mode 3, read only");

        cart.cpu_write(0x5104, 0, 0);
        assert_eq!(cart.cpu_read(0x5C00), None, "mode 0, open bus");
        cart.cpu_write(0x5C00, 0x33, 0);
        cart.cpu_write(0x5104, 2, 0);
        assert_eq!(cart.cpu_read(0x5C00), Some(0x11), "mode 0, written not rendering");

        cart.cpu_write(0x5104, 0, 0);
        in_frame(&mut cart, false);
        cart.cpu_write(0x5C00, 0x33, 0);
        cart.cpu_write(0x5104, 2, 0);
        assert_eq!(cart.cpu_read(0x5C00), Some(0x33), "mode 0, written rendering");
    }

    #[test]
    fn extended_attributes_give_each_tile_a_palette_and_a_4k_bank() {
        let mut cart = mmc5();
        cart.cpu_write(0x5104, 2, 0);
        cart.cpu_write(0x5C10, 0b10_000011, 0); // palette 2, 4K bank 3
        cart.cpu_write(0x5104, 1, 0);
        in_frame(&mut cart, false);

        assert_eq!(cart.nametable_read(0x2010), None, "the tile's still the nametable's");
        assert_eq!(cart.nametable_read(0x23C4), Some(0xAA), "the attribute's its byte's");
        assert_eq!(cart.chr_read(0x0400), 3 * 4 + 1, "the pattern's its 4K's");
        cart.sprite_fetch = true;
        assert_ne!(cart.chr_read(0x0400), 3 * 4 + 1, "a sprite's its");
        cart.sprite_fetch = false;

        // Not rendering, none of it.
        cart.ppu_register_written(0x2001, 0);
        assert_eq!(cart.nametable_read(0x23C4), None);
    }

    #[test]
    fn the_split_draws_its_side_from_exram() {
        let mut cart = mmc5();
        cart.cpu_write(0x5104, 2, 0);
        cart.cpu_write(0x5C01, 0x5A, 0); // row 0, tile 1
        cart.cpu_write(0x5FC0, 0b11, 0); // its palette
        cart.cpu_write(0x5104, 0, 0);
        cart.cpu_write(0x5200, 0x80 | 2, 0); // left, 2 tiles
        cart.cpu_write(0x5202, 5, 0); // a 4K bank
        in_frame(&mut cart, false);
        cart.sprite_fetch = true;
        cart.chr_read(0); // a sprite's fetch ends the count of a line's tiles
        cart.sprite_fetch = false;

        assert_eq!(cart.nametable_read(0x2100), Some(0));
        assert_eq!(cart.nametable_read(0x2101), Some(0x5A), "the second tile, in the split");
        assert_eq!(cart.nametable_read(0x23C0), Some(0xFF), "its attribute");
        assert_eq!(cart.chr_read(0x0000), 5 * 4, "its 4K bank");
        assert_eq!(cart.nametable_read(0x2102), None, "the third, out of it");
    }

    #[test]
    fn its_multiplier_multiplies() {
        let mut cart = mmc5();
        cart.cpu_write(0x5205, 200, 0);
        cart.cpu_write(0x5206, 250, 0);
        let product = cart.cpu_read(0x5205).unwrap() as u16 | (cart.cpu_read(0x5206).unwrap() as u16) << 8;
        assert_eq!(product, 50000);
        cart.cpu_write(0x5205, 3, 0);
        assert_eq!(cart.cpu_read(0x5205), Some((3u16 * 250) as u8), "a number written, the other kept");
    }

    #[test]
    fn its_irq_comes_at_its_line() {
        let mut cart = mmc5();
        cart.cpu_write(0x5203, 2, 0);
        cart.cpu_write(0x5204, 0x80, 0);
        in_frame(&mut cart, false);
        assert_eq!(cart.cpu_read(0x5204), Some(0x40), "in frame, line 0");
        line_start(&mut cart);
        assert!(!cart.irq(), "at line 1");
        line_start(&mut cart);
        assert!(cart.irq(), "not at line 2");

        // Read, it's acknowledged, and in frame goes on.
        assert_eq!(cart.cpu_read(0x5204), Some(0xC0));
        assert!(!cart.irq(), "acknowledged");
        assert_eq!(cart.cpu_read(0x5204), Some(0x40));

        // Pending without being enabled; held when it is.
        cart.cpu_write(0x5204, 0, 0);
        cart.cpu_write(0x5203, 3, 0);
        line_start(&mut cart);
        assert!(!cart.irq(), "held, not enabled");
        cart.cpu_write(0x5204, 0x80, 0);
        assert!(cart.irq(), "not held, enabled pending");

        // Reset disables it, and nothing else.
        cart.reset();
        assert!(!cart.irq(), "held over a reset");
    }

    #[test]
    fn its_line_comes_at_the_same_place_on_every_frame() {
        // The PPU's own reads, rendering: on odd frames a dot short, and its
        // reads ending each line have to be there all the same.
        let mut cart = mmc5();
        let mut ppu = Ppu::new(Region::Ntsc);
        while !ppu.frame_ready {
            ppu.tick(&mut cart);
        }
        ppu.frame_ready = false;
        while ppu.line != 100 {
            ppu.tick(&mut cart);
        }
        ppu.write_register(0x2001, 0x18, &mut cart);
        cart.ppu_register_written(0x2001, 0x18);
        cart.cpu_write(0x5203, 46, 0);
        cart.cpu_write(0x5204, 0x80, 0);

        let mut lines = Vec::new();
        let mut held = false;
        while lines.len() < 8 {
            for _ in 0..3 {
                ppu.tick(&mut cart);
            }
            cart.cycle();
            if cart.irq() && !held {
                lines.push((ppu.line, ppu.dot < 40));
            }
            held = cart.irq();
            if ppu.frame_ready {
                ppu.frame_ready = false;
                cart.cpu_read(0xFFFA);
            }
            if held {
                cart.cpu_read(0x5204);
            }
        }
        // The first's the frame rendering was turned on in.
        assert_eq!(lines[1..], [(46, true); 7], "the IRQ's line moved");
    }

    #[test]
    fn line_0_is_no_irq() {
        let mut cart = mmc5();
        cart.cpu_write(0x5204, 0x80, 0);
        in_frame(&mut cart, false);
        for _ in 0..5 {
            line_start(&mut cart);
        }
        assert!(!cart.irq());
    }

    #[test]
    fn a_frame_ends_as_it_should() {
        let in_frame_now = |cart: &mut Cartridge| cart.cpu_read(0x5204).unwrap() & 0x40 != 0;

        // Three CPU cycles with no PPU read.
        let mut cart = mmc5();
        in_frame(&mut cart, false);
        cart.cycle();
        cart.cycle();
        assert!(in_frame_now(&mut cart), "ended early");
        cart.cycle();
        cart.cycle();
        assert!(!in_frame_now(&mut cart), "not ended by the PPU's going quiet");

        // Rendering turned off.
        let mut cart = mmc5();
        in_frame(&mut cart, false);
        cart.ppu_register_written(0x2001, 0);
        assert!(!in_frame_now(&mut cart), "not ended by $2001");

        // The NMI's vector, which takes a pending IRQ with it.
        let mut cart = mmc5();
        cart.cpu_write(0x5203, 1, 0);
        cart.cpu_write(0x5204, 0x80, 0);
        in_frame(&mut cart, false);
        line_start(&mut cart);
        assert!(cart.irq());
        cart.cpu_read(0xFFFA);
        assert!(!cart.irq() && !in_frame_now(&mut cart), "not ended by the NMI");
    }

    #[test]
    fn a_frame_after_counts_from_0_again() {
        let mut cart = mmc5();
        cart.cpu_write(0x5203, 2, 0);
        cart.cpu_write(0x5204, 0x80, 0);
        in_frame(&mut cart, false);
        line_start(&mut cart);
        cart.cpu_read(0xFFFA);
        line_start(&mut cart); // line 0
        line_start(&mut cart); // line 1
        assert!(!cart.irq(), "the last frame's count kept");
        line_start(&mut cart);
        assert!(cart.irq());
    }
}
