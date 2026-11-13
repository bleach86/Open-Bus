//! MARIA: a line drawn from a display list into its buffer, shown a line
//! later through the palette; its DMA's cycles; holey DMA; indirect mode;
//! the DLI; and the read modes.

#[cfg(test)]
mod tests {
    use crate::machines::atari7800::maria::Maria;

    const LINES: u32 = 263;
    const BACKGROUND: u8 = 0x00;
    const CTRL: u8 = 0x1C;
    const DPPH: u8 = 0x0C;
    const DPPL: u8 = 0x10;
    const CHARBASE: u8 = 0x14;
    /// CTRL: DMA on, 160A.
    const DMA_ON: u8 = 0x40;

    /// 64K of memory: the DLL at $2000, its first zone's DL at $2100.
    struct Memory(Vec<u8>);

    impl Memory {
        fn new() -> Memory {
            Memory(vec![0; 0x10000])
        }

        fn put(&mut self, addr: u16, bytes: &[u8]) {
            let at = addr as usize;
            self.0[at..at + bytes.len()].copy_from_slice(bytes);
        }

        /// A DLL of zones, each `(flags, DL)`, at $2000.
        fn dll(&mut self, zones: &[(u8, u16)]) {
            for (n, &(flags, dl)) in zones.iter().enumerate() {
                self.put(0x2000 + 3 * n as u16, &[flags, (dl >> 8) as u8, dl as u8]);
            }
        }
    }

    /// MARIA on, its DLL at $2000, its colours palette p's colour c, p * 16
    /// + c, the background $0E.
    fn maria() -> Maria {
        let mut maria = Maria::new();
        maria.write(DPPH, 0x20);
        maria.write(DPPL, 0x00);
        maria.write(BACKGROUND, 0x0E);
        for register in (0..32u8).filter(|register| register & 3 != 0) {
            maria.write(register, (register >> 2) << 4 | register & 3);
        }
        maria.write(CTRL, DMA_ON);
        maria
    }

    /// Lines 16 and 17's DMA, the frame's first, `memory` read: line 16's
    /// cycles, and whether it interrupted.
    fn first_line(maria: &mut Maria, memory: &Memory) -> (u32, bool) {
        let mut read = |addr: u16| memory.0[addr as usize];
        let first = maria.dma(16, LINES, &mut read);
        maria.dma(17, LINES, &mut read);
        first
    }

    /// The cells shown, `x` to `x + n`, each its left dot's colour.
    fn shown(maria: &Maria, x: usize, n: usize) -> Vec<u8> {
        (x..x + n).map(|x| maria.dots(x)[0]).collect()
    }

    #[test]
    fn an_object_is_drawn_at_its_place_in_its_palette_shown_a_line_later() {
        let mut memory = Memory::new();
        memory.dll(&[(0x00, 0x2100)]);
        // A byte at $3000, palette 2, 1 wide, at cell 10: colours 3, 2, 1, 0.
        memory.put(0x2100, &[0x00, 2 << 5 | 0x1F, 0x30, 10]);
        memory.put(0x3000, &[0b11_10_01_00]);
        let mut maria = self::maria();
        let (cycles, interrupted) = first_line(&mut maria, &memory);
        assert_eq!(
            shown(&maria, 9, 6),
            [0x0E, 0x23, 0x22, 0x21, 0x0E, 0x0E],
            "colour 0 not drawn: the background"
        );
        assert_eq!(
            cycles,
            16 + 8 + 3 + 8,
            "start, a header, a byte, a zone's end"
        );
        assert!(!interrupted);
    }

    #[test]
    fn a_zones_lines_are_its_graphics_a_page_apart_the_offset_counting_down() {
        let mut memory = Memory::new();
        // A zone of 2 lines: offset 1, then 0.
        memory.dll(&[(0x01, 0x2100), (0x00, 0x2200)]);
        memory.put(0x2100, &[0x00, 0x1F, 0x30, 0]);
        memory.put(0x3100, &[0b01_01_01_01]);
        memory.put(0x3000, &[0b10_10_10_10]);
        let mut maria = self::maria();
        let mut read = |addr: u16| memory.0[addr as usize];
        maria.dma(16, LINES, &mut read);
        maria.dma(17, LINES, &mut read);
        assert_eq!(shown(&maria, 0, 1), [0x01], "offset 1: $3100");
        maria.dma(18, LINES, &mut read);
        assert_eq!(shown(&maria, 0, 1), [0x01], "line 17, the same");
        maria.dma(19, LINES, &mut read);
        assert_eq!(shown(&maria, 0, 1), [0x02], "offset 0: $3000");
    }

    #[test]
    fn holey_dma_leaves_out_graphics_where_a12_is_set_counting_a_hole_once() {
        let mut memory = Memory::new();
        // 4K holes; the graphics at $9000, A12 set.
        memory.dll(&[(0x40, 0x2100)]);
        memory.put(0x2100, &[0x00, 0x1E, 0x90, 0]);
        memory.put(0x9000, &[0xFF, 0xFF]);
        let mut maria = self::maria();
        let (cycles, _) = first_line(&mut maria, &memory);
        assert_eq!(shown(&maria, 0, 4), [0x0E; 4], "nothing drawn");
        assert_eq!(cycles, 16 + 8 + 3 + 8, "a hole's 3 cycles, once");
    }

    #[test]
    fn indirect_mode_reads_a_character_then_its_graphics_at_charbase() {
        let mut memory = Memory::new();
        memory.dll(&[(0x00, 0x2100)]);
        // An extended header: indirect, palette 1, 1 wide, at cell 0.
        memory.put(0x2100, &[0x00, 0x60, 0x30, 1 << 5 | 0x1F, 0]);
        memory.put(0x3000, &[0x05]);
        memory.put(0x4005, &[0b11_11_11_11]);
        let mut maria = self::maria();
        maria.write(CHARBASE, 0x40);
        let (cycles, _) = first_line(&mut maria, &memory);
        assert_eq!(shown(&maria, 0, 4), [0x13; 4]);
        assert_eq!(
            cycles,
            16 + 10 + 3 + 3 + 8,
            "an extended header, a character, its byte"
        );
    }

    #[test]
    fn a_zone_flagged_interrupts_as_its_entry_is_read() {
        let mut memory = Memory::new();
        memory.dll(&[(0x00, 0x2100), (0x80, 0x2100)]);
        let mut maria = self::maria();
        let mut read = |addr: u16| memory.0[addr as usize];
        let (cycles, interrupted) = maria.dma(16, LINES, &mut read);
        assert!(!interrupted);
        assert_eq!(cycles, 16 + 8 + 17, "a zone's end, its DLI's start");
        let (_, interrupted) = maria.dma(17, LINES, &mut read);
        assert!(interrupted, "the next zone's flag, as it's read");
    }

    #[test]
    fn kangaroo_mode_draws_colour_0_and_write_mode_two_cells_a_byte() {
        // Two objects in one place: the second's colour 0 cells, left out,
        // or in kangaroo mode drawn, the background.
        let mut memory = Memory::new();
        memory.dll(&[(0x00, 0x2100)]);
        memory.put(0x2100, &[0x00, 0x1F, 0x30, 0, 0x01, 0x1F, 0x30, 0]);
        memory.put(0x3000, &[0b11_11_11_11, 0b00_11_00_11]);
        let mut maria = self::maria();
        first_line(&mut maria, &memory);
        assert_eq!(shown(&maria, 0, 4), [0x03; 4]);
        let mut maria = self::maria();
        maria.write(CTRL, DMA_ON | 0x04);
        first_line(&mut maria, &memory);
        assert_eq!(shown(&maria, 0, 4), [0x0E, 0x03, 0x0E, 0x03]);

        // 160B: an extended header's write mode, a byte's two cells, each
        // two bits of colour and two of palette.
        let mut memory = Memory::new();
        memory.dll(&[(0x00, 0x2100)]);
        memory.put(0x2100, &[0x00, 0xC0, 0x30, 0x1F, 0]);
        memory.put(0x3000, &[0b01_10_11_00]);
        let mut maria = self::maria();
        first_line(&mut maria, &memory);
        assert_eq!(
            shown(&maria, 0, 2),
            [(0b11 << 4) | 0b01, (0b00 << 4) | 0b10]
        );
    }

    #[test]
    fn the_320_modes_show_a_cell_as_two_dots() {
        let mut memory = Memory::new();
        memory.dll(&[(0x00, 0x2100)]);
        memory.put(0x2100, &[0x00, 1 << 5 | 0x1F, 0x30, 0]);
        // Cell 0 colour 3, palette 1: bits 1 and 0 the two dots.
        memory.put(0x3000, &[0b11_00_00_00]);
        let mut maria = self::maria();
        maria.write(CTRL, DMA_ON | 0x03);
        first_line(&mut maria, &memory);
        assert_eq!(
            maria.dots(0),
            [0x12, 0x12],
            "320A: each dot colour 2 or the background"
        );
        maria.write(CTRL, DMA_ON | 0x02);
        assert_eq!(
            maria.dots(0),
            [0x02, 0x03],
            "320B: its bits 1 and 0 a dot each"
        );
    }

    #[test]
    fn the_background_is_every_fourth_colour_and_colour_kill_greys_them() {
        let mut maria = self::maria();
        maria.write(BACKGROUND, 0x46);
        assert!(
            maria
                .colours
                .iter()
                .step_by(4)
                .all(|&colour| colour == 0x46)
        );
        maria.write(CTRL, 0x80);
        assert_eq!(maria.dots(0), [0x06, 0x06]);
    }
}
