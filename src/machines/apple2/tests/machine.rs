//! The Apple II: its memory map, keyboard, soft switches, text screen and
//! frames, on the bus; its picture's pixels, a line at a time; the CPU
//! running a ROM of the test's own on it, roms/apple2_test.s, and its Reset
//! key starting it over (`make -C roms` builds it; without it those tests
//! say so and pass). The Apple's own ROM isn't needed, or used.

#[cfg(test)]
mod tests {
    use crate::machines::apple2::disk2::Disk2;
    use crate::machines::apple2::language_card::LanguageCard;
    use crate::machines::apple2::video::{self, Monitor, Source, LORES, Switches, HEIGHT, PICTURE_WIDTH, WIDTH};
    use crate::machines::apple2::{Apple2, CLOCK_HZ, CYCLES_PER_FRAME, CYCLES_PER_LINE, VOLUME};
    use crate::cpu::bus::Bus;
    use crate::cpu::CPU;
    use crate::host::display::{Display, Joystick, Key};
    use std::cell::{Cell, RefCell};
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
    }

    #[test]
    fn the_scanner_reads_what_each_line_draws() {
        // Each column of each line seen, in its cycle (from 25, after the
        // 25 blanked): what the line is drawn from.
        for page2 in [false, true] {
            let text = Switches { page2, ..Switches::default() };
            let hires = Switches { graphics: true, hires: true, page2, ..Switches::default() };
            let mixed = Switches { mixed: true, ..hires };
            for y in 0..HEIGHT {
                for x in 0..40 {
                    let (row, line) = (video::text_address(page2, y / 8) + x, video::hires_address(page2, y) + x);
                    assert_eq!(video::scanner_address(text, y, 25 + x), row, "text, page2 {page2}, line {y}, column {x}");
                    assert_eq!(video::scanner_address(hires, y, 25 + x), line, "high-res, page2 {page2}, line {y}, column {x}");
                    let mixed_want = if y < 160 { line } else { row };
                    assert_eq!(video::scanner_address(mixed, y, 25 + x), mixed_want, "mixed, page2 {page2}, line {y}, column {x}");
                }
            }
        }
        // Blanked, text is read $1000 up on the II+: line 0's 25 cycles,
        // the counter's preset then $40 to $57, $1468 to $147F, the 24
        // before row 0's in the interleave. High-res isn't.
        let text = Switches::default();
        let blanked: Vec<usize> = (0..25).map(|cycle| video::scanner_address(text, 0, cycle)).collect();
        let want: Vec<usize> = [0x1468].into_iter().chain(0x1468..=0x147F).collect();
        assert_eq!(blanked, want, "line 0, blanked");
        let hires = Switches { graphics: true, hires: true, ..Switches::default() };
        assert_eq!(video::scanner_address(hires, 0, 1), 0x2068, "high-res, line 0, blanked");
    }

    #[test]
    fn an_address_nothing_answers_reads_the_floating_bus() {
        let mut apple = machine();
        apple.disk2 = Some(Disk2::new(vec![0; 256], &[0; 256]).unwrap());
        for (addr, byte) in apple.ram.iter_mut().enumerate() {
            *byte = (addr ^ addr >> 8 ^ 0x5A) as u8;
        }
        let ram = apple.ram.clone();
        // Ticked to line 0's column 0, cycle 25, and a tick more: the bus
        // is ticked before a read, as the CPU does it.
        for _ in 0..26 {
            apple.tick();
        }
        for addr in [0xC010, 0xC020, 0xC030, 0xC040, 0xC070, 0xC080, 0xC0E1, 0xC0F0, 0xC100, 0xC7FF, 0xCFFF] {
            assert_eq!(apple.read(addr), ram[0x0400], "${addr:04X}: not the floating bus, $0400's");
        }
        assert_eq!(apple.read(0xC061), ram[0x0400] & 0x7F, "the button up: bit 7 clear, the rest floating");
        assert_eq!(apple.read(0xC0EC), 0, "the Disk II's data register, at an even address");
        assert_eq!(apple.read(0xC000) & 0x7F, 0, "the keyboard, its own");
        apple.tick();
        assert_eq!(apple.read(0xC0F0), ram[0x0401], "column 1: $0401's");
        for _ in 0..39 {
            apple.tick();
        }
        assert_eq!(apple.read(0xC0F0), ram[0x1468], "line 1, blanked: $1468's");
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

    /// A window of the test's: its Reset key held as the test says.
    struct ResetKey(Rc<Cell<bool>>);

    impl Display for ResetKey {
        fn show(&mut self, _: &[u32], _: usize, _: usize) {}

        fn key(&mut self) -> Option<Key> {
            None
        }

        fn reset_held(&self) -> bool {
            self.0.get()
        }
    }

    #[test]
    fn the_reset_key_holds_the_cpu_and_cards_then_starts_over() {
        let roms = Path::new(env!("CARGO_MANIFEST_DIR")).join("roms");
        let Ok(rom) = std::fs::read(roms.join("apple2_test.bin")) else {
            eprintln!("apple2 test: no program; run make -C roms");
            return;
        };
        let mut apple = Apple2::new(rom);
        let held = Rc::new(Cell::new(false));
        apple.display = Some(Box::new(ResetKey(held.clone())));
        apple.language_card = Some(LanguageCard::new());
        apple.disk2 = Some(Disk2::new(vec![0; 256], &[0; 256]).unwrap());
        let mut cpu = CPU::with_bus(apple);
        cpu.clock_hz = u32::MAX; // as fast as it goes
        cpu.reset();
        cpu.execute(2_000); // HELLO, then waiting for a key
        cpu.memory.read(0xC08A); // the card: bank 1, write protected
        cpu.memory.read(0xC0E9); // the drive's motor on
        cpu.memory.write(0x0400, 0xA0); // HELLO's H rubbed out

        // Held: seen as the frame ends; then nothing runs.
        held.set(true);
        cpu.execute(CYCLES_PER_FRAME);
        let pc = cpu.pc;
        cpu.execute(CYCLES_PER_FRAME);
        assert_eq!(cpu.pc, pc, "the CPU ran, held");
        let card = cpu.memory.language_card.as_ref().unwrap();
        assert!(!card.read_ram && card.write_enabled && card.bank2, "the language card not reset");
        let drive = cpu.memory.disk2.as_ref().unwrap();
        assert!(!drive.motor && drive.spinning(), "the motor not switched off, or not running on");
        assert_eq!(&cpu.memory.text()[0][..5], " ELLO");

        // Let go: from the reset vector, HELLO again.
        held.set(false);
        cpu.execute(CYCLES_PER_FRAME + 2_000);
        assert_eq!(&cpu.memory.text()[0][..5], "HELLO", "not started over");
    }

    /// Line `y` of the picture, drawn from `ram` by `switches`.
    fn picture_line(ram: &[u8], switches: Switches, y: usize) -> Vec<u32> {
        let mut out = vec![0x123456; WIDTH];
        video::line(&Source { main: ram, aux: &[], characters: None }, switches, false, y, &mut out);
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
            assert_eq!((width, height), (PICTURE_WIDTH, HEIGHT));
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
        // Each of a 40 column mode's dots two of the picture's.
        assert!(frames[0].chunks(2).all(|pair| pair[0] == pair[1]), "a dot not shown twice");
        let at = |x: usize, y: usize| frames[0][y * PICTURE_WIDTH + 2 * x];
        assert_eq!(at(0, 0), WHITE, "line 0: text, the inverse space");
        assert_eq!(at(0, 99), BLACK, "line 99: text, a space");
        assert_eq!(at(0, 128), LORES[0x0], "line 128: low-res, row 16's top: black");
        assert_eq!(at(0, 132), LORES[0x2], "line 132: row 16's bottom: the $20's dark blue");
        assert_eq!(at(7, 132), LORES[0xA], "line 132: the next block's, $A0's, grey");
    }

    /// A window of the test's with a joystick held as it says.
    struct Stick(Joystick);

    impl Display for Stick {
        fn show(&mut self, _: &[u32], _: usize, _: usize) {}

        fn key(&mut self) -> Option<Key> {
            None
        }

        fn joystick(&self) -> Joystick {
            self.0
        }
    }

    #[test]
    fn the_paddles_time_the_sticks_position() {
        let mut apple = machine();
        // Left and down: paddle 0 at 0, paddle 1 at 255.
        let stick = Joystick { x: -1.0, y: 1.0, buttons: [false; 3] };
        apple.display = Some(Box::new(Stick(stick)));
        apple.read(0xC070);
        assert_eq!(apple.read(0xC064) & 0x80, 0, "paddle 0, at 0: still counting");
        for _ in 0..255 * 11 - 1 {
            apple.tick();
        }
        assert_eq!(apple.read(0xC065) & 0x80, 0x80, "paddle 1, at 255: out before 255 steps");
        apple.tick();
        assert_eq!(apple.read(0xC065) & 0x80, 0, "paddle 1: still counting after 255 steps");

        // Centred: 128, started again by a write.
        apple.display = Some(Box::new(Stick(Joystick::CENTRED)));
        apple.write(0xC070, 0);
        for _ in 0..128 * 11 - 1 {
            apple.tick();
        }
        assert_eq!(apple.read(0xC064) & 0x80, 0x80, "centred: out before 128 steps");
        apple.tick();
        assert_eq!(apple.read(0xC064) & 0x80, 0, "centred: still counting after 128 steps");
    }

    #[test]
    fn the_buttons_are_bit_7() {
        let mut apple = machine();
        assert_eq!(apple.read(0xC061) & 0x80, 0, "a button without a joystick");
        let stick = Joystick { buttons: [true, false, true], ..Joystick::CENTRED };
        apple.display = Some(Box::new(Stick(stick)));
        assert_eq!(apple.read(0xC061) & 0x80, 0x80, "button 0");
        assert_eq!(apple.read(0xC062) & 0x80, 0, "button 1");
        assert_eq!(apple.read(0xC063) & 0x80, 0x80, "button 2");
    }

    /// A window of the test's: the sound it's given, kept.
    struct Heard(Rc<RefCell<Vec<f32>>>);

    impl Display for Heard {
        fn show(&mut self, _: &[u32], _: usize, _: usize) {}

        fn key(&mut self) -> Option<Key> {
            None
        }

        fn play(&mut self, samples: &[f32]) {
            self.0.borrow_mut().extend_from_slice(samples);
        }
    }

    /// 60 frames of the machine, $C030 read every `flip` cycles (0, never):
    /// the sound it played.
    fn sound(flip: u32) -> Vec<f32> {
        let mut apple = machine();
        let heard = Rc::new(RefCell::new(Vec::new()));
        apple.display = Some(Box::new(Heard(heard.clone())));
        for cycle in 1..=60 * CYCLES_PER_FRAME {
            apple.tick();
            if flip != 0 && cycle % flip == 0 {
                apple.read(0xC030);
            }
        }
        // In stereo, a left sample then a right: the speaker in both.
        let (left, right): (Vec<f32>, Vec<f32>) = heard.take().chunks(2).map(|frame| (frame[0], frame[1])).unzip();
        assert_eq!(left, right, "the speaker not the same in both channels");
        left
    }

    #[test]
    fn the_sound_is_48000_samples_a_second_given_a_frame_at_a_time() {
        let samples = sound(0).len() as u64;
        let due = 60 * CYCLES_PER_FRAME as u64 * 48_000 / CLOCK_HZ as u64;
        assert_eq!(samples, due, "samples for 60 frames");
    }

    #[test]
    fn the_speaker_flips_at_c030() {
        // Every 1,023 cycles: a square wave of 500 Hz, its level crossing
        // the middle a thousand times a second.
        let samples = sound(1023);
        let crossings = samples.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
        let seconds = 60.0 * CYCLES_PER_FRAME as f32 / CLOCK_HZ as f32;
        let per_second = crossings as f32 / seconds;
        assert!((990.0..=1010.0).contains(&per_second), "{per_second} crossings a second");
        // Its swing nearly the speaker's volume either way, the settling
        // taking a little of it in a half wave.
        let loud = VOLUME * 2.0 / 3.0;
        assert!(samples.iter().any(|&s| s > loud) && samples.iter().any(|&s| s < -loud), "too quiet");
    }

    #[test]
    fn the_speaker_left_still_settles_to_silence() {
        assert!(sound(0).iter().all(|&s| s == 0.0), "a sound without a flip");
        // Flipped once, at the start: a click, then nothing.
        let mut apple = machine();
        let heard = Rc::new(RefCell::new(Vec::new()));
        apple.display = Some(Box::new(Heard(heard.clone())));
        apple.read(0xC030);
        for _ in 0..10 * CYCLES_PER_FRAME {
            apple.tick();
        }
        let samples = heard.take();
        assert!(samples[0].abs() > VOLUME, "no click: the cone in to out, twice the volume");
        assert!(samples[samples.len() - 100..].iter().all(|s| s.abs() < 0.001), "still sounding");
    }

    /// Line `y` of the picture as shown on `monitor`, 560 dots.
    fn shown(ram: &[u8], switches: Switches, monitor: Monitor, y: usize) -> Vec<u32> {
        let mut out = vec![0x123456; PICTURE_WIDTH];
        video::picture_line(&Source { main: ram, aux: &[], characters: None }, switches, monitor, false, y, &mut out);
        out
    }

    #[test]
    fn a_monochrome_monitor_shows_the_signals_dots() {
        let green = 0x33FF33;
        let mut ram = vec![0; 0xC000];
        // High-res: each dot two of the signal's; a byte with its top bit
        // set a dot later, the dot before held over the gap.
        ram[0x2000] = 0x01; // dot 0: 0 and 1
        ram[0x2001] = 0x81; // shifted: 15 and 16, 14 held (13's, off)
        ram[0x2002] = 0x40; // dot 6: 40 and 41
        ram[0x2003] = 0x81; // shifted: 42 held (41's, on), 43 and 44
        let hires = Switches { graphics: true, hires: true, ..Switches::default() };
        let line = shown(&ram, hires, Monitor::Green, 0);
        let lit: Vec<usize> = (0..60).filter(|&x| line[x] == green).collect();
        assert_eq!(lit, [0, 1, 15, 16, 40, 41, 42, 43, 44], "high-res's dots lit");
        assert!(line.iter().all(|&dot| dot == green || dot == BLACK), "a colour on a monochrome monitor");

        // Low-res: a block its colour's bits, over and over, each dot's
        // place in the cycle its bit: 5, %0101, dots 0 and 2 of each 4.
        ram[0x0400] = 0x05;
        let lores = Switches { graphics: true, ..Switches::default() };
        let line = shown(&ram, lores, Monitor::Amber, 0);
        let lit: Vec<bool> = line[..14].iter().map(|&dot| dot == 0xFFB000).collect();
        assert_eq!(lit, [true, false, true, false, true, false, true, false, true, false, true, false, true, false]);

        // Text, in the phosphor; and colour as it was.
        ram[0x0400] = 0x20; // an inverse space: all lit
        let text = Switches::default();
        assert!(shown(&ram, text, Monitor::Green, 0)[..14].iter().all(|&dot| dot == green), "text not green");
        ram[0x2000] = 0x01; // a dot alone: violet, in colour
        assert_eq!(shown(&ram, hires, Monitor::Colour, 0)[0], LORES[3], "colour not as it was");
    }

    /// A window of the test's: function keys pressed, as the test says.
    struct FunctionKeys(Rc<RefCell<Vec<u8>>>);

    impl Display for FunctionKeys {
        fn show(&mut self, _: &[u32], _: usize, _: usize) {}

        fn key(&mut self) -> Option<Key> {
            None
        }

        fn function_key(&mut self) -> Option<u8> {
            let keys = &mut self.0.borrow_mut();
            (!keys.is_empty()).then(|| keys.remove(0))
        }
    }

    #[test]
    fn f8_switches_the_monitor() {
        let mut apple = machine();
        let keys = Rc::new(RefCell::new(Vec::new()));
        apple.display = Some(Box::new(FunctionKeys(keys.clone())));
        for want in [Monitor::White, Monitor::Green, Monitor::Amber, Monitor::Colour] {
            keys.borrow_mut().push(8);
            for _ in 0..CYCLES_PER_FRAME {
                apple.tick();
            }
            assert_eq!(apple.monitor, want, "F8");
            assert!(apple.picture[(HEIGHT - 12) * PICTURE_WIDTH..].iter().any(|&dot| dot == WHITE), "no notice of it");
        }
    }
}
