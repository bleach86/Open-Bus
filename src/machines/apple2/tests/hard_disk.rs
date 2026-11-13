//! The hard disk card: its firmware built in, the source's own when cc65
//! is here to check (the clock card's too); its registers, a block read, written (to its image
//! as well) and its errors; its driver, called by the CPU as ProDOS calls
//! it, roms/hard_disk_test.s (`make -C roms` builds it; without it the
//! test says so and passes); and ProDOS booting from it, on a II+, a
//! volume the .po of a ProDOS disk in roms/apple2/ (not part of the
//! repository; without it, likewise).

#[cfg(test)]
mod tests {
    use crate::cpu::CPU;
    use crate::cpu::bus::Bus;
    use crate::machines::apple2::hard_disk::{FIRMWARE, HardDisk, Volume};
    use crate::machines::apple2::{Apple2, CLOCK_HZ};
    use std::path::{Path, PathBuf};

    fn roms() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("roms")
    }

    #[test]
    fn the_cards_firmware_is_its_sources() {
        let cc65 = std::env::var("HOME")
            .map(|home| Path::new(&home).join("cc65/bin"))
            .unwrap_or_default();
        let tool = |name: &str| {
            if cc65.join(name).exists() {
                cc65.join(name)
            } else {
                PathBuf::from(name)
            }
        };
        let out = std::env::temp_dir().join(format!("cpu_emu_firmware_{}", std::process::id()));
        std::fs::create_dir_all(&out).unwrap();
        for (name, firmware) in [
            ("hard_disk", FIRMWARE),
            ("clock", crate::machines::apple2::clock_card::FIRMWARE),
        ] {
            let object = out.join(format!("{name}.o"));
            let assembled = std::process::Command::new(tool("ca65"))
                .args([
                    "-o",
                    object.to_str().unwrap(),
                    roms().join(format!("{name}.s")).to_str().unwrap(),
                ])
                .status();
            if !assembled.is_ok_and(|status| status.success()) {
                eprintln!("hard disk test: no cc65, to check the firmware against its source");
                return;
            }
            let binary = out.join(format!("{name}.bin"));
            let linked = std::process::Command::new(tool("ld65"))
                .args(["-C", roms().join(format!("{name}.cfg")).to_str().unwrap()])
                .args(["-o", binary.to_str().unwrap(), object.to_str().unwrap()])
                .status()
                .unwrap();
            assert!(linked.success(), "ld65, {name}");
            let fresh = std::fs::read(&binary).unwrap();
            assert_eq!(
                &fresh[..],
                &firmware[..],
                "roms/{name}.bin not {name}.s's: run make -C roms and commit it"
            );
        }
        std::fs::remove_dir_all(&out).unwrap();
        // The hard disk's signature, and its last bytes: two volumes, the
        // driver's place.
        assert_eq!(
            [FIRMWARE[1], FIRMWARE[3], FIRMWARE[5], FIRMWARE[7]],
            [0x20, 0x00, 0x03, 0x3C]
        );
        assert_eq!(FIRMWARE[0xFE], 0x1F);
    }

    /// A volume of `blocks` blocks in a temporary image, block n's bytes n.
    fn image(name: &str, blocks: usize) -> PathBuf {
        let file = std::env::temp_dir().join(format!("cpu_emu_{}_{name}.hdv", std::process::id()));
        std::fs::write(
            &file,
            (0..blocks * 512)
                .map(|at| (at / 512) as u8)
                .collect::<Vec<u8>>(),
        )
        .unwrap();
        file
    }

    #[test]
    fn its_registers_read_and_write_a_block() {
        let file = image("registers", 300);
        let mut card = HardDisk::new();
        card.units[0] = Some(Volume::open(&file).unwrap());
        let reg = |card: &mut HardDisk, n: u16, value: u8| card.io(0xC0F0 + n, Some(value));
        let get = |card: &mut HardDisk, n: u16| card.io(0xC0F0 + n, None).unwrap();

        // STATUS: 300 blocks.
        reg(&mut card, 0, 0);
        reg(&mut card, 1, 0x70);
        reg(&mut card, 4, 0);
        assert_eq!(
            (get(&mut card, 4), get(&mut card, 5), get(&mut card, 6)),
            (0, 44, 1),
            "STATUS: 300 blocks, $012C"
        );

        assert!(
            !std::mem::take(&mut card.used),
            "STATUS: no block, no light"
        );

        // READ block 7: its bytes, 7.
        reg(&mut card, 0, 1);
        reg(&mut card, 2, 7);
        reg(&mut card, 3, 0);
        reg(&mut card, 4, 0);
        assert_eq!(get(&mut card, 4), 0, "READ's error");
        assert!(std::mem::take(&mut card.used), "a block read, no light");
        let block: Vec<u8> = (0..512).map(|_| get(&mut card, 8)).collect();
        assert!(block.iter().all(|&byte| byte == 7), "block 7 not read");

        // WRITE block 9: in the volume, and in its image.
        reg(&mut card, 0, 2);
        reg(&mut card, 2, 9);
        for n in 0..512 {
            reg(&mut card, 8, n as u8);
        }
        reg(&mut card, 4, 0);
        assert_eq!(get(&mut card, 4), 0, "WRITE's error");
        let saved = std::fs::read(&file).unwrap();
        assert!(
            saved[9 * 512..10 * 512]
                .iter()
                .enumerate()
                .all(|(n, &byte)| byte == n as u8),
            "block 9 not in the image"
        );
        assert!(
            saved[8 * 512..9 * 512].iter().all(|&byte| byte == 8),
            "block 8 written"
        );

        // The errors: past the volume's end; drive 2, none; write protected.
        reg(&mut card, 0, 1);
        reg(&mut card, 2, 0x2C);
        reg(&mut card, 3, 1); // block 300
        reg(&mut card, 4, 0);
        assert_eq!(get(&mut card, 4), 0x27, "past the end: an I/O error");
        reg(&mut card, 1, 0xF0);
        reg(&mut card, 4, 0);
        assert_eq!(get(&mut card, 4), 0x28, "drive 2: no device");
        reg(&mut card, 1, 0x70);
        card.units[0].as_mut().unwrap().write_protected = true;
        reg(&mut card, 0, 2);
        reg(&mut card, 3, 0);
        reg(&mut card, 4, 0);
        assert_eq!(get(&mut card, 4), 0x2B, "write protected");
        std::fs::remove_file(&file).unwrap();
    }

    #[test]
    fn its_driver_does_as_prodos_asks() {
        let Ok(rom) = std::fs::read(roms().join("hard_disk_test.bin")) else {
            eprintln!("hard disk test: no program; run make -C roms");
            return;
        };
        let file = image("driver", 1000);
        let mut apple = Apple2::new(rom);
        let mut card = HardDisk::new();
        card.units[0] = Some(Volume::open(&file).unwrap());
        apple.hard_disk = Some(card);
        let mut cpu = CPU::with_bus(apple);
        cpu.reset();
        cpu.execute(200_000);

        let ram = &cpu.memory.ram;
        assert_eq!(
            u16::from_le_bytes([ram[0], ram[1]]),
            1000,
            "STATUS's blocks"
        );
        assert_eq!(
            (ram[2], ram[3] & 1),
            (0, 0),
            "WRITE: an error, or the carry set"
        );
        assert_eq!(
            (ram[4], ram[5] & 1),
            (0, 0),
            "READ: an error, or the carry set"
        );
        assert_eq!(
            ram[0x2000..0x2200],
            ram[0x3000..0x3200],
            "the block read back not the one written"
        );
        assert_eq!(
            (ram[6], ram[7] & 1),
            (0x28, 1),
            "drive 2: not no device, the carry set"
        );
        let saved = std::fs::read(&file).unwrap();
        assert_eq!(
            saved[5 * 512..6 * 512],
            ram[0x2000..0x2200],
            "block 5 not in the image"
        );
        std::fs::remove_file(&file).unwrap();
    }

    #[test]
    fn prodos_boots_from_it() {
        let dir = roms().join("apple2");
        let (Ok(rom), Ok(volume)) = (
            std::fs::read(dir.join("Apple2_Plus.rom")),
            Volume::open(&dir.join("disks/Million.Perfect.Tiles.v1.1.po")),
        ) else {
            eprintln!("hard disk test: no II+ ROM or ProDOS disk in roms/apple2/");
            return;
        };
        // Read only, the image as it is.
        let mut volume = volume;
        volume.write_protected = true;
        let mut apple = Apple2::new(rom);
        apple.language_card = Some(crate::machines::apple2::language_card::LanguageCard::new());
        let mut card = HardDisk::new();
        card.units[0] = Some(volume);
        apple.hard_disk = Some(card);
        let mut cpu = CPU::with_bus(apple);
        cpu.clock_hz = u32::MAX; // as fast as it goes
        cpu.reset();
        let up = |cpu: &CPU<Apple2>| {
            cpu.memory
                .text()
                .iter()
                .any(|line| line.contains("PRODOS 8"))
        };
        while !up(&cpu) && cpu.clock < 5 * CLOCK_HZ {
            cpu.execute(100_000);
        }
        assert!(up(&cpu), "ProDOS not up: {:#?}", cpu.memory.text());
        // On into the game, its high-res, loaded from the card alone.
        cpu.execute(5 * CLOCK_HZ);
        assert!(
            cpu.memory.graphics && cpu.memory.hires,
            "the game not up: {:#?}",
            cpu.memory.text()
        );
        assert_eq!(
            cpu.memory.read(0xC7FF),
            FIRMWARE[0xFF],
            "slot 7 not the card's firmware"
        );
    }
}
