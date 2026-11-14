//! VERA's FX, as its reference's examples have it: the multiplier and the
//! accumulator; the line draw helper stepping ADDR1 across and up; the
//! polygon filler's fill length and ADDR1; the affine helper reading a
//! map of tiles, wrapping; the cache filled, and written by a nibble mask
//! and transparently; the 16-bit hop; 4-bit mode's nibbles.

#[cfg(test)]
mod tests {
    use crate::machines::x16::vera::Vera;

    const ADDR_L: u8 = 0x00;
    const ADDR_M: u8 = 0x01;
    const ADDR_H: u8 = 0x02;
    const DATA0: u8 = 0x03;
    const DATA1: u8 = 0x04;
    const CTRL: u8 = 0x05;
    /// $9F29 to $9F2C, as DCSEL has them.
    const FX: [u8; 4] = [0x09, 0x0A, 0x0B, 0x0C];

    /// ADDRSEL's port at `address`, stepping by `step` (ADDRx_H's bits 7
    /// to 1: its increment, DECR, nibble increment and nibble).
    fn at(vera: &mut Vera, port: u8, address: u32, step: u8) {
        vera.write(CTRL, port);
        vera.write(ADDR_L, address as u8);
        vera.write(ADDR_M, (address >> 8) as u8);
        vera.write(ADDR_H, step << 1 | (address >> 16) as u8);
    }

    /// DCSEL `dcsel`'s four registers written, those `Some`.
    fn fx(vera: &mut Vera, dcsel: u8, values: [Option<u8>; 4]) {
        vera.write(CTRL, dcsel << 1);
        for (reg, value) in FX.into_iter().zip(values) {
            if let Some(value) = value {
                vera.write(reg, value);
            }
        }
        vera.write(CTRL, 0);
    }

    /// The port's address, as ADDRx_L to _H have it.
    fn address(vera: &mut Vera, port: u8) -> u32 {
        vera.write(CTRL, port);
        let address = vera.read(ADDR_L) as u32 | (vera.read(ADDR_M) as u32) << 8;
        address | (vera.read(ADDR_H) as u32 & 1) << 16
    }

    /// Increments of 1 to 640 by their ADDRx_H index, times 2 for the
    /// register's bits 7 to 3.
    const BY_1: u8 = 1 << 3;
    const BY_4: u8 = 3 << 3;
    const BY_320: u8 = 14 << 3;
    const DECR: u8 = 1 << 2;
    const NIBBLES: u8 = 1 << 1;
    /// FX_TILEBASE's and FX_MAPBASE's bits 7 to 2, for $2000 and $3000.
    const TILES: u8 = (0x2000 >> 9) as u8;
    const MAP: u8 = (0x3000 >> 9) as u8;

    #[test]
    fn the_multiplier_writes_a_product_and_the_accumulator_sums_them() {
        let mut vera = Vera::new();
        // As the reference's example: 69 times 420, written out at 0.
        fx(&mut vera, 2, [Some(0), None, None, Some(0x10)]);
        let [l, m] = 69i16.to_le_bytes();
        let [h, u] = 420i16.to_le_bytes();
        fx(&mut vera, 6, [Some(l), Some(m), Some(h), Some(u)]);
        fx(&mut vera, 2, [Some(0x40), None, None, None]); // cache write
        at(&mut vera, 0, 0, 0);
        vera.write(DATA0, 0);
        let word = |vera: &Vera| u32::from_le_bytes([0, 1, 2, 3].map(|n| vera.peek(n)));
        assert_eq!(word(&vera) as i32, 28980);
        // Accumulated twice (reading FX_ACCUM), then -3 times 7 taken.
        vera.write(CTRL, 6 << 1);
        vera.read(FX[1]);
        vera.read(FX[1]);
        vera.write(CTRL, 0);
        fx(&mut vera, 6, [Some(0xFD), Some(0xFF), Some(7), Some(0)]);
        fx(&mut vera, 2, [None, None, None, Some(0x30)]); // multiply, subtract
        vera.write(DATA0, 0);
        assert_eq!(word(&vera) as i32, 2 * 28980 + 21);
        // FX_ACCUM_RESET, read.
        vera.write(CTRL, 6 << 1);
        assert_eq!(vera.read(FX[0]), b'V', "as DC_VER0 reads");
        vera.write(CTRL, 0);
        vera.write(DATA0, 0);
        assert_eq!(word(&vera) as i32, 21, "0 less -21");
    }

    #[test]
    fn the_line_helper_steps_addr1_on_and_by_addr0s_as_x_carries() {
        let mut vera = Vera::new();
        // Octant 0: ADDR1 +1 always, ADDR0's -320 as X carries; a slope
        // of 1/2, X at half a dot as its increment's high byte's written.
        fx(&mut vera, 2, [Some(1), None, None, None]);
        fx(&mut vera, 3, [Some(0), Some(0x01), None, None]);
        at(&mut vera, 0, 0, BY_320 | DECR);
        let start = 0x2000 + 100 * 320;
        at(&mut vera, 1, start, BY_1);
        for colour in 1..=6 {
            vera.write(DATA1, colour);
        }
        let row = |n: u32| start - n * 320;
        let wanted = [
            (row(0), 1),
            (row(1) + 1, 2),
            (row(1) + 2, 3),
            (row(2) + 3, 4),
            (row(2) + 4, 5),
            (row(3) + 5, 6),
        ];
        for (address, colour) in wanted {
            assert_eq!(vera.peek(address as usize), colour, "at {address:#X}");
        }
    }

    #[test]
    fn the_polygon_filler_gives_addr1_and_the_fill_length_each_line() {
        let mut vera = Vera::new();
        // X1 at 10, still; X2 at 20, half a dot each half step.
        fx(&mut vera, 2, [Some(2), None, None, None]);
        fx(&mut vera, 3, [Some(0), Some(0), Some(0), Some(0x01)]);
        fx(&mut vera, 4, [Some(10), Some(0), Some(20), Some(0)]);
        at(&mut vera, 0, 0x1000, BY_320);
        at(&mut vera, 1, 0, BY_1);
        vera.read(DATA1);
        assert_eq!(address(&mut vera, 1), 0x1000 + 10);
        vera.write(CTRL, 5 << 1);
        let (low, high) = (vera.read(FX[2]), vera.read(FX[3]));
        // 11: past 15 not, its low 4 bits; X1's bits 1 and 0; its high bits.
        assert_eq!(low, 0x40 | 11 << 1);
        assert_eq!(high, ((11 & 0x3F8) >> 2) as u8, "its bits 9 to 3");
        vera.write(CTRL, 0);
        for _ in 0..11 {
            vera.write(DATA1, 7);
        }
        assert!((0x100A..0x1015).all(|at| vera.peek(at) == 7));
        assert_eq!((vera.peek(0x1009), vera.peek(0x1015)), (0, 0));
        // DATA0 read: the next line, one wider.
        vera.read(DATA0);
        vera.read(DATA1);
        assert_eq!(address(&mut vera, 1), 0x1000 + 320 + 10);
        vera.write(CTRL, 5 << 1);
        assert_eq!(vera.read(FX[2]) >> 1 & 0x0F, 12, "X2 a dot on: 22 less 10");
    }

    #[test]
    fn the_affine_helper_reads_its_maps_tiles_wrapping_round() {
        let mut vera = Vera::new();
        // 8-bit tiles at $2000, an 8 by 8 map at $3000: tiles 1 and 2
        // across its top, their rows' dots $10 on and $20 on.
        for n in 0..8 {
            for (tile, base) in [(1, 0x10), (2, 0x20)] {
                at(&mut vera, 0, 0x2000 + tile * 64 + n, 0);
                vera.write(DATA0, base + n as u8);
            }
        }
        at(&mut vera, 0, 0x3000, BY_1);
        for tile in [1, 2, 0, 0, 0, 0, 0, 0] {
            vera.write(DATA0, tile);
        }
        fx(&mut vera, 2, [Some(3), Some(TILES), Some(MAP | 1), None]);
        // A dot across each read: X's increment 1, Y's 0; X and Y 0.
        fx(&mut vera, 3, [Some(0), Some(0x02), Some(0), Some(0)]);
        fx(&mut vera, 4, [Some(0), Some(0), Some(0), Some(0)]);
        let row: Vec<u8> = (0..16).map(|_| vera.read(DATA1)).collect();
        let wanted: Vec<u8> = (0x10..0x18).chain(0x20..0x28).collect();
        assert_eq!(row, wanted);
        // Its map 8 tiles across: at 64 dots, round to the first again.
        fx(&mut vera, 4, [Some(64), Some(0), None, None]);
        assert_eq!(vera.read(DATA1), 0x10);
        // Clipped: past its edge, tile 0's.
        fx(&mut vera, 2, [None, Some(TILES | 2), None, None]);
        fx(&mut vera, 4, [Some(64), Some(0), None, None]);
        assert_eq!(vera.read(DATA1), 0, "tile 0, all 0s");
    }

    #[test]
    fn the_cache_is_filled_by_reads_and_written_by_a_mask_or_transparently() {
        let mut vera = Vera::new();
        at(&mut vera, 0, 0x100, BY_1);
        for byte in [0x11, 0x00, 0x33, 0x44] {
            vera.write(DATA0, byte);
        }
        at(&mut vera, 0, 0x200, BY_1);
        for _ in 0..4 {
            vera.write(DATA0, 0xEE);
        }
        // Filled, as 4 bytes are read.
        fx(&mut vera, 2, [Some(0x20), None, None, Some(0)]);
        at(&mut vera, 0, 0x100, BY_1);
        for _ in 0..4 {
            vera.read(DATA0);
        }
        // Written, but its second byte's: the mask's bits 3 and 2.
        fx(&mut vera, 2, [Some(0x40), None, None, None]);
        at(&mut vera, 0, 0x202, 0);
        vera.write(DATA0, 0b0000_1100);
        let bytes = |vera: &Vera| [0, 1, 2, 3].map(|n| vera.peek(0x200 + n));
        assert_eq!(bytes(&vera), [0x11, 0xEE, 0x33, 0x44], "4-byte aligned");
        // Transparently: its 0, left.
        fx(&mut vera, 2, [Some(0), None, None, None]);
        at(&mut vera, 0, 0x200, BY_1);
        for _ in 0..4 {
            vera.write(DATA0, 0xEE);
        }
        fx(&mut vera, 2, [Some(0xC0), None, None, None]);
        at(&mut vera, 0, 0x200, 0);
        vera.write(DATA0, 0);
        assert_eq!(bytes(&vera), [0x11, 0xEE, 0x33, 0x44]);
        // A byte of it, cycled, 4 times over: FX_MULT's index 2.
        fx(&mut vera, 2, [Some(0x50), None, None, Some(2 << 2)]);
        vera.write(DATA0, 0);
        assert_eq!(bytes(&vera), [0x33; 4]);
    }

    #[test]
    fn the_16_bit_hop_steps_addr1_by_1_then_3() {
        let mut vera = Vera::new();
        fx(&mut vera, 2, [Some(0x08), None, None, None]);
        at(&mut vera, 1, 0x400, BY_4);
        let mut addresses = Vec::new();
        for _ in 0..4 {
            vera.read(DATA1);
            addresses.push(address(&mut vera, 1));
        }
        assert_eq!(addresses, [0x401, 0x404, 0x405, 0x408]);
    }

    #[test]
    fn in_4_bit_mode_a_port_steps_and_writes_a_nibble_at_a_time() {
        let mut vera = Vera::new();
        fx(&mut vera, 2, [Some(0x04), None, None, None]);
        at(&mut vera, 0, 0x600, NIBBLES);
        for colour in [0x1, 0x2, 0x3] {
            vera.write(DATA0, colour * 0x11);
        }
        assert_eq!((vera.peek(0x600), vera.peek(0x601)), (0x12, 0x30));
        assert_eq!(address(&mut vera, 0), 0x601);
        assert_eq!(
            vera.read(ADDR_H) & 0x06,
            0x06,
            "its nibble, and stepping by nibbles"
        );
        // FX_CTRL reads back; the rest of DCSEL 2 as DC_VER.
        vera.write(CTRL, 2 << 1);
        assert_eq!((vera.read(FX[0]), vera.read(FX[1])), (0x04, 47));
    }
}
