//! VERA: its ports' addresses, stepping and fetching; its palette, and
//! RESET; its lines' time, SCANLINE, and its interrupts; a text layer and
//! a tile layer drawn, scaled; its SPI port, no card on it.

#[cfg(test)]
mod tests {
    use crate::machines::x16::vera::{HEIGHT, LINE_CYCLES, LINES, Vera, WIDTH};

    const ADDR_L: u8 = 0x00;
    const ADDR_M: u8 = 0x01;
    const ADDR_H: u8 = 0x02;
    const DATA0: u8 = 0x03;
    const DATA1: u8 = 0x04;
    const CTRL: u8 = 0x05;
    const IEN: u8 = 0x06;
    const ISR: u8 = 0x07;
    const IRQLINE_L: u8 = 0x08;
    const DC_VIDEO: u8 = 0x09;
    const DC_HSCALE: u8 = 0x0A;
    const L0_CONFIG: u8 = 0x0D;
    const L0_TILEBASE: u8 = 0x0F;
    const L1_CONFIG: u8 = 0x14;
    const L1_MAPBASE: u8 = 0x15;
    const L1_TILEBASE: u8 = 0x16;
    const SPI_DATA: u8 = 0x1E;
    const SPI_CTRL: u8 = 0x1F;

    /// Port 0 at `address`, stepping by `step` (ADDRx_H's bits 7 to 3).
    fn at(vera: &mut Vera, address: u32, step: u8) {
        vera.write(ADDR_L, address as u8);
        vera.write(ADDR_M, (address >> 8) as u8);
        vera.write(ADDR_H, step << 3 | (address >> 16) as u8);
    }

    /// `bytes` written from `address` on.
    fn poke(vera: &mut Vera, address: u32, bytes: &[u8]) {
        at(vera, address, 0x02);
        for &byte in bytes {
            vera.write(DATA0, byte);
        }
    }

    fn lines(vera: &mut Vera, lines: u32) {
        for _ in 0..lines * LINE_CYCLES {
            vera.tick();
        }
    }

    #[test]
    fn a_port_reads_and_writes_stepping_as_its_address_says() {
        let mut vera = Vera::new();
        poke(&mut vera, 0x1_2340, &[0x11, 0x22, 0x33]);
        assert_eq!(
            (vera.peek(0x1_2340), vera.peek(0x1_2342)),
            (0x11, 0x33),
            "by 1, bit 16 too"
        );
        at(&mut vera, 0x1_2340, 0x04); // by 2
        assert_eq!((vera.read(DATA0), vera.read(DATA0)), (0x11, 0x33));
        at(&mut vera, 0x1_2342, 0x03); // by 1, down
        assert_eq!((vera.read(DATA0), vera.read(DATA0)), (0x33, 0x22));
        at(&mut vera, 0x100, 0x16); // by 40
        vera.write(DATA0, 0xAA);
        assert_eq!(
            vera.read(ADDR_L) as u32 | (vera.read(ADDR_M) as u32) << 8,
            0x128
        );
        // Port 1, as ADDRSEL picks it, with an address of its own.
        vera.write(CTRL, 0x01);
        vera.write(ADDR_L, 0x00);
        vera.write(ADDR_M, 0x01);
        vera.write(ADDR_H, 0x00);
        assert_eq!(vera.read(DATA1), 0xAA);
        assert_eq!(vera.read(DATA1), 0xAA, "not stepped: by 0");
        vera.write(CTRL, 0x00);
        assert_eq!(vera.read(ADDR_L), 0x28, "port 0's as it was");
    }

    #[test]
    fn data_is_fetched_as_the_address_is_set_not_as_its_written() {
        let mut vera = Vera::new();
        poke(&mut vera, 0x500, &[0x11]);
        at(&mut vera, 0x500, 0x00);
        vera.write(CTRL, 0x01);
        vera.write(ADDR_L, 0x00);
        vera.write(ADDR_M, 0x05);
        vera.write(DATA1, 0x99);
        vera.write(CTRL, 0x00);
        assert_eq!(vera.read(DATA0), 0x11, "port 0 fetched it before");
        at(&mut vera, 0x500, 0x00);
        assert_eq!(vera.read(DATA0), 0x99);
    }

    #[test]
    fn scanline_counts_lines_and_vsync_and_line_interrupt_as_theyre_on() {
        let mut vera = Vera::new();
        vera.write(IRQLINE_L, 100);
        lines(&mut vera, 100);
        assert_eq!(vera.read(IRQLINE_L), 100, "SCANLINE");
        assert_eq!(vera.read(ISR) & 0x03, 0x02, "LINE, its line begun");
        assert!(!vera.irq(), "not on");
        vera.write(IEN, 0x03);
        assert!(vera.irq());
        vera.write(ISR, 0x02);
        assert!(!vera.irq(), "cleared");
        lines(&mut vera, HEIGHT as u32 - 101);
        assert_eq!(vera.read(ISR) & 0x01, 0, "VSYNC as line 480 begins");
        lines(&mut vera, 1);
        assert_eq!(vera.read(ISR) & 0x01, 0x01);
        assert!(vera.irq());
        lines(&mut vera, 511 - HEIGHT as u32);
        assert_eq!(vera.read(IEN) & 0x40, 0x40, "SCANLINE's bit 8");
        lines(&mut vera, 1);
        assert_eq!(vera.read(IRQLINE_L), 0xFF, "511 past 511");
        assert!(!vera.frame_ended());
        lines(&mut vera, LINES as u32 - 512);
        assert!(vera.frame_ended());
        assert_eq!(vera.read(IRQLINE_L), 0);
    }

    #[test]
    fn aflow_is_always_on_the_fifo_empty() {
        let mut vera = Vera::new();
        assert_eq!(vera.read(ISR) & 0x08, 0x08);
        assert_eq!(vera.read(0x1B) & 0x40, 0x40, "AUDIO_CTRL: empty");
        vera.write(IEN, 0x08);
        assert!(vera.irq());
    }

    /// Layer 1 as text, its map at 0, 32 by 32, its font at $1000: a
    /// character, 1, at its top left, its first dot set, white on blue.
    fn text(vera: &mut Vera) {
        vera.write(L1_CONFIG, 0x00);
        vera.write(L1_MAPBASE, 0x00);
        vera.write(L1_TILEBASE, 0x08); // $1000
        poke(vera, 0x0000, &[0x01, 0x61]);
        poke(vera, 0x1008, &[0x80]);
        vera.write(DC_VIDEO, 0x21);
    }

    #[test]
    fn a_text_layer_shows_its_characters_in_their_colours() {
        let mut vera = Vera::new();
        text(&mut vera);
        // Two frames: the first's line 0 drawn as the registers were as the
        // line before it ended, before they were set.
        lines(&mut vera, LINES as u32 * 2);
        assert_eq!(
            (vera.picture[0], vera.picture[1]),
            (0xFFFFFF, 0x0000AA),
            "colour 1 on 6"
        );
        assert_eq!(vera.picture[WIDTH], 0x0000AA, "its next line");
        // The palette's entry 1 made red, as it's written at $1FA02.
        poke(&mut vera, 0x1FA02, &[0x00, 0x0F]);
        lines(&mut vera, LINES as u32);
        assert_eq!(vera.picture[0], 0xFF0000);
        vera.write(DC_HSCALE, 64);
        lines(&mut vera, LINES as u32);
        assert_eq!(
            (vera.picture[1], vera.picture[2]),
            (0xFF0000, 0x0000AA),
            "each dot twice"
        );
        vera.write(CTRL, 0x80);
        assert_eq!(vera.peek(0x1008), 0x80, "RESET leaves video RAM");
        text(&mut vera);
        lines(&mut vera, LINES as u32 * 2);
        assert_eq!(vera.picture[0], 0xFFFFFF, "and the palette its own");
        vera.write(DC_VIDEO, 0x20);
        lines(&mut vera, LINES as u32);
        assert_eq!(vera.picture[0], 0, "its output off");
    }

    #[test]
    fn a_tiles_dots_are_flipped_and_offset_in_the_palette() {
        let mut vera = Vera::new();
        // Layer 0, 4 bits a dot, its map at 0, its tiles at $1000: tile 1,
        // palette offset 1, flipped across; its first row's dots 1 and 2.
        vera.write(L0_CONFIG, 0x02);
        vera.write(L0_TILEBASE, 0x08); // $1000
        poke(&mut vera, 0x0000, &[0x01, 0x14]);
        poke(&mut vera, 0x1020, &[0x12]);
        vera.write(DC_VIDEO, 0x11);
        lines(&mut vera, LINES as u32 * 2);
        // The default palette's 17 and 18, greys: $111 and $222.
        assert_eq!(
            (vera.picture[7], vera.picture[6], vera.picture[0]),
            (0x111111, 0x222222, 0x000000)
        );
    }

    #[test]
    fn the_spi_port_has_no_card_and_its_busy_a_byte_s_time() {
        let mut vera = Vera::new();
        vera.write(SPI_DATA, 0x40);
        assert_eq!(vera.read(SPI_CTRL) & 0x80, 0, "not selected: not sent");
        vera.write(SPI_CTRL, 0x01);
        vera.write(SPI_DATA, 0x40);
        assert_eq!(vera.read(SPI_CTRL), 0x81, "busy");
        for _ in 0..6 {
            vera.tick();
        }
        assert_eq!(vera.read(SPI_CTRL), 0x01);
        assert_eq!(vera.read(SPI_DATA), 0xFF, "nothing answered");
    }
}
