//! VERA: its ports' addresses, stepping and fetching; its palette, and
//! RESET; its lines' time, SCANLINE, and its interrupts, VGA's and NTSC's
//! and RGB's fields, each drawing half the lines; a text layer and
//! a tile layer drawn, scaled; sprites, over and under the layers, their
//! collisions and their renderer's time; its SPI port, no card on it.

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

    /// Ticks to the end of the field, or frame, being sent.
    fn to_field_end(vera: &mut Vera) -> u32 {
        let mut ticks = 0;
        while !vera.frame_ended() {
            vera.tick();
            ticks += 1;
        }
        ticks
    }

    /// DC_VIDEO's bit 7, the field being sent.
    fn field(vera: &mut Vera) -> u8 {
        vera.read(DC_VIDEO) >> 7
    }

    #[test]
    fn ntsc_and_rgb_are_two_fields_each_of_525_half_lines_interrupting_each() {
        let mut vera = Vera::new();
        vera.write(DC_VIDEO, 0x03); // RGB
        vera.write(IRQLINE_L, 101);
        assert_eq!(field(&mut vera), 0);
        // 525 half-lines of 794 dots at 25 MHz, the CPU's at 8.
        assert_eq!(to_field_end(&mut vera), 133_392, "a field");
        assert_eq!(field(&mut vera), 1, "the second");
        assert_eq!(vera.read(ISR) & 0x03, 0x03, "VSYNC and LINE, the first's");
        vera.write(ISR, 0x03);
        // LINE at line 100 of the picture, IRQ_LINE's bit 0 left out:
        // SCANLINE even in the first field, odd in the second.
        let line_begun = |vera: &mut Vera| {
            while vera.read(ISR) & 0x02 == 0 {
                vera.tick();
            }
            vera.write(ISR, 0x02);
            (vera.read(IRQLINE_L), field(vera))
        };
        assert_eq!(line_begun(&mut vera), (101, 1));
        assert_eq!(vera.read(ISR) & 0x01, 0, "not VSYNC yet");
        assert_eq!(line_begun(&mut vera), (100, 0), "the next frame's first");
        assert_eq!(vera.read(ISR) & 0x01, 0x01, "VSYNC, the second's");
        vera.write(DC_VIDEO, 0x01);
        vera.frame_ended();
        to_field_end(&mut vera);
        assert_eq!(
            to_field_end(&mut vera),
            134_400,
            "VGA: a frame of 525 lines"
        );
    }

    #[test]
    fn a_field_draws_the_pictures_even_lines_or_its_odd() {
        let mut vera = Vera::new();
        text(&mut vera);
        vera.write(DC_VIDEO, 0x23); // RGB
        to_field_end(&mut vera);
        to_field_end(&mut vera);
        vera.picture.fill(0);
        to_field_end(&mut vera);
        let first = |vera: &Vera, y: usize| vera.picture[y * WIDTH];
        assert_eq!(
            [first(&vera, 0), first(&vera, 1)],
            [0xFFFFFF, 0],
            "the first's even"
        );
        to_field_end(&mut vera);
        assert_eq!(first(&vera, 1), 0x0000AA, "the second's odd");
        // 240P, VSCALE half: both fields the even lines, the picture's line
        // 0 on 0, the odd ones cleared and left dark.
        vera.write(0x0B, 64);
        vera.write(DC_VIDEO, 0x2B);
        assert_eq!(first(&vera, 0), 0, "cleared");
        for _ in 0..2 {
            to_field_end(&mut vera);
            assert_eq!([first(&vera, 0), first(&vera, 1)], [0xFFFFFF, 0]);
        }
        assert_eq!(
            first(&vera, 2),
            0x0000AA,
            "its line 1: its 240 on the even 480"
        );
    }

    #[test]
    fn ntsc_shows_a_tvs_edge_dimmed_rgb_not() {
        let mut vera = Vera::new();
        poke(&mut vera, 0x1FA00, &[0xFF, 0x0F]); // colour 0 white
        let corner_and_middle = |vera: &mut Vera, video: u8| {
            vera.write(DC_VIDEO, video);
            for _ in 0..4 {
                to_field_end(vera);
            }
            [vera.picture[0], vera.picture[240 * WIDTH + 320]]
        };
        assert_eq!(corner_and_middle(&mut vera, 0x02), [0x3F3F3F, 0xFFFFFF]);
        assert_eq!(corner_and_middle(&mut vera, 0x03), [0xFFFFFF, 0xFFFFFF]);
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

    /// Sprite `n`'s attributes: its image at `image`, 4 bits a dot or
    /// `bytes`, at `(x, y)`; byte 6 (mask, Z, flips) and 7 (size, offset).
    fn sprite(
        vera: &mut Vera,
        n: u32,
        image: u32,
        bytes: bool,
        (x, y): (i32, i32),
        six: u8,
        seven: u8,
    ) {
        let (x, y) = ((x & 0x3FF) as u16, (y & 0x3FF) as u16);
        let attributes = [
            (image >> 5) as u8,
            (image >> 13) as u8 | (bytes as u8) << 7,
            x as u8,
            (x >> 8) as u8,
            y as u8,
            (y >> 8) as u8,
            six,
            seven,
        ];
        poke(vera, 0x1FC00 + n * 8, &attributes);
    }

    const RED: u32 = 0x880000; // colour 2
    const BLUE: u32 = 0x0000AA; // 6

    #[test]
    fn a_sprite_is_drawn_over_or_under_the_layers_as_its_z_depth_says() {
        let mut vera = Vera::new();
        // Layer 1's character cell at 0 to 7 across; a sprite of 8 by 8 in
        // colour 2 at 4, half over the cell and half over nothing.
        text(&mut vera);
        poke(&mut vera, 0x2000, &[0x22; 32]);
        vera.write(DC_VIDEO, 0x61);
        let shown = |vera: &mut Vera, z: u8| {
            sprite(vera, 0, 0x2000, false, (4, 0), z << 2, 0x00);
            lines(vera, LINES as u32 * 2);
            (
                vera.picture[3],
                vera.picture[4],
                vera.picture[8],
                vera.picture[12],
            )
        };
        assert_eq!(shown(&mut vera, 3), (BLUE, RED, RED, 0), "over layer 1");
        assert_eq!(shown(&mut vera, 2), (BLUE, BLUE, RED, 0), "under it");
        assert_eq!(shown(&mut vera, 1), (BLUE, BLUE, RED, 0), "under it");
        assert_eq!(shown(&mut vera, 0), (BLUE, BLUE, 0, 0), "off");
        vera.write(DC_VIDEO, 0x21);
        assert_eq!(shown(&mut vera, 3), (BLUE, BLUE, 0, 0), "all off");
    }

    #[test]
    fn a_sprites_dots_are_flipped_offset_and_shown_off_its_edge() {
        let mut vera = Vera::new();
        vera.write(DC_VIDEO, 0x41);
        // 4 bits a dot: its first row's first two dots, 1 and 2; palette
        // offset 1, the default's greys, 17 and 18.
        poke(&mut vera, 0x2000, &[0x12]);
        let line = |vera: &mut Vera, y: usize, xs: [usize; 2]| {
            lines(vera, LINES as u32 * 2);
            xs.map(|x| vera.picture[y * WIDTH + x])
        };
        sprite(&mut vera, 0, 0x2000, false, (4, 0), 0x0C, 0x01);
        assert_eq!(line(&mut vera, 0, [4, 5]), [0x111111, 0x222222]);
        sprite(&mut vera, 0, 0x2000, false, (4, 0), 0x0D, 0x01);
        assert_eq!(line(&mut vera, 0, [11, 10]), [0x111111, 0x222222], "H-flip");
        sprite(&mut vera, 0, 0x2000, false, (4, 0), 0x0E, 0x01);
        assert_eq!(line(&mut vera, 7, [4, 5]), [0x111111, 0x222222], "V-flip");
        // 8 bits a dot, at -4: its dots 4 and 5 at 0 and 1. The offset's
        // added to 1 to 15 only.
        poke(&mut vera, 0x2000, &[0, 0, 0, 0, 0x01, 0x30, 0, 0]);
        sprite(&mut vera, 0, 0x2000, true, (-4, 0), 0x0C, 0x01);
        assert_eq!(line(&mut vera, 0, [0, 1]), [0x111111, 0x661111]);
    }

    #[test]
    fn sprites_masks_that_meet_at_a_dot_are_a_collision_in_isr_at_vsync() {
        let mut vera = Vera::new();
        vera.write(DC_VIDEO, 0x41);
        vera.write(IEN, 0x04);
        poke(&mut vera, 0x2000, &[0x11; 32]);
        // Masks $10 and $30 at the same place, $40 apart.
        sprite(&mut vera, 0, 0x2000, false, (0, 0), 0x1C, 0x00);
        sprite(&mut vera, 1, 0x2000, false, (4, 4), 0x3C, 0x00);
        sprite(&mut vera, 2, 0x2000, false, (100, 0), 0x4C, 0x00);
        lines(&mut vera, HEIGHT as u32 - 1);
        assert_eq!(vera.read(ISR) & 0xF4, 0, "not till line 480");
        lines(&mut vera, 1);
        assert_eq!(vera.read(ISR) & 0xF4, 0x14, "the bit they share, SPRCOL");
        assert!(vera.irq());
        vera.write(ISR, 0xF4);
        assert_eq!(vera.read(ISR) & 0xF4, 0x10, "only SPRCOL's cleared");
        assert!(!vera.irq());
        sprite(&mut vera, 1, 0x2000, false, (8, 8), 0x3C, 0x00);
        lines(&mut vera, LINES as u32);
        assert_eq!(vera.read(ISR) & 0xF4, 0, "apart by the next frame");
    }

    #[test]
    fn the_sprite_renderer_draws_what_it_has_time_for_and_no_more() {
        let mut vera = Vera::new();
        vera.write(DC_VIDEO, 0x41);
        // Ten sprites of 64 by 8, 8 bits a dot, side by side across the
        // line, colours 1 to 10. Each one's 81 clocks: looked at, 16
        // fetches, 64 dots. The tenth has 70 left: 14 fetches and 55 dots.
        for n in 0..10 {
            let image = 0x4000 + n * 0x200;
            poke(&mut vera, image, &[n as u8 + 1; 64]);
            sprite(&mut vera, n, image, true, (n as i32 * 64, 0), 0x0C, 0x30);
        }
        lines(&mut vera, LINES as u32 * 2);
        assert_eq!(vera.picture[575], 0x664400, "the ninth's last, colour 9");
        assert_eq!(vera.picture[576 + 54], 0xFF7777, "the tenth's 55th dot");
        assert_eq!(vera.picture[576 + 55], 0, "and no more");
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
