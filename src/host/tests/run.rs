//! The host, on a machine of no particular kind: here the NES, which has
//! none of these of its own. Its picture shown, as big as it is, and its
//! sound played, a frame at a time; its title; a notice over its picture;
//! F2 and F4; its Reset button, and the keys given to it; the window
//! closed, and what it keeps saved. And on the X16, a machine that takes
//! the keyboard's keys: them, but the host's own; and its mouse, only
//! while a program has it on.

#[cfg(test)]
mod tests {
    use crate::cpu::{CPU, Variant};
    use crate::host::controls::{Action, Controls};
    use crate::host::display::{Display, Gamepad, Key, Scancode};
    use crate::host::machine::Machine;
    use crate::host::run::Host;
    use crate::machines::nes::cartridge::Cartridge;
    use crate::machines::nes::{Nes, ppu};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// What the test's window has been told, and what's pressed in it.
    #[derive(Default)]
    struct Seen {
        pictures: usize,
        size: (usize, usize),
        last: Vec<u32>,
        samples: usize,
        title: String,
        function_keys: Vec<u8>,
        gamepad: Gamepad,
        gamepad_2: Gamepad,
        reset: bool,
        fast: bool,
        rewind: bool,
        closed: bool,
        /// Closed once it's shown this many pictures, if it's to be.
        close_after: Option<usize>,
        /// What's pressed, as bindings, while it's looking; the controls as
        /// they were at the last picture.
        pressed: Vec<String>,
        controls: Option<Controls>,
        /// Characters typed, and keys pressed and let go.
        keys: Vec<Key>,
        key_changes: Vec<(Scancode, bool)>,
        /// Whether the machine has a mouse, each time it's told.
        mouse: Vec<bool>,
    }

    /// The test's window: its own controls, never saved, as the window's
    /// are, to the user's file.
    struct Window(Rc<RefCell<Seen>>, Controls);

    impl Display for Window {
        fn show(&mut self, pixels: &[u32], width: usize, height: usize) {
            let seen = &mut self.0.borrow_mut();
            seen.pictures += 1;
            seen.size = (width, height);
            seen.last = pixels.to_vec();
            seen.controls = Some(self.1.clone());
        }

        fn can_close(&self) -> bool {
            true
        }

        fn close(&mut self) {
            self.0.borrow_mut().closed = true;
        }

        fn pressed(&mut self) -> Option<String> {
            let pressed = &mut self.0.borrow_mut().pressed;
            (!pressed.is_empty()).then(|| pressed.remove(0))
        }

        fn controls(&mut self) -> Option<&mut Controls> {
            Some(&mut self.1)
        }

        fn key(&mut self) -> Option<Key> {
            let keys = &mut self.0.borrow_mut().keys;
            (!keys.is_empty()).then(|| keys.remove(0))
        }

        fn key_change(&mut self) -> Option<(Scancode, bool)> {
            let changes = &mut self.0.borrow_mut().key_changes;
            (!changes.is_empty()).then(|| changes.remove(0))
        }

        fn gamepad(&self) -> Gamepad {
            self.0.borrow().gamepad
        }

        fn gamepad_2(&self) -> Gamepad {
            self.0.borrow().gamepad_2
        }

        fn reset_held(&self) -> bool {
            self.0.borrow().reset
        }

        fn fast_forward(&self) -> bool {
            self.0.borrow().fast
        }

        fn rewinding(&self) -> bool {
            self.0.borrow().rewind
        }

        fn closed(&self) -> bool {
            let seen = self.0.borrow();
            seen.closed
                || seen
                    .close_after
                    .is_some_and(|pictures| seen.pictures >= pictures)
        }

        fn function_key(&mut self) -> Option<u8> {
            let keys = &mut self.0.borrow_mut().function_keys;
            (!keys.is_empty()).then(|| keys.remove(0))
        }

        fn title(&mut self, title: &str) {
            self.0.borrow_mut().title = title.to_string();
        }

        fn play(&mut self, samples: &[f32]) {
            self.0.borrow_mut().samples += samples.len();
        }

        fn mouse(&mut self, on: bool) {
            self.0.borrow_mut().mouse.push(on);
        }
    }

    /// The NES with a cartridge of NOPs, battery backed, in the test's
    /// window, its state kept at a file for `name`.
    fn host(name: &str) -> (Host<Nes>, Rc<RefCell<Seen>>, std::path::PathBuf) {
        let mut image = b"NES\x1A\x01\x01\x02".to_vec();
        image.extend([0; 9]);
        image.extend([0xEA; 0x4000 + 0x2000]);
        let mut nes = Nes::new(Cartridge::new(&image).unwrap());
        nes.name = "Test".to_string();
        nes.overscan = 8;
        let mut cpu = CPU::with_bus(nes);
        cpu.variant = Variant::Ricoh2A03;
        cpu.reset();
        let seen = Rc::new(RefCell::new(Seen::default()));
        let state =
            std::env::temp_dir().join(format!("cpu_emu_{}_host_{name}.state", std::process::id()));
        (
            Host::new(
                cpu,
                Box::new(Window(seen.clone(), Controls::default())),
                state.clone(),
            ),
            seen,
            state,
        )
    }

    #[test]
    fn a_frame_is_shown_and_played_and_its_title_set() {
        let (mut host, seen, _) = host("frame");
        assert!(host.frame(), "closed");
        assert!(host.frame(), "closed");
        let seen = seen.borrow();
        assert_eq!(seen.pictures, 2, "a picture a frame");
        assert_eq!(
            seen.size,
            (ppu::WIDTH - 16, ppu::HEIGHT - 16),
            "the picture not as big as it's seen"
        );
        assert!(seen.samples > 0, "no sound played");
        assert_eq!(seen.title, "NES   Test");
        assert!(
            host.cpu.memory.sound().is_empty(),
            "the sound played not taken"
        );
    }

    #[test]
    fn fast_forwarded_its_three_times_as_quick_a_picture_of_three_and_quiet() {
        let (mut host, seen, _) = host("fast");
        // A whole frame's cycles: the first, from Reset, is short.
        host.frame();
        let start = host.cycles;
        host.frame();
        let (frame, played) = (host.cycles - start, seen.borrow().samples);
        seen.borrow_mut().fast = true;
        for _ in 0..6 {
            host.frame();
        }
        let ran = host.cycles - start - frame;
        assert!(
            ran.abs_diff(2 * frame) < 10,
            "six frames in the time of {ran} cycles, not two frames' {}",
            2 * frame
        );
        assert_eq!(
            seen.borrow().pictures,
            2 + 2,
            "a picture a frame, then of each three"
        );
        assert_eq!(seen.borrow().samples, played, "heard while fast");
        assert_eq!(host.notice.as_ref().unwrap().0, "Fast forward");

        seen.borrow_mut().fast = false;
        host.frame();
        assert_eq!(seen.borrow().pictures, 5, "let go, its picture shown");
        assert!(seen.borrow().samples > played, "and heard");
        assert!(host.notice.is_none(), "its notice still up");
    }

    #[test]
    fn rewound_it_goes_back_two_frames_a_frame_quiet_to_the_oldest_kept() {
        let (mut host, seen, _) = host("rewind");
        // A frame's mark in RAM, as it's kept every other frame: 2, 4, ... 20.
        for frame in 1..=20 {
            host.cpu.memory.ram[0x123] = frame;
            host.frame();
        }
        assert_eq!(host.moments.as_ref().unwrap().len(), 10);
        let (pictures, played) = (seen.borrow().pictures, seen.borrow().samples);
        seen.borrow_mut().rewind = true;
        let mut back = Vec::new();
        for _ in 0..12 {
            host.frame();
            back.push(host.cpu.memory.ram[0x123]);
        }
        assert_eq!(
            back,
            [20, 18, 16, 14, 12, 10, 8, 6, 4, 2, 2, 2],
            "to the oldest, and there"
        );
        assert_eq!(seen.borrow().pictures, pictures + 12, "each seen");
        assert_eq!(seen.borrow().samples, played, "heard");
        assert_eq!(host.notice.as_ref().unwrap().0, "Rewind");

        // Let go: on from there, kept again, its notice gone.
        seen.borrow_mut().rewind = false;
        host.cpu.memory.ram[0x123] = 99;
        host.frame();
        host.frame();
        assert!(host.notice.is_none(), "its notice still up");
        let moments = host.moments.as_ref().unwrap();
        assert_eq!(moments.len(), 2, "the oldest, and one since");
    }

    #[test]
    fn turbo_presses_a_and_b_for_two_frames_and_lets_go_for_two() {
        let (mut host, seen, _) = host("turbo");
        seen.borrow_mut().gamepad = Gamepad {
            turbo_a: true,
            ..Gamepad::default()
        };
        seen.borrow_mut().gamepad_2 = Gamepad {
            turbo_b: true,
            b: true,
            ..Gamepad::default()
        };
        let mut a = String::new();
        for _ in 0..8 {
            host.frame();
            let nes = &host.cpu.memory;
            a.push(if nes.gamepad.a { 'A' } else { '.' });
            assert!(!nes.gamepad.turbo_a, "turbo given to the machine");
            assert!(nes.gamepad_2.b, "B held as well, let go");
        }
        // Two pressed, two not, from wherever it starts.
        assert!("AA..".repeat(3).contains(&a), "{a}");
    }

    #[test]
    fn f2_and_f4_and_their_notices_are_any_machines() {
        let (mut host, seen, state) = host("state");
        seen.borrow_mut().function_keys.push(4);
        host.frame();
        assert_eq!(host.notice.as_ref().unwrap().0, "No state saved");
        host.frame();
        let width = ppu::WIDTH - 16;
        let band = seen.borrow().last[(ppu::HEIGHT - 16 - 12) * width..].to_vec();
        assert!(band.contains(&0xFFFFFF), "no notice on its picture");

        host.cpu.memory.ram[0x123] = 0x5A;
        seen.borrow_mut().function_keys.push(2);
        host.frame();
        assert_eq!(host.notice.as_ref().unwrap().0, "State saved");
        let clock = host.cpu.clock;
        host.cpu.memory.ram[0x123] = 0;
        host.frame();
        seen.borrow_mut().function_keys.push(4);
        host.frame();
        assert_eq!(host.notice.as_ref().unwrap().0, "State loaded");
        assert_eq!(
            (host.cpu.memory.ram[0x123], host.cpu.clock),
            (0x5A, clock),
            "not as it was saved"
        );
        assert_eq!(
            host.cpu.memory.cartridge.prg_rom.len(),
            0x4000,
            "its ROM not kept"
        );
        host.frame();
        assert_eq!(
            seen.borrow().size,
            (width, ppu::HEIGHT - 16),
            "its overscan not kept"
        );
        assert!(
            host.cpu.memory.title().ends_with("Test"),
            "its name not kept"
        );
        std::fs::remove_file(&state).unwrap();
    }

    #[test]
    fn a_state_of_another_machine_says_so() {
        let (mut host, seen, state) = host("garbage");
        std::fs::write(&state, b"not a state").unwrap();
        seen.borrow_mut().function_keys.push(4);
        host.frame();
        assert_eq!(host.notice.as_ref().unwrap().0, "State not loaded");
        std::fs::remove_file(&state).unwrap();
    }

    #[test]
    fn f3_turns_the_debug_panel_on_and_off() {
        let (mut host, seen, _) = host("debug");
        seen.borrow_mut().function_keys.push(3);
        host.frame();
        host.frame();
        assert!(host.debug, "not on");
        let on = seen.borrow().last.clone();

        seen.borrow_mut().function_keys.push(3);
        host.frame();
        host.frame();
        assert!(!host.debug, "not off");
        assert_ne!(seen.borrow().last, on, "the panel drawn with it off");
    }

    #[test]
    fn the_controls_and_reset_are_given_to_the_machine() {
        let (mut host, seen, _) = host("controls");
        seen.borrow_mut().gamepad.start = true;
        seen.borrow_mut().gamepad_2.a = true;
        host.frame();
        assert!(host.cpu.memory.gamepad.start, "the gamepad not given");
        assert!(
            host.cpu.memory.gamepad_2.a && !host.cpu.memory.gamepad_2.start,
            "the second gamepad not given"
        );

        seen.borrow_mut().reset = true;
        host.frame();
        let pc = host.cpu.pc;
        host.frame();
        assert_eq!(host.cpu.pc, pc, "the CPU ran, Reset held");
        seen.borrow_mut().reset = false;
        host.frame();
        host.frame();
        assert_ne!(host.cpu.pc, pc, "the CPU not started again");
    }

    #[test]
    fn closed_it_saves_what_the_machine_keeps_and_stops() {
        let (mut host, seen, _) = host("closed");
        let file = std::env::temp_dir().join(format!("cpu_emu_{}_host.srm", std::process::id()));
        host.cpu.memory.battery_file = Some(file.clone());
        host.cpu.memory.cartridge.cpu_write(0x6000, 0x77, 0);
        assert!(host.frame(), "closed");
        assert!(!file.exists(), "saved before a second was up");
        seen.borrow_mut().closed = true;
        assert!(!host.frame(), "not stopped");
        assert_eq!(
            std::fs::read(&file).unwrap()[0],
            0x77,
            "its battery's RAM not saved"
        );
        std::fs::remove_file(&file).unwrap();
    }

    #[test]
    fn put_away_it_resumes_where_it_was() {
        let (mut first, seen, state) = host("resume");
        let resume = state.with_extension("resume");
        let _ = std::fs::remove_file(&resume);
        first.resume();
        assert!(first.notice.is_none(), "resumed with nothing to resume");
        first.frame();
        first.cpu.memory.ram[0x123] = 0x45;
        first.keep_for_resuming();
        assert!(!state.exists(), "F2's state written");
        let (mut again, _, _) = host("resume");
        again.resume();
        assert_eq!(again.cpu.memory.ram[0x123], 0x45, "not where it was");
        assert_eq!(
            again.notice.as_ref().map(|(text, _)| text.as_str()),
            Some("Resumed")
        );
        assert_eq!(seen.borrow().pictures, 1);
        std::fs::remove_file(&resume).unwrap();
    }

    #[test]
    fn a_state_to_resume_that_wont_load_starts_it_afresh() {
        let (mut started, _, state) = host("unresumable");
        let resume = state.with_extension("resume");
        std::fs::write(&resume, b"not a state").unwrap();
        started.cpu.memory.ram[0x123] = 0x45;
        started.resume();
        assert_eq!(started.cpu.memory.ram[0x123], 0x45);
        assert!(started.notice.is_none(), "said it resumed");
        std::fs::remove_file(&resume).unwrap();
    }

    /// Frames with the first gamepad as `pad` has it.
    fn frames_with(host: &mut Host<Nes>, seen: &Rc<RefCell<Seen>>, pad: Gamepad, frames: usize) {
        for _ in 0..frames {
            seen.borrow_mut().gamepad = pad;
            host.frame();
        }
    }

    const SELECT_START: Gamepad = Gamepad {
        select: true,
        start: true,
        up: false,
        down: false,
        left: false,
        right: false,
        a: false,
        b: false,
        turbo_a: false,
        turbo_b: false,
        accept: false,
        back: false,
    };
    const NONE: Gamepad = Gamepad {
        select: false,
        start: false,
        up: false,
        down: false,
        left: false,
        right: false,
        a: false,
        b: false,
        turbo_a: false,
        turbo_b: false,
        accept: false,
        back: false,
    };

    #[test]
    fn select_and_start_tapped_together_open_the_menu_and_the_game_never_sees_them() {
        let (mut host, seen, _) = host("menu");
        frames_with(&mut host, &seen, NONE, 2);
        frames_with(&mut host, &seen, SELECT_START, 5);
        assert_eq!(host.cpu.memory.gamepad, NONE, "the game saw them");
        frames_with(&mut host, &seen, NONE, 1);
        let paused = host.cpu.clock;
        frames_with(&mut host, &seen, NONE, 3);
        assert_eq!(host.cpu.clock, paused, "the game went on under the menu");
        assert!(seen.borrow().pictures >= 10, "nothing shown while it's up");
        // Menu back goes back to it.
        frames_with(&mut host, &seen, Gamepad { back: true, ..NONE }, 1);
        frames_with(&mut host, &seen, NONE, 2);
        assert!(host.cpu.clock > paused, "still paused");
    }

    #[test]
    fn held_long_they_dont_open_it_and_alone_the_game_has_them_at_once() {
        let (mut host, seen, _) = host("held");
        frames_with(&mut host, &seen, SELECT_START, 60);
        frames_with(&mut host, &seen, NONE, 2);
        let running = host.cpu.clock;
        frames_with(&mut host, &seen, NONE, 1);
        assert!(host.cpu.clock > running, "a long hold opened the menu");
        frames_with(
            &mut host,
            &seen,
            Gamepad {
                start: true,
                ..NONE
            },
            1,
        );
        assert!(host.cpu.memory.gamepad.start, "Start alone held back");
        // Joined by Select, it's let go of, as far as the game knows, until
        // both are.
        frames_with(&mut host, &seen, SELECT_START, 1);
        assert_eq!(host.cpu.memory.gamepad, NONE, "the game saw them together");
        frames_with(
            &mut host,
            &seen,
            Gamepad {
                start: true,
                ..NONE
            },
            1,
        );
        assert!(
            !host.cpu.memory.gamepad.start,
            "Start given again before it was let go"
        );
    }

    #[test]
    fn from_the_menu_a_state_is_saved_and_its_quit() {
        let (mut host, seen, state) = host("menu_items");
        let _ = std::fs::remove_file(&state);
        seen.borrow_mut().function_keys.push(1);
        frames_with(&mut host, &seen, NONE, 2);
        let down = Gamepad { down: true, ..NONE };
        let choose = Gamepad {
            accept: true,
            ..NONE
        };
        // Resume, then Save state.
        for pad in [down, NONE, choose, NONE] {
            frames_with(&mut host, &seen, pad, 1);
        }
        assert!(state.exists(), "not saved");
        assert_eq!(
            host.notice.as_ref().map(|(text, _)| text.as_str()),
            Some("State saved")
        );
        // Up from Resume is the last, Quit.
        seen.borrow_mut().function_keys.push(1);
        frames_with(&mut host, &seen, NONE, 2);
        frames_with(&mut host, &seen, Gamepad { up: true, ..NONE }, 1);
        frames_with(&mut host, &seen, NONE, 1);
        seen.borrow_mut().gamepad = choose;
        assert!(!host.frame(), "not quit");
        std::fs::remove_file(&state).unwrap();
    }

    #[test]
    fn controls_are_changed_by_the_gamepad_alone_and_cancelled() {
        let (mut host, seen, _) = host("controls");
        seen.borrow_mut().function_keys.push(6);
        host.frame();
        // Down the list to B, chosen, and the next press bound to it.
        seen.borrow_mut().pressed.extend(
            [
                "pad:dpdown",
                "pad:dpdown",
                "pad:dpdown",
                "pad:dpdown",
                "pad:dpdown",
                "pad:a",
                "pad:y",
            ]
            .map(String::from),
        );
        host.frame();
        host.frame();
        let bound = seen.borrow().controls.clone().unwrap();
        assert!(
            bound.of(0, Action::B).contains(&"pad:y"),
            "not bound: {:?}",
            bound.of(0, Action::B)
        );
        assert!(
            !bound.of(0, Action::B).contains(&"pad:x"),
            "the button it had is still bound"
        );
        // B cancels: as they were.
        seen.borrow_mut().pressed.push("pad:b".to_string());
        host.frame();
        host.frame();
        let after = seen.borrow().controls.clone().unwrap();
        assert_eq!(after, Controls::default());
        let running = host.cpu.clock;
        host.frame();
        assert!(host.cpu.clock > running, "still changing them");
    }

    #[test]
    fn it_runs_to_the_cpus_clock_until_its_closed() {
        let (mut host, seen, _) = host("clock");
        host.cpu.clock_hz = crate::machines::nes::CLOCK_HZ;
        seen.borrow_mut().close_after = Some(30);
        let start = std::time::Instant::now();
        host.run();
        // 30 frames of 60: half a second, never less but for the last
        // frame's, which isn't waited out. Busy hosts may take longer.
        let took = start.elapsed().as_secs_f32();
        assert!(took >= 0.45, "30 frames in {took} seconds");
        assert_eq!(seen.borrow().pictures, 30);
    }

    #[test]
    fn a_machine_that_takes_the_keys_has_them_but_the_hosts_not_what_they_type() {
        use crate::machines::x16::X16;
        let mut cpu = CPU::with_bus(X16::new(&[0xEA; 0x4000]).unwrap());
        cpu.variant = Variant::Wdc65C02;
        cpu.reset();
        let seen = Rc::new(RefCell::new(Seen::default()));
        let state =
            std::env::temp_dir().join(format!("cpu_emu_{}_host_keys.state", std::process::id()));
        let window = Window(seen.clone(), Controls::default());
        let mut host = Host::new(cpu, Box::new(window), state);
        let a = Scancode(0x04);
        seen.borrow_mut().keys.push(Key::Char(b'a'));
        seen.borrow_mut().key_changes = vec![
            (a, true),
            (Scancode::function(2), true), // the host's: saving a state
            (Scancode::function(5), true),
            (a, false),
        ];
        host.frame();
        let keys: Vec<u8> = host.cpu.memory.i2c.smc.keys.iter().copied().collect();
        assert_eq!(
            keys,
            [31, 116, 31 | 0x80],
            "A, F5, A let go; not F2, nor 'a' typed"
        );
    }

    #[test]
    fn the_display_is_told_of_a_mouse_as_a_program_turns_it_on_and_off() {
        use crate::machines::x16::X16;
        let mut cpu = CPU::with_bus(X16::new(&[0xEA; 0x4000]).unwrap());
        cpu.variant = Variant::Wdc65C02;
        cpu.reset();
        cpu.memory.i2c.smc.set_default_read(0x41);
        let seen = Rc::new(RefCell::new(Seen::default()));
        let state =
            std::env::temp_dir().join(format!("cpu_emu_{}_host_mouse.state", std::process::id()));
        let window = Window(seen.clone(), Controls::default());
        let mut host = Host::new(cpu, Box::new(window), state);
        host.frame();
        host.cpu.memory.i2c.smc.set_default_read(0x43);
        host.frame();
        host.frame();
        host.cpu.memory.i2c.smc.set_default_read(0x41);
        host.frame();
        assert_eq!(
            seen.borrow().mouse,
            [false, true, false],
            "told as it changes, only"
        );
    }
}
