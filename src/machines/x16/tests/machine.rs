//! The X16 as a whole: its memory map and banks, its vectors from ROM bank
//! 0, its slow accesses, VIA 1's wiring to the SNES controllers and the IEC
//! bus, the SMC's reset and power off, its keyboard's keys; and with its
//! ROM (roms/x16/, a release's rom.bin; without one those tests say so and
//! pass), booted to BASIC, typed to, its mouse pointer shown, and a
//! program saved to its SD card and loaded back (with mtools, to make the
//! card).

#[cfg(test)]
mod tests {
    use crate::cpu::CPU;
    use crate::cpu::bus::Bus;
    use crate::host::display::{Gamepad, Key, Pointer};
    use crate::host::machine::{Input, Machine};
    use crate::machines::x16::i2c::Asked;
    use crate::machines::x16::{self, X16, keyboard};
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
    fn pasted_capitals_are_letters_but_in_mixed_case_shifted() {
        // Its codes, in the SMC and waiting for room there.
        let typed = |text: &str| {
            let mut x16 = x16();
            x16.type_text(text);
            let codes: Vec<u8> = x16
                .i2c
                .smc
                .keys
                .iter()
                .chain(&x16.typing)
                .copied()
                .collect();
            codes
        };
        let codes = |text: &str| -> Vec<u8> {
            text.bytes()
                .flat_map(|c| keyboard::codes(Key::Char(c)))
                .collect()
        };
        assert_eq!(
            typed("LOAD\"A\""),
            codes("load\"a\""),
            "no lower case: unshifted, letters"
        );
        assert_eq!(typed("Hi"), codes("Hi"), "as written");
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
}
