//! Konami's VRC2 and VRC4: its banks, each board's wiring of its registers,
//! a guess's of both, and VRC4's IRQ. VRC1's banks; VRC3's, and its IRQ.
//! VRC6's banks, 26's wiring, its CHR's banking and its nametables, of the
//! PPU's RAM or CHR ROM; its RAM's enable, its IRQ; and its sound, a step
//! at a time.

#[cfg(test)]
mod tests {
    use crate::machines::nes::cartridge::{Cartridge, Mirroring};

    /// A cartridge's IRQ held after `cycles` CPU cycles, not before.
    fn held_after(cart: &mut Cartridge, cycles: usize) {
        for cycle in 0..cycles {
            assert!(!cart.irq(), "held after {cycle} cycles");
            cart.cycle();
        }
        assert!(cart.irq(), "not held after {cycles}");
    }

    /// A VRC's cartridge, NES 2.0's, `number` with `submapper` (0, an old
    /// header's); 256K of PRG and 128K of CHR, each 16K bank of PRG its
    /// number in every byte, each 8K of CHR $C0 and its number.
    fn vrc(number: u8, submapper: u8) -> Cartridge {
        let mut image = b"NES\x1A\x10\x10".to_vec();
        let (flags_7, byte_8) = if submapper == 0 {
            (number & 0xF0, 0)
        } else {
            (number & 0xF0 | 0x08, submapper << 4)
        };
        image.extend([number << 4, flags_7, byte_8, 0, 0, 0, 0, 0, 0, 0]);
        for bank in 0..16 {
            image.extend([bank; 0x4000]);
        }
        for bank in 0..16 {
            image.extend([0xC0 | bank; 0x2000]);
        }
        Cartridge::new(&image).unwrap()
    }

    /// What's at $8020, $A020, $C020 and $E020: each its 16K bank's number.
    fn prg(cart: &mut Cartridge) -> [u8; 4] {
        [0x8020, 0xA020, 0xC020, 0xE020].map(|addr| cart.cpu_read(addr).unwrap())
    }

    #[test]
    fn its_8k_banks_and_its_prg_swapped() {
        let mut cart = vrc(21, 1); // VRC4a: A0 on A1, A1 on A2
        cart.cpu_write(0x8000, 5, 0);
        cart.cpu_write(0xA000, 7, 0);
        assert_eq!(prg(&mut cart), [2, 3, 15, 15], "the last two fixed");
        cart.cpu_write(0x9004, 0x02, 0);
        assert_eq!(prg(&mut cart), [15, 3, 2, 15], "swapped, $9002");
    }

    #[test]
    fn each_boards_wiring_of_its_registers() {
        // Each, its A0's and A1's lines: CHR bank 0's low and high written,
        // and bank 1's low, at $B000, A0 and A1.
        for (number, submapper, a0, a1, want) in [
            (21, 1, 0x02, 0x04, [0xC2, 0xC1]), // VRC4a
            (21, 2, 0x40, 0x80, [0xC2, 0xC1]), // VRC4c
            (22, 0, 0x02, 0x01, [0xC1, 0xC0]), // VRC2a, half the number
            (23, 3, 0x01, 0x02, [0xC2, 0xC1]), // VRC2b
            (23, 2, 0x04, 0x08, [0xC2, 0xC1]), // VRC4e
            (25, 1, 0x02, 0x01, [0xC2, 0xC1]), // VRC4b
            (25, 2, 0x08, 0x04, [0xC2, 0xC1]), // VRC4d
            (25, 3, 0x02, 0x01, [0xC2, 0xC1]), // VRC2c
        ] {
            let mut cart = vrc(number, submapper);
            cart.cpu_write(0xB000, 0x05, 0);
            cart.cpu_write(0xB000 | a0, 0x01, 0);
            cart.cpu_write(0xB000 | a1, 0x0E, 0);
            assert_eq!(
                [cart.chr_read(0x0000), cart.chr_read(0x0400)],
                want,
                "mapper {number}, submapper {submapper}"
            );
        }
    }

    #[test]
    fn an_old_headers_wiring_is_both_of_its_numbers() {
        for a0 in [0x02, 0x40] {
            let mut cart = vrc(21, 0);
            cart.cpu_write(0xB000, 0x05, 0);
            cart.cpu_write(0xB000 | a0, 0x01, 0);
            assert_eq!(cart.chr_read(0), 0xC2, "A0 on ${a0:02X}");
        }
    }

    #[test]
    fn a_vrc2s_nametables_are_vertical_or_horizontal_only() {
        let mut cart = vrc(23, 3);
        cart.cpu_write(0x9000, 3, 0);
        assert_eq!(cart.mirroring, Mirroring::Horizontal, "bit 1 not there");
        let mut cart = vrc(23, 0);
        cart.cpu_write(0x9000, 3, 0);
        assert_eq!(
            cart.mirroring,
            Mirroring::SingleUpper,
            "a guess, maybe a VRC4"
        );
    }

    #[test]
    fn a_vrc2s_latch_at_6000_with_no_ram() {
        // NES 2.0's, with no RAM said: VRC2b's latch, its bit 0 read back,
        // the rest open bus.
        let mut cart = vrc(23, 3);
        cart.cpu_write(0x6000, 0xFF, 0);
        assert_eq!(cart.register_read(0x6000, 0xA4), 0xA5);
        cart.cpu_write(0x6000, 0x00, 0);
        assert_eq!(cart.register_read(0x6000, 0xA5), 0xA4);
        // A VRC4's has none.
        let mut cart = vrc(21, 1);
        cart.cpu_write(0x6000, 0xFF, 0);
        assert_eq!(cart.register_read(0x6000, 0xA4), 0xA4);
    }

    #[test]
    fn vrc4s_irq_counts_cycles_or_lines_up_past_ff() {
        let mut cart = vrc(21, 1);
        // Its latch $FD, counting cycles: held on the third, past $FF.
        cart.cpu_write(0xF000, 0x0D, 0);
        cart.cpu_write(0xF002, 0x0F, 0);
        cart.cpu_write(0xF004, 0x06, 0);
        for cycle in 1..=3 {
            assert!(!cart.irq(), "held after {} cycles", cycle - 1);
            cart.cycle();
        }
        assert!(cart.irq(), "not held");
        cart.cpu_write(0xF006, 0, 0);
        assert!(!cart.irq(), "acknowledged");
        for _ in 0..1000 {
            cart.cycle();
        }
        assert!(!cart.irq(), "not enabled again on acknowledging");

        // Its latch $FF, counting lines: a line's 341 dots, 113 2/3 cycles.
        cart.cpu_write(0xF000, 0x0F, 0);
        cart.cpu_write(0xF004, 0x02, 0);
        for _ in 0..113 {
            cart.cycle();
        }
        assert!(!cart.irq(), "before the line's end");
        cart.cycle();
        assert!(cart.irq(), "at its end");
    }

    #[test]
    fn vrc1s_8k_banks_its_chrs_4k_and_its_nametables() {
        let mut cart = vrc(75, 0);
        cart.cpu_write(0x8000, 5, 0);
        cart.cpu_write(0xA000, 7, 0);
        cart.cpu_write(0xC000, 9, 0);
        assert_eq!(prg(&mut cart), [2, 3, 4, 15], "the last fixed");
        // 4K banks 3, and $15, its high bit in $9000's bit 2.
        cart.cpu_write(0xE000, 3, 0);
        cart.cpu_write(0xF000, 5, 0);
        cart.cpu_write(0x9000, 0x05, 0);
        assert_eq!([cart.chr_read(0x0000), cart.chr_read(0x1000)], [0xC1, 0xCA]);
        assert_eq!(cart.mirroring, Mirroring::Horizontal);
        cart.cpu_write(0x9000, 0x02, 0);
        assert_eq!([cart.chr_read(0x0000), cart.chr_read(0x1000)], [0xC9, 0xC2]);
        assert_eq!(cart.mirroring, Mirroring::Vertical);
    }

    #[test]
    fn vrc3s_16k_bank() {
        let mut cart = vrc(73, 0);
        cart.cpu_write(0xF000, 3, 0);
        assert_eq!(prg(&mut cart), [3, 3, 15, 15], "the last fixed");
    }

    #[test]
    fn vrc3s_irq_counts_cycles_up_past_its_16_bits_or_its_low_8() {
        let mut cart = vrc(73, 0);
        // Its latch $FFFD, all 16 bits: held on the third cycle.
        for (addr, nibble) in [(0x8000, 0xD), (0x9000, 0xF), (0xA000, 0xF), (0xB000, 0xF)] {
            cart.cpu_write(addr, nibble, 0);
        }
        cart.cpu_write(0xC000, 0x02, 0);
        held_after(&mut cart, 3);
        cart.cpu_write(0xD000, 0, 0);
        assert!(!cart.irq(), "acknowledged");
        for _ in 0..0x20000 {
            cart.cycle();
        }
        assert!(!cart.irq(), "not enabled again on acknowledging");

        // $12FE, its low 8 bits only: past $FF on the second, those
        // reloaded, and again, enabled on acknowledging.
        for (addr, nibble) in [(0x8000, 0xE), (0x9000, 0xF), (0xA000, 0x2), (0xB000, 0x1)] {
            cart.cpu_write(addr, nibble, 0);
        }
        cart.cpu_write(0xC000, 0x07, 0);
        held_after(&mut cart, 2);
        cart.cpu_write(0xD000, 0, 0);
        held_after(&mut cart, 2);
    }

    /// VRC6's cartridge, `number` 24 or 26, an old header's: 256K of PRG,
    /// each 8K its number in every byte, and 256K of CHR, each 1K its
    /// number.
    fn vrc6(number: u8) -> Cartridge {
        let mut image = b"NES\x1A\x10\x20".to_vec();
        image.extend([number << 4, number & 0xF0, 0, 0, 0, 0, 0, 0, 0, 0]);
        for bank in 0..32 {
            image.extend([bank; 0x2000]);
        }
        for page in 0..256 {
            image.extend([page as u8; 0x400]);
        }
        Cartridge::new(&image).unwrap()
    }

    fn pages(cart: &mut Cartridge) -> Vec<u8> {
        (0..8).map(|page| cart.chr_read(page * 0x400)).collect()
    }

    #[test]
    fn vrc6s_banks_and_26s_wiring() {
        let mut cart = vrc6(24);
        assert_eq!(cart.mapper_name(), "MAPPER 24 VRC6");
        cart.cpu_write(0x8000, 3, 0);
        cart.cpu_write(0xC000, 9, 0);
        assert_eq!(prg(&mut cart), [6, 7, 9, 31], "16K, 8K, the last fixed");
        for (n, at) in [
            0xD000, 0xD001, 0xD002, 0xD003, 0xE000, 0xE001, 0xE002, 0xE003,
        ]
        .into_iter()
        .enumerate()
        {
            cart.cpu_write(at, 0x40 + n as u8, 0);
        }
        assert_eq!(
            pages(&mut cart),
            [0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47]
        );
        // 26's: A0 and A1 the other way round.
        let mut cart = vrc6(26);
        assert_eq!(cart.mapper_name(), "MAPPER 26 VRC6");
        cart.cpu_write(0xD001, 0x55, 0);
        cart.cpu_write(0xD002, 0x66, 0);
        assert_eq!(pages(&mut cart)[1..3], [0x66, 0x55]);
    }

    #[test]
    fn vrc6s_chr_banked_by_1k_or_2k() {
        let mut cart = vrc6(24);
        for register in 0..8 {
            let at = [0xD000, 0xE000][register / 4] + register as u16 % 4;
            cart.cpu_write(at, 0x10 * register as u8 + 1, 0);
        }
        // Four 2K, each its number's even and odd, or each half all of it.
        cart.cpu_write(0xB003, 0x21, 0);
        assert_eq!(
            pages(&mut cart),
            [0x00, 0x01, 0x10, 0x11, 0x20, 0x21, 0x30, 0x31]
        );
        cart.cpu_write(0xB003, 0x01, 0);
        assert_eq!(
            pages(&mut cart),
            [0x01, 0x01, 0x11, 0x11, 0x21, 0x21, 0x31, 0x31]
        );
        // Four 1K, then registers 4 and 5 2K.
        cart.cpu_write(0xB003, 0x22, 0);
        assert_eq!(
            pages(&mut cart),
            [0x01, 0x11, 0x21, 0x31, 0x40, 0x41, 0x50, 0x51]
        );
    }

    #[test]
    fn vrc6s_nametables_the_ppus_or_chr_roms() {
        let mut cart = vrc6(24);
        let tables = |cart: &mut Cartridge| {
            [0x2000, 0x2400, 0x2800, 0x2C00].map(|addr| cart.nametable_page(addr))
        };
        for (mode, want) in [
            (0x20, [0, 1, 0, 1]),
            (0x24, [0, 0, 1, 1]),
            (0x28, [0, 0, 0, 0]),
            (0x2C, [1, 1, 1, 1]),
        ] {
            cart.cpu_write(0xB003, mode, 0);
            assert_eq!(tables(&mut cart), want, "mode {mode:02X}");
        }
        // Without bit 5, registers 6 and 7's bit 0.
        cart.cpu_write(0xE002, 0x11, 0);
        cart.cpu_write(0xE003, 0x20, 0);
        cart.cpu_write(0xB003, 0x00, 0);
        assert_eq!(tables(&mut cart), [1, 1, 0, 0]);
        assert_eq!(cart.nametable_read(0x2000), None);
        assert!(!cart.watches_fetches());
        // CHR ROM's, those pages, not written.
        cart.cpu_write(0xB003, 0x30, 0);
        let read = [0x2000, 0x2400, 0x2800, 0x2C00].map(|addr| cart.nametable_read(addr));
        assert_eq!(
            read,
            [0x10, 0x11, 0x20, 0x21].map(Some),
            "vertical, of CHR ROM"
        );
        assert!(cart.watches_fetches());
        assert!(cart.nametable_write(0x2000, 0x99));
    }

    #[test]
    fn vrc6s_ram_once_enabled_and_its_irq() {
        let mut cart = vrc6(24);
        cart.cpu_write(0x6000, 0x5A, 0);
        assert_eq!(cart.cpu_read(0x6000), None);
        cart.cpu_write(0xB003, 0x80, 0);
        cart.cpu_write(0x6000, 0x5A, 0);
        assert_eq!(cart.cpu_read(0x6000), Some(0x5A));
        // As VRC4's: up from its latch, held going past $FF.
        cart.cpu_write(0xF000, 0xFE, 0);
        cart.cpu_write(0xF001, 0x06, 0);
        held_after(&mut cart, 2);
        cart.cpu_write(0xF002, 0, 0);
        assert!(!cart.irq(), "acknowledged");
    }

    /// The VRC6's sound now: its channels' levels added.
    fn level(cart: &Cartridge) -> u8 {
        (cart.output() / 0.015).round() as u8
    }

    /// Its level after each of `cycles` cycles.
    fn levels(cart: &mut Cartridge, cycles: usize) -> Vec<u8> {
        (0..cycles)
            .map(|_| {
                cart.cycle();
                level(cart)
            })
            .collect()
    }

    #[test]
    fn vrc6s_pulses_their_duty_in_16ths() {
        let mut cart = vrc6(24);
        cart.cpu_write(0x9000, 0x1F, 0); // volume 15, on for 2 steps of 16
        cart.cpu_write(0x9002, 0x80, 0); // period 0: a step a cycle
        assert_eq!(level(&cart), 15);
        let mut want = vec![15];
        want.extend([0; 14]);
        want.push(15);
        assert_eq!(levels(&mut cart, 16), want);
        cart.cpu_write(0x9000, 0x8F, 0);
        assert_eq!(levels(&mut cart, 16), [15; 16], "always on");
        // A period of 1, two cycles a step; shifted right 4, the same.
        cart.cpu_write(0x9000, 0x0F, 0);
        cart.cpu_write(0x9002, 0x00, 0); // disabled: back at its first step
        cart.cpu_write(0xA001, 0x10, 0);
        cart.cpu_write(0xA000, 0x17, 0); // volume 7, on for 2 steps of 16
        cart.cpu_write(0xA002, 0x80, 0);
        cart.cpu_write(0x9003, 0x02, 0);
        assert_eq!(levels(&mut cart, 4)[..], [7, 7, 0, 0]);
        // Halted: none of them steps.
        cart.cpu_write(0x9003, 0x01, 0);
        assert_eq!(levels(&mut cart, 4), [0; 4]);
    }

    #[test]
    fn vrc6s_sawtooth_its_rate_added_every_other_step_of_14() {
        let mut cart = vrc6(24);
        cart.cpu_write(0xB000, 0x08, 0);
        cart.cpu_write(0xB002, 0x80, 0);
        assert_eq!(
            levels(&mut cart, 14),
            [0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 0]
        );
        levels(&mut cart, 4);
        cart.cpu_write(0xB002, 0x00, 0);
        assert_eq!(level(&cart), 0, "disabled");
        cart.cpu_write(0xB002, 0x80, 0);
        assert_eq!(levels(&mut cart, 2), [0, 1], "started over");
    }
}
