//! The Disk II: the disk images' bits, the stepper and the motor, without
//! anything of Apple's; the card reading every sector of DOS 3.3's System
//! Master through its own sequencer PROM, from the .dsk and the .nib alike;
//! and DOS 3.3 booting from it. Those last need the card's ROMs and the
//! disks in roms/apple2/ (not part of the repository); without them they
//! say so and pass.

#[cfg(test)]
mod tests {
    use crate::machines::apple2::disk2::Disk2;
    use crate::machines::apple2::disk_image::{self, Disk, DOS_ORDER, TRANSLATE};
    use crate::machines::apple2::{Apple2, CLOCK_HZ};
    use crate::cpu::CPU;
    use std::path::{Path, PathBuf};

    fn dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("roms/apple2")
    }

    const DISK: &str = "disks/Apple DOS 3.3 January 1983";

    /// A data field's 256 bytes back from its disk bytes: 6-and-2 undone.
    fn decode(field: &[u8]) -> Option<[u8; 256]> {
        let mut values = [0u8; 342];
        let mut last = 0;
        for (i, &byte) in field[3..345].iter().enumerate() {
            last ^= TRANSLATE.iter().position(|&t| t == byte)? as u8;
            values[i] = last;
        }
        if TRANSLATE[last as usize] != field[345] {
            return None; // the checksum
        }
        let swap = |b: u8| ((b & 1) << 1) | ((b >> 1) & 1);
        let mut data = [0u8; 256];
        for i in 0..256 {
            data[i] = values[86 + i] << 2 | swap(values[i % 86] >> (2 * (i / 86)) & 3);
        }
        Some(data)
    }

    #[test]
    fn a_sector_encodes_and_decodes() {
        let data: [u8; 256] = std::array::from_fn(|i| (i * 7 + 3) as u8);
        let field = disk_image::data_field(&data);
        assert_eq!(field.len(), 3 + 343 + 3);
        assert_eq!(&field[..3], &[0xD5, 0xAA, 0xAD]);
        assert_eq!(&field[346..], &[0xDE, 0xAA, 0xEB]);
        assert!(field[3..346].iter().all(|b| TRANSLATE.contains(b)), "a byte not a disk byte");
        assert_eq!(decode(&field), Some(data));
    }

    #[test]
    fn an_address_field_is_four_and_four() {
        // Volume 254, track 17, sector 5, and their checksum, 254 ^ 17 ^ 5,
        // $EA: each its odd bits, then its even, with 1s between.
        let field = disk_image::address_field(254, 17, 5);
        assert_eq!(field, [0xD5, 0xAA, 0x96, 0xFF, 0xFE, 0xAA, 0xBB, 0xAA, 0xAF, 0xFF, 0xEA, 0xDE, 0xAA, 0xEB]);
    }

    #[test]
    fn only_the_gaps_have_sync_bytes() {
        // A .dsk of zeros: each track 48 syncs, then each sector's 14
        // address bytes, 6 syncs, 349 data bytes and 27 syncs.
        let disk = Disk::from_dsk(&vec![0; 143_360]).unwrap();
        let bits = 48 * 10 + 16 * (14 * 8 + 6 * 10 + 349 * 8 + 27 * 10);
        assert!(disk.tracks.iter().all(|track| track.len() == bits));
        // A .nib's FF in an address field (its volume, 254, is FF FE) is 8 bits.
        let mut nib = vec![0xFF; 6656 * 35];
        nib[100..114].copy_from_slice(&disk_image::address_field(254, 0, 0));
        let disk = Disk::from_nib(&nib).unwrap();
        // Every byte outside the field a sync of 10 bits; the field's 14, 8.
        assert_eq!(disk.tracks[0].len(), (6656 - 14) * 10 + 14 * 8);
    }

    fn card() -> Disk2 {
        Disk2::new(vec![0; 256], &[0; 256]).unwrap()
    }

    #[test]
    fn the_stepper_moves_the_head_toward_a_magnet() {
        let mut card = card();
        // Inward a track: magnet 1 on, 0 off, 2 on, 1 off.
        for addr in [0xC0E3, 0xC0E0, 0xC0E5, 0xC0E2] {
            card.io(addr, None);
        }
        assert_eq!(card.half_track, 2, "a track in");
        // Out again: magnet 1 on, 2 off, 0 on, 1 off.
        for addr in [0xC0E3, 0xC0E4, 0xC0E1, 0xC0E2] {
            card.io(addr, None);
        }
        assert_eq!(card.half_track, 0, "back to track 0");
        // Outward from track 0: against the stop, no farther.
        for addr in [0xC0E7, 0xC0E0] {
            card.io(addr, None);
        }
        assert_eq!(card.half_track, 0, "past track 0");
    }

    #[test]
    fn the_motor_runs_on_for_a_second() {
        let mut card = card();
        card.io(0xC0E9, None);
        card.io(0xC0E8, None);
        for _ in 0..CLOCK_HZ - 1 {
            card.tick();
        }
        assert!(card.spinning(), "stopped before a second");
        card.tick();
        assert!(!card.spinning(), "still turning after a second");

        card.io(0xC0E9, None);
        card.io(0xC0E8, None);
        card.io(0xC0E9, None);
        for _ in 0..2 * CLOCK_HZ {
            card.tick();
        }
        assert!(card.spinning(), "switched on again, it stopped");
    }

    /// The card with the real PROMs, and an image in it; None without them.
    fn real_card(image: &str) -> Option<Disk2> {
        let p5 = std::fs::read(dir().join("disk2-p5-341-0027.bin")).ok()?;
        let p6 = std::fs::read(dir().join("disk2-p6-341-0028.bin")).ok()?;
        let bytes = std::fs::read(dir().join(format!("{DISK}.{image}"))).ok()?;
        let mut card = Disk2::new(p5, &p6).unwrap();
        card.disk = Some(if image == "nib" { Disk::from_nib(&bytes) } else { Disk::from_dsk(&bytes) }.unwrap());
        Some(card)
    }

    /// Each sector of the track under the head, as the card reads it in a
    /// revolution and a bit: (sector, its 256 bytes).
    fn read_track(card: &mut Disk2) -> Vec<(u8, [u8; 256])> {
        let (mut bytes, mut last) = (Vec::new(), 0u8);
        for _ in 0..60_000 * 4 {
            card.tick();
            if card.data & 0x80 != 0 && last & 0x80 == 0 {
                bytes.push(card.data);
            }
            last = card.data;
        }
        let mut sectors = Vec::new();
        for at in 0..bytes.len().saturating_sub(14) {
            if bytes[at..at + 3] == [0xD5, 0xAA, 0x96] {
                let sector = ((bytes[at + 7] << 1) | 1) & bytes[at + 8];
                let data = (at + 14..bytes.len().saturating_sub(349)).find(|&d| bytes[d..d + 3] == [0xD5, 0xAA, 0xAD]);
                if let Some(data) = data.and_then(|d| decode(&bytes[d..d + 349])) {
                    if !sectors.iter().any(|&(s, _)| s == sector) {
                        sectors.push((sector, data));
                    }
                }
            }
        }
        sectors.sort_by_key(|&(s, _)| s);
        sectors
    }

    #[test]
    fn the_card_reads_every_sector_of_both_images() {
        let (Some(mut dsk), Some(mut nib)) = (real_card("dsk"), real_card("nib")) else {
            eprintln!("disk2 test: no card ROMs or disks in roms/apple2/");
            return;
        };
        let image = std::fs::read(dir().join(format!("{DISK}.dsk"))).unwrap();
        for card in [&mut dsk, &mut nib] {
            card.io(0xC0E9, None); // the motor on
        }
        for track in 0..35 {
            let (from_dsk, from_nib) = (read_track(&mut dsk), read_track(&mut nib));
            assert_eq!(from_dsk.len(), 16, "track {track} of the .dsk: sectors read");
            assert_eq!(from_nib.len(), 16, "track {track} of the .nib: sectors read");
            for ((sector, data), (_, nib_data)) in from_dsk.iter().zip(&from_nib) {
                let at = track * 4096 + DOS_ORDER[*sector as usize] * 256;
                assert_eq!(data[..], image[at..at + 256], "track {track} sector {sector} of the .dsk");
                assert_eq!(nib_data, data, "track {track} sector {sector}: the .nib's");
            }
            // In a track: magnets in turn, as DOS steps.
            let half = 2 * track;
            for magnet in [half + 1, half + 2] {
                let card_magnets = |card: &mut Disk2| {
                    card.io(0xC0E1 + 2 * (magnet % 4) as u16, None);
                    card.io(0xC0E0 + 2 * ((magnet + 3) % 4) as u16, None);
                };
                card_magnets(&mut dsk);
                card_magnets(&mut nib);
            }
        }
    }

    #[test]
    fn dos_3_3_boots_and_catalogs_its_disk() {
        let (Some(card), Ok(rom)) = (real_card("dsk"), std::fs::read(dir().join("Apple2_Plus.rom"))) else {
            eprintln!("disk2 test: no II+ ROM, card ROMs or disk in roms/apple2/");
            return;
        };
        let mut apple = Apple2::new(rom);
        apple.disk2 = Some(card);
        let mut cpu = CPU::with_bus(apple);
        cpu.clock_hz = u32::MAX; // as fast as it goes
        cpu.reset();
        let up = |cpu: &CPU<Apple2>| cpu.memory.text().iter().any(|line| line.contains("DOS VERSION 3.3"));
        while !up(&cpu) && cpu.clock < 8 * CLOCK_HZ {
            cpu.execute(100_000);
        }
        assert!(up(&cpu), "DOS 3.3 not up in 8 seconds");

        cpu.execute(CLOCK_HZ); // to the prompt
        for &key in b"CATALOG\r" {
            while cpu.memory.keyboard & 0x80 != 0 {
                cpu.execute(1_000);
            }
            cpu.memory.press(key);
        }
        cpu.execute(2 * CLOCK_HZ);
        let text = cpu.memory.text();
        assert!(text.iter().any(|line| line.contains("DISK VOLUME 254")), "the catalog: {text:#?}");
        assert!(text.iter().any(|line| line.contains("A 003 HELLO")), "HELLO not in it");
    }
}
