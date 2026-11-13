//! The 7800 as a whole: its memory map and its mirrors, INPTCTRL, its time
//! (a frame's on the CPU's clock however much MARIA's DMA has of it, WSYNC,
//! the RIOT's slow cycles), MSTAT, the buttons, and the DLI. Its programs
//! are roms/atari7800_test.s (`make -C roms` builds it; without it those
//! tests say so and pass).

#[cfg(test)]
mod tests {
    use crate::cpu::bus::Bus;
    use crate::cpu::{CPU, Variant};
    use crate::host::display::Gamepad;
    use crate::host::machine::{Input, Machine};
    use crate::machines::atari7800::cartridge::{Cartridge, Kind};
    use crate::machines::atari7800::{Atari7800, LINE, NTSC_CLOCK_HZ};
    use crate::machines::region::Region;
    use std::path::Path;

    /// A frame's CPU cycles: 263 lines of 454 of MARIA's clocks, 4 a cycle.
    const FRAME_CYCLES: f64 = 263.0 * LINE as f64 / 4.0;
    const MSTAT: u16 = 0x28;
    const DPPH: u16 = 0x2C;
    const DPPL: u16 = 0x30;
    const CTRL: u16 = 0x3C;
    /// The test's programs, as it numbers them at $2600.
    const COUNTING: u8 = 0;
    const LINES: u8 = 1;
    const READING_THE_RIOT: u8 = 2;
    const READING_RAM: u8 = 3;

    /// A console with `rom`, a 16K cartridge at $C000, reset.
    fn console_of(rom: &[u8]) -> CPU<Atari7800> {
        let cartridge = Cartridge::new(rom, Kind::Plain, None).unwrap();
        let mut cpu = CPU::with_bus(Atari7800::new(cartridge, Region::Ntsc));
        cpu.variant = Variant::Nmos6502;
        cpu.clock_hz = NTSC_CLOCK_HZ;
        cpu.reset();
        cpu
    }

    /// A console with the test's cartridge, to run `program`, its MARIA as
    /// `set_up` sets it; if it's been built.
    fn console(program: u8, set_up: impl FnOnce(&mut Atari7800)) -> Option<CPU<Atari7800>> {
        let rom = Path::new(env!("CARGO_MANIFEST_DIR")).join("roms/atari7800_test.bin");
        let Ok(rom) = std::fs::read(rom) else {
            eprintln!("atari7800 test: no program; run make -C roms");
            return None;
        };
        let mut cpu = console_of(&rom);
        cpu.memory.write(0x2600, program);
        set_up(&mut cpu.memory);
        cpu.reset();
        Some(cpu)
    }

    /// The count at $2400, 16 bits.
    fn count(cpu: &mut CPU<Atari7800>) -> u32 {
        cpu.memory.read(0x2400) as u32 | (cpu.memory.read(0x2401) as u32) << 8
    }

    /// The count a frame, and the CPU's cycles a frame, over 10 after one.
    fn a_frames(cpu: &mut CPU<Atari7800>) -> (u32, f64) {
        cpu.run_frame();
        let (count_from, clock_from) = (count(cpu), cpu.clock);
        for _ in 0..10 {
            cpu.run_frame();
        }
        let counted = (count(cpu) - count_from) / 10;
        (counted, cpu.clock.wrapping_sub(clock_from) as f64 / 10.0)
    }

    /// MARIA on, its DLL at $1800: `zones`, each 3 bytes.
    fn display(atari: &mut Atari7800, zones: &[u8]) {
        for (n, &byte) in zones.iter().enumerate() {
            atari.write(0x1800 + n as u16, byte);
        }
        atari.write(DPPH, 0x18);
        atari.write(DPPL, 0x00);
        atari.write(CTRL, 0x40);
    }

    #[test]
    fn its_ram_and_its_mirrors() {
        let mut cpu = console_of(&[0xEA; 0x4000]);
        let atari = &mut cpu.memory;
        atari.write(0x2040, 0x11);
        atari.write(0x0141, 0x22);
        atari.write(0x1800, 0x33);
        atari.write(0x0480, 0x44);
        assert_eq!(atari.read(0x0040), 0x11, "zero page: $2040");
        assert_eq!(atari.read(0x2141), 0x22, "the stack's page: $2141");
        assert_eq!(atari.read(0x1800), 0x33);
        assert_eq!(atari.read(0x0580), 0x44, "the RIOT's RAM again at $580");
        assert_eq!(atari.read(0xC000), 0xEA, "the cartridge");
    }

    #[test]
    fn inptctrl_locks_and_the_tias_sound_is_written_once_maria_is_on() {
        let mut cpu = console_of(&[0xEA; 0x4000]);
        let atari = &mut cpu.memory;
        let quiet = atari.tia.channels.clone();
        // AUDV0, and INPTCTRL too while it's unlocked: MARIA off, unlocked.
        atari.write(0x19, 0x0E);
        assert_eq!(atari.tia.channels, quiet, "written with MARIA off");
        atari.write(0x01, 0x07); // locked, MARIA on, the BIOS off
        atari.write(0x01, 0x00);
        atari.write(0x19, 0x0F);
        assert_ne!(atari.tia.channels, quiet, "not written");
        assert_eq!(atari.debug()[2], "INPTCTRL 07", "not locked");
    }

    #[test]
    fn a_frame_is_its_cycles_on_the_cpus_clock_however_much_dma_has() {
        let Some(mut cpu) = console(COUNTING, |_| {}) else {
            return;
        };
        let (free, cycles) = a_frames(&mut cpu);
        assert!(
            (cycles - FRAME_CYCLES).abs() < 1.0,
            "{cycles} cycles a frame"
        );

        // MARIA drawing 31 bytes a line, every line: 16 zones of 16 lines,
        // each a DL at $1900 of one object, at $2000.
        let Some(mut cpu) = console(COUNTING, |atari| {
            display(atari, &[0x0F, 0x19, 0x00].repeat(16));
            for (n, byte) in [0x00, 0x01, 0x20, 0x00].into_iter().enumerate() {
                atari.write(0x1900 + n as u16, byte);
            }
        }) else {
            return;
        };
        let (drawing, cycles) = a_frames(&mut cpu);
        assert!(
            (cycles - FRAME_CYCLES).abs() < 1.0,
            "{cycles} cycles a frame, drawing"
        );
        // 242 lines of 16 + 8 + 31 * 3 of MARIA's cycles, a CPU cycle each
        // 4: a quarter of the frame.
        let share = drawing as f64 / free as f64;
        assert!(
            (0.72..0.78).contains(&share),
            "{drawing} of {free} counted, drawing"
        );
    }

    #[test]
    fn wsync_stops_the_cpu_to_the_lines_end() {
        let Some(mut cpu) = console(LINES, |_| {}) else {
            return;
        };
        let (lines, _) = a_frames(&mut cpu);
        assert!((262..=264).contains(&lines), "{lines} lines a frame");
    }

    #[test]
    fn the_riots_cycles_are_slow() {
        let (Some(mut slow), Some(mut fast)) = (
            console(READING_THE_RIOT, |_| {}),
            console(READING_RAM, |_| {}),
        ) else {
            return;
        };
        let (slow, _) = a_frames(&mut slow);
        let (fast, _) = a_frames(&mut fast);
        // 13 cycles a count: 52 of MARIA's clocks, or with the RIOT's 54.
        let ratio = slow as f64 / fast as f64;
        assert!(
            (ratio - 52.0 / 54.0).abs() < 0.005,
            "{slow} counted slow, {fast} fast"
        );
    }

    #[test]
    fn mstat_says_whether_its_the_vertical_blank() {
        let mut cpu = console_of(&[0xEA; 0x4000]);
        let atari = &mut cpu.memory;
        let at_line = |atari: &mut Atari7800, line: u32| {
            while atari.line != line {
                atari.tick();
            }
            atari.read(MSTAT) & 0x80
        };
        assert_eq!(at_line(atari, 15), 0x80);
        assert_eq!(at_line(atari, 16), 0x00, "the picture's first line");
        assert_eq!(at_line(atari, 257), 0x00);
        assert_eq!(at_line(atari, 258), 0x80, "5 lines before the frame's end");
    }

    #[test]
    fn its_buttons_are_two_or_one_as_swchb_drives_them() {
        let mut cpu = console_of(&[0xEA; 0x4000]);
        let atari = &mut cpu.memory;
        let pad = Gamepad {
            a: true,
            ..Gamepad::default()
        };
        atari.input(Input {
            gamepad: pad,
            ..Input::default()
        });
        assert_eq!(atari.read(0x09) & 0x80, 0x80, "button 1, INPT1, high");
        assert_eq!(atari.read(0x08) & 0x80, 0x00, "button 2 not pressed");
        assert_eq!(atari.read(0x0C) & 0x80, 0x80, "INPT4, two buttons: not");
        atari.write(0x0283, 0x04); // SWBCNT: bit 2 an output
        atari.write(0x0282, 0x04); // SWCHB: driven high
        assert_eq!(atari.read(0x0C) & 0x80, 0x00, "one button: INPT4 low");
    }

    #[test]
    fn a_zone_flagged_interrupts_once_a_frame() {
        // Two zones, the second flagged; their DL, at $1900, empty.
        let Some(mut cpu) = console(COUNTING, |atari| {
            display(atari, &[0x00, 0x19, 0x00, 0x80, 0x19, 0x00]);
        }) else {
            return;
        };
        for _ in 0..5 {
            cpu.run_frame();
        }
        let interrupts = cpu.memory.read(0x2410);
        assert!((4..=5).contains(&interrupts), "{interrupts} in 5 frames");
    }

    #[test]
    fn no_dli_is_lost_to_a_slow_cycle_however_its_dma_falls() {
        // 16 zones of 16 lines, each flagged: 16 DLIs a frame, the program
        // reading the RIOT all the while, the DMA's start often in the slow
        // cycle's last two clocks.
        let Some(mut cpu) = console(READING_THE_RIOT, |atari| {
            display(atari, &[0x8F, 0x19, 0x00].repeat(16));
        }) else {
            return;
        };
        for _ in 0..10 {
            cpu.run_frame();
        }
        assert_eq!(cpu.memory.read(0x2410), 160, "DLIs in 10 frames");
    }

    #[test]
    fn a_cartridges_pokey_is_at_4000_0450_or_0800_as_its_put() {
        for (place, random) in [(0x4000, 0x400A), (0x0450, 0x045A), (0x0800, 0x0F0A)] {
            let cartridge = Cartridge::new(&[0xEA; 0x4000], Kind::Plain, Some(place)).unwrap();
            let mut cpu = CPU::with_bus(Atari7800::new(cartridge, Region::Ntsc));
            let atari = &mut cpu.memory;
            atari.write(random | 0x0F, 0x03); // SKCTL: out of reset
            let first = atari.read(random);
            for _ in 0..100 {
                atari.tick();
            }
            assert_ne!(atari.read(random), first, "${place:04X}: RANDOM still");
        }
        // No POKEY: 48K of ROM there.
        let mut cpu = console_of(&[0xEA; 0xC000]);
        assert_eq!(cpu.memory.read(0x400A), 0xEA, "no POKEY: the ROM");
    }
}
