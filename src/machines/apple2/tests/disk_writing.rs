//! The Disk II writing: the card, through its own sequencer PROM, writing
//! bytes as DOS 3.3 does and reading them back, and not writing a disk
//! write protected; and DOS 3.3 saving a program, and the image saved
//! booting and running it, formatting a blank disk that then boots, and
//! saying a disk is write protected. They need the card's ROMs, the II+
//! ROM and the System Master in roms/apple2/ (not part of the repository);
//! without them they say so and pass.

#[cfg(test)]
mod tests {
    use crate::cpu::CPU;
    use crate::host::files;
    use crate::host::machine::Machine;
    use crate::machines::apple2::disk_image::Disk;
    use crate::machines::apple2::disk2::Disk2;
    use crate::machines::apple2::{Apple2, CLOCK_HZ};
    use std::path::{Path, PathBuf};

    fn dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("roms/apple2")
    }

    /// The card with its PROMs, and `disk` in it; None without them.
    fn card(disk: Disk) -> Option<Disk2> {
        let p5 = std::fs::read(dir().join("disk2-p5-341-0027.bin")).ok()?;
        let p6 = std::fs::read(dir().join("disk2-p6-341-0028.bin")).ok()?;
        let mut card = Disk2::new(p5, &p6).unwrap();
        card.drives[0].disk = Some(disk);
        Some(card)
    }

    /// The System Master, its .dsk; None without it.
    fn system_master() -> Option<Disk> {
        let image = std::fs::read(dir().join("disks/Apple DOS 3.3 January 1983.dsk")).ok()?;
        Some(Disk::from_dsk(&image).unwrap())
    }

    /// `bytes` written as DOS 3.3 writes them, a byte each 32 cycles: Q7
    /// on, then each byte loaded (Q6 on, the byte written) and shifted out
    /// (Q6 off); then Q7 off, reading.
    fn write_bytes(card: &mut Disk2, bytes: &[u8]) {
        card.io(0xC0EF, Some(0xFF));
        for &byte in bytes {
            card.io(0xC0ED, Some(byte));
            for cycle in 0..32 {
                if cycle == 4 {
                    card.io(0xC0EC, None);
                }
                card.tick();
            }
        }
        card.io(0xC0EE, None);
    }

    /// The bytes the card reads in a turn and a bit.
    fn read_bytes(card: &mut Disk2) -> Vec<u8> {
        let (mut bytes, mut last) = (Vec::new(), 0u8);
        for _ in 0..60_000 * 4 {
            card.tick();
            if card.data & 0x80 != 0 && last & 0x80 == 0 {
                bytes.push(card.data);
            }
            last = card.data;
        }
        bytes
    }

    #[test]
    fn the_card_reads_back_what_it_writes() {
        let Some(mut card) = card(Disk::from_dsk(&vec![0; 143_360]).unwrap()) else {
            eprintln!("disk writing test: no card ROMs in roms/apple2/");
            return;
        };
        card.io(0xC0E9, None); // the motor on
        let field = [0xD5, 0xAA, 0xAD, 0x96, 0xFF, 0xB7, 0xE5, 0xDE, 0xAA, 0xEB];
        let mut bytes = vec![0xFF; 20]; // syncs ahead, to read in step
        bytes.extend(field);
        write_bytes(&mut card, &bytes);
        assert!(
            card.drives[0].disk.as_ref().unwrap().written,
            "the disk not marked written"
        );
        let read = read_bytes(&mut card);
        assert!(
            read.windows(field.len()).any(|bytes| bytes == field),
            "{field:02X?} not read back"
        );
    }

    #[test]
    fn a_write_protected_disk_isnt_written() {
        let Some(mut card) = card(Disk::from_dsk(&vec![0; 143_360]).unwrap()) else {
            eprintln!("disk writing test: no card ROMs in roms/apple2/");
            return;
        };
        let sense = |card: &mut Disk2| {
            card.io(0xC0ED, None); // Q6 on, Q7 off: sense write protect
            for _ in 0..8 {
                card.tick();
            }
            let protected = card.data & 0x80 != 0;
            card.io(0xC0EC, None);
            protected
        };
        card.io(0xC0E9, None);
        assert!(
            !sense(&mut card),
            "write protect sensed, the disk not protected"
        );

        card.drives[0].disk.as_mut().unwrap().write_protected = true;
        assert!(sense(&mut card), "write protect not sensed");
        let before = card.drives[0].disk.as_ref().unwrap().tracks.clone();
        write_bytes(&mut card, &[0x96; 100]);
        let disk = card.drives[0].disk.as_ref().unwrap();
        assert!(
            !disk.written && disk.tracks == before,
            "a write protected disk written"
        );
    }

    /// An Apple II+ with the card and `disk`, reset; None without the ROMs.
    fn apple(disk: Disk) -> Option<CPU<Apple2>> {
        let rom = std::fs::read(dir().join("Apple2_Plus.rom")).ok()?;
        let mut apple = Apple2::new(rom);
        apple.disk2 = Some(card(disk)?);
        let mut cpu = CPU::with_bus(apple);
        cpu.clock_hz = u32::MAX; // as fast as it goes
        cpu.reset();
        Some(cpu)
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

    /// Run until the cursor's line (the monitor's CV, at $25) is the
    /// prompt, `]` alone, for up to `seconds`.
    fn run_to_prompt(cpu: &mut CPU<Apple2>, seconds: u32) -> bool {
        let until = cpu.clock + seconds * CLOCK_HZ;
        let prompt = |cpu: &CPU<Apple2>| {
            cpu.memory.text()[(cpu.memory.ram[0x25] as usize).min(23)].trim_end() == "]"
        };
        while !prompt(cpu) && cpu.clock < until {
            cpu.execute(100_000);
        }
        prompt(cpu)
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

    /// DOS 3.3 booted, at its prompt, the screen cleared.
    fn booted(disk: Disk) -> Option<CPU<Apple2>> {
        let mut cpu = apple(disk)?;
        assert!(
            run_until(&mut cpu, "DOS VERSION 3.3", 10),
            "DOS 3.3 not up: {:#?}",
            cpu.memory.text()
        );
        cpu.execute(CLOCK_HZ); // to the prompt
        type_in(&mut cpu, b"HOME\r");
        cpu.execute(CLOCK_HZ / 10);
        Some(cpu)
    }

    #[test]
    fn dos_saves_a_program_and_the_image_runs_it() {
        let Some(mut cpu) = system_master().and_then(booted) else {
            eprintln!("disk writing test: no ROMs or System Master in roms/apple2/");
            return;
        };
        let file = std::env::temp_dir().join(format!("cpu_emu_saved_{}.dsk", std::process::id()));
        cpu.memory.disk2.as_mut().unwrap().drives[0]
            .disk
            .as_mut()
            .unwrap()
            .path = Some(file.clone());
        type_in(
            &mut cpu,
            b"NEW\r10 PRINT \"SAVED ON DISK\"\rSAVE TEST\rCATALOG\r",
        );
        assert!(
            run_until(&mut cpu, " TEST", 10),
            "TEST not in the catalog: {:#?}",
            cpu.memory.text()
        );
        cpu.memory.keep_saved(false);
        files::all_saved();
        assert!(!file.exists(), "saved while the disk was still turning");
        // Saved at the frame's end after the drive stops.
        while cpu.memory.disk2.as_ref().unwrap().spinning() {
            cpu.execute(100_000);
        }
        cpu.memory.keep_saved(false);
        files::all_saved();

        let image = std::fs::read(&file).expect("the image not saved");
        std::fs::remove_file(&file).unwrap();
        let mut cpu = booted(Disk::from_dsk(&image).unwrap()).unwrap();
        type_in(&mut cpu, b"RUN TEST\r");
        assert!(
            run_until(&mut cpu, "SAVED ON DISK", 5),
            "TEST didn't run: {:#?}",
            cpu.memory.text()
        );
    }

    #[test]
    fn dos_formats_a_blank_disk_that_then_boots() {
        let Some(mut cpu) = system_master().and_then(booted) else {
            eprintln!("disk writing test: no ROMs or System Master in roms/apple2/");
            return;
        };
        type_in(&mut cpu, b"NEW\r10 PRINT \"FORMATTED BY INIT\"\r");
        cpu.execute(CLOCK_HZ / 10);
        // A disk never formatted: no flux changes, anywhere.
        let mut blank = Disk::from_dsk(&vec![0; 143_360]).unwrap();
        blank.tracks.iter_mut().for_each(|track| track.fill(0));
        cpu.memory.disk2.as_mut().unwrap().drives[0].disk = Some(blank);
        type_in(&mut cpu, b"INIT HELLO\r");
        cpu.execute(CLOCK_HZ);
        assert!(
            run_to_prompt(&mut cpu, 60),
            "INIT didn't finish: {:#?}",
            cpu.memory.text()
        );
        while cpu.memory.disk2.as_ref().unwrap().spinning() {
            cpu.execute(100_000);
        }

        let image = cpu.memory.disk2.as_ref().unwrap().drives[0]
            .disk
            .as_ref()
            .unwrap()
            .to_image();
        let image =
            image.unwrap_or_else(|e| panic!("the formatted disk doesn't read as a .dsk: {e}"));
        let mut cpu = apple(Disk::from_dsk(&image).unwrap()).unwrap();
        assert!(
            run_until(&mut cpu, "FORMATTED BY INIT", 10),
            "it didn't boot: {:#?}",
            cpu.memory.text()
        );
    }

    #[test]
    fn dos_says_a_disk_is_write_protected() {
        let Some(mut cpu) = system_master().and_then(booted) else {
            eprintln!("disk writing test: no ROMs or System Master in roms/apple2/");
            return;
        };
        cpu.memory.disk2.as_mut().unwrap().drives[0]
            .disk
            .as_mut()
            .unwrap()
            .write_protected = true;
        type_in(&mut cpu, b"NEW\r10 END\rSAVE TEST\r");
        assert!(
            run_until(&mut cpu, "WRITE PROTECTED", 10),
            "no WRITE PROTECTED: {:#?}",
            cpu.memory.text()
        );
        assert!(
            !cpu.memory.disk2.as_ref().unwrap().drives[0]
                .disk
                .as_ref()
                .unwrap()
                .written,
            "written"
        );
    }
}
