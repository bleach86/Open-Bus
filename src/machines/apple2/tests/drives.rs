//! The Disk II's two drives: drive 2 switched to, alone stepping and
//! turning; the disks to hand changed by F9 and F10, never one in both
//! drives, a written one saved as it's taken out; and DOS 3.3 saving to
//! drive 2's disk, not drive 1's. That last needs the card's ROMs, the II+
//! ROM and the System Master in roms/apple2/ (not part of the
//! repository); without them it says so and passes.

#[cfg(test)]
mod tests {
    use crate::cpu::CPU;
    use crate::host::display::{Display, Key};
    use crate::host::run::Host;
    use crate::machines::apple2::disk2::Disk2;
    use crate::machines::apple2::disk_image::Disk;
    use crate::machines::apple2::video::{HEIGHT, PICTURE_WIDTH};
    use crate::machines::apple2::{Apple2, CLOCK_HZ};
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    fn card() -> Disk2 {
        Disk2::new(vec![0; 256], &[0; 256]).unwrap()
    }

    #[test]
    fn drive_2_alone_steps_and_turns() {
        let mut card = card();
        card.io(0xC0EB, None); // drive 2
        card.io(0xC0E9, None); // the motor on
        for addr in [0xC0E3, 0xC0E0, 0xC0E5, 0xC0E2] {
            card.io(addr, None); // a track in
        }
        assert_eq!(card.drives[1].half_track, 2, "drive 2's head not a track in");
        assert_eq!(card.drives[0].half_track, 0, "drive 1's head moved");
        assert!(card.turning(1) && !card.turning(0), "the wrong drive turning");
        card.io(0xC0EA, None); // drive 1
        assert!(card.turning(0) && !card.turning(1), "drive 1 not turning, switched to");
    }

    /// A window of the test's: function keys pressed, as the test says;
    /// its title, and the last picture it showed, kept.
    struct Keys(Rc<RefCell<(Vec<u8>, String, Vec<u32>)>>);

    impl Display for Keys {
        fn show(&mut self, pixels: &[u32], _: usize, _: usize) {
            self.0.borrow_mut().2 = pixels.to_vec();
        }

        fn key(&mut self) -> Option<Key> {
            None
        }

        fn function_key(&mut self) -> Option<u8> {
            let keys = &mut self.0.borrow_mut().0;
            (!keys.is_empty()).then(|| keys.remove(0))
        }

        fn title(&mut self, title: &str) {
            self.0.borrow_mut().1 = title.to_string();
        }
    }

    /// An image file in the temporary directory, `name` for this test run.
    fn image(name: &str, fill: u8) -> PathBuf {
        let file = std::env::temp_dir().join(format!("cpu_emu_{}_{name}.dsk", std::process::id()));
        std::fs::write(&file, vec![fill; 143_360]).unwrap();
        file
    }

    #[test]
    fn f9_and_f10_change_the_disks() {
        let paths = [image("a", 1), image("b", 2), image("c", 3)];
        let mut apple = Apple2::new(vec![0; 0x3000]);
        let window = Rc::new(RefCell::new((Vec::new(), String::new(), Vec::new())));
        apple.disk2 = Some(card());
        apple.disks = paths.to_vec();
        apple.change_disk(0);
        let state = std::env::temp_dir().join("cpu_emu_no_state");
        let mut host = Host::new(CPU::with_bus(apple), Box::new(Keys(window.clone())), state);
        // Each key seen as a frame ends.
        let press = |host: &mut Host<Apple2>, key: u8| {
            window.borrow_mut().0.push(key);
            host.frame();
        };
        let in_drives = |host: &Host<Apple2>| host.cpu.memory.in_drive;
        host.frame();
        assert_eq!(in_drives(&host), [Some(0), None]);
        assert!(window.borrow().1.contains("Drive 1: cpu_emu_"), "the title: {}", window.borrow().1);

        // F9: drive 1 through each in turn.
        for want in [1, 2, 0] {
            press(&mut host, 9);
            assert_eq!(in_drives(&host), [Some(want), None], "drive 1, F9");
        }
        // F10: drive 2 through each not in drive 1, then none.
        for want in [Some(1), Some(2), None, Some(1)] {
            press(&mut host, 10);
            assert_eq!(in_drives(&host), [Some(0), want], "drive 2, F10");
        }
        // The disk put in, said over the picture's foot, for 3 seconds.
        let band = || window.borrow().2[(HEIGHT - 12) * PICTURE_WIDTH..].to_vec();
        assert!(band()[..PICTURE_WIDTH].iter().all(|&dot| dot == 0), "no notice's band");
        assert!(band().iter().any(|&dot| dot == 0xFFFFFF), "no notice's text");
        for _ in 0..180 {
            host.frame();
        }
        assert!(host.notice.is_none(), "the notice still there after 3 seconds");

        // Drive 1 skips the one in drive 2.
        press(&mut host, 9);
        assert_eq!(in_drives(&host), [Some(2), Some(1)], "drive 1, past drive 2's");
        let disk = host.cpu.memory.disk2.as_ref().unwrap().drives[0].disk.as_ref().unwrap();
        assert_eq!(disk.path.as_deref(), Some(paths[2].as_path()), "drive 1's disk not read from its image");

        // Written, a disk is saved as it's taken out.
        let saved = image("saved", 0);
        std::fs::remove_file(&saved).unwrap();
        let disk = host.cpu.memory.disk2.as_mut().unwrap().drives[0].disk.as_mut().unwrap();
        disk.path = Some(saved.clone());
        disk.written = true;
        press(&mut host, 9);
        assert!(saved.exists(), "the disk taken out not saved");
        for path in paths.iter().chain([&saved]) {
            std::fs::remove_file(path).unwrap();
        }
    }

    fn dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("roms/apple2")
    }

    /// The keys typed, each when the last has been taken.
    fn type_in(cpu: &mut CPU<Apple2>, keys: &[u8]) {
        for &key in keys {
            while cpu.memory.keyboard & 0x80 != 0 {
                cpu.execute(1_000);
            }
            cpu.memory.press(key);
        }
    }

    /// Run until a line on the screen has `text`, for up to `seconds`.
    fn run_until(cpu: &mut CPU<Apple2>, text: &str, seconds: u32) -> bool {
        let until = cpu.clock + seconds * CLOCK_HZ;
        let shown = |cpu: &CPU<Apple2>| cpu.memory.text().iter().any(|line| line.contains(text));
        while !shown(cpu) && cpu.clock < until {
            cpu.execute(100_000);
        }
        shown(cpu)
    }

    #[test]
    fn dos_saves_to_drive_2s_disk() {
        let read = |name: &str| std::fs::read(dir().join(name)).ok();
        let (Some(rom), Some(p5), Some(p6), Some(master)) = (
            read("Apple2_Plus.rom"),
            read("disk2-p5-341-0027.bin"),
            read("disk2-p6-341-0028.bin"),
            read("disks/Apple DOS 3.3 January 1983.dsk"),
        ) else {
            eprintln!("drives test: no ROMs or System Master in roms/apple2/");
            return;
        };
        let mut card = Disk2::new(p5, &p6).unwrap();
        card.drives[0].disk = Some(Disk::from_dsk(&master).unwrap());
        card.drives[1].disk = Some(Disk::from_dsk(&master).unwrap());
        let mut apple = Apple2::new(rom);
        apple.disk2 = Some(card);
        let mut cpu = CPU::with_bus(apple);
        cpu.clock_hz = u32::MAX; // as fast as it goes
        cpu.reset();
        assert!(run_until(&mut cpu, "DOS VERSION 3.3", 10), "DOS 3.3 not up: {:#?}", cpu.memory.text());
        cpu.execute(CLOCK_HZ); // to the prompt

        // The catalogs a screen and more: a key for the rest.
        type_in(&mut cpu, b"NEW\r10 END\rSAVE TEST,D2\rHOME\rCATALOG,D2\r");
        assert!(run_until(&mut cpu, " BOOT13", 10), "drive 2's catalog: {:#?}", cpu.memory.text());
        type_in(&mut cpu, b" ");
        assert!(run_until(&mut cpu, " TEST", 10), "TEST not on drive 2's disk: {:#?}", cpu.memory.text());
        let card = cpu.memory.disk2.as_ref().unwrap();
        assert!(card.drives[1].disk.as_ref().unwrap().written, "drive 2's disk not written");
        assert!(!card.drives[0].disk.as_ref().unwrap().written, "drive 1's disk written");

        type_in(&mut cpu, b"HOME\rCATALOG,D1\r");
        assert!(run_until(&mut cpu, " BOOT13", 10), "drive 1's catalog: {:#?}", cpu.memory.text());
        type_in(&mut cpu, b" ");
        cpu.execute(CLOCK_HZ);
        assert!(!cpu.memory.text().iter().any(|line| line.contains(" TEST")), "TEST on drive 1's disk");
    }
}
