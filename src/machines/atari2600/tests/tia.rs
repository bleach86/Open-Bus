//! The TIA, driven a color clock at a time: where RESP0 puts a player, and
//! a missile and the ball a clock sooner; the playfield's bits and its
//! reflection; what's over what; the collisions, and CXCLR; GRP0's write a
//! clock late, and VDELP0; HMOVE's movement and its comb; VBLANK's black;
//! the frame from VSYNC; and the fire buttons, latched.

#[cfg(test)]
mod tests {
    use crate::machines::atari2600::tia::{LINE, Tia, WIDTH};
    use crate::machines::region::Region;

    const COLUP0: u16 = 0x06;
    const COLUPF: u16 = 0x08;
    const COLUBK: u16 = 0x09;
    const CTRLPF: u16 = 0x0A;
    const PF1: u16 = 0x0E;
    const RESP0: u16 = 0x10;
    const RESM0: u16 = 0x12;
    const RESBL: u16 = 0x14;
    const GRP0: u16 = 0x1B;
    const GRP1: u16 = 0x1C;
    const ENAM0: u16 = 0x1D;
    const ENABL: u16 = 0x1F;
    const HMP0: u16 = 0x20;
    const VDELP0: u16 = 0x25;
    const HMOVE: u16 = 0x2A;
    const CXCLR: u16 = 0x2C;
    const CXP0FB: u16 = 0x02;
    const VSYNC: u16 = 0x00;
    const VBLANK: u16 = 0x01;

    /// The TIA at the top of a frame's picture, a VSYNC of 3 lines and the
    /// 23 after it gone by.
    fn at_picture() -> Tia {
        let mut tia = Tia::new(Region::Ntsc);
        tia.write(VSYNC, 0x02);
        lines(&mut tia, 3);
        tia.write(VSYNC, 0x00);
        lines(&mut tia, 23);
        assert!(tia.frame.picture());
        tia.frame_ready = false; // the frame before's
        tia
    }

    fn lines(tia: &mut Tia, lines: usize) {
        for _ in 0..lines * LINE as usize {
            tia.clock();
        }
    }

    /// On to color clock `hctr` of this line, or the next's if it's past.
    fn to(tia: &mut Tia, hctr: u8) {
        while tia.hctr != hctr {
            tia.clock();
        }
    }

    /// The frame ended by a VSYNC, and its picture's line `y`.
    fn picture_line(tia: &mut Tia, y: usize) -> Vec<u8> {
        to(tia, 0);
        tia.write(VSYNC, 0x02);
        lines(tia, 3);
        tia.write(VSYNC, 0x00);
        assert!(tia.frame_ready);
        tia.picture[y * WIDTH..(y + 1) * WIDTH].to_vec()
    }

    /// The dots of `line` that are `color`.
    fn dots(line: &[u8], color: u8) -> Vec<usize> {
        (0..WIDTH).filter(|&x| line[x] == color).collect()
    }

    #[test]
    fn resp0_puts_a_player_5_dots_on_and_a_missile_and_the_ball_4() {
        let mut tia = at_picture();
        tia.write(COLUP0, 0x1E);
        tia.write(COLUPF, 0x44);
        // Strobed at color clock 68 + 40, picture dot 40.
        to(&mut tia, 68 + 40);
        tia.write(RESP0, 0);
        to(&mut tia, 68 + 100);
        tia.write(RESM0, 0);
        to(&mut tia, 68 + 120);
        tia.write(RESBL, 0);
        tia.write(GRP0, 0x80);
        tia.write(ENAM0, 0x02);
        tia.write(ENABL, 0x02);
        lines(&mut tia, 2);
        let line = picture_line(&mut tia, 1);
        assert_eq!(dots(&line, 0x1E), [45, 104], "the player's, the missile's");
        assert_eq!(dots(&line, 0x44), [124], "the ball's");
    }

    #[test]
    fn the_playfields_bits_and_its_reflection() {
        let mut tia = at_picture();
        tia.write(COLUPF, 0x44);
        tia.write(PF1, 0x81);
        lines(&mut tia, 1);
        let line = picture_line(&mut tia, 0);
        // PF1's bit 7 at 16, bit 0 at 44; the right half the same again.
        assert_eq!(
            dots(&line, 0x44),
            [
                16, 17, 18, 19, 44, 45, 46, 47, 96, 97, 98, 99, 124, 125, 126, 127
            ]
        );
        let mut tia = at_picture();
        tia.write(COLUPF, 0x44);
        tia.write(CTRLPF, 0x01);
        tia.write(PF1, 0x80);
        lines(&mut tia, 1);
        let line = picture_line(&mut tia, 0);
        assert_eq!(
            dots(&line, 0x44),
            [16, 17, 18, 19, 140, 141, 142, 143],
            "reflected"
        );
    }

    #[test]
    fn players_over_the_playfield_but_not_with_its_priority_and_they_collide() {
        let mut tia = at_picture();
        tia.write(COLUP0, 0x1E);
        tia.write(COLUPF, 0x44);
        tia.write(PF1, 0xFF);
        to(&mut tia, 68 + 15);
        tia.write(RESP0, 0); // at 20, over PF1's dots 16 to 47
        tia.write(GRP0, 0xFF);
        // Line 1 drawn, and line 2 to dot 150.
        lines(&mut tia, 2);
        to(&mut tia, 68 + 150);
        assert_eq!(
            tia.read(CXP0FB, 0) & 0x80,
            0x80,
            "the player's hit the playfield"
        );
        tia.write(CXCLR, 0);
        assert_eq!(tia.read(CXP0FB, 0) & 0x80, 0, "cleared");
        tia.write(CTRLPF, 0x04);
        lines(&mut tia, 1);
        let normal = picture_line(&mut tia.clone(), 1);
        let priority = picture_line(&mut tia, 3);
        assert!(dots(&normal, 0x1E).contains(&20));
        assert!(
            !dots(&priority, 0x1E).contains(&20),
            "the playfield over it"
        );
    }

    #[test]
    fn grp0_is_taken_a_clock_late_and_vdelp0_waits_for_grp1() {
        let mut tia = at_picture();
        tia.write(COLUP0, 0x1E);
        to(&mut tia, 68 + 15);
        tia.write(RESP0, 0); // at 20
        lines(&mut tia, 1);
        // Written as its first dot's drawn: not in time for it.
        to(&mut tia, 68 + 20);
        tia.write(GRP0, 0xFF);
        lines(&mut tia, 1);
        let late = picture_line(&mut tia.clone(), 1);
        assert_eq!(dots(&late, 0x1E), [21, 22, 23, 24, 25, 26, 27]);
        // VDELP0: the old GRP0, until GRP1's written.
        tia.write(VDELP0, 0x01);
        lines(&mut tia, 1);
        let delayed = picture_line(&mut tia.clone(), 2);
        assert!(dots(&delayed, 0x1E).is_empty(), "the old GRP0, 0");
        tia.write(GRP1, 0);
        lines(&mut tia, 1);
        let shown = picture_line(&mut tia, 4);
        assert_eq!(dots(&shown, 0x1E).len(), 8);
    }

    #[test]
    fn hmove_moves_a_player_and_blacks_out_the_lines_first_8_dots() {
        let mut tia = at_picture();
        tia.write(COLUP0, 0x1E);
        tia.write(COLUBK, 0x82);
        to(&mut tia, 68 + 15);
        tia.write(RESP0, 0); // at 20
        tia.write(GRP0, 0x80);
        tia.write(HMP0, 0x30); // 3 to the left
        lines(&mut tia, 1);
        to(&mut tia, 3); // line 2's start, HMOVE there, as games do
        tia.write(HMOVE, 0);
        lines(&mut tia, 1);
        let line = picture_line(&mut tia, 2);
        assert_eq!(dots(&line, 0x1E), [17]);
        assert_eq!(&line[..8], &[0; 8], "the comb");
        assert_eq!(line[8], 0x82);
    }

    #[test]
    fn vblank_draws_black_and_vsync_ends_the_frame() {
        let mut tia = at_picture();
        tia.write(COLUBK, 0x82);
        lines(&mut tia, 1);
        tia.write(VBLANK, 0x02);
        lines(&mut tia, 1);
        let mut copy = tia.clone();
        assert!(!copy.frame_ready);
        let line = picture_line(&mut copy, 0);
        assert!(line.iter().all(|&dot| dot == 0x82));
        let line = picture_line(&mut tia, 1);
        assert!(line.iter().all(|&dot| dot == 0), "blanked");
    }

    #[test]
    fn a_fire_button_reads_0_pressed_and_latches_with_vblanks_bit_6() {
        let mut tia = at_picture();
        assert_eq!(tia.read(0x0C, 0) & 0x80, 0x80);
        tia.fire[0] = true;
        assert_eq!(tia.read(0x0C, 0) & 0x80, 0);
        tia.fire[0] = false;
        assert_eq!(tia.read(0x0C, 0) & 0x80, 0x80, "let go");
        tia.write(VBLANK, 0x40);
        tia.fire[0] = true;
        tia.read(0x0C, 0);
        tia.fire[0] = false;
        assert_eq!(tia.read(0x0C, 0) & 0x80, 0, "latched");
        tia.write(VBLANK, 0x00);
        assert_eq!(
            tia.read(0x0C, 0) & 0x80,
            0x80,
            "till VBLANK's bit 6's cleared"
        );
    }
}
