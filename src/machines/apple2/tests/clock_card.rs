//! The clock card: its firmware's signature and entries, as ProDOS looks
//! for them; its text of the time; its firmware called, as ProDOS calls
//! it, the text in the input buffer; and ProDOS's own clock driver taking
//! the date and time from it, on a II+, with a ProDOS disk in roms/apple2/
//! (not part of the repository; without it the test says so and passes).

#[cfg(test)]
mod tests {
    use crate::cpu::CPU;
    use crate::cpu::bus::Bus;
    use crate::host::clock::DateTime;
    use crate::machines::apple2::clock_card::{ClockCard, FIRMWARE};
    use crate::machines::apple2::disk_image::Disk;
    use crate::machines::apple2::disk2::Disk2;
    use crate::machines::apple2::language_card::LanguageCard;
    use crate::machines::apple2::{Apple2, CLOCK_HZ};
    use std::path::Path;

    /// Saturday the 31st of October 2026, 14:30:05.
    const HALLOWEEN: DateTime = DateTime {
        year: 2026,
        month: 10,
        day: 31,
        weekday: 6,
        hour: 14,
        minute: 30,
        second: 5,
    };

    fn card(time: DateTime) -> ClockCard {
        let mut card = ClockCard::new();
        card.fixed = Some(time);
        card
    }

    #[test]
    fn its_signature_and_its_text() {
        assert_eq!(
            [FIRMWARE[0], FIRMWARE[2], FIRMWARE[4], FIRMWARE[6]],
            [0x08, 0x28, 0x58, 0x70],
            "the signature"
        );
        let mut card = card(HALLOWEEN);
        card.io(0xC090, Some(0));
        let text: Vec<u8> = (0..20)
            .map(|_| card.io(0xC091, None).unwrap() & 0x7F)
            .collect();
        assert_eq!(&text[..18], b"10,06,31,14,30,05\r", "the time's text");
        assert!(
            text[18..].iter().all(|&byte| byte == b'\r'),
            "past its end, its return"
        );
    }

    #[test]
    fn its_firmware_reads_the_time_into_the_input_buffer() {
        let mut apple = Apple2::new(vec![0xEA; 0x3000]);
        apple.clock = Some(card(HALLOWEEN));
        let mut cpu = CPU::with_bus(apple);
        // Called at $C108, X its $Cn, as ProDOS calls it; back to $1000.
        cpu.sp = 0xFF;
        cpu.push_word(0x0FFF);
        cpu.x = 0xC1;
        cpu.pc = 0xC108;
        while cpu.pc != 0x1000 && cpu.clock < 10_000 {
            cpu.execute(1);
        }
        assert_eq!(cpu.pc, 0x1000, "not back");
        let text: Vec<u8> = cpu.memory.ram[0x0200..0x0212]
            .iter()
            .map(|byte| byte & 0x7F)
            .collect();
        assert_eq!(&text, b"10,06,31,14,30,05\r", "the input buffer");
        assert!(
            cpu.memory.ram[0x0200..0x0212]
                .iter()
                .all(|byte| byte & 0x80 != 0),
            "bit 7 not set"
        );
    }

    #[test]
    fn prodos_has_the_date_and_time_from_it() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("roms/apple2");
        let read = |name: &str| std::fs::read(dir.join(name)).ok();
        let (Some(rom), Some(p5), Some(p6), Some(prodos)) = (
            read("Apple2_Plus.rom"),
            read("disk2-p5-341-0027.bin"),
            read("disk2-p6-341-0028.bin"),
            read("disks/Million.Perfect.Tiles.v1.1.po"),
        ) else {
            eprintln!("clock test: no II+ ROM, Disk II ROMs or ProDOS disk in roms/apple2/");
            return;
        };
        // ProDOS 2.4 works the year out from 2023 to 2028.
        let leap_day = DateTime {
            year: 2024,
            month: 2,
            day: 29,
            weekday: 4,
            hour: 9,
            minute: 5,
            second: 0,
        };
        for time in [HALLOWEEN, leap_day] {
            let mut drive = Disk2::new(p5.clone(), &p6).unwrap();
            drive.drives[0].disk = Some(Disk::from_po(&prodos).unwrap());
            let mut apple = Apple2::new(rom.clone());
            apple.language_card = Some(LanguageCard::new());
            apple.disk2 = Some(drive);
            apple.clock = Some(card(time));
            let mut cpu = CPU::with_bus(apple);
            cpu.clock_hz = u32::MAX; // as fast as it goes
            cpu.reset();
            while !cpu
                .memory
                .text()
                .iter()
                .any(|line| line.contains("PRODOS 8"))
                && cpu.clock < 10 * CLOCK_HZ
            {
                cpu.execute(100_000);
            }
            cpu.execute(CLOCK_HZ);
            assert_eq!(
                cpu.memory.ram[0xBF06], 0x4C,
                "ProDOS's clock call not a JMP: the card not found"
            );

            // Its driver, in the language card's bank 1, called through it.
            cpu.memory.read(0xC08B);
            cpu.memory.read(0xC08B);
            cpu.sp = 0xF0;
            cpu.push_word(0x0FFF);
            cpu.pc = 0xBF06;
            let until = cpu.clock + 100_000;
            while cpu.pc != 0x1000 && cpu.clock < until {
                cpu.execute(1);
            }
            assert_eq!(cpu.pc, 0x1000, "the driver not back");
            let ram = &cpu.memory.ram;
            let date = u16::from_le_bytes([ram[0xBF90], ram[0xBF91]]);
            let (year, month, day) = (date >> 9, (date >> 5) & 0x0F, date & 0x1F);
            assert_eq!(
                (year as i64 + 2000, month as u32, day as u32),
                (time.year, time.month, time.day),
                "ProDOS's date"
            );
            assert_eq!(
                (ram[0xBF93] as u32, ram[0xBF92] as u32),
                (time.hour, time.minute),
                "ProDOS's time"
            );
        }
    }
}
