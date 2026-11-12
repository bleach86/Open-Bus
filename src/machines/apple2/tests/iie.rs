//! The Apple IIe: its MMU's banking of its 128K, its own ROM in the slots'
//! places, its language card's two 16Ks, RESET; its keyboard at $C010 and
//! its switches read back; its 80 columns, characters and double graphics;
//! its floating bus. Those with a ROM and characters of the test's own,
//! made to tell each place from another. Then, with its own ROMs and the
//! System Master and AppleWorks in roms/apple2/ (not part of the
//! repository; without them they say so and pass): the self-tests the
//! IIe's firmware has, enhanced and first; DOS 3.3 in 80 columns; and
//! AppleWorks to its main menu, in 128K.

#[cfg(test)]
mod tests {
    use crate::cpu::bus::Bus;
    use crate::cpu::{CPU, Variant};
    use crate::host::display::Joystick;
    use crate::host::machine::{Input, Machine};
    use crate::machines::apple2::disk_image::Disk;
    use crate::machines::apple2::disk2::Disk2;
    use crate::machines::apple2::iie::{ENHANCED_ROMS, ROMS};
    use crate::machines::apple2::video::{self, LORES, Monitor, PICTURE_WIDTH, Source, Switches};
    use crate::machines::apple2::{Apple2, CLOCK_HZ, CYCLES_PER_FRAME, CYCLES_PER_LINE};
    use std::path::{Path, PathBuf};

    /// A character set of the test's: each character's every row its own
    /// code's low 6 bits lit, and its top bit as the 7th (stored inverted,
    /// as the video ROM is).
    fn characters() -> Vec<u8> {
        (0..0x1000)
            .map(|at| (at / 8) as u8)
            .map(|code| !((code & 0x3F) | (code >> 7) << 6))
            .collect()
    }

    /// A IIe with a ROM of the test's: its own ROM's $C100 to $CFFF each its
    /// page's number; $D000 up, $D0 and so on likewise.
    fn machine() -> Apple2 {
        let cd: Vec<u8> = (0..0x2000).map(|at| (0xC0 + at / 0x100) as u8).collect();
        let ef: Vec<u8> = (0..0x2000).map(|at| (0xE0 + at / 0x100) as u8).collect();
        Apple2::new_iie(&cd, &ef, characters())
    }

    /// The main and auxiliary bytes at `addr`, set apart: main $11, aux $AA.
    fn apart(apple: &mut Apple2, addr: u16) {
        apple.ram[addr as usize] = 0x11;
        apple.iie.as_mut().unwrap().aux[addr as usize] = 0xAA;
    }

    #[test]
    fn the_mmu_banks_its_128k() {
        let mut apple = machine();
        for addr in [0x0080, 0x0180, 0x0480, 0x2080, 0x8000] {
            apart(&mut apple, addr);
        }
        assert_eq!(apple.read(0x8000), 0x11, "main, at power-on");

        // RAMRD and RAMWRT: reads and writes apart.
        apple.write(0xC003, 0); // RAMRDON
        assert_eq!(apple.read(0x8000), 0xAA, "RAMRD: not the auxiliary's");
        assert_eq!(apple.read(0x0080), 0x11, "RAMRD: zero page not main's");
        apple.write(0x8000, 0x22);
        assert_eq!(
            apple.ram[0x8000], 0x22,
            "RAMRD alone: written to the auxiliary"
        );
        apple.write(0xC005, 0); // RAMWRTON
        apple.write(0x8000, 0x33);
        assert_eq!(
            apple.iie.as_ref().unwrap().aux[0x8000],
            0x33,
            "RAMWRT: not written to the auxiliary"
        );
        assert_eq!(
            (apple.read(0xC013) & 0x80, apple.read(0xC014) & 0x80),
            (0x80, 0x80),
            "RAMRD, RAMWRT read back"
        );
        apple.write(0xC002, 0);
        apple.write(0xC004, 0);

        // ALTZP: zero page and the stack.
        apple.write(0xC009, 0);
        assert_eq!(
            (apple.read(0x0080), apple.read(0x0180)),
            (0xAA, 0xAA),
            "ALTZP: not the auxiliary's"
        );
        assert_eq!(apple.read(0x0480), 0x11, "ALTZP: the text page");
        assert_eq!(apple.read(0xC016) & 0x80, 0x80, "ALTZP read back");
        apple.write(0xC008, 0);

        // 80STORE: the text page by PAGE2, and with HIRES high-res's.
        apple.write(0xC001, 0);
        apple.read(0xC055); // PAGE2
        assert_eq!(
            apple.read(0x0480),
            0xAA,
            "80STORE, PAGE2: the text page not the auxiliary's"
        );
        assert_eq!(
            apple.read(0x2080),
            0x11,
            "80STORE, PAGE2, not HIRES: high-res's"
        );
        apple.read(0xC057); // HIRES
        assert_eq!(
            apple.read(0x2080),
            0xAA,
            "80STORE, PAGE2, HIRES: high-res's not the auxiliary's"
        );
        apple.read(0xC054); // PAGE1
        assert_eq!(
            (apple.read(0x0480), apple.read(0x2080)),
            (0x11, 0x11),
            "PAGE1: not main's"
        );
        assert_eq!(apple.read(0xC018) & 0x80, 0x80, "80STORE read back");
        assert!(
            !apple.iie.as_ref().unwrap().shown_page2(true),
            "80STORE: PAGE2 the page shown"
        );
    }

    #[test]
    fn its_own_rom_answers_the_slots_places() {
        let mut apple = machine();
        apple.disk2 = Some(Disk2::new(vec![0x66; 256], &[0; 256]).unwrap());
        assert_eq!(apple.read(0xC600), 0x66, "the Disk II's ROM, in slot 6");
        assert_eq!(apple.read(0xC300), 0xC3, "slot 3: not its own");
        assert_eq!(
            apple.read(0xC800),
            0xC8,
            "$C800 up: its own, after slot 3's"
        );
        apple.read(0xCFFF);
        assert_ne!(apple.read(0xC800), 0xC8, "$C800 up: its own, after $CFFF");

        apple.write(0xC00B, 0); // SLOTC3ROM
        assert_ne!(apple.read(0xC300), 0xC3, "SLOTC3ROM: slot 3 its own");
        assert_ne!(
            apple.read(0xC800),
            0xC8,
            "SLOTC3ROM: $C800 its own, slot 3's read"
        );
        assert_eq!(apple.read(0xC017) & 0x80, 0x80, "SLOTC3ROM read back");

        apple.write(0xC007, 0); // INTCXROM
        for page in 0xC1..=0xCFu16 {
            assert_eq!(
                apple.read(page << 8),
                page as u8,
                "INTCXROM: ${page:02X}00 not its own"
            );
        }
        assert_eq!(apple.read(0xC015) & 0x80, 0x80, "INTCXROM read back");
        apple.write(0xC006, 0);
        assert_eq!(apple.read(0xC600), 0x66, "INTCXROM off: not the Disk II's");
    }

    #[test]
    fn its_language_card_has_two_16ks() {
        let mut apple = machine();
        apple.read(0xC083);
        apple.read(0xC083); // RAM read, written; bank 2
        apple.write(0xD000, 0x11);
        apple.write(0xC009, 0); // ALTZP
        assert_eq!(
            apple.read(0xD000),
            0x00,
            "ALTZP: main's 16K, not the auxiliary's"
        );
        apple.write(0xD000, 0xAA);
        apple.write(0xC008, 0);
        assert_eq!(apple.read(0xD000), 0x11, "main's 16K written with ALTZP");
        assert_eq!(
            (apple.read(0xC011) & 0x80, apple.read(0xC012) & 0x80),
            (0x80, 0x80),
            "bank 2, RAM read, read back"
        );
    }

    #[test]
    fn reset_switches_the_mmu_off() {
        let mut apple = machine();
        for addr in [
            0xC001, 0xC003, 0xC005, 0xC007, 0xC009, 0xC00B, 0xC00D, 0xC00F,
        ] {
            apple.write(addr, 0);
        }
        apple.iie.as_mut().unwrap().reset();
        for addr in 0xC013..=0xC018 {
            assert_eq!(apple.read(addr) & 0x80, 0, "${addr:04X} after RESET");
        }
        assert_eq!(
            (apple.read(0xC01E) & 0x80, apple.read(0xC01F) & 0x80),
            (0, 0),
            "ALTCHARSET, 80COL after RESET"
        );
    }

    #[test]
    fn its_keyboard_at_c010_and_its_switches_read_back() {
        let mut apple = machine();
        apple.press(b'a');
        assert_eq!(apple.read(0xC000), 0xE1, "a, lower case, bit 7 set");
        assert_eq!(apple.read(0xC011) & 0x7F, 0x61, "$C011: the key, too");
        assert_eq!(apple.read(0xC000), 0xE1, "the strobe cleared by $C011");
        apple.input(Input {
            key_down: true,
            ..Input::default()
        });
        assert_eq!(
            apple.read(0xC010),
            0xE1,
            "$C010: a key down, bit 7, and the key"
        );
        assert_eq!(apple.read(0xC000), 0x61, "the strobe not cleared by $C010");
        apple.input(Input::default());
        assert_eq!(apple.read(0xC010), 0x61, "$C010: no key down");

        // TEXT, MIXED, PAGE2, HIRES, ALTCHARSET, 80COL.
        for (on, read_at) in [
            (0xC051, 0xC01A),
            (0xC053, 0xC01B),
            (0xC055, 0xC01C),
            (0xC057, 0xC01D),
            (0xC00F, 0xC01E),
            (0xC00D, 0xC01F),
        ] {
            apple.write(on, 0);
            assert_eq!(
                apple.read(read_at) & 0x80,
                0x80,
                "${read_at:04X} after ${on:04X}"
            );
        }
        apple.write(0xC05E, 0);
        assert!(
            apple.iie.as_ref().unwrap().dhires,
            "$C05E: not double high-res"
        );
        apple.write(0xC05F, 0);
        assert!(
            !apple.iie.as_ref().unwrap().dhires,
            "$C05F: double high-res"
        );
    }

    #[test]
    fn c019_is_low_in_the_vertical_blank() {
        let mut apple = machine();
        let mut lows = Vec::new();
        for cycle in 0..CYCLES_PER_FRAME {
            apple.tick();
            if apple.read(0xC019) & 0x80 == 0 {
                lows.push(cycle / CYCLES_PER_LINE);
            }
        }
        lows.dedup();
        assert_eq!(
            lows,
            (192..262).collect::<Vec<_>>(),
            "low but in lines 192 to 261"
        );
    }

    /// Line `y` of the picture, 560 dots, from `main` and `aux` by `switches`.
    fn picture(main: &[u8], aux: &[u8], switches: Switches, flash_on: bool, y: usize) -> Vec<u32> {
        let chars = characters();
        let source = Source {
            main,
            aux,
            characters: Some(&chars),
        };
        let mut out = vec![0x123456; PICTURE_WIDTH];
        video::picture_line(&source, switches, Monitor::Colour, flash_on, y, &mut out);
        out
    }

    /// The dots lit, as bits, of a line's `count` dots from `x`.
    fn lit(line: &[u32], x: usize, count: usize) -> u32 {
        (0..count).fold(0, |bits, dot| {
            bits | ((line[x + dot] == 0xFFFFFF) as u32) << dot
        })
    }

    #[test]
    fn eighty_columns_each_an_auxiliary_byte_then_a_main() {
        let (mut main, mut aux) = (vec![0; 0xC000], vec![0; 0xC000]);
        aux[0x0400] = 0xC1;
        main[0x0400] = 0xC2;
        aux[0x0401] = 0xC3;
        let text80 = Switches {
            iie: true,
            col80: true,
            ..Switches::default()
        };
        let line = picture(&main, &aux, text80, false, 0);
        assert_eq!(
            (lit(&line, 0, 7), lit(&line, 7, 7), lit(&line, 14, 7)),
            (0x41, 0x42, 0x43),
            "aux, main, aux"
        );

        // 40 columns: main's alone, each dot twice.
        let text40 = Switches {
            iie: true,
            ..Switches::default()
        };
        let line = picture(&main, &aux, text40, false, 0);
        assert_eq!(
            lit(&line, 0, 14),
            0b11_0000_0000_1100,
            "$C2's dots, 1 and 6, each twice"
        );
    }

    #[test]
    fn the_primary_set_flashes_where_the_alternate_doesnt() {
        let (mut main, aux) = (vec![0; 0xC000], vec![0; 0xC000]);
        main[0x0400] = 0x41;
        let primary = Switches {
            iie: true,
            ..Switches::default()
        };
        let alternate = Switches {
            altcharset: true,
            ..primary
        };
        // The primary's $41: its inverse character ($01) then its normal ($81).
        assert_eq!(
            lit(&picture(&main, &aux, primary, true, 0), 0, 14),
            0b11,
            "the flash on: not $01"
        );
        assert_eq!(
            lit(&picture(&main, &aux, primary, false, 0), 0, 14),
            0b11_0000_0000_0011,
            "the flash off: not $81"
        );
        // The alternate's $41: its own, flash or not.
        assert_eq!(
            lit(&picture(&main, &aux, alternate, true, 0), 0, 14),
            0b11,
            "the alternate set's $41"
        );
        assert_eq!(
            lit(&picture(&main, &aux, alternate, false, 0), 0, 14),
            0b11,
            "the alternate set flashing"
        );
    }

    #[test]
    fn double_graphics() {
        let (mut main, mut aux) = (vec![0; 0xC000], vec![0; 0xC000]);
        // Double low-res: an auxiliary block then a main, the auxiliary's
        // nibble rotated left.
        aux[0x0400] = 0x01;
        main[0x0400] = 0x01;
        let dlores = Switches {
            graphics: true,
            iie: true,
            col80: true,
            dhires: true,
            ..Switches::default()
        };
        let line = picture(&main, &aux, dlores, false, 0);
        assert_eq!(
            (line[0], line[7]),
            (LORES[0x2], LORES[0x1]),
            "auxiliary, then main"
        );

        // Double high-res: each dot the colour of the 4 around it, each
        // its place in the cycle's bit.
        aux[0x2000] = 0x7F;
        main[0x2000] = 0x01; // the 7 auxiliary dots, then one main: 8 lit
        aux[0x2001] = 0x08; // dot 3 of the third byte: dot 17
        let dhires = Switches {
            hires: true,
            ..dlores
        };
        let line = picture(&main, &aux, dhires, false, 0);
        assert_eq!(
            (line[2], line[4], line[10]),
            (LORES[0xF], LORES[0xF], LORES[0x0]),
            "white, white, then black"
        );
        assert_eq!(line[0], LORES[0x7], "the line's edge: dots 0 to 2, fringed");
        assert_eq!(
            line[8], LORES[0x8],
            "past the 8 lit: dot 7 alone, bit 3, fringed"
        );
        assert_eq!(
            (line[16], line[18]),
            (LORES[0x2], LORES[0x2]),
            "dot 17 alone: bit 1, around it"
        );
    }

    #[test]
    fn its_floating_bus_hasnt_the_ii_pluses_1000() {
        let switches = Switches {
            iie: true,
            ..Switches::default()
        };
        assert_eq!(
            video::scanner_address(switches, 0, 1),
            0x0468,
            "line 0, blanked"
        );
    }

    fn dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("roms/apple2")
    }

    /// A IIe from its own ROMs, enhanced (its 65C02) or the first (its
    /// 6502), reset with the Solid-Apple key held if `self_test`; None
    /// without them.
    fn iie(enhanced: bool, self_test: bool) -> Option<CPU<Apple2>> {
        let names = if enhanced { ENHANCED_ROMS } else { ROMS };
        let [cd, ef, characters] = names.map(|name| std::fs::read(dir().join(name)).ok());
        let apple = Apple2::new_iie(&cd?, &ef?, characters?);
        let mut cpu = CPU::with_bus(apple);
        cpu.variant = if enhanced {
            Variant::Synertek65C02
        } else {
            Variant::Nmos6502
        };
        cpu.clock_hz = u32::MAX; // as fast as it goes
        // The Solid-Apple key held if `self_test`, for the self-test.
        let joystick = Joystick {
            buttons: [false, self_test, false],
            ..Joystick::CENTRED
        };
        cpu.memory.input(Input {
            joystick,
            ..Input::default()
        });
        cpu.memory.reset_button(true);
        cpu.reset();
        cpu.execute(3 * CYCLES_PER_FRAME); // RESET held, as the key is
        cpu.memory.reset_button(false);
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

    #[test]
    fn the_iies_pass_their_self_tests() {
        for (enhanced, passed) in [(true, "System OK"), (false, "KERNEL OK")] {
            let Some(mut cpu) = iie(enhanced, true) else {
                eprintln!("IIe test: no IIe ROMs in roms/apple2/");
                return;
            };
            let model = if enhanced {
                "the enhanced IIe"
            } else {
                "the first IIe"
            };
            assert!(
                run_until(&mut cpu, passed, 60),
                "{model}: no {passed}: {:#?}",
                cpu.memory.text()
            );
        }
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

    #[test]
    fn a_basic_program_pasted_runs_and_its_answer_is_copied() {
        let Some(mut cpu) = iie(true, false) else {
            eprintln!("IIe test: no IIe ROMs in roms/apple2/");
            return;
        };
        assert!(run_until(&mut cpu, "]", 10), "{:#?}", cpu.memory.text());
        // Typed as fast as BASIC takes it, no faster: all of it there.
        cpu.memory
            .type_text("10 FOR I = 1 TO 3\n20 PRINT I * 7\n30 NEXT\nLIST\nRUN\n");
        assert!(run_until(&mut cpu, "21", 10), "{:#?}", cpu.memory.text());
        let copied = cpu.memory.screen_text().unwrap();
        let lines: Vec<&str> = copied.lines().collect();
        // As LIST has them, a space before each.
        for line in [" 10  FOR I = 1 TO 3", " 20  PRINT I * 7", " 30  NEXT"] {
            assert!(lines.contains(&line), "{line}: {copied}");
        }
        let ran = lines
            .iter()
            .position(|&line| line == "]RUN")
            .expect(&copied);
        assert_eq!(&lines[ran + 1..ran + 4], ["7", "14", "21"], "{copied}");
    }

    /// The enhanced IIe, with the Disk II, `disk` in drive 1, reset; None
    /// without the ROMs or the disk.
    fn booting(disk: &str) -> Option<CPU<Apple2>> {
        let read = |name: &str| std::fs::read(dir().join(name)).ok();
        let mut card = Disk2::new(
            read("disk2-p5-341-0027.bin")?,
            &read("disk2-p6-341-0028.bin")?,
        )
        .unwrap();
        card.drives[0].disk = Some(Disk::open(&dir().join(disk)).ok()?);
        let mut cpu = iie(true, false)?;
        cpu.memory.disk2 = Some(card);
        cpu.reset();
        Some(cpu)
    }

    #[test]
    fn dos_3_3_boots_and_catalogs_in_80_columns() {
        let Some(mut cpu) = booting("disks/Apple DOS 3.3 January 1983.dsk") else {
            eprintln!("IIe test: no IIe ROMs, Disk II ROMs or System Master in roms/apple2/");
            return;
        };
        assert!(
            run_until(&mut cpu, "DOS VERSION 3.3", 10),
            "DOS 3.3 not up: {:#?}",
            cpu.memory.text()
        );
        cpu.execute(CLOCK_HZ);
        type_in(&mut cpu, b"PR#3\rCATALOG\r");
        assert!(
            run_until(&mut cpu, "A 003 HELLO", 5),
            "no catalog: {:#?}",
            cpu.memory.text()
        );
        assert!(cpu.memory.iie.as_ref().unwrap().col80, "not 80 columns");
        let text = cpu.memory.text();
        assert!(
            text.iter().all(|line| line.len() == 80),
            "lines not 80 columns"
        );
        assert!(
            text.iter().any(|line| line.contains("DISK VOLUME 254")),
            "the catalog's head: {text:#?}"
        );
    }

    #[test]
    fn appleworks_to_its_main_menu() {
        let Some(mut cpu) = booting("disks/AppleWorks/AppleWorks 2.0 Boot Disk.po") else {
            eprintln!("IIe test: no IIe ROMs, Disk II ROMs or AppleWorks in roms/apple2/");
            return;
        };
        let Ok(program) =
            Disk::open(&dir().join("disks/AppleWorks/AppleWorks 2.0 Program Disk.po"))
        else {
            return;
        };
        let asked = "Place the AppleWorks PROGRAM disk in Drive 1 and press Return.";
        assert!(
            run_until(&mut cpu, asked, 20),
            "the program disk not asked for: {:#?}",
            cpu.memory.text()
        );
        // 128K: the desktop the auxiliary 64K makes.
        assert!(
            cpu.memory
                .text()
                .iter()
                .any(|line| line.contains("56K Avail.")),
            "not 56K: {:#?}",
            cpu.memory.text()
        );
        cpu.memory.disk2.as_mut().unwrap().drives[0].disk = Some(program);
        type_in(&mut cpu, b"\r");
        // Today's date asked: its own taken.
        assert!(
            run_until(&mut cpu, "Type today's date or press Return", 20),
            "no date asked: {:#?}",
            cpu.memory.text()
        );
        type_in(&mut cpu, b"\r");
        assert!(
            run_until(&mut cpu, "Main Menu", 10),
            "no main menu: {:#?}",
            cpu.memory.text()
        );
    }
}
