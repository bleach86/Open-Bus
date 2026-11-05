//! A NES's region: how long its frames are, its dots a CPU cycle, what's
//! said of a game's region and by whom; its colours; a state kept to it.

#[cfg(test)]
mod tests {
    use crate::cpu::bus::Bus;
    use crate::host::machine::Machine;
    use crate::machines::nes::cartridge::Cartridge;
    use crate::machines::nes::palette::palette;
    use crate::machines::nes::ppu::Ppu;
    use crate::machines::nes::region::{PAL_CLOCK_HZ, Region};
    use crate::machines::nes::{CLOCK_HZ, Nes};

    /// An iNES image of NOPs, its header's bytes `header` set.
    fn image(header: &[(usize, u8)]) -> Vec<u8> {
        let mut image = b"NES\x1A\x01\x01".to_vec();
        image.extend([0; 10]);
        for &(at, byte) in header {
            image[at] = byte;
        }
        image.extend([0xEA; 0x4000 + 0x2000]);
        image
    }

    /// A NES of `region`, rendering or not, past its warming up: the cycles
    /// `frames` frames take.
    fn cycles(region: Region, rendering: bool, frames: u64) -> u64 {
        let mut nes = Nes::of(Cartridge::new(&image(&[])).unwrap(), region);
        for _ in 0..3 {
            while !nes.hand_back() {
                nes.tick();
            }
        }
        nes.write(0x2001, if rendering { 0x18 } else { 0 });
        let start = nes.cycles;
        for _ in 0..frames {
            while !nes.hand_back() {
                nes.tick();
            }
        }
        nes.cycles - start
    }

    #[test]
    fn a_pal_frame_is_longer_and_never_a_dot_short() {
        // 341 dots by 262 lines, at 3 a cycle, every other frame a dot short.
        assert_eq!(cycles(Region::Ntsc, false, 300), 341 * 262 * 300 / 3);
        assert_eq!(cycles(Region::Ntsc, true, 300), (341 * 262 * 300 - 150) / 3);
        // 341 by 312, at 3.2: 33,247.5 cycles each, rendering or not.
        assert_eq!(cycles(Region::Pal, false, 300), 341 * 312 * 300 * 5 / 16);
        assert_eq!(cycles(Region::Pal, true, 300), 341 * 312 * 300 * 5 / 16);
    }

    #[test]
    fn a_pals_16_dots_fall_in_5_cycles() {
        let dots: Vec<u64> = (1..=5).map(|cycle| Region::Pal.dots(cycle)).collect();
        assert_eq!(dots, [3, 3, 3, 3, 4]);
        for cycle in 1..=1000 {
            assert_eq!(Region::Ntsc.dots(cycle), 3);
        }
        assert_eq!((1..=5000).map(|cycle| Region::Pal.dots(cycle)).sum::<u64>(), 16000);
    }

    #[test]
    fn they_run_at_their_own_speeds() {
        assert_eq!((Region::Ntsc.clock_hz(), Region::Pal.clock_hz()), (CLOCK_HZ, PAL_CLOCK_HZ));
        assert!((Region::Ntsc.frames_per_second() - 60.0988).abs() < 0.001, "{}", Region::Ntsc.frames_per_second());
        assert!((Region::Pal.frames_per_second() - 50.007).abs() < 0.001, "{}", Region::Pal.frames_per_second());
    }

    /// The line the vertical blank starts on, and the frame's last.
    fn lines(region: Region) -> (u16, u16) {
        let mut nes = Nes::of(Cartridge::new(&image(&[])).unwrap(), region);
        let mut blank = None;
        let mut last = 0;
        for _ in 0..2 * 400_000 {
            nes.tick();
            last = last.max(nes.ppu.line);
            if blank.is_none() && nes.hand_back() {
                blank = Some(nes.ppu.line);
            }
        }
        (blank.unwrap(), last)
    }

    #[test]
    fn its_blank_starts_where_ntscs_does_and_goes_on_to_line_311() {
        assert_eq!(lines(Region::Ntsc), (241, 261));
        assert_eq!(lines(Region::Pal), (241, 311));
    }

    /// What's in v by dot 260 of a line, rendering, $0040 written to PPUADDR
    /// as the PPU comes to `dot`.
    fn v_after_writing_at(dot: u16) -> u16 {
        let mut cart = Cartridge::new(&image(&[])).unwrap();
        let mut ppu = Ppu::new(Region::Ntsc);
        // Past the writes being ignored, and onto a line of the next frame.
        while !(ppu.line == 100 && ppu.dot == 0) || !ppu.frame_ready {
            ppu.tick(&mut cart);
        }
        ppu.write_register(0x2001, 0x18, &mut cart);
        while ppu.dot != dot {
            ppu.tick(&mut cart);
        }
        ppu.write_register(0x2006, 0x00, &mut cart);
        ppu.write_register(0x2006, 0x40, &mut cart);
        while ppu.dot != 260 {
            ppu.tick(&mut cart);
        }
        ppu.v
    }

    #[test]
    fn a_write_of_ppuaddr_reaches_v_a_little_later() {
        // Past dot 256 by then: the line's not moved on over it (and dot
        // 257 has X back from t, which is the same).
        assert_eq!(v_after_writing_at(256), 0x0040);
        assert_eq!(v_after_writing_at(258), 0x0040);
        // There by 256: fine Y moved on a line.
        assert_eq!(v_after_writing_at(250), 0x1040);
    }

    #[test]
    fn a_files_name_says_its_region() {
        for name in ["Aladdin (Europe).zip", "Elite (Europe) (En,Fr,De).zip", "Game (Australia).nes", "Spiel (Germany).zip", "Game (E).nes", "Game (PAL).nes", "Rod Land (Europe) (Rev A).zip"] {
            assert_eq!(Region::from_name(name), Some(Region::Pal), "{name}");
        }
        for name in ["Zelda (USA).zip", "Tetris (USA, Europe).zip", "Game (Japan, Europe).zip", "Game (World).zip", "Game (U).nes"] {
            assert_eq!(Region::from_name(name), Some(Region::Ntsc), "{name}");
        }
        for name in ["Homebrew.nes", "Game (Unl).zip", "Game (En,Fr).nes", "Bible Adventures (v1.4).zip"] {
            assert_eq!(Region::from_name(name), None, "{name}");
        }
    }

    #[test]
    fn a_header_can_say_its_region() {
        let region = |header: &[(usize, u8)]| Cartridge::new(&image(header)).unwrap().region;
        assert_eq!(region(&[]), None, "silent");
        assert_eq!(region(&[(9, 1)]), Some(Region::Pal), "flags 9");
        assert_eq!(region(&[(10, 2)]), Some(Region::Pal), "flags 10");
        assert_eq!(region(&[(9, 1), (15, b'!')]), None, "a ripper's junk");
        for (byte, said) in [(0, Some(Region::Ntsc)), (1, Some(Region::Pal)), (2, None), (3, None)] {
            assert_eq!(region(&[(7, 0x08), (12, byte)]), said, "NES 2.0, {byte}");
        }
    }

    #[test]
    fn pals_emphasis_has_red_and_green_the_other_way_round() {
        let channels = |rgb: u32| ((rgb >> 16) & 255, (rgb >> 8) & 255, rgb & 255);
        let (white, red_bit, green_bit) = (0x30, 1 << 6, 2 << 6);
        let ntsc = palette(Region::Ntsc);
        let pal = palette(Region::Pal);
        let (r, g, _) = channels(ntsc[white | red_bit]);
        assert!(r > g, "NTSC's bit 5 not red: {r} {g}");
        let (r, g, _) = channels(pal[white | red_bit]);
        assert!(g > r, "PAL's bit 5 not green: {r} {g}");
        let (r, g, _) = channels(pal[white | green_bit]);
        assert!(r > g, "PAL's bit 6 not red: {r} {g}");
    }

    #[test]
    fn its_colours_are_half_a_hue_on_and_its_greys_the_same() {
        let (ntsc, pal) = (palette(Region::Ntsc), palette(Region::Pal));
        for grey in [0x00, 0x10, 0x20, 0x30, 0x0F] {
            assert_eq!(ntsc[grey], pal[grey], "grey {grey:02X}");
        }
        assert_ne!(ntsc[0x16], pal[0x16], "a colour the same");
    }

    #[test]
    fn a_state_keeps_to_its_region() {
        let nes = |region| Nes::of(Cartridge::new(&image(&[])).unwrap(), region);
        let (ntsc, pal) = (nes(Region::Ntsc).identity(), nes(Region::Pal).identity());
        assert!(ntsc.starts_with("NES, ROMs ") && ntsc.len() == "NES, ROMs 12345678".len(), "an NTSC game's old states refused: {ntsc}");
        assert_eq!(pal, format!("{ntsc}, PAL"));
    }
}
