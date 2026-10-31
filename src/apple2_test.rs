//! The Apple II: its memory map, keyboard, soft switches, text screen and
//! frames, on the bus; its picture's pixels, a line at a time; and the CPU
//! running a ROM of the test's own on it, roms/apple2_test.s (`make -C
//! roms` builds it; without it that test says so and passes). The Apple's
//! own ROM isn't needed, or used.

#[cfg(test)]
mod tests {
    use crate::apple2::video::{self, LORES, Switches, HEIGHT, WIDTH};
    use crate::apple2::{Apple2, CLOCK_HZ, CYCLES_PER_FRAME, CYCLES_PER_LINE};
    use crate::bus::Bus;
    use crate::cpu::CPU;
    use crate::display::{Display, Key};
    use std::cell::RefCell;
    use std::path::Path;
    use std::rc::Rc;

    const BLACK: u32 = 0x000000;
    const WHITE: u32 = 0xFFFFFF;

    /// The machine with a ROM of NOPs, but its first byte $11 and last $22.
    fn machine() -> Apple2 {
        let mut rom = vec![0xEA; 0x3000];
        rom[0] = 0x11;
        rom[0x2FFF] = 0x22;
        Apple2::new(rom)
    }

    #[test]
    fn the_memory_map() {
        let mut apple = machine();
        for addr in [0x0000, 0x1234, 0xBFFF] {
            apple.write(addr, 0x5A);
            assert_eq!(apple.read(addr), 0x5A, "RAM at ${addr:04X}");
        }
        assert_eq!(apple.read(0xD000), 0x11, "ROM at $D000");
        assert_eq!(apple.read(0xFFFF), 0x22, "ROM at $FFFF");
        apple.write(0xD000, 0x00);
        assert_eq!(apple.read(0xD000), 0x11, "ROM written");
        assert_eq!(apple.read(0xC600), 0, "a card where there is none");
    }

    #[test]
    fn the_keyboard_holds_a_key_until_the_strobe_is_cleared() {
        let mut apple = machine();
        assert_eq!(apple.read(0xC000) & 0x80, 0, "a key before one is pressed");
        apple.press(b'A');
        assert_eq!(apple.read(0xC000), 0xC1, "the key, bit 7 set");
        assert_eq!(apple.read(0xC000), 0xC1, "the key, still");
        apple.read(0xC010);
        assert_eq!(apple.read(0xC000), 0x41, "the key, its strobe cleared");
        apple.press(b'B');
        apple.write(0xC010, 0);
        assert_eq!(apple.read(0xC000), 0x42, "the strobe cleared by a write");
    }

    #[test]
    fn the_soft_switches_are_read_or_written() {
        let mut apple = machine();
        apple.read(0xC050);
        apple.write(0xC053, 0);
        apple.read(0xC055);
        apple.read(0xC057);
        assert!(apple.graphics && apple.mixed && apple.page2 && apple.hires);
        apple.write(0xC051, 0);
        apple.read(0xC052);
        apple.read(0xC054);
        apple.write(0xC056, 0);
        assert!(!apple.graphics && !apple.mixed && !apple.page2 && !apple.hires);
    }

    #[test]
    fn the_text_screens_rows_are_interleaved() {
        let mut apple = machine();
        // Rows 0, 1, 8 and 23: $0400, $0480, $0428, $07D0.
        for (addr, c) in [(0x0400, b'A'), (0x0480, b'B'), (0x0428, b'C'), (0x07D0, b'D')] {
            apple.write(addr, c | 0x80);
        }
        apple.write(0x0401, 0x01); // an inverse A
        apple.write(0x0402, 0x60); // a flashing space
        let text = apple.text();
        assert_eq!(&text[0][..3], "AA ");
        assert_eq!(&text[1][..1], "B");
        assert_eq!(&text[8][..1], "C");
        assert_eq!(&text[23][..1], "D");
    }

    #[test]
    fn a_frame_is_17030_cycles() {
        let mut apple = machine();
        for _ in 0..3 * CYCLES_PER_FRAME - 1 {
            apple.tick();
        }
        assert_eq!(apple.frames, 2);
        apple.tick();
        assert_eq!(apple.frames, 3);
    }

    #[test]
    fn the_cpu_runs_a_program_on_it() {
        let roms = Path::new(env!("CARGO_MANIFEST_DIR")).join("roms");
        let Ok(rom) = std::fs::read(roms.join("apple2_test.bin")) else {
            eprintln!("apple2 test: no program; run make -C roms");
            return;
        };
        let mut cpu = CPU::with_bus(Apple2::new(rom));
        cpu.clock_hz = CLOCK_HZ;
        cpu.reset();
        cpu.execute(2_000); // HELLO, then waiting for a key
        assert_eq!(&cpu.memory.text()[0][..5], "HELLO");
        assert!(!cpu.memory.graphics, "graphics before the key");

        cpu.memory.press(b'X');
        cpu.execute(2_000);
        let text = cpu.memory.text();
        assert_eq!(&text[1][..1], "X", "the key on row 1");
        assert_eq!(&text[8][..1], "*", "row 8 marked");
        assert_eq!(cpu.memory.keyboard & 0x80, 0, "the key not taken");
        assert!(cpu.memory.graphics, "graphics not switched on");
        assert_eq!(cpu.memory.frames, 0, "a frame: 4,000 cycles are less");
    }

    /// Line `y` of the picture, drawn from `ram` by `switches`.
    fn picture_line(ram: &[u8], switches: Switches, y: usize) -> Vec<u32> {
        let mut out = vec![0x123456; WIDTH];
        video::line(ram, switches, false, y, &mut out);
        out
    }

    #[test]
    fn a_character_is_its_dots() {
        let mut ram = vec![0; 0xC000];
        ram[0x0400] = b'A' | 0x80; // A, normal
        ram[0x0401] = 0x01; // A, inverse
        let text = Switches::default();
        // A's top line, ..#.., is its third dot of five: the fourth of seven.
        let top = picture_line(&ram, text, 0);
        let dots: Vec<bool> = top[..14].iter().map(|&p| p == WHITE).collect();
        let a = [false, false, false, true, false, false, false];
        assert_eq!(&dots[..7], &a, "A's top line");
        assert_eq!(&dots[7..], &a.map(|on| !on), "inverse A's top line");
        // Its middle line, #####, and its 8th, blank.
        let middle = picture_line(&ram, text, 4);
        assert_eq!(&middle[..7], &[BLACK, WHITE, WHITE, WHITE, WHITE, WHITE, BLACK]);
        assert!(picture_line(&ram, text, 7)[..7].iter().all(|&p| p == BLACK));
    }

    #[test]
    fn a_low_res_block_is_two_colours() {
        let mut ram = vec![0; 0xC000];
        ram[0x0400] = 0xD2; // dark blue above, yellow below
        let lores = Switches { graphics: true, ..Switches::default() };
        assert!(picture_line(&ram, lores, 0)[..7].iter().all(|&p| p == LORES[2]));
        assert!(picture_line(&ram, lores, 3)[..7].iter().all(|&p| p == LORES[2]));
        assert!(picture_line(&ram, lores, 4)[..7].iter().all(|&p| p == LORES[13]));
        assert_eq!(picture_line(&ram, lores, 0)[7], LORES[0], "the next block, black");
    }

    #[test]
    fn high_res_dots_are_coloured_by_their_column() {
        let hires = Switches { graphics: true, hires: true, ..Switches::default() };
        let line = |byte: u8| {
            let mut ram = vec![0; 0xC000];
            ram[0x2000] = byte;
            picture_line(&ram, hires, 0)[..3].to_vec()
        };
        assert_eq!(line(0x01), [LORES[3], BLACK, BLACK], "a dot at 0: violet");
        assert_eq!(line(0x02), [BLACK, LORES[12], BLACK], "at 1: green");
        assert_eq!(line(0x81), [LORES[6], BLACK, BLACK], "at 0, shifted: blue");
        assert_eq!(line(0x82), [BLACK, LORES[9], BLACK], "at 1, shifted: orange");
        assert_eq!(line(0x03), [WHITE, WHITE, BLACK], "two side by side: white");
        assert_eq!(line(0x05), [LORES[3]; 3], "a gap between two at 0 and 2: filled");
    }

    #[test]
    fn high_res_lines_are_interleaved() {
        let mut ram = vec![0; 0xC000];
        // Lines 0, 1, 8, 64 and 191.
        for (n, y) in [0usize, 1, 8, 64, 191].iter().enumerate() {
            ram[video::hires_address(false, *y) + n] = 0x03;
        }
        let hires = Switches { graphics: true, hires: true, ..Switches::default() };
        for (n, y) in [0usize, 1, 8, 64, 191].iter().enumerate() {
            assert_eq!(picture_line(&ram, hires, *y)[n * 7], WHITE, "line {y}");
        }
        assert_eq!(video::hires_address(false, 191), 0x3FD0);
    }

    #[test]
    fn mixed_mode_is_text_from_line_160() {
        let mut ram = vec![0; 0xC000];
        ram[0x0400 + 39] = 0xFF; // low-res white, at the top's last block
        ram[video::text_address(false, 20)] = 0x20; // an inverse space, row 20
        let mixed = Switches { graphics: true, mixed: true, ..Switches::default() };
        assert_eq!(picture_line(&ram, mixed, 0)[WIDTH - 1], WHITE, "low-res above");
        assert_eq!(picture_line(&ram, mixed, 159)[0], BLACK, "low-res black at 159");
        assert_eq!(picture_line(&ram, mixed, 160)[0], WHITE, "text at 160: the inverse space");
    }

    /// A window of the test's: the frames shown, kept.
    struct Shown(Rc<RefCell<Vec<Vec<u32>>>>);

    impl Display for Shown {
        fn show(&mut self, pixels: &[u32], width: usize, height: usize) {
            assert_eq!((width, height), (WIDTH, HEIGHT));
            self.0.borrow_mut().push(pixels.to_vec());
        }

        fn key(&mut self) -> Option<Key> {
            None
        }
    }

    #[test]
    fn each_line_is_drawn_as_the_switches_are_then() {
        let mut apple = machine();
        let frames = Rc::new(RefCell::new(Vec::new()));
        apple.display = Some(Box::new(Shown(frames.clone())));
        for addr in 0x0400..0x0800 {
            apple.write(addr, 0xA0); // text: spaces
        }
        apple.write(0x0400, 0x20); // and an inverse one at the top
        apple.write(0x0450, 0x20); // and at row 16's start, $0450

        // Text for the first 100 lines, then low-res: its blocks $A0, black
        // above and grey below, but $20's, black above and dark blue below.
        for _ in 0..100 * CYCLES_PER_LINE {
            apple.tick();
        }
        apple.read(0xC050);
        for _ in 100 * CYCLES_PER_LINE..CYCLES_PER_FRAME - 1 {
            apple.tick();
        }
        assert!(frames.borrow().is_empty(), "shown before the frame's end");
        apple.tick();

        let frames = frames.borrow();
        assert_eq!(frames.len(), 1, "a frame shown");
        let at = |x: usize, y: usize| frames[0][y * WIDTH + x];
        assert_eq!(at(0, 0), WHITE, "line 0: text, the inverse space");
        assert_eq!(at(0, 99), BLACK, "line 99: text, a space");
        assert_eq!(at(0, 128), LORES[0x0], "line 128: low-res, row 16's top: black");
        assert_eq!(at(0, 132), LORES[0x2], "line 132: row 16's bottom: the $20's dark blue");
        assert_eq!(at(7, 132), LORES[0xA], "line 132: the next block's, $A0's, grey");
    }
}
