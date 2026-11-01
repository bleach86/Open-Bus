//! Save states: where one's kept; F2 saving one and F4 loading it back,
//! the window and the ROM kept; one refused by another machine, or not one
//! at all; and a state loaded going on just as the machine it was saved
//! from did, to the last bit of every part of it: a II+ booting DOS 3.3,
//! the drive turning, with a Mockingboard; a IIe booting ProDOS from the
//! hard disk, a .woz in the drive, two Mockingboards. Those need the
//! Apple's ROMs and the disks in roms/apple2/ (not part of the repository;
//! without them, they say so and pass).

#[cfg(test)]
mod tests {
    use crate::cpu::CPU;
    use crate::host::clock::DateTime;
    use crate::host::display::{Display, Key};
    use crate::machines::apple2::disk2::Disk2;
    use crate::machines::apple2::disk_image::Disk;
    use crate::machines::apple2::hard_disk::{HardDisk, Volume};
    use crate::machines::apple2::save_state::{self, restore, state, Wanted};
    use crate::machines::apple2::{
        clock_card::ClockCard, iie, language_card::LanguageCard, mockingboard::Mockingboard, Apple2, CLOCK_HZ,
        CYCLES_PER_FRAME,
    };
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    fn dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("roms/apple2")
    }

    #[test]
    fn a_state_is_kept_beside_the_first_disk() {
        assert_eq!(save_state::path(Path::new("disks/Lode Runner.do")), Path::new("disks/Lode Runner.do.state"));
        assert_eq!(save_state::path(Path::new("disks/Ultima IV/")), Path::new("disks/Ultima IV.state"));
        assert_eq!(save_state::path(Path::new("roms/Apple2_Plus.rom")), Path::new("roms/Apple2_Plus.rom.state"));
    }

    /// A window of the test's: its function keys pressed as the test says.
    struct Keys(Rc<RefCell<VecDeque<u8>>>);

    impl Display for Keys {
        fn show(&mut self, _: &[u32], _: usize, _: usize) {}

        fn key(&mut self) -> Option<Key> {
            None
        }

        fn function_key(&mut self) -> Option<u8> {
            self.0.borrow_mut().pop_front()
        }
    }

    /// A II+ of a ROM of NOPs, but its first byte $11.
    fn nops() -> Apple2 {
        let mut rom = vec![0xEA; 0x3000];
        rom[0] = 0x11;
        Apple2::new(rom)
    }

    #[test]
    fn f2_saves_a_state_and_f4_loads_it_back() {
        let path = std::env::temp_dir().join(format!("cpu_emu_{}_keys.state", std::process::id()));
        let keys = Rc::new(RefCell::new(VecDeque::new()));
        let mut apple = nops();
        apple.display = Some(Box::new(Keys(keys.clone())));
        let mut cpu = CPU::with_bus(apple);
        cpu.clock_hz = u32::MAX; // as fast as it goes
        cpu.reset();

        // F4, none saved yet.
        keys.borrow_mut().push_back(4);
        cpu.execute(2 * CYCLES_PER_FRAME);
        assert_eq!(cpu.memory.state_wanted, Some(Wanted::Load), "F4: the run loop not handed back for it");
        save_state::wanted(&mut cpu, &path);
        assert_eq!(cpu.memory.notice.as_ref().unwrap().0, "No state saved");

        // F2: handed back as the frame ends, after that instruction.
        cpu.memory.ram[0x1234] = 0x5A;
        keys.borrow_mut().push_back(2);
        cpu.execute(2 * CYCLES_PER_FRAME);
        assert_eq!(cpu.memory.state_wanted, Some(Wanted::Save), "F2: the run loop not handed back for it");
        assert!(cpu.memory.cycle < 8, "not handed back as the frame ended: cycle {}", cpu.memory.cycle);
        let (pc, clock) = (cpu.pc, cpu.clock);
        save_state::wanted(&mut cpu, &path);
        assert_eq!(cpu.memory.notice.as_ref().unwrap().0, "State saved");
        assert!(path.exists(), "no state saved");

        // On, and changed; then F4: as it was, the window and ROM kept.
        cpu.execute(CYCLES_PER_FRAME / 2);
        cpu.memory.ram[0x1234] = 0;
        cpu.a = 0x77;
        keys.borrow_mut().push_back(4);
        cpu.execute(2 * CYCLES_PER_FRAME);
        save_state::wanted(&mut cpu, &path);
        assert_eq!(cpu.memory.notice.as_ref().unwrap().0, "State loaded");
        assert_eq!((cpu.pc, cpu.clock, cpu.memory.ram[0x1234]), (pc, clock, 0x5A), "not as it was saved");
        assert!(cpu.memory.display.is_some(), "the window not kept");
        assert_eq!((cpu.memory.rom.len(), cpu.memory.rom[0]), (0x3000, 0x11), "the ROM not kept");
        assert_eq!(cpu.memory.picture.len(), crate::machines::apple2::video::PICTURE_WIDTH * 192, "the picture not kept");
        // The keys still work, the window's.
        keys.borrow_mut().push_back(2);
        cpu.execute(2 * CYCLES_PER_FRAME);
        assert_eq!(cpu.memory.state_wanted, Some(Wanted::Save), "the keys gone");
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn another_machines_state_is_refused() {
        let mut cpu = CPU::with_bus(nops());
        cpu.reset();
        let saved = state(&cpu).unwrap();

        // Other ROMs; a Disk II; an Apple //e.
        let mut other = CPU::with_bus(Apple2::new(vec![0xEA; 0x3000]));
        let refused = restore(&mut other, &saved).unwrap_err();
        assert!(refused.starts_with("a state of another machine"), "{refused}");
        let mut with_disk2 = nops();
        with_disk2.disk2 = Some(Disk2::new(vec![0; 256], &[0; 256]).unwrap());
        assert!(restore(&mut CPU::with_bus(with_disk2), &saved).is_err(), "a Disk II's state without one");
        let iie = Apple2::new_iie(&vec![0xEA; 0x2000], &vec![0xEA; 0x2000], vec![0; 0x1000]);
        assert!(restore(&mut CPU::with_bus(iie), &saved).is_err(), "a II+'s state on a IIe");

        // Not a state.
        assert_eq!(restore(&mut cpu, b"not a state at all").unwrap_err(), "not a save state");
        assert_eq!(restore(&mut cpu, &[]).unwrap_err(), "not a save state");
        // Its own, loaded.
        assert!(restore(&mut cpu, &saved).is_ok());
    }

    /// The clock card, its time the same each run.
    fn clock() -> ClockCard {
        let mut card = ClockCard::new();
        card.fixed = Some(DateTime { year: 2026, month: 10, day: 31, weekday: 6, hour: 12, minute: 0, second: 0 });
        card
    }

    /// `machine` (none without the ROMs and disks) run a while, saved, and
    /// run on `after` cycles; then another of them loaded with the state,
    /// and run on the same: the two the same, every part of them; and the
    /// last as `up` says it is.
    fn goes_on_as_it_did(name: &str, machine: fn() -> Option<CPU<Apple2>>, before: u32, after: u32, up: fn(&CPU<Apple2>) -> bool) {
        let Some(mut cpu) = machine() else {
            eprintln!("save state test: no ROMs or disks in roms/apple2/ for the {name}");
            return;
        };
        cpu.execute(before);
        let saved = state(&cpu).unwrap();
        cpu.execute(after);

        let mut loaded = machine().unwrap();
        loaded.execute(CLOCK_HZ / 10); // somewhere else, first
        restore(&mut loaded, &saved).unwrap();
        loaded.execute(after);
        assert!(up(&loaded), "the {name} not up, loaded: {:#?}", loaded.memory.text());
        assert_eq!((loaded.pc, loaded.clock), (cpu.pc, cpu.clock), "the {name}: not where it went on to");
        assert!(loaded.memory.ram == cpu.memory.ram, "the {name}: its RAM not as it went on to");
        assert!(state(&loaded).unwrap() == state(&cpu).unwrap(), "the {name}: not as it went on to");
    }

    /// A II+ with a language card, the clock card, a Mockingboard and a
    /// Disk II, DOS 3.3 in drive 1.
    fn ii_plus() -> Option<CPU<Apple2>> {
        let read = |name: &str| std::fs::read(dir().join(name)).ok();
        let (rom, p5, p6) = (read("Apple2_Plus.rom")?, read("disk2-p5-341-0027.bin")?, read("disk2-p6-341-0028.bin")?);
        let mut disk = Disk::open(&dir().join("disks/Apple DOS 3.3 January 1983.dsk")).ok()?;
        disk.write_protected = true; // the image as it is
        let mut apple = Apple2::new(rom[rom.len() - 0x3000..].to_vec());
        apple.language_card = Some(LanguageCard::new());
        apple.clock = Some(clock());
        apple.mockingboards[0] = Some(Mockingboard::new());
        let mut card = Disk2::new(p5, &p6).unwrap();
        card.drives[0].disk = Some(disk);
        apple.disk2 = Some(card);
        let mut cpu = CPU::with_bus(apple);
        cpu.clock_hz = u32::MAX; // as fast as it goes
        cpu.reset();
        Some(cpu)
    }

    #[test]
    fn a_ii_plus_state_goes_on_as_it_did() {
        // Saved half a second into the boot, the drive turning; on to DOS.
        let up = |cpu: &CPU<Apple2>| cpu.memory.text().iter().any(|line| line.contains("DOS VERSION 3.3"));
        goes_on_as_it_did("II+", ii_plus, CLOCK_HZ / 2, 6 * CLOCK_HZ, up);
        if let Some(mut cpu) = ii_plus() {
            cpu.execute(CLOCK_HZ / 2);
            assert!(cpu.memory.disk2.as_ref().unwrap().motor, "the drive not turning when saved");
        }
    }

    /// An enhanced IIe, its 65C02, with the clock card, two Mockingboards,
    /// a Disk II, Pac-Man's .woz in drive 1, and the hard disk, a ProDOS
    /// game's volume.
    fn enhanced_iie() -> Option<CPU<Apple2>> {
        let read = |name: &str| std::fs::read(dir().join(name)).ok();
        let [cd, ef, characters] = iie::ENHANCED_ROMS.map(read);
        let (p5, p6) = (read("disk2-p5-341-0027.bin")?, read("disk2-p6-341-0028.bin")?);
        let mut disk = Disk::open(&dir().join("disks/Pac-Man (Atarisoft).woz")).ok()?;
        disk.write_protected = true;
        let mut volume = Volume::open(&dir().join("disks/Million.Perfect.Tiles.v1.1.po")).ok()?;
        volume.write_protected = true;
        let mut apple = Apple2::new_iie(&cd?, &ef?, characters?);
        apple.clock = Some(clock());
        apple.mockingboards = [Some(Mockingboard::new()), Some(Mockingboard::new())];
        let mut card = Disk2::new(p5, &p6).unwrap();
        card.drives[0].disk = Some(disk);
        apple.disk2 = Some(card);
        let mut hard_disk = HardDisk::new();
        hard_disk.units[0] = Some(volume);
        apple.hard_disk = Some(hard_disk);
        let mut cpu = CPU::with_bus(apple);
        cpu.variant = crate::cpu::Variant::Synertek65C02;
        cpu.clock_hz = u32::MAX;
        cpu.reset();
        Some(cpu)
    }

    #[test]
    fn a_iie_state_goes_on_as_it_did() {
        // Saved as ProDOS loads, on into the game's high-res.
        let game = |cpu: &CPU<Apple2>| cpu.memory.graphics && cpu.memory.hires;
        goes_on_as_it_did("IIe", enhanced_iie, CLOCK_HZ / 2, 6 * CLOCK_HZ, game);
    }
}
