//! The host, on a machine of no particular kind: here the NES, which has
//! none of these of its own. Its picture shown, as big as it is, and its
//! sound played, a frame at a time; its title; a notice over its picture;
//! F2 and F4; its Reset button, and the keys given to it; the window
//! closed, and what it keeps saved.

#[cfg(test)]
mod tests {
    use crate::cpu::{CPU, Variant};
    use crate::host::display::{Display, Gamepad, Key};
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
        closed: bool,
        /// Closed once it's shown this many pictures, if it's to be.
        close_after: Option<usize>,
    }

    struct Window(Rc<RefCell<Seen>>);

    impl Display for Window {
        fn show(&mut self, pixels: &[u32], width: usize, height: usize) {
            let seen = &mut self.0.borrow_mut();
            seen.pictures += 1;
            seen.size = (width, height);
            seen.last = pixels.to_vec();
        }

        fn key(&mut self) -> Option<Key> {
            None
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

        fn closed(&self) -> bool {
            let seen = self.0.borrow();
            seen.closed || seen.close_after.is_some_and(|pictures| seen.pictures >= pictures)
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
        let state = std::env::temp_dir().join(format!("cpu_emu_{}_host_{name}.state", std::process::id()));
        (Host::new(cpu, Box::new(Window(seen.clone())), state.clone()), seen, state)
    }

    #[test]
    fn a_frame_is_shown_and_played_and_its_title_set() {
        let (mut host, seen, _) = host("frame");
        assert!(host.frame(), "closed");
        assert!(host.frame(), "closed");
        let seen = seen.borrow();
        assert_eq!(seen.pictures, 2, "a picture a frame");
        assert_eq!(seen.size, (ppu::WIDTH - 16, ppu::HEIGHT - 16), "the picture not as big as it's seen");
        assert!(seen.samples > 0, "no sound played");
        assert_eq!(seen.title, "NES   Test");
        assert!(host.cpu.memory.sound().is_empty(), "the sound played not taken");
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
        assert!(band.iter().any(|&dot| dot == 0xFFFFFF), "no notice on its picture");

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
        assert_eq!((host.cpu.memory.ram[0x123], host.cpu.clock), (0x5A, clock), "not as it was saved");
        assert_eq!(host.cpu.memory.cartridge.prg_rom.len(), 0x4000, "its ROM not kept");
        host.frame();
        assert_eq!(seen.borrow().size, (width, ppu::HEIGHT - 16), "its overscan not kept");
        assert!(host.cpu.memory.title().ends_with("Test"), "its name not kept");
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
        assert!(host.cpu.memory.gamepad_2.a && !host.cpu.memory.gamepad_2.start, "the second gamepad not given");

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
        assert_eq!(std::fs::read(&file).unwrap()[0], 0x77, "its battery's RAM not saved");
        std::fs::remove_file(&file).unwrap();
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
}
