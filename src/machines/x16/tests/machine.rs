//! The X16 as a whole: its memory map and banks, its vectors from ROM bank
//! 0, its slow accesses, VIA 1's wiring to the SNES controllers and the IEC
//! bus, the SMC's reset and power off, its keyboard's keys, the host's in
//! their places; and with its ROM (roms/x16/, a release's rom.bin; without
//! one those tests say so and pass), booted to BASIC, typed to, a key held,
//! its mouse pointer shown, its date the host's, a note played on the PSG
//! and on the YM2151, and a program saved to its SD card and loaded back
//! (with mtools, to make the card); VERA's FX, drawing what x16emu R49 does
//! (roms/x16_fx_test.c, as make -C roms builds it). Its clock's RAM kept in
//! a file.

#[cfg(test)]
mod tests {
    use crate::cpu::CPU;
    use crate::cpu::bus::Bus;
    use crate::host::clock;
    use crate::host::display::{Gamepad, Key, Pointer, Scancode};
    use crate::host::files;
    use crate::host::machine::{Input, Machine};
    use crate::host::zip::crc32;
    use crate::machines::x16::i2c::Asked;
    use crate::machines::x16::{self, KEYD, NDX, X16, keyboard};
    use std::path::Path;

    const VIA1_PB: u16 = 0x9F00;
    const VIA1_PA: u16 = 0x9F01;
    const VIA1_DDRB: u16 = 0x9F02;
    const VIA1_DDRA: u16 = 0x9F03;

    /// One with a ROM of two banks, each its number's bytes: $B0, $B1.
    fn x16() -> X16 {
        let mut rom = vec![0xB0; 0x4000];
        rom.extend([0xB1; 0x4000]);
        X16::new(&rom).unwrap()
    }

    /// The X16 with its ROM, if there's one in roms/x16/.
    fn booting() -> Option<CPU<X16>> {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join(x16::ROM_FOLDER);
        let rom = std::fs::read_dir(&folder)
            .ok()?
            .filter_map(|entry| Some(entry.ok()?.path().join("rom.bin")))
            .chain([folder.join("rom.bin")])
            .find(|rom| rom.exists());
        let Some(rom) = rom else {
            eprintln!("x16 test: no ROM; put a release's rom.bin in roms/x16/");
            return None;
        };
        Some(x16::build(&rom.to_string_lossy(), None).unwrap())
    }

    fn frames(cpu: &mut CPU<X16>, frames: usize) {
        pointed(cpu, frames, Pointer::default());
    }

    /// `frames` frames, the host's pointer as `pointer`.
    fn pointed(cpu: &mut CPU<X16>, frames: usize, pointer: Pointer) {
        for _ in 0..frames {
            cpu.memory.input(Input {
                pointer,
                ..Input::default()
            });
            cpu.run_frame();
        }
    }

    #[test]
    fn ram_and_rom_are_banked_as_00_and_01_say() {
        let mut x16 = x16();
        x16.write(0x0000, 1);
        x16.write(0xA000, 0x55);
        x16.write(0x0000, 2);
        assert_eq!(x16.read(0xA000), 0x00);
        x16.write(0x0000, 1);
        assert_eq!(x16.read(0xA000), 0x55);
        x16.write(0x0000, 64);
        x16.write(0xA000, 0x66);
        assert_eq!(x16.read(0xA000), 0xA0, "past its 512K: nothing");
        assert_eq!(x16.read(0xC000), 0xB0, "ROM bank 0");
        x16.write(0x0001, 1);
        assert_eq!(x16.read(0xFFFF), 0xB1);
        x16.write(0xC000, 0x00);
        assert_eq!(x16.read(0xC000), 0xB1, "not written");
        x16.write(0x0001, 2);
        assert_eq!(x16.read(0xC000), 0xFF, "past the ROM's: erased");
        x16.write(0x0001, 32);
        assert_eq!(x16.read(0xC000), 0xC0, "a cartridge's: none");
    }

    #[test]
    fn a_vector_is_read_from_rom_bank_0_its_bank_as_it_was() {
        let mut x16 = x16();
        x16.write(0x0001, 1);
        x16.vector_pull(true);
        assert_eq!(x16.read(0xFFFE), 0xB0);
        x16.vector_pull(false);
        assert_eq!((x16.read(0xFFFE), x16.read(0x0001)), (0xB1, 1));
    }

    #[test]
    fn the_ym2151_and_the_slow_cards_take_3_cycles_more() {
        let mut x16 = x16();
        x16.read(0x9F20);
        assert_eq!(x16.stolen(), 0);
        assert_eq!(x16.read(0x9F41), 0x00, "the YM2151 not busy");
        assert_eq!(x16.stolen(), 3);
        assert_eq!(x16.read(0x9FA0), 0x9F, "nothing there");
        assert_eq!(x16.stolen(), 3);
        assert_eq!(x16.read(0x9F60), 0x9F);
        assert_eq!(x16.stolen(), 0, "at 8 MHz");
    }

    #[test]
    fn the_snes_controllers_are_latched_and_shifted_on_via_1() {
        let mut x16 = x16();
        let held = Gamepad {
            b: true,
            start: true,
            ..Gamepad::default()
        };
        x16.input(Input {
            gamepad: held,
            ..Input::default()
        });
        x16.write(VIA1_DDRA, 0x0C); // the latch and the clock, outputs
        x16.write(VIA1_PA, 0x04);
        x16.write(VIA1_PA, 0x00);
        let mut bits = Vec::new();
        for _ in 0..17 {
            bits.push(x16.read(VIA1_PA) & 0xF0);
            x16.write(VIA1_PA, 0x08);
            x16.write(VIA1_PA, 0x00);
        }
        assert_eq!(bits[0], 0x70, "B held: controller 1's 0, the rest 1");
        assert_eq!(bits[1], 0xF0, "Y");
        assert_eq!(bits[3], 0x70, "Start held");
        assert_eq!(bits[15], 0xF0, "its last 4, 1s");
        assert_eq!(bits[16], 0x30, "then 0s; 3 and 4 not there");
    }

    #[test]
    fn the_iec_bus_reads_low_as_its_pulled_nothing_else_on_it() {
        let mut x16 = x16();
        assert_eq!(x16.read(VIA1_PB) & 0xC0, 0xC0, "let go");
        x16.write(VIA1_DDRB, 0x38);
        x16.write(VIA1_PB, 0x10);
        assert_eq!(x16.read(VIA1_PB) & 0xC0, 0x80, "CLK pulled");
        x16.write(VIA1_PB, 0x20);
        assert_eq!(x16.read(VIA1_PB) & 0xC0, 0x40, "DATA pulled");
    }

    #[test]
    fn the_smc_resets_it_or_holds_it_powered_off_till_reset() {
        let mut x16 = x16();
        x16.write(0x0001, 1);
        x16.i2c.smc.asked = Some(Asked::Reset);
        x16.tick();
        assert!(x16.reset_held());
        x16.tick();
        assert!(!x16.reset_held(), "a cycle of it");
        assert_eq!(x16.read(0x0001), 0, "ROM bank 0");
        x16.i2c.smc.asked = Some(Asked::PowerOff);
        for _ in 0..10 {
            x16.tick();
        }
        assert!(x16.reset_held(), "off");
        x16.reset_button(true);
        x16.reset_button(false);
        assert!(!x16.reset_held(), "on again");
    }

    #[test]
    fn a_character_is_typed_as_its_keys_shift_or_ctrl_around_it() {
        assert_eq!(keyboard::codes(Key::Char(b'a')), [31, 31 | 0x80]);
        assert_eq!(
            keyboard::codes(Key::Char(b'"')),
            [44, 41, 41 | 0x80, 44 | 0x80],
            "Shift and '"
        );
        assert_eq!(
            keyboard::codes(Key::Char(3)),
            [58, 48, 48 | 0x80, 58 | 0x80],
            "Ctrl and C"
        );
        assert_eq!(keyboard::codes(Key::Char(13)), [43, 43 | 0x80], "Return");
        assert_eq!(keyboard::codes(Key::Left), [79, 79 | 0x80]);
        assert!(keyboard::codes(Key::Char(0x7F)).is_empty());
    }

    #[test]
    fn a_host_key_is_the_x16s_in_its_place() {
        let number = |usage| keyboard::number(Scancode(usage));
        assert_eq!([number(0x04), number(0x1D)], [Some(31), Some(46)], "A, Z");
        assert_eq!([number(0x1E), number(0x27)], [Some(2), Some(11)], "1, 0");
        assert_eq!(number(0x3A), Some(112), "F1");
        assert_eq!(
            [number(0xE1), number(0xE5)],
            [Some(44), Some(57)],
            "the Shifts"
        );
        assert_eq!(number(0x46), None, "Print Screen: none");
        assert_eq!(keyboard::change(Scancode(0x04), false), Some(31 | 0x80));
        let mut x16 = x16();
        x16.key_change(Scancode(0x52), true);
        x16.key_change(Scancode(0x52), true);
        x16.key_change(Scancode(0x52), false);
        let keys: Vec<u8> = x16.i2c.smc.keys.iter().copied().collect();
        assert_eq!(keys, [83, 83, 83 | 0x80], "Up, again as it repeats, let go");
    }

    #[test]
    fn pasted_capitals_are_letters_but_in_mixed_case_shifted() {
        // Its PETSCII, waiting for room in the KERNAL's keyboard buffer.
        let pasted = |text: &str| {
            let mut x16 = x16();
            x16.type_text(text);
            x16.pasting.iter().copied().collect::<Vec<u8>>()
        };
        assert_eq!(
            pasted("LOAD\"A\"\r\n"),
            b"LOAD\"A\"\r",
            "no lower case: letters"
        );
        assert_eq!(pasted("Hi\n"), [0xC8, b'I', 13], "as written, H shifted");
    }

    #[test]
    fn pasted_text_goes_in_the_keyboard_buffer_while_interrupts_are_let_in() {
        let mut x16 = x16();
        x16.type_text("print 1234567890\n");
        x16.between_instructions(true);
        assert_eq!(x16.banked[NDX], 0, "masked: none");
        x16.between_instructions(false);
        assert_eq!(x16.banked[NDX], 10, "as many as it holds");
        assert_eq!(&x16.banked[KEYD..KEYD + 10], b"PRINT 1234");
        x16.banked[NDX] = 8;
        x16.between_instructions(false);
        assert_eq!(&x16.banked[KEYD + 8..KEYD + 10], b"56", "as it has room");
    }

    #[test]
    fn a_long_paste_is_taken_by_basic_as_fast_as_its_typed_in_x16emu() {
        let Some(mut cpu) = booting() else { return };
        frames(&mut cpu, 300);
        let program: String = (1..=100)
            .map(|n| format!("{n}0 print \"line {n} of a long program\"\n"))
            .collect();
        cpu.memory.type_text(&format!("{program}run\n"));
        frames(&mut cpu, 600);
        let text = cpu.memory.screen_text().unwrap_or_default();
        assert!(
            cpu.memory.pasting.is_empty(),
            "ten seconds: all of it taken"
        );
        assert!(
            text.contains("LINE 99 OF A LONG PROGRAM\nLINE 100 OF A LONG PROGRAM\n\nREADY."),
            "the screen:\n{text}"
        );
    }

    #[test]
    fn it_boots_to_basic_and_basic_is_typed_to() {
        let Some(mut cpu) = booting() else { return };
        frames(&mut cpu, 300);
        let text = cpu.memory.screen_text().unwrap_or_default();
        assert!(text.contains("COMMANDER X16 BASIC"), "the screen:\n{text}");
        assert!(text.ends_with("READY."), "the screen:\n{text}");
        cpu.memory.type_text("print 6*7\n");
        frames(&mut cpu, 120);
        let text = cpu.memory.screen_text().unwrap_or_default();
        assert!(
            text.ends_with("PRINT 6*7\n 42\n\nREADY."),
            "the screen:\n{text}"
        );
    }

    #[test]
    fn mouse_1_shows_the_kernals_pointer_a_sprite() {
        let Some(mut cpu) = booting() else { return };
        frames(&mut cpu, 300);
        cpu.memory.type_text("mouse 1\n");
        frames(&mut cpu, 120);
        // Sprite 0, at the middle: its tip white, black inside, on blue.
        let picture = cpu.memory.picture().pixels;
        let at = |x: usize, y: usize| picture[y * 640 + x];
        assert_eq!(
            [at(318, 239), at(319, 239), at(320, 240), at(321, 242)],
            [0x0000AA, 0xFFFFFF, 0x000000, 0x000000]
        );
    }

    #[test]
    fn the_hosts_pointer_moves_the_mouse_and_clicks_its_buttons() {
        let Some(mut cpu) = booting() else { return };
        frames(&mut cpu, 300);
        cpu.memory.type_text("mouse 1\n");
        frames(&mut cpu, 120);
        let sprite_0 = |cpu: &CPU<X16>| {
            let byte = |n: usize| cpu.memory.vera.peek(0x1FC00 + n) as u16;
            (byte(2) | byte(3) << 8, byte(4) | byte(5) << 8)
        };
        assert_eq!(sprite_0(&cpu), (319, 239), "the middle");
        let at = |x, y, left| Pointer {
            at: Some((x, y)),
            buttons: [left, false],
        };
        pointed(&mut cpu, 2, at(100.0, 100.0, false));
        assert_eq!(sprite_0(&cpu), (319, 239), "onto the picture: not moved");
        pointed(&mut cpu, 2, at(110.4, 70.0, false));
        assert_eq!(sprite_0(&cpu), (329, 209), "right 10, up 30");
        pointed(&mut cpu, 2, at(700.0, 70.0, false));
        assert_eq!(sprite_0(&cpu).0, 639, "past the edge: at it");
        pointed(&mut cpu, 2, at(700.0, 70.0, true));
        cpu.memory.type_text("print mb\n");
        pointed(&mut cpu, 60, at(700.0, 70.0, true));
        let text = cpu.memory.screen_text().unwrap_or_default();
        assert!(
            text.ends_with(" 1\n\nREADY."),
            "MB, its left button:\n{text}"
        );
    }

    #[test]
    fn a_program_is_saved_to_its_sd_card_and_loaded_back() {
        let Some(rom) = booting().map(|_| x16::rom_file().unwrap()) else {
            return;
        };
        // A new card, as tools/make-x16-sd makes one, if mtools is here.
        let image = std::env::temp_dir().join(format!("cpu_emu_{}_x16_sd.img", std::process::id()));
        let _ = std::fs::remove_file(&image);
        let tool = Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/make-x16-sd");
        let made = std::process::Command::new(tool)
            .args(["--size=34", "-o"])
            .arg(&image)
            .output();
        if !made.is_ok_and(|made| made.status.success()) {
            eprintln!("x16 test: no SD card made; tools/make-x16-sd needs mtools");
            return;
        }
        let mut cpu = x16::build(&rom.to_string_lossy(), image.to_str()).unwrap();
        frames(&mut cpu, 300);
        cpu.memory.type_text("10 print 6*7\nsave\"answer\"\n");
        frames(&mut cpu, 200);
        cpu.memory.type_text("new\nload\"answer\"\nrun\n");
        frames(&mut cpu, 200);
        let text = cpu.memory.screen_text().unwrap_or_default();
        assert!(text.ends_with("RUN\n 42\n\nREADY."), "the screen:\n{text}");
        std::fs::remove_file(&image).unwrap();
    }

    #[test]
    fn its_clock_is_the_hosts_time_as_basic_has_it() {
        let Some(mut cpu) = booting() else { return };
        frames(&mut cpu, 300);
        cpu.memory.type_text("print da$\n");
        frames(&mut cpu, 60);
        let now = clock::now();
        let date = format!("{}{:02}{:02}", now.year, now.month, now.day);
        let text = cpu.memory.screen_text().unwrap_or_default();
        assert!(text.contains(&date), "{date}, the screen:\n{text}");
    }

    #[test]
    fn its_clocks_ram_is_read_from_its_file_and_saved_as_its_written() {
        let file = std::env::temp_dir().join(format!("cpu_emu_{}_nvram.bin", std::process::id()));
        std::fs::write(&file, [0x5A; 64]).unwrap();
        let mut x16 = x16();
        x16.keep_nvram(&file);
        assert_eq!(x16.i2c.rtc.ram, [0x5A; 64]);
        x16.keep_saved(false);
        files::all_saved();
        assert_eq!(std::fs::read(&file).unwrap(), [0x5A; 64], "not written");
        x16.i2c.rtc.ram[3] = 0x33;
        x16.i2c.rtc.ram_written = true;
        x16.keep_saved(false);
        files::all_saved();
        assert_eq!(std::fs::read(&file).unwrap()[3], 0x33);
        std::fs::remove_file(&file).unwrap();
        let mut x16 = self::x16();
        x16.keep_nvram(&file);
        assert_eq!(x16.i2c.rtc.ram, [0; 64], "no file yet: as it was");
    }

    #[test]
    fn a_held_key_is_held_for_the_kernals_keyboard_joystick() {
        let Some(mut cpu) = booting() else { return };
        frames(&mut cpu, 300);
        cpu.memory.type_text(
            "10 if joy(0)<>1 then 10\n20 print joy(0):if joy(0)=1 then 20\n30 print \"up\"\nrun\n",
        );
        // Return, typing RUN, is START for a moment: past that, waiting.
        frames(&mut cpu, 300);
        let text = cpu.memory.screen_text().unwrap_or_default();
        assert!(text.ends_with("RUN"), "waiting for RIGHT:\n{text}");
        let right = Scancode(0x4F);
        cpu.memory.key_change(right, true);
        frames(&mut cpu, 30);
        let text = cpu.memory.screen_text().unwrap_or_default();
        assert!(text.ends_with(" 1"), "RIGHT, 1, held:\n{text}");
        cpu.memory.key_change(right, false);
        frames(&mut cpu, 30);
        let text = cpu.memory.screen_text().unwrap_or_default();
        assert!(text.ends_with(" 1\nUP\n\nREADY."), "let go:\n{text}");
    }

    #[test]
    fn psgnote_plays_its_note_at_its_pitch() {
        let Some(mut cpu) = booting() else { return };
        cpu.memory.sound_on();
        frames(&mut cpu, 300);
        cpu.memory.type_text("psgnote 1,$4a\n");
        frames(&mut cpu, 60);
        cpu.memory.sound().clear();
        frames(&mut cpu, 60);
        // A second's samples, left and right, at 48 kHz: A4, 440 Hz.
        let samples = std::mem::take(cpu.memory.sound());
        assert!(
            (95_000..97_000).contains(&samples.len()),
            "{} samples",
            samples.len()
        );
        let left: Vec<f32> = samples.iter().step_by(2).copied().collect();
        let rises = left
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count();
        assert!((435..=445).contains(&rises), "{rises} cycles in a second");
        let loudest = left
            .iter()
            .fold(0.0f32, |most, level| most.max(level.abs()));
        assert!(loudest > 0.01, "loudest {loudest}");
    }

    #[test]
    fn fmnote_plays_its_note_on_the_ym2151() {
        let Some(mut cpu) = booting() else { return };
        cpu.memory.sound_on();
        frames(&mut cpu, 300);
        cpu.memory.type_text("fmnote 0,$4a\n");
        frames(&mut cpu, 30);
        cpu.memory.sound().clear();
        frames(&mut cpu, 60);
        let samples = std::mem::take(cpu.memory.sound());
        let left: Vec<f32> = samples.iter().step_by(2).copied().collect();
        let loudest = left
            .iter()
            .fold(0.0f32, |most, level| most.max(level.abs()));
        assert!(loudest > 0.01, "loudest {loudest}");
        // Its period, where it's most like itself: A4's, 48,000 / 440.
        let likeness =
            |lag: usize| -> f32 { left.iter().zip(&left[lag..]).map(|(a, b)| a * b).sum() };
        let period = (60..300)
            .max_by(|&a, &b| likeness(a).total_cmp(&likeness(b)))
            .unwrap();
        assert!(
            (108..=110).contains(&period),
            "a period of {period} samples"
        );
    }

    #[test]
    fn a_games_card_made_boots_into_its_program() {
        let Some(rom) = booting().map(|_| x16::rom_file().unwrap()) else {
            return;
        };
        // A game's folder: its program, `10 PRINT"CARD OK"`, and its data.
        let folder = std::env::temp_dir().join(format!("cpu_emu_{}_x16_game", std::process::id()));
        std::fs::create_dir_all(folder.join("data")).unwrap();
        let mut program = vec![0x01, 0x08, 0x10, 0x08, 0x0A, 0x00, 0x99, b'"'];
        program.extend(b"CARD OK\"\0\0\0");
        std::fs::write(folder.join("Hello World.prg"), program).unwrap();
        std::fs::write(folder.join("data/level one.bin"), [0, 0xA0, 1]).unwrap();
        let card = folder.with_extension("img");
        let _ = std::fs::remove_file(&card);
        x16::make_card(&folder, &card).unwrap();
        let mut cpu = x16::build(&rom.to_string_lossy(), card.to_str()).unwrap();
        frames(&mut cpu, 400);
        let text = cpu.memory.screen_text().unwrap_or_default();
        assert!(text.contains("CARD OK\n\nREADY."), "the screen:\n{text}");
        cpu.memory.type_text("dos\"$\"\n");
        frames(&mut cpu, 100);
        let text = cpu.memory.screen_text().unwrap_or_default();
        assert!(
            text.contains("\"HELLO WORLD.PRG\""),
            "its long name:\n{text}"
        );
        // Saved to, and loaded back: written in its 32K clusters.
        cpu.memory
            .type_text("new\n10 print 6*7\nsave\"answer\"\nnew\nload\"answer\"\nrun\n");
        frames(&mut cpu, 300);
        let text = cpu.memory.screen_text().unwrap_or_default();
        assert!(text.ends_with("RUN\n 42\n\nREADY."), "the screen:\n{text}");
        std::fs::remove_dir_all(&folder).unwrap();
        std::fs::remove_file(&card).unwrap();
    }

    #[test]
    fn loud_pcm_is_turned_down_by_the_limiter_not_clipped() {
        let mut x16 = x16();
        x16.sound_on();
        // VERA's PCM, 16 bits mono at full volume, a full square wave:
        // twice full, as VERA's doubled.
        let vera = &mut x16.vera;
        vera.write(0x1B, 0x2F);
        for n in 0..2000 {
            let level: i16 = if n / 20 % 2 == 0 { 32767 } else { -32768 };
            for byte in level.to_le_bytes() {
                vera.write(0x1D, byte);
            }
        }
        vera.write(0x1C, 128);
        for _ in 0..200_000 {
            x16.cycle();
        }
        let samples = std::mem::take(x16.sound());
        let peak = samples
            .iter()
            .fold(0f32, |most, level| most.max(level.abs()));
        assert!(peak <= 0.7 + 1e-3, "past its volume: {peak}");
        assert!(peak > 0.69, "turned down too far: {peak}");
        // Square still: as many dots of it each way.
        let high = samples.iter().filter(|&&level| level > 0.6).count();
        let low = samples.iter().filter(|&&level| level < -0.6).count();
        assert!(
            high > samples.len() / 3 && low > samples.len() / 3,
            "{high} {low} of {}",
            samples.len()
        );
    }

    /// The FX test's regions of video RAM, and their CRC-32s as x16emu R49
    /// leaves them.
    const FX_REGIONS: [(&str, usize, usize, u32); 12] = [
        ("line 8-bit", 0x00000, 0x08000, 0x8846A5A2),
        ("line 4-bit", 0x08000, 0x0C000, 0x13141804),
        ("polygon 8-bit", 0x0C000, 0x12400, 0x5B7F3597),
        ("polygon 4-bit", 0x12400, 0x14000, 0x26D078EC),
        ("affine tiles and map", 0x14000, 0x15000, 0x0B37CCDA),
        ("affine 8-bit wrapping", 0x15000, 0x15600, 0x75A57EFD),
        ("affine 8-bit clipped", 0x15600, 0x15C00, 0xB2CB9197),
        ("affine 4-bit", 0x15C00, 0x16400, 0xF9926578),
        ("cache", 0x16400, 0x16800, 0xD93E9260),
        ("multiplier", 0x16800, 0x17000, 0x3EADC0E4),
        ("16-bit hop", 0x17000, 0x17800, 0xA98FCB62),
        ("reads", 0x17800, 0x18000, 0xE97EAF80),
    ];

    #[test]
    fn vera_fx_draws_what_x16emu_r49_does() {
        let Some(rom) = booting().map(|_| x16::rom_file().unwrap()) else {
            return;
        };
        let program = Path::new(env!("CARGO_MANIFEST_DIR")).join("roms/x16_fx_test.bin");
        let Ok(program) = std::fs::read(program) else {
            eprintln!("x16 FX test: no program; run make -C roms");
            return;
        };
        let card = std::env::temp_dir().join(format!("cpu_emu_{}_x16_fx.img", std::process::id()));
        let files = [("FXTEST.PRG".to_string(), program)];
        std::fs::write(&card, x16::card::card(&files, clock::now()).unwrap()).unwrap();
        let mut cpu = x16::build(&rom.to_string_lossy(), card.to_str()).unwrap();
        frames(&mut cpu, 300);
        cpu.memory.type_text("load\"fxtest.prg\"\nrun\n");
        let done = |cpu: &mut CPU<X16>| {
            let text = cpu.memory.screen_text().unwrap_or_default();
            text.contains("FXTEST DONE 272")
        };
        for _ in 0..30 {
            if done(&mut cpu) {
                break;
            }
            frames(&mut cpu, 60);
        }
        std::fs::remove_file(&card).unwrap();
        assert!(
            done(&mut cpu),
            "the screen:\n{}",
            cpu.memory.screen_text().unwrap_or_default()
        );
        let differing: Vec<&str> = FX_REGIONS
            .iter()
            .filter(|&&(_, from, to, crc)| {
                let drawn: Vec<u8> = (from..to).map(|at| cpu.memory.vera.peek(at)).collect();
                crc32(&drawn) != crc
            })
            .map(|&(name, ..)| name)
            .collect();
        assert!(
            differing.is_empty(),
            "not as x16emu R49 draws them: {differing:?}"
        );
    }
}
