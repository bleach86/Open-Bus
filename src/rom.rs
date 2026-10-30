//! The system ROM (BIOS and system software), built from `rom/` by
//! `build.rs`.

use crate::cart;

pub const SYSTEM: &[u8; cart::ROM_SIZE] = include_bytes!(concat!(env!("OUT_DIR"), "/system.bin"));

/// The whole machine running its ROM, with the example carts on its drive:
/// the BIOS, the menu, the shell and carts built with the SDK.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ram::{Host, Memory};
    use crate::render::{Frame, FrameSlot};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, OnceLock};
    use std::time::{Duration, Instant};

    const COLUMNS: usize = 40;
    const ROWS: usize = 30;
    const UP: u8 = 0x80;
    const DOWN: u8 = 0x81;
    const LEFT: u8 = 0x82;
    const RIGHT: u8 = 0x83;
    const ESCAPE: u8 = 0x1B;
    const BACKSPACE: u8 = 0x08;
    const TAB: u8 = 0x09;
    const EXAMPLES: [&str; 3] = ["banks", "sprout", "test"];

    /// The example carts, built by their Makefiles with the SDK once per test
    /// run, as anyone's cart is.
    fn carts() -> PathBuf {
        static CARTS: OnceLock<PathBuf> = OnceLock::new();
        CARTS
            .get_or_init(|| {
                let out = Path::new(env!("OUT_DIR"));
                let drive = out.join("carts");
                let _ = std::fs::remove_dir_all(&drive);
                for example in EXAMPLES {
                    let status = Command::new("make")
                        .arg("-s")
                        .arg("-C")
                        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join(example))
                        .arg(format!("BUILD={}", out.join("examples").join(example).display()))
                        .arg(format!("OUT={}", drive.display()))
                        .status()
                        .expect("failed to run make");
                    assert!(status.success(), "building {example} failed");
                }
                drive
            })
            .clone()
    }

    /// A drive of its own for each machine, with the example carts on it:
    /// what one machine saves, another doesn't see.
    fn drive() -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let drive = Path::new(env!("OUT_DIR")).join("drives").join(NEXT.fetch_add(1, Ordering::Relaxed).to_string());
        let _ = std::fs::remove_dir_all(&drive);
        std::fs::create_dir_all(&drive).unwrap();
        for cart in std::fs::read_dir(carts()).unwrap() {
            let cart = cart.unwrap().path();
            std::fs::copy(&cart, drive.join(cart.file_name().unwrap())).unwrap();
        }
        drive
    }

    struct Machine {
        host_input: Arc<crate::input::Input>,
        frames: Arc<FrameSlot>,
        drive: PathBuf,
    }

    impl Machine {
        /// Power on: the menu.
        fn start(launcher: bool) -> Machine {
            Machine::start_with(launcher, None)
        }

        fn start_with(launcher: bool, monitor: Option<Arc<crate::monitor::Link>>) -> Machine {
            let drive = drive();
            let host = Host {
                display: Some(Arc::new(FrameSlot::default())),
                drive: Some(drive.clone()),
                launcher,
                monitor,
                ..Host::default()
            };
            let host_input = Arc::clone(&host.input);
            let frames = Arc::clone(host.display.as_ref().unwrap());
            std::thread::spawn(move || {
                let mut cpu = crate::cpu::CPU::new();
                cpu.memory = Memory::new(host);
                // As main() does: power on again after every reset.
                loop {
                    cpu.power_on(SYSTEM);
                    if !cpu.execute(u32::MAX) {
                        break;
                    }
                }
            });
            Machine { host_input, frames, drive }
        }

        /// Power on and go to the shell, the system page's first entry: the
        /// machine, and the shell's first screen.
        fn boot_screen() -> (Machine, Screen) {
            let machine = Machine::start(false);
            machine.until(|screen| screen.row(2).contains("CARTS"));
            machine.type_keys(&[TAB, b'\r']);
            let screen = machine.until(|screen| screen.waiting_at(3));
            (machine, screen)
        }

        fn boot() -> Machine {
            Machine::boot_screen().0
        }

        fn type_keys(&self, keys: &[u8]) {
            for &key in keys {
                self.host_input.press_key(key);
            }
        }

        /// Hold `buttons` down on the pad, until `release`.
        fn hold(&self, buttons: u16) {
            self.host_input.pad.store(buttons, Ordering::Relaxed);
        }

        fn release(&self) {
            self.host_input.pad.store(0, Ordering::Relaxed);
        }

        fn reset(&self) {
            self.host_input.reset.store(true, Ordering::Release);
        }

        /// The first frame for which `done` holds.
        fn until(&self, done: impl Fn(&Screen) -> bool) -> Screen {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut last = None;
            while Instant::now() < deadline {
                match self.frames.take() {
                    Some(frame) => {
                        let screen = Screen(frame);
                        if done(&screen) {
                            return screen;
                        }
                        last = Some(screen);
                    }
                    None => std::thread::sleep(Duration::from_millis(1)),
                }
            }
            panic!("condition never held; last screen:\n{}", last.expect("no frames").text());
        }

        /// Type `keys` and wait for an empty prompt again.
        fn run(&self, keys: &[u8]) -> Screen {
            self.type_keys(keys);
            self.until(|screen| screen.idle() && !self.host_input.keys_waiting())
        }
    }

    /// A frame, read as the BIOS's text screen: the map under the scroll.
    struct Screen(Frame);

    impl Screen {
        fn entry(&self, column: usize, row: usize) -> u16 {
            let map_row = (self.0.scroll(0).1 as usize / 8 + row) % 32;
            let at = 0x8000 + (map_row * 64 + column) * 2;
            u16::from_le_bytes([self.0.vram[at], self.0.vram[at + 1]])
        }

        /// The pixels of tile `tile`, 64 colour numbers.
        fn tile(&self, tile: usize) -> [u8; 64] {
            let bytes = &self.0.vram[tile * 32..tile * 32 + 32];
            std::array::from_fn(|i| bytes[i / 2] >> (4 - i % 2 * 4) & 15)
        }

        /// What shows in the `columns` x `rows` cells from (column, row), as
        /// 12-bit colours, None where nothing does: the map's tiles in their
        /// palettes, as the video chip draws them. (Flips aside.)
        fn picture(&self, column: usize, row: usize, columns: usize, rows: usize) -> Vec<Option<u16>> {
            let mut picture = Vec::new();
            for y in 0..rows * 8 {
                for x in 0..columns * 8 {
                    let entry = self.entry(column + x / 8, row + y / 8) as usize;
                    let index = self.tile(entry & 0x3FF)[y % 8 * 8 + x % 8] as usize;
                    let at = 0x9100 + (entry >> 12) * 32 + index * 2;
                    let colour = u16::from_le_bytes([self.0.vram[at], self.0.vram[at + 1]]) & 0xFFF;
                    picture.push((index != 0).then_some(colour));
                }
            }
            picture
        }

        /// A row as text. Text is any copy of the font in any colours, so a
        /// cell reads as the character whose shape its tile has: the BIOS's
        /// font, at tiles $20-$7E in colour 1, is the reference. A tile of
        /// one colour is a space; any other shape, '#'.
        fn row(&self, row: usize) -> String {
            let glyph = |pixels: [u8; 64], colour: u8| pixels.map(|p| p == colour);
            let font: Vec<([bool; 64], char)> =
                (0x20..0x7F).map(|c| (glyph(self.tile(c), 1), c as u8 as char)).collect();
            (0..COLUMNS)
                .map(|column| {
                    let pixels = self.tile(self.entry(column, row) as usize & 0x3FF);
                    if pixels.iter().all(|&p| p == pixels[0]) {
                        return ' ';
                    }
                    let shapes = pixels.iter().map(|&colour| glyph(pixels, colour));
                    shapes
                        .filter_map(|shape| font.iter().find(|(glyph, _)| *glyph == shape))
                        .map(|&(_, c)| c)
                        .next()
                        .unwrap_or('#')
                })
                .collect::<String>()
                .trim_end()
                .to_string()
        }

        fn rows(&self) -> Vec<String> {
            (0..ROWS).map(|row| self.row(row)).collect()
        }

        fn text(&self) -> String {
            self.rows().join("\n")
        }

        /// Where the cursor sprite is, as a cell, if it shows.
        fn cursor(&self) -> Option<(usize, usize)> {
            let sprite = &self.0.vram[0x9000 + 63 * 4..0x9000 + 64 * 4];
            let x = sprite[1] as usize | (sprite[3] as usize & 1) << 8;
            (sprite[0] < 240).then(|| (x / 8, sprite[0] as usize / 8))
        }

        /// Row `row` is an empty prompt with the cursor after it.
        fn waiting_at(&self, row: usize) -> bool {
            self.row(row) == ">" && self.cursor() == Some((2, row))
        }

        /// The last prompt is empty and waiting for a line.
        fn idle(&self) -> bool {
            self.rows()
                .iter()
                .rposition(|row| row.starts_with('>'))
                .is_some_and(|row| self.waiting_at(row))
        }

        /// Rows from the one after `> command`, up to the next prompt.
        fn output_of(&self, command: &str) -> Vec<String> {
            let rows = self.rows();
            let start = rows
                .iter()
                .rposition(|row| row == &format!("> {command}"))
                .unwrap_or_else(|| panic!("no '> {command}' on screen:\n{}", self.text()));
            rows[start + 1..].iter().take_while(|row| !row.starts_with('>')).cloned().collect()
        }
    }

    #[test]
    fn the_menu_lists_the_carts_on_the_drive() {
        let machine = Machine::start(false);
        let screen = machine.until(|screen| screen.row(25).contains("cart"));
        assert!(screen.row(0).contains("6502"), "the status bar");
        assert!(screen.row(2).contains("CARTS") && screen.row(2).contains("SYSTEM"));
        // Each cart's name on its card, between the card's edges.
        let row = screen.row(10);
        let names: Vec<&str> = row.split(['#', ' ']).filter(|word| !word.is_empty()).collect();
        assert_eq!(names, EXAMPLES, "{}", screen.text());
        assert!(screen.row(23).contains("banks") && screen.row(23).contains("1 of 3"), "{}", screen.text());
        assert!(screen.row(25).contains("128K cart"), "{}", screen.text());
        // Right twice: the test cart, 64K with save memory.
        machine.type_keys(&[RIGHT, RIGHT]);
        machine.until(|screen| screen.row(23).contains("test") && screen.row(25).contains("64K cart, saves 1K"));
    }

    /// Each cart's icon.png goes through the SDK's converter into the cart
    /// file, and the menu shows it as drawn, colour for colour: the test
    /// cart's has 21 colours, so it takes both of its palettes.
    #[test]
    fn the_menu_shows_each_carts_own_icon() {
        let machine = Machine::start(false);
        let screen = machine.until(|screen| screen.row(10).contains("test"));
        for (slot, example) in EXAMPLES.iter().enumerate() {
            let png = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join(example).join("icon.png");
            let (_, _, drawn) = gfx::read_colours(&png).unwrap();
            // The cards are 13 columns apart from column 1, the icon 4 in
            // and 1 down from the card's corner at row 4.
            let shown = screen.picture(1 + slot * 13 + 4, 5, 4, 4);
            assert!(shown == drawn, "{example}'s icon differs from its icon.png");
        }
    }

    /// Sprout starts as an egg, keeps itself in save memory, and the menu
    /// keeps that on the drive.
    #[test]
    fn sprout_starts_as_an_egg_and_is_kept() {
        let machine = Machine::start(false);
        machine.until(|screen| screen.row(10).contains("sprout"));
        machine.type_keys(&[RIGHT, b'\r']);
        machine.until(|screen| screen.row(28).contains("An egg"));
        // From Feed, left round to the last care icon: how it's doing.
        machine.type_keys(&[LEFT]);
        machine.until(|screen| screen.row(26).contains("How it's doing"));
        machine.type_keys(b"\r");
        machine.until(|screen| screen.row(7).contains("Still an egg."));
        machine.type_keys(&[ESCAPE]);
        machine.until(|screen| !screen.text().contains("Still an egg."));

        machine.reset();
        machine.until(|screen| screen.row(2).contains("CARTS"));
        let save = std::fs::read(machine.drive.join("sprout.sav")).expect("sprout.sav");
        assert_eq!(&save[..4], b"SPRT");
    }

    #[test]
    fn the_system_page_has_the_shell_and_what_a_launcher_offers() {
        let machine = Machine::start(false);
        machine.until(|screen| screen.row(2).contains("CARTS"));
        machine.type_keys(&[TAB]);
        let screen = machine.until(|screen| screen.row(10).contains("Power off"));
        assert!(screen.row(5).contains("Shell"));
        assert!(!screen.text().contains("Games"));

        let machine = Machine::start(true);
        machine.until(|screen| screen.row(2).contains("CARTS"));
        machine.type_keys(&[TAB]);
        let screen = machine.until(|screen| screen.row(20).contains("Power off"));
        assert!(screen.row(5).contains("Shell"));
        assert!(screen.row(10).contains("Games"));
        assert!(screen.row(15).contains("Reboot"));
        assert!(screen.row(20).contains("Power off"));
    }

    #[test]
    fn a_chosen_cart_has_the_machine_until_reset() {
        let machine = Machine::start(false);
        machine.until(|screen| screen.row(10).contains("test"));
        machine.type_keys(&[RIGHT, RIGHT, b'\r']);
        machine.until(|screen| screen.row(0).contains("VIDEO"));
        machine.reset();
        machine.until(|screen| screen.row(2).contains("CARTS"));
    }

    /// What the monitor says, kept for the test to read.
    #[derive(Clone, Default)]
    struct Said(Arc<std::sync::Mutex<String>>);

    impl std::io::Write for Said {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().push_str(&String::from_utf8_lossy(bytes));
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Said {
        /// Everything said since the last call, once it has `text` in it.
        fn until(&self, text: &str) -> String {
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                let mut said = self.0.lock().unwrap();
                if said.contains(text) {
                    return std::mem::take(&mut *said);
                }
                drop(said);
                std::thread::sleep(Duration::from_millis(1));
            }
            panic!("the monitor never said {text:?}; it said:\n{}", self.0.lock().unwrap());
        }
    }

    /// The monitor stops the machine when a client attaches, shows where it
    /// is, stops at a breakpoint, steps, disassembles and dumps memory by
    /// the system ROM's labels, and lets the machine go on.
    #[test]
    fn the_monitor_stops_steps_and_shows_the_machine() {
        let link = crate::monitor::Link::new(None);
        let machine = Machine::start_with(false, Some(Arc::clone(&link)));
        machine.until(|screen| screen.row(2).contains("CARTS"));
        let said = Said::default();
        let lines = link.attach(Box::new(said.clone()));
        said.until("(monitor) ");

        lines.send("r".into()).unwrap();
        assert!(said.until("(monitor) ").contains("pc $"));
        lines.send("b bios_waitframe".into()).unwrap();
        assert!(said.until("(monitor) ").contains("A breakpoint at _bios_waitframe"));
        lines.send("g".into()).unwrap();
        let stopped = said.until("(monitor) ");
        assert!(stopped.contains("Breakpoint.") && stopped.contains("pc $C01B (_bios_waitframe)"), "{stopped}");
        lines.send("s".into()).unwrap();
        let step = said.until("(monitor) ");
        assert!(step.contains("(waitframe)"), "stepped through the jump table: {step}");
        lines.send("d waitframe 2".into()).unwrap();
        let code = said.until("(monitor) ");
        assert!(code.contains("waitframe:") && code.contains("lda $FF01"), "{code}");
        lines.send("m ff00 ff0f".into()).unwrap();
        assert!(said.until("(monitor) ").contains("$FF00"));

        // Gone on: frames come again.
        lines.send("q".into()).unwrap();
        machine.until(|_| true);
        machine.until(|screen| screen.row(2).contains("CARTS"));
    }

    /// The test cart's video page splits the screen with a raster
    /// interrupt: the title row (lines 0-7) stays put while the landscape
    /// below scrolls, which shows in the scroll the chip kept for each line.
    #[test]
    fn a_raster_interrupt_splits_the_screen() {
        let machine = Machine::start(false);
        machine.until(|screen| screen.row(10).contains("test"));
        machine.type_keys(&[RIGHT, RIGHT, b'\r']);
        let screen = machine.until(|screen| screen.row(0).contains("VIDEO") && screen.0.scroll(100).0 > 8);
        let scroll_x: Vec<u16> = (0..240).map(|line| screen.0.scroll(line).0).collect();
        assert!(scroll_x[..8].iter().all(|&x| x == 0), "the title row: {:?}", &scroll_x[..8]);
        assert!(scroll_x[8..].iter().all(|&x| x == scroll_x[8]), "the landscape, all one scroll");
        assert_ne!(scroll_x[8], 0);
        assert!(screen.row(0).starts_with(" VIDEO"), "unscrolled: {}", screen.row(0));
    }

    /// The test cart's sound page plays its song (music/demo.song, through
    /// the SDK's music driver) on Start, showing each voice's note: the song
    /// starts on the notes it is written with, and moves on.
    #[test]
    fn the_music_driver_plays_a_song() {
        let machine = Machine::start(false);
        machine.until(|screen| screen.row(10).contains("test"));
        machine.type_keys(&[RIGHT, RIGHT, b'\r']);
        machine.until(|screen| screen.row(0).contains("VIDEO"));
        machine.type_keys(&[TAB]);
        machine.until(|screen| screen.row(16).contains("play a song"));
        machine.hold(crate::input::PAD_START);
        let notes = |screen: &Screen| -> Vec<String> { (9..13).map(|row| screen.row(row).get(30..).unwrap_or("").trim().to_string()).collect() };
        let first = machine.until(|screen| notes(screen).iter().all(|note| !note.is_empty()));
        machine.release();
        // Lead E5, harmony C4, bass C2, drum C6: the song's first notes.
        assert_eq!(notes(&first), ["E5", "C4", "C2", "C6"], "{}", first.text());
        machine.until(|screen| notes(screen)[0] == "G5");
    }

    /// The test cart's video page puts a foreground of pillars on plane B,
    /// scrolling faster than the landscape on plane A, with the hero behind
    /// it (sprite priority 1) and the balls in front.
    #[test]
    fn plane_b_and_sprite_priority() {
        let machine = Machine::start(false);
        machine.until(|screen| screen.row(10).contains("test"));
        machine.type_keys(&[RIGHT, RIGHT, b'\r']);
        let screen = machine.until(|screen| screen.row(0).contains("VIDEO") && screen.0.scroll(100).0 > 20);
        let frame = &screen.0;
        assert_ne!(frame.ctrl & 0x08, 0, "plane B on");
        assert_eq!(&frame.vram[0x9300..0x9303], [1, 0, 0], "the hero behind B, the balls in front");
        let entry = |at: usize| u16::from_le_bytes([frame.vram[at], frame.vram[at + 1]]) & 0x3FF;
        assert_ne!(entry(0xA000 + (12 * 64 + 6) * 2), 0, "a pillar's top on plane B");
        assert_eq!(entry(0xA000 + (5 * 64 + 6) * 2), 0, "the sky on plane B: nothing");
        let (a, b) = (frame.scroll(100).0, frame.scroll_b(100).0);
        assert!(b > a, "the foreground scrolls faster: A {a}, B {b}");
    }

    /// The test cart counts its starts in save memory, which the menu keeps
    /// on the drive between runs. Its program runs from RAM, and its system
    /// page, in bank 6, calls into bank 5 and reads the clock.
    #[test]
    fn the_test_cart_keeps_its_save_and_reaches_its_banks() {
        let machine = Machine::start(false);
        let system_page = || {
            machine.until(|screen| screen.row(10).contains("test"));
            machine.type_keys(&[RIGHT, RIGHT, b'\r']);
            machine.until(|screen| screen.row(0).contains("VIDEO"));
            machine.type_keys(&[TAB, TAB]);
            machine.until(|screen| screen.row(20).contains("FRAMES"))
        };
        let words = |screen: &Screen, row| screen.row(row).split_whitespace().map(String::from).collect::<Vec<_>>();

        let screen = system_page();
        assert_eq!(words(&screen, 3), ["Started", "1", "times"], "{}", screen.text());
        assert!(screen.row(9).contains("Bank 5 says 5"), "{}", screen.text());
        let clock = screen.row(13);
        assert!(clock.trim().len() == 19 && clock.trim().starts_with("20"), "the clock: {clock}");

        machine.reset();
        let screen = system_page();
        assert_eq!(words(&screen, 3), ["Started", "2", "times"], "kept: {}", screen.text());
        machine.hold(crate::input::PAD_A);
        machine.until(|screen| words(screen, 3) == ["Started", "3", "times"]);
        machine.release();
    }

    /// The 128K banks cart maps banks 1 to 14 at $A000 itself, and calls C in
    /// bank 15 through the SDK's bank_call.
    #[test]
    fn carts_switch_banks() {
        let machine = Machine::start(false);
        machine.until(|screen| screen.row(10).contains("test"));
        machine.type_keys(b"\r");
        let screen = machine.until(|screen| screen.row(16) == "All banks good.");
        let mut expected = vec!["Banks 1 to 15, at $A000:".to_string()];
        expected.extend((1..15).map(|bank| format!("Bank {bank} says {bank}: This is bank {bank}.")));
        expected.push("Bank 15 runs C: 12 squared is 144.".into());
        expected.push("All banks good.".into());
        assert_eq!(screen.rows()[..17], expected[..]);
    }

    #[test]
    fn boots_into_the_shell() {
        let (_machine, screen) = Machine::boot_screen();
        assert_eq!(screen.row(0), "6502 SYSTEM  BIOS 04");
        assert_eq!(screen.row(1), "Type HELP for a list of commands.");
    }

    #[test]
    fn menu_goes_back_to_the_menu() {
        let machine = Machine::boot();
        machine.type_keys(b"menu\r");
        machine.until(|screen| screen.row(2).contains("CARTS"));
    }

    #[test]
    fn history_recalls_lines_as_typed() {
        let machine = Machine::boot();
        machine.run(b"mem c000\r");
        machine.run(b"echo  x\r");
        // Up twice: the first line; once more does nothing (only two lines).
        let screen = machine.run(&[UP, UP, UP, b'\r']);
        assert_eq!(screen.output_of("mem c000"), ["C000: 4C"]);
        // Down steps back.
        let screen = machine.run(&[UP, UP, UP, DOWN, b'\r']);
        assert_eq!(screen.output_of("echo  x"), ["x"]);
        // Escape clears the line, and an empty line runs nothing.
        let screen = machine.run(&[UP, ESCAPE, b'\r']);
        let rows = screen.rows();
        let prompt = (0..ROWS).rposition(|row| screen.waiting_at(row)).unwrap();
        assert_eq!(rows[prompt - 1], ">", "{}", screen.text());
    }

    #[test]
    fn left_and_right_edit_inside_the_line() {
        let machine = Machine::boot();
        // "em c0x00": insert the 'm' at the start, delete the 'x', then go
        // back to the end and add a '0'.
        let mut keys = b"em c0x00".to_vec();
        keys.extend_from_slice(&[LEFT; 8]);
        keys.push(b'm');
        keys.extend_from_slice(&[RIGHT; 6]);
        keys.push(BACKSPACE);
        keys.extend_from_slice(&[RIGHT, RIGHT, RIGHT]); // the last one does nothing
        keys.extend_from_slice(b"0\r");
        let screen = machine.run(&keys);
        assert_eq!(screen.output_of("mem c0000"), ["Usage: MEM addr [end]"]);
        // Recalled lines are edited the same way.
        let screen = machine.run(&[UP, LEFT, BACKSPACE, b'\r']);
        assert_eq!(screen.output_of("mem c000"), ["C000: 4C"]);
    }

    #[test]
    fn long_lines_wrap() {
        let machine = Machine::boot();
        // Start at the bottom, so the long line scrolls the screen.
        machine.run(&b"mem c000\r".repeat(30));
        // Type past the end of the row, then go back across the row
        // boundary and insert a word.
        let xs = "x".repeat(50);
        let mut keys = format!("echo {xs}").into_bytes();
        keys.extend_from_slice(&[LEFT; 50]);
        keys.extend_from_slice(b"a \r");
        let expected: Vec<String> = [format!("> echo a {xs}"), format!("a {xs}")]
            .iter()
            .flat_map(|line| {
                line.as_bytes()
                    .chunks(COLUMNS)
                    .map(|row| String::from_utf8(row.to_vec()).unwrap())
                    .collect::<Vec<_>>()
            })
            .collect();
        let check = |screen: Screen| {
            let rows = screen.rows();
            assert!(screen.waiting_at(ROWS - 1), "{}", screen.text());
            assert_eq!(rows[ROWS - 1 - expected.len()..ROWS - 1], expected[..]);
        };
        check(machine.run(&keys));
        // A recalled long line comes back whole.
        check(machine.run(&[UP, b'\r']));
    }

    #[test]
    fn memory_commands() {
        let machine = Machine::boot();
        machine.run(b"poke 6000 de ad be ef\r");
        let screen = machine.run(b"mem 5ffe 6009\r");
        assert_eq!(
            screen.output_of("mem 5ffe 6009"),
            ["5FFE: 00 00", "6000: DE AD BE EF 00 00 00 00", "6008: 00 00"]
        );
        let screen = machine.run(b"poke 6000 100\r");
        assert_eq!(screen.output_of("poke 6000 100"), ["Bytes must be hex, 00 to FF."]);
        let screen = machine.run(b"mem\r");
        assert_eq!(screen.output_of("mem"), ["Usage: MEM addr [end]"]);
    }

    #[test]
    fn unknown_commands_are_reported() {
        let machine = Machine::boot();
        let screen = machine.run(b"bogus 1 2\r");
        assert_eq!(screen.output_of("bogus 1 2"), ["Unknown command. Type HELP for a list."]);
    }

    #[test]
    fn the_screen_scrolls() {
        let machine = Machine::boot();
        let screen = machine.run(&b"mem c000\r".repeat(30));
        assert!(screen.waiting_at(ROWS - 1));
        assert_eq!(screen.row(ROWS - 3), "> mem c000");
        assert!(!screen.text().contains("6502 SYSTEM"), "banner scrolled away");
        assert_eq!(screen.row(ROWS - 2), "C000: 4C");
        assert_ne!(screen.0.scroll(0).1, 0, "by moving SCROLLY");
    }
}
