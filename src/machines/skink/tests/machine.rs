//! Skink as a whole: its memory map and boot image, its picture, and, run
//! by its test boot image (roms/skink_test.s; `make -C roms` builds it,
//! and without it those tests say so and pass), its frame's interrupt, the
//! VIA's, its keyboard and the ACIA's.

#[cfg(test)]
mod tests {
    use crate::cpu::CPU;
    use crate::cpu::bus::Bus;
    use crate::host::display::Key;
    use crate::host::machine::Machine;
    use crate::machines::skink::{self, CLOCK_HZ, COLOURS, Disk, PALETTE, SCREEN, Skink, WIDTH};
    use std::path::Path;

    /// Skink running the test's boot image, if it's been built.
    fn running() -> Option<CPU<Skink>> {
        let image = Path::new(env!("CARGO_MANIFEST_DIR")).join("roms/skink_test.bin");
        if !image.exists() {
            eprintln!("skink test: no boot image; run make -C roms");
            return None;
        }
        Some(skink::build(&image.to_string_lossy(), None).unwrap())
    }

    /// A 16-bit count at `addr`.
    fn count(cpu: &mut CPU<Skink>, addr: u16) -> u32 {
        cpu.memory.read(addr) as u32 | (cpu.memory.read(addr + 1) as u32) << 8
    }

    #[test]
    fn its_boot_image_is_at_the_top_of_memory_and_io_at_9000() {
        let mut skink = Skink::new(&[0x11, 0x22, 0x33]).unwrap();
        assert_eq!(
            (skink.read(0xFFFD), skink.read(0xFFFF)),
            (0x11, 0x33),
            "at the top"
        );
        skink.write(0x1234, 0x56);
        skink.write(0x9100, 0x78);
        assert_eq!((skink.read(0x1234), skink.read(0x9100)), (0x56, 0x78));
        skink.write(0x9050, 0x12);
        assert_eq!(skink.read(0x9050), 0xFF, "nothing there");
        assert_eq!(
            skink.read(COLOURS as u16 + 1999),
            0x0E,
            "the colours light grey on black"
        );
        assert!(
            Skink::new(&vec![0; 0x10000 - 0x9400 + 1]).is_err(),
            "into the disk's window"
        );
        assert!(Skink::new(&[]).is_err());
    }

    #[test]
    fn its_picture_is_80_by_25_characters_in_their_colours() {
        let mut skink = Skink::new(&[0; 6]).unwrap();
        skink.write(SCREEN as u16, b'H');
        skink.write(COLOURS as u16, 0x1F); // white on dark grey
        skink.write(SCREEN as u16 + 1, b' ' | 0x80); // inverse space
        let picture = skink.picture();
        assert_eq!((picture.width, picture.height), (640, 200));
        // H's first line: # . . . #, from its second dot.
        let line: Vec<u32> = picture.pixels[1..6].to_vec();
        let (white, grey) = (PALETTE[15], PALETTE[1]);
        assert_eq!(line, [white, grey, grey, grey, white]);
        assert_eq!(picture.pixels[0], grey, "its background");
        assert_eq!(
            picture.pixels[8 + 3 * WIDTH],
            PALETTE[14],
            "inverse: a space light grey"
        );
        assert_eq!(skink.screen_text().unwrap(), "H");
    }

    #[test]
    fn a_frames_interrupt_comes_60_times_a_second() {
        let Some(mut cpu) = running() else { return };
        cpu.execute(CLOCK_HZ);
        assert_eq!(count(&mut cpu, 0x0300), 60);
        assert_eq!(cpu.memory.screen_text().unwrap(), "Skink test");
    }

    #[test]
    fn the_vias_timer_interrupts_every_1002_cycles() {
        let Some(mut cpu) = running() else { return };
        cpu.execute(1002 * 100 + 500);
        assert_eq!(count(&mut cpu, 0x0302), 100);
    }

    #[test]
    fn keys_typed_are_read_in_order_and_0_when_theres_none() {
        let Some(mut cpu) = running() else { return };
        cpu.execute(10_000);
        cpu.memory.type_text("ls\n");
        cpu.memory.key(Key::Up);
        cpu.execute(10_000);
        let row: Vec<u8> = (0..4)
            .map(|x| cpu.memory.read(SCREEN as u16 + 80 + x))
            .collect();
        assert_eq!(row, [b'l', b's', b'\r', 0x82]);
        assert_eq!(cpu.memory.read(0x9021), 0, "none waiting");
        assert_eq!(cpu.memory.read(0x9020), 0);
    }

    #[test]
    fn what_the_acia_receives_it_sends_back() {
        let Some(mut cpu) = running() else { return };
        cpu.execute(10_000);
        cpu.memory.acia.incoming.extend(b"hi");
        cpu.execute(CLOCK_HZ / 100);
        assert_eq!(cpu.memory.acia.outgoing, b"hi");
    }

    #[test]
    fn its_disks_sector_is_at_9200_as_its_number_says() {
        let mut skink = Skink::new(&[0; 6]).unwrap();
        assert_eq!(skink.read(0x9200), 0xFF, "no disk");
        // 3 sectors, each byte its sector's number.
        let image = (0..3 * 512).map(|at| (at / 512) as u8).collect();
        skink.disk = Some(Disk::new(image, None));
        skink.write(0x9041, 2);
        assert_eq!((skink.read(0x9200), skink.read(0x93FF)), (2, 2));
        skink.write(0x9201, 0x55);
        skink.write(0x9041, 0);
        assert_eq!(skink.read(0x9201), 0);
        skink.write(0x9041, 2);
        assert_eq!(skink.read(0x9201), 0x55, "written to the disk");
        assert_eq!(skink.disk.as_ref().unwrap().image[2 * 512 + 1], 0x55);
        assert!(skink.light(), "used");
        assert!(!skink.light(), "and asked");
        skink.write(0x9041, 3);
        assert_eq!(skink.read(0x9200), 0xFF, "past its end");
        skink.write(0x9041, 0);
        skink.write(0x9040, 1);
        assert_eq!(skink.read(0x9200), 0xFF, "a drive it hasn't");
        assert_eq!(skink.read(0x9040), 1, "its drive, as written");
    }

    #[test]
    fn its_sector_number_is_3_bytes_low_first() {
        let mut skink = Skink::new(&[0; 6]).unwrap();
        let mut image = vec![0; 0x10101 * 512 + 512];
        image[0x10101 * 512] = 0xAB;
        skink.disk = Some(Disk::new(image, None));
        for (addr, byte) in [(0x9041, 0x01), (0x9042, 0x01), (0x9043, 0x01)] {
            skink.write(addr, byte);
        }
        assert_eq!(skink.read(0x9200), 0xAB);
    }
}
