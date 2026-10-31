//! The Apple II: its memory map, keyboard, soft switches, text screen and
//! frames, on the bus; and the CPU running a ROM of the test's own on it,
//! roms/apple2_test.s (`make -C roms` builds it; without it that test says
//! so and passes). The Apple's own ROM isn't needed, or used.

#[cfg(test)]
mod tests {
    use crate::apple2::{Apple2, CLOCK_HZ, CYCLES_PER_FRAME};
    use crate::bus::Bus;
    use crate::cpu::CPU;
    use std::path::Path;

    /// The machine with a ROM of NOPs, but its first byte $11 and last $22.
    fn machine() -> Apple2 {
        let mut rom = vec![0xEA; 0x3000];
        rom[0] = 0x11;
        rom[0x2FFF] = 0x22;
        Apple2::new(rom)
    }

    #[test]
    fn the_memory_map() {
        let mut apple = machine();
        for addr in [0x0000, 0x1234, 0xBFFF] {
            apple.write(addr, 0x5A);
            assert_eq!(apple.read(addr), 0x5A, "RAM at ${addr:04X}");
        }
        assert_eq!(apple.read(0xD000), 0x11, "ROM at $D000");
        assert_eq!(apple.read(0xFFFF), 0x22, "ROM at $FFFF");
        apple.write(0xD000, 0x00);
        assert_eq!(apple.read(0xD000), 0x11, "ROM written");
        assert_eq!(apple.read(0xC600), 0, "a card where there is none");
    }

    #[test]
    fn the_keyboard_holds_a_key_until_the_strobe_is_cleared() {
        let mut apple = machine();
        assert_eq!(apple.read(0xC000) & 0x80, 0, "a key before one is pressed");
        apple.press(b'A');
        assert_eq!(apple.read(0xC000), 0xC1, "the key, bit 7 set");
        assert_eq!(apple.read(0xC000), 0xC1, "the key, still");
        apple.read(0xC010);
        assert_eq!(apple.read(0xC000), 0x41, "the key, its strobe cleared");
        apple.press(b'B');
        apple.write(0xC010, 0);
        assert_eq!(apple.read(0xC000), 0x42, "the strobe cleared by a write");
    }

    #[test]
    fn the_soft_switches_are_read_or_written() {
        let mut apple = machine();
        apple.read(0xC050);
        apple.write(0xC053, 0);
        apple.read(0xC055);
        apple.read(0xC057);
        assert!(apple.graphics && apple.mixed && apple.page2 && apple.hires);
        apple.write(0xC051, 0);
        apple.read(0xC052);
        apple.read(0xC054);
        apple.write(0xC056, 0);
        assert!(!apple.graphics && !apple.mixed && !apple.page2 && !apple.hires);
    }

    #[test]
    fn the_text_screens_rows_are_interleaved() {
        let mut apple = machine();
        // Rows 0, 1, 8 and 23: $0400, $0480, $0428, $07D0.
        for (addr, c) in [(0x0400, b'A'), (0x0480, b'B'), (0x0428, b'C'), (0x07D0, b'D')] {
            apple.write(addr, c | 0x80);
        }
        apple.write(0x0401, 0x01); // an inverse A
        apple.write(0x0402, 0x60); // a flashing space
        let text = apple.text();
        assert_eq!(&text[0][..3], "AA ");
        assert_eq!(&text[1][..1], "B");
        assert_eq!(&text[8][..1], "C");
        assert_eq!(&text[23][..1], "D");
    }

    #[test]
    fn a_frame_is_17030_cycles() {
        let mut apple = machine();
        for _ in 0..3 * CYCLES_PER_FRAME - 1 {
            apple.tick();
        }
        assert_eq!(apple.frames, 2);
        apple.tick();
        assert_eq!(apple.frames, 3);
    }

    #[test]
    fn the_cpu_runs_a_program_on_it() {
        let roms = Path::new(env!("CARGO_MANIFEST_DIR")).join("roms");
        let Ok(rom) = std::fs::read(roms.join("apple2_test.bin")) else {
            eprintln!("apple2 test: no program; run make -C roms");
            return;
        };
        let mut cpu = CPU::with_bus(Apple2::new(rom));
        cpu.clock_hz = CLOCK_HZ;
        cpu.reset();
        cpu.execute(2_000); // HELLO, then waiting for a key
        assert_eq!(&cpu.memory.text()[0][..5], "HELLO");
        assert!(!cpu.memory.graphics, "graphics before the key");

        cpu.memory.press(b'X');
        cpu.execute(2_000);
        let text = cpu.memory.text();
        assert_eq!(&text[1][..1], "X", "the key on row 1");
        assert_eq!(&text[8][..1], "*", "row 8 marked");
        assert_eq!(cpu.memory.keyboard & 0x80, 0, "the key not taken");
        assert!(cpu.memory.graphics, "graphics not switched on");
        assert_eq!(cpu.memory.frames, 0, "a frame: 4,000 cycles are less");
    }
}
