//! Konami's VRC2 and VRC4: its banks, each board's wiring of its registers,
//! a guess's of both, and VRC4's IRQ.

#[cfg(test)]
mod tests {
    use crate::machines::nes::cartridge::{Cartridge, Mirroring};

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
}
