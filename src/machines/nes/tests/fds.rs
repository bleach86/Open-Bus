//! The Disk System: its images, its drive reading and writing a byte at a
//! time, changing disks, its IRQ timer and its registers.

#[cfg(test)]
mod tests {
    use crate::host::machine::Machine;
    use crate::machines::nes::Nes;
    use crate::machines::nes::cartridge::{Cartridge, Mirroring};
    use crate::machines::nes::fds::{CHANGE_CYCLES, sides_of};

    const SIDE: usize = 65500;
    /// Disk registers on; the drive's bits as $4025 has them.
    const MOTOR: u8 = 0x01;
    const READ: u8 = 0x04;
    const READY: u8 = 0x40;

    /// A side: a disk info block, the file count, and a file of `data`.
    fn side(data: &[u8]) -> Vec<u8> {
        let mut side = vec![1; 56];
        side.extend([2, 1]);
        let mut header = vec![3; 16];
        (header[13], header[14]) = (data.len() as u8, 0);
        side.extend(header);
        side.push(4);
        side.extend(data);
        side.resize(SIDE, 0);
        side
    }

    fn drive(sides: &[Vec<u8>]) -> Cartridge {
        let image = sides.concat();
        let mut cartridge = Cartridge::disk(&image, sides_of(&image).unwrap(), &[0; 0x2000]);
        cartridge.cpu_write(0x4023, 0x83, 0);
        cartridge
    }

    fn flags(cartridge: &mut Cartridge) -> u8 {
        cartridge.register_read(0x4030, 0)
    }

    /// The disk's bytes, as read one by one from the start of a side, its
    /// first `count`.
    fn read_bytes(cartridge: &mut Cartridge, count: usize) -> Vec<u8> {
        cartridge.cpu_write(0x4025, MOTOR | READ | READY, 0);
        let mut bytes = Vec::new();
        for _ in 0..4_000_000 {
            cartridge.cycle();
            if flags(cartridge) & 2 != 0 {
                bytes.push(cartridge.register_read(0x4031, 0));
                if bytes.len() == count {
                    break;
                }
            }
        }
        bytes
    }

    #[test]
    fn an_image_is_known_with_a_header_or_without() {
        let sides = side(&[1, 2, 3]);
        assert_eq!(sides_of(&sides).unwrap().len(), SIDE);
        let headed = [b"FDS\x1A\x01".to_vec(), vec![0; 11], sides.clone()].concat();
        assert_eq!(sides_of(&headed).unwrap(), &sides[..]);
    }

    #[test]
    fn nothing_else_is_taken_for_one() {
        assert!(sides_of(&[0; 1000]).is_none(), "the wrong size");
        assert!(sides_of(&[]).is_none());
        let cartridge = [b"NES\x1A".to_vec(), vec![0; SIDE - 4]].concat();
        assert!(
            sides_of(&cartridge).is_none(),
            "an iNES file that happens to fit"
        );
    }

    #[test]
    fn its_ram_is_6000_to_dfff_and_its_bios_above_it() {
        let image = side(&[]);
        let bios: Vec<u8> = (0..0x2000).map(|at| (at >> 5) as u8).collect();
        let mut cartridge = Cartridge::disk(&image, &image, &bios);
        cartridge.cpu_write(0xDFFF, 0x5A, 0);
        cartridge.cpu_write(0x6000, 0xA5, 0);
        assert_eq!(cartridge.cpu_read(0xDFFF), Some(0x5A));
        assert_eq!(cartridge.cpu_read(0x6000), Some(0xA5));
        assert_eq!(cartridge.cpu_read(0xFFFF), Some(bios[0x1FFF]));
        cartridge.cpu_write(0xE000, 0x5A, 0);
        assert_eq!(cartridge.cpu_read(0xE000), Some(bios[0]), "the BIOS is ROM");
    }

    #[test]
    fn a_side_is_read_block_by_block_each_after_its_gap() {
        let mut cartridge = drive(&[side(&[0xDE, 0xAD])]);
        let bytes = read_bytes(&mut cartridge, 57);
        assert_eq!(bytes[0], 0x80, "what marks a block's start");
        assert_eq!(bytes[1..57], [1; 56]);
    }

    #[test]
    fn the_blocks_after_it_come_after_their_own_gaps() {
        let mut cartridge = drive(&[side(&[0xDE, 0xAD])]);
        let start = 57 + 2 + 122; // disk info, its CRC and the gap after
        let bytes = read_bytes(&mut cartridge, start + 3);
        assert_eq!(bytes[57..59], [0x4D, 0x62], "a CRC");
        assert_eq!(bytes[start..start + 3], [0x80, 2, 1], "the file count");
    }

    #[test]
    fn with_the_motor_off_nothing_comes() {
        let mut cartridge = drive(&[side(&[])]);
        cartridge.cpu_write(0x4025, READ | READY, 0);
        for _ in 0..100_000 {
            cartridge.cycle();
        }
        assert_eq!(flags(&mut cartridge) & 2, 0);
    }

    #[test]
    fn what_a_game_writes_is_on_the_disk_and_to_be_saved() {
        let mut cartridge = drive(&[side(&[])]);
        let before = cartridge.prg_ram.clone();
        cartridge.cpu_write(0x4024, 0x77, 0);
        cartridge.cpu_write(0x4025, MOTOR | READY, 0); // not READ: writing
        for _ in 0..100_000 {
            cartridge.cycle();
        }
        assert!(cartridge.written, "kept as a battery's saves are");
        let at = (0..before.len()).find(|&at| before[at] != cartridge.prg_ram[at]);
        assert_eq!(at.map(|at| cartridge.prg_ram[at]), Some(0x77));
    }

    #[test]
    fn a_disk_changing_leaves_the_drive_empty_a_while_then_the_next_side_is_in() {
        let mut cartridge = drive(&[side(&[]), side(&[])]);
        assert_eq!(
            cartridge.register_read(0x4032, 0) & 1,
            0,
            "the first side's in"
        );
        assert_eq!(cartridge.change_disk().as_deref(), Some("Disk 1 side B"));
        assert_eq!(cartridge.register_read(0x4032, 0) & 1, 1, "out at once");
        for _ in 0..CHANGE_CYCLES {
            cartridge.cycle();
        }
        assert_eq!(cartridge.register_read(0x4032, 0) & 1, 0, "in again");
        assert_eq!(
            cartridge.change_disk().as_deref(),
            Some("Disk 1 side A"),
            "and round again"
        );
    }

    #[test]
    fn a_cartridge_has_no_disk_to_change() {
        let mut image = b"NES\x1A\x01\x01".to_vec();
        image.extend([0; 10]);
        image.extend([0xEA; 0x4000 + 0x2000]);
        let mut cartridge = Cartridge::new(&image).unwrap();
        assert_eq!(cartridge.change_disk(), None);
        assert_eq!(cartridge.disk_crc(), None);
    }

    #[test]
    fn the_timer_raises_its_irq_once_unless_told_to_repeat() {
        let mut cartridge = drive(&[side(&[])]);
        cartridge.cpu_write(0x4020, 10, 0);
        cartridge.cpu_write(0x4021, 0, 0);
        cartridge.cpu_write(0x4022, 2, 0);
        (0..10).for_each(|_| cartridge.cycle());
        assert!(!cartridge.irq(), "not yet");
        cartridge.cycle();
        assert!(cartridge.irq());
        assert_eq!(flags(&mut cartridge) & 1, 1);
        assert!(!cartridge.irq(), "acknowledged by reading $4030");
        (0..100).for_each(|_| cartridge.cycle());
        assert!(!cartridge.irq(), "the once");
        cartridge.cpu_write(0x4022, 3, 0);
        (0..11).for_each(|_| cartridge.cycle());
        flags(&mut cartridge);
        (0..11).for_each(|_| cartridge.cycle());
        assert!(cartridge.irq(), "and again");
    }

    #[test]
    fn the_registers_can_be_turned_off() {
        let mut cartridge = drive(&[side(&[])]);
        cartridge.cpu_write(0x4023, 0, 0);
        assert_eq!(cartridge.register_read(0x4033, 0x41), 0x41, "open bus");
        cartridge.cpu_write(0x4026, 0x12, 0);
        cartridge.cpu_write(0x4023, 1, 0);
        assert_eq!(
            cartridge.register_read(0x4033, 0),
            0,
            "that write went nowhere"
        );
    }

    #[test]
    fn the_mirroring_is_written_with_the_drive() {
        let mut cartridge = drive(&[side(&[])]);
        cartridge.cpu_write(0x4025, 0x08, 0);
        assert_eq!(cartridge.mirroring, Mirroring::Horizontal);
        cartridge.cpu_write(0x4025, 0, 0);
        assert_eq!(cartridge.mirroring, Mirroring::Vertical);
    }

    #[test]
    fn its_sound_plays_its_wave_at_its_volume() {
        let mut cartridge = drive(&[side(&[])]);
        cartridge.cpu_write(0x4089, 0x80, 0); // the wave open
        (0..64).for_each(|at| cartridge.cpu_write(0x4040 + at, 63, 0));
        cartridge.cpu_write(0x4089, 0, 0);
        assert_eq!(cartridge.output(), 0.0, "silent yet");
        cartridge.cpu_write(0x4080, 0x80 | 32, 0); // a fixed volume
        cartridge.cpu_write(0x4082, 0xFF, 0);
        cartridge.cpu_write(0x4083, 0x0F, 0);
        cartridge.cycle();
        assert!(cartridge.output() > 0.2, "{}", cartridge.output());
    }

    #[test]
    fn two_games_on_one_bios_are_told_apart_by_their_disks() {
        let (one, other) = (side(&[1]), side(&[2]));
        let nes = |image: &[u8]| Nes::new(Cartridge::disk(image, image, &[0; 0x2000]));
        assert_ne!(nes(&one).identity(), nes(&other).identity());
    }
}
