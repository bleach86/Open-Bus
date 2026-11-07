//! The language card: its switches, on the bus, with a ROM of the test's
//! own; and DOS 3.3's System Master loading Integer BASIC into it, INT
//! running it, and Reset back into it. That last needs the II+ ROM, the Disk II's ROMs and the disk
//! in roms/apple2/ (not part of the repository); without them it says so
//! and passes.

#[cfg(test)]
mod tests {
    use crate::cpu::CPU;
    use crate::cpu::bus::Bus;
    use crate::host::machine::Machine;
    use crate::machines::apple2::disk_image::Disk;
    use crate::machines::apple2::disk2::Disk2;
    use crate::machines::apple2::language_card::LanguageCard;
    use crate::machines::apple2::{Apple2, CLOCK_HZ, CYCLES_PER_FRAME};
    use std::path::Path;

    /// The machine with the card, and a ROM of $11s at $D000 and $22s at $E000 up.
    fn machine() -> Apple2 {
        let mut rom = vec![0x22; 0x3000];
        rom[..0x1000].fill(0x11);
        let mut apple = Apple2::new(rom);
        apple.language_card = Some(LanguageCard::new());
        apple
    }

    /// Write enabled, by reading `addr` twice.
    fn enable(apple: &mut Apple2, addr: u16) {
        apple.read(addr);
        apple.read(addr);
    }

    #[test]
    fn at_power_on_the_rom_is_read_and_bank_2_written() {
        let mut apple = machine();
        apple.write(0xD000, 0xAA);
        apple.write(0xE000, 0xBB);
        assert_eq!(apple.read(0xD000), 0x11, "the ROM at $D000");
        assert_eq!(apple.read(0xE000), 0x22, "the ROM at $E000");
        apple.read(0xC080); // the RAM read, bank 2
        assert_eq!(apple.read(0xD000), 0xAA, "bank 2 not written");
        assert_eq!(apple.read(0xE000), 0xBB, "$E000 not written");
    }

    #[test]
    fn each_switch_reads_the_ram_or_the_rom() {
        let mut apple = machine();
        apple.write(0xF000, 0x33); // into the RAM, under the ROM
        for base in [0xC080, 0xC084, 0xC088, 0xC08C] {
            for (switch, ram) in [(0, true), (1, false), (2, false), (3, true)] {
                apple.read(base + switch);
                let expected = if ram { 0x33 } else { 0x22 };
                assert_eq!(apple.read(0xF000), expected, "after ${:04X}", base + switch);
            }
        }
    }

    #[test]
    fn writes_take_two_reads_of_an_odd_switch() {
        let mut apple = machine();
        apple.read(0xC082); // write protected
        apple.write(0xE000, 0x01);
        apple.read(0xC083); // once: the pre-write
        apple.write(0xE000, 0x02);
        assert_eq!(
            apple.read(0xE000),
            0x00,
            "written, protected or after one read"
        );

        apple.read(0xC083); // twice
        apple.write(0xE000, 0x03);
        assert_eq!(apple.read(0xE000), 0x03, "not written after two reads");

        // Another access between, not to a switch, doesn't count.
        apple.read(0xC080);
        apple.read(0xC081);
        apple.read(0x1234);
        apple.read(0xC083);
        apple.write(0xE000, 0x04);
        assert_eq!(
            apple.read(0xE000),
            0x04,
            "not written: a read between broke the pair"
        );

        // Any even switch protects it again.
        apple.read(0xC080);
        apple.write(0xE000, 0x05);
        assert_eq!(apple.read(0xE000), 0x04, "written after $C080");
        apple.write(0xC08A, 0);
        apple.read(0xC083);
        apple.write(0xE000, 0x05);
        assert_eq!(apple.read(0xE000), 0x04, "written after writing $C08A");

        // A write between two reads breaks the pair, and a write doesn't
        // count as the first.
        apple.read(0xC080);
        apple.read(0xC083);
        apple.write(0xC083, 0);
        apple.read(0xC083);
        apple.write(0xE000, 0x06);
        assert_eq!(
            apple.read(0xE000),
            0x04,
            "written: a write between two reads"
        );
        apple.read(0xC083);
        apple.write(0xE000, 0x07);
        assert_eq!(
            apple.read(0xE000),
            0x07,
            "not written after two reads at last"
        );
        apple.read(0xC080);
        apple.write(0xC083, 0);
        apple.read(0xC083);
        apple.write(0xE000, 0x08);
        assert_eq!(apple.read(0xE000), 0x07, "written: a write as the first");

        // Writes on while the ROM is read.
        enable(&mut apple, 0xC081);
        apple.write(0xE000, 0x09);
        assert_eq!(apple.read(0xE000), 0x22, "the RAM read, not the ROM");
        apple.read(0xC080);
        assert_eq!(
            apple.read(0xE000),
            0x09,
            "not written while the ROM was read"
        );
    }

    #[test]
    fn d000_is_either_bank_and_e000_up_both() {
        let mut apple = machine();
        enable(&mut apple, 0xC08B); // bank 1
        apple.write(0xD000, 0x01);
        apple.write(0xDFFF, 0x0F);
        apple.write(0xE000, 0xE1);
        enable(&mut apple, 0xC083); // bank 2
        assert_eq!(apple.read(0xD000), 0x00, "bank 1's $D000 in bank 2");
        apple.write(0xD000, 0x02);
        apple.write(0xDFFF, 0x2F);
        assert_eq!(apple.read(0xE000), 0xE1, "$E000 not the same in bank 2");
        apple.write(0xE000, 0xE2);

        apple.read(0xC088); // bank 1, read
        assert_eq!(apple.read(0xD000), 0x01, "bank 1's $D000");
        assert_eq!(apple.read(0xDFFF), 0x0F, "bank 1's $DFFF");
        assert_eq!(apple.read(0xE000), 0xE2, "$E000 not the same in bank 1");
        apple.read(0xC080); // bank 2, read
        assert_eq!(apple.read(0xD000), 0x02, "bank 2's $D000");
        assert_eq!(apple.read(0xDFFF), 0x2F, "bank 2's $DFFF");
    }

    #[test]
    fn without_the_card_the_switches_do_nothing() {
        let mut apple = machine();
        apple.language_card = None;
        enable(&mut apple, 0xC083);
        apple.write(0xD000, 0xAA);
        assert_eq!(apple.read(0xD000), 0x11, "the ROM at $D000");
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

    /// The screen's line `up` lines above the cursor's (the monitor's CV, at $25).
    fn line(cpu: &CPU<Apple2>, up: usize) -> String {
        let row = (cpu.memory.ram[0x25] as usize).saturating_sub(up);
        cpu.memory.text()[row.min(23)].trim_end().to_string()
    }

    /// Run until the cursor's line is `prompt`, for up to `seconds`.
    fn run_until(cpu: &mut CPU<Apple2>, prompt: &str, seconds: u32) -> bool {
        let until = cpu.clock + seconds * CLOCK_HZ;
        while line(cpu, 0) != prompt && cpu.clock < until {
            cpu.execute(100_000);
        }
        line(cpu, 0) == prompt
    }

    #[test]
    fn dos_loads_integer_basic_into_it() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("roms/apple2");
        let read = |name: &str| std::fs::read(dir.join(name)).ok();
        let (Some(rom), Some(p5), Some(p6), Some(disk)) = (
            read("Apple2_Plus.rom"),
            read("disk2-p5-341-0027.bin"),
            read("disk2-p6-341-0028.bin"),
            read("disks/Apple DOS 3.3 January 1983.dsk"),
        ) else {
            eprintln!("language card test: no II+ ROM, card ROMs or disk in roms/apple2/");
            return;
        };
        let mut card = Disk2::new(p5, &p6).unwrap();
        card.drives[0].disk = Some(Disk::from_dsk(&disk).unwrap());
        let mut apple = Apple2::new(rom);
        apple.language_card = Some(LanguageCard::new());
        apple.disk2 = Some(card);
        let mut cpu = CPU::with_bus(apple);
        cpu.clock_hz = u32::MAX; // as fast as it goes
        cpu.reset();

        // HELLO run, Integer BASIC loaded, and Applesoft's prompt.
        assert!(
            run_until(&mut cpu, "]", 20),
            "no prompt: {:#?}",
            cpu.memory.text()
        );
        type_in(&mut cpu, b"INT\r");
        assert!(
            run_until(&mut cpu, ">", 2),
            "no Integer BASIC: {:#?}",
            cpu.memory.text()
        );
        let card = cpu.memory.language_card.as_ref().unwrap();
        assert!(
            card.read_ram && !card.write_enabled,
            "Integer BASIC not read from the card"
        );

        // Integer BASIC's arithmetic: whole numbers.
        type_in(&mut cpu, b"PRINT 7/2\r");
        assert!(
            run_until(&mut cpu, ">", 1),
            "PRINT 7/2: {:#?}",
            cpu.memory.text()
        );
        let text = cpu.memory.text();
        let typed = text
            .iter()
            .position(|line| line.trim_end() == ">PRINT 7/2")
            .expect("PRINT 7/2 not on the screen");
        assert_eq!(text[typed + 1].trim_end(), "3", "7/2: {text:#?}");

        // Reset: the card reads the ROM, and the ROM's reset goes to DOS's,
        // which switches it back in for Integer BASIC.
        cpu.memory.reset_button(true);
        cpu.execute(2 * CYCLES_PER_FRAME);
        assert!(
            !cpu.memory.language_card.as_ref().unwrap().read_ram,
            "the card not reset"
        );
        cpu.memory.reset_button(false);
        cpu.execute(CYCLES_PER_FRAME);
        assert!(
            run_until(&mut cpu, ">", 1),
            "not back to Integer BASIC: {:#?}",
            cpu.memory.text()
        );
        assert_eq!(
            cpu.memory.ram[0x25],
            23,
            "the prompt not reset's, at the bottom: {:#?}",
            cpu.memory.text()
        );
        assert!(
            cpu.memory.language_card.as_ref().unwrap().read_ram,
            "Integer BASIC not read from the card"
        );

        type_in(&mut cpu, b"FP\r");
        assert!(
            run_until(&mut cpu, "]", 2),
            "not back to Applesoft: {:#?}",
            cpu.memory.text()
        );
        assert!(
            !cpu.memory.language_card.as_ref().unwrap().read_ram,
            "Applesoft not read from the ROM"
        );
    }
}
