//! The Disk II: the disk images' bits, a .po's sectors in ProDOS's order,
//! a .woz's tracks and map, the stepper and the motor, without anything of
//! Apple's; the card reading every sector of DOS 3.3's System Master
//! through its own sequencer PROM, from the .dsk and the .nib alike, and
//! noise where there's no track; DOS 3.3 booting from it; and Pac-Man
//! booting from a .woz. Those last need the card's ROMs and the disks in
//! roms/apple2/ (not part of the repository); without them they say so and
//! pass.

#[cfg(test)]
mod tests {
    use crate::machines::apple2::disk2::Disk2;
    use crate::machines::apple2::disk_image::{self, crc32, Disk, DOS_ORDER, TRANSLATE};
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

    #[test]
    fn a_po_is_the_same_disk_in_prodos_blocks() {
        // Each sector of a .dsk its own: track, sector, and the byte's place.
        let mut dsk = vec![0; 143_360];
        for (at, byte) in dsk.iter_mut().enumerate() {
            *byte = (at / 4096 * 16 + at / 256 % 16) as u8 ^ (at % 256) as u8;
        }
        // The .po: a track's 8 blocks, each two DOS 3.3 sectors, as ProDOS has them.
        let blocks = [(0x0, 0xE), (0xD, 0xC), (0xB, 0xA), (0x9, 0x8), (0x7, 0x6), (0x5, 0x4), (0x3, 0x2), (0x1, 0xF)];
        let mut po = Vec::new();
        for track in dsk.chunks(4096) {
            for (first, second) in blocks {
                po.extend_from_slice(&track[first * 256..(first + 1) * 256]);
                po.extend_from_slice(&track[second * 256..(second + 1) * 256]);
            }
        }
        let (from_dsk, from_po) = (Disk::from_dsk(&dsk).unwrap(), Disk::from_po(&po).unwrap());
        assert!(from_dsk.tracks == from_po.tracks, "the .po not the .dsk's disk");
    }

    /// A .woz of `version` with `tracks`, mapped by `tmap`: its header and
    /// CRC; INFO, a 5.25" disk, `protected` or not; TMAP; and TRKS, in WOZ1
    /// 6,656 bytes a track, in WOZ2 a list of where they are, the bits from
    /// block 3.
    fn woz(version: u8, tracks: &[Vec<u8>], tmap: &[u8; 160], protected: bool) -> Vec<u8> {
        let packed = |bits: &Vec<u8>| -> Vec<u8> {
            bits.chunks(8).map(|byte| byte.iter().enumerate().map(|(i, &bit)| bit << (7 - i)).sum()).collect()
        };
        let chunk = |id: &[u8], body: Vec<u8>| [id, &(body.len() as u32).to_le_bytes(), &body].concat();
        let mut info = vec![0; 60];
        info[0] = version;
        info[1] = 1; // 5.25"
        info[2] = protected as u8;
        let mut trks = Vec::new();
        if version == 1 {
            for bits in tracks {
                let mut track = packed(bits);
                track.resize(6646, 0);
                track.extend((bits.len().div_ceil(8) as u16).to_le_bytes());
                track.extend((bits.len() as u16).to_le_bytes());
                track.resize(6656, 0);
                trks.extend(track);
            }
        } else {
            let mut data = Vec::new();
            for n in 0..160 {
                let Some(bits) = tracks.get(n) else {
                    trks.extend([0; 8]);
                    continue;
                };
                let mut track = packed(bits);
                track.resize(track.len().div_ceil(512) * 512, 0);
                trks.extend(((3 + data.len() / 512) as u16).to_le_bytes());
                trks.extend(((track.len() / 512) as u16).to_le_bytes());
                trks.extend((bits.len() as u32).to_le_bytes());
                data.extend(track);
            }
            trks.extend(data); // at 256 + 1,280: block 3
        }
        let body = [chunk(b"INFO", info), chunk(b"TMAP", tmap.to_vec()), chunk(b"TRKS", trks)].concat();
        let magic: &[u8] = if version == 1 { b"WOZ1\xFF\n\r\n" } else { b"WOZ2\xFF\n\r\n" };
        [magic, &crc32(&body).to_le_bytes(), &body].concat()
    }

    #[test]
    fn a_woz_is_its_tracks_where_its_map_puts_them() {
        let dsk: Vec<u8> = (0..143_360).map(|at| (at / 256 ^ at) as u8).collect();
        let tracks = Disk::from_dsk(&dsk).unwrap().tracks;
        // Each track at its quarter-track and those either side, none
        // between; but track 2 at half-track 3 too.
        let mut tmap = [0xFF; 160];
        for track in 0..35usize {
            for quarter in (4 * track).saturating_sub(1)..=4 * track + 1 {
                tmap[quarter] = track as u8;
            }
        }
        tmap[6] = 2;
        for version in [1, 2] {
            let disk = Disk::from_woz(&woz(version, &tracks, &tmap, false)).unwrap();
            // WOZ2 has room for 160: the rest none.
            assert!(disk.tracks[..35] == tracks[..], "WOZ{version}'s tracks");
            assert!(disk.tracks[35..].iter().all(Vec::is_empty), "WOZ{version}'s tracks past 35");
            for half in 0..70 {
                let want = if half % 2 == 0 { Some(half / 2) } else if half == 3 { Some(2) } else { None };
                assert_eq!(disk.half_tracks[half], want, "WOZ{version}'s half-track {half}");
            }
            let mut broken = woz(version, &tracks, &tmap, false);
            broken[100] ^= 1;
            assert!(Disk::from_woz(&broken).is_err(), "WOZ{version} with a wrong CRC");
        }
    }

    #[test]
    fn each_image_saves_as_it_was_read() {
        let dsk: Vec<u8> = (0..143_360).map(|at| (at / 256 * 7 ^ at) as u8).collect();
        assert!(Disk::from_dsk(&dsk).unwrap().to_image().unwrap() == dsk, "the .dsk");
        assert!(Disk::from_po(&dsk).unwrap().to_image().unwrap() == dsk, "the .po");

        // A .nib: on each track, a sector's fields among FFs.
        let mut nib = vec![0xFF; 6656 * 35];
        for track in 0..35 {
            let at = track * 6656 + 100;
            nib[at..at + 14].copy_from_slice(&disk_image::address_field(254, track as u8, 0));
            let data = disk_image::data_field(&[track as u8; 256]);
            nib[at + 20..at + 20 + data.len()].copy_from_slice(&data);
        }
        assert!(Disk::from_nib(&nib).unwrap().to_image().unwrap() == nib, "the .nib");

        // A .woz, either version: a WOZ2 of the same tracks, its map the
        // same at every quarter-track, write protected as it was.
        let tracks = Disk::from_dsk(&dsk).unwrap().tracks;
        let mut tmap = [0xFF; 160];
        for track in 0..35usize {
            for quarter in (4 * track).saturating_sub(1)..=4 * track + 1 {
                tmap[quarter] = track as u8;
            }
        }
        for (version, protected) in [(1, false), (2, true)] {
            let saved = Disk::from_woz(&woz(version, &tracks, &tmap, protected)).unwrap().to_image().unwrap();
            assert_eq!(&saved[..4], b"WOZ2", "WOZ{version} saved");
            assert_eq!(&saved[88..248], &tmap[..], "WOZ{version}'s map, saved"); // TMAP's, after INFO's 60
            let disk = Disk::from_woz(&saved).unwrap();
            assert!(disk.tracks[..35] == tracks[..], "WOZ{version}'s tracks, saved");
            assert_eq!(disk.write_protected, protected, "WOZ{version}'s write protect, saved");
        }
    }

    #[test]
    fn a_read_only_image_is_write_protected() {
        let file = std::env::temp_dir().join(format!("cpu_emu_read_only_{}.dsk", std::process::id()));
        std::fs::write(&file, vec![0; 143_360]).unwrap();
        assert!(!Disk::open(&file).unwrap().write_protected, "a file that can be written, write protected");
        let mut permissions = std::fs::metadata(&file).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&file, permissions).unwrap();
        let protected = Disk::open(&file).unwrap().write_protected;
        std::fs::remove_file(&file).unwrap();
        assert!(protected, "a read only file not write protected");
    }

    #[test]
    fn writing_where_there_is_no_track_begins_one() {
        let mut disk = Disk::from_dsk(&vec![0; 143_360]).unwrap();
        assert!(disk.track(1).is_none());
        disk.write_bit(1, 7, 1);
        let track = disk.track(1).expect("no track at half-track 1, written");
        assert_eq!(track.len(), disk_image::REVOLUTION_BITS);
        assert_eq!(track.iter().filter(|&&bit| bit == 1).count(), 1, "the bit written, and only it");
        assert_eq!(track[7], 1);
        assert!(disk.written);
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
    fn where_there_is_no_track_the_card_reads_noise() {
        let (Ok(p5), Ok(p6)) = (std::fs::read(dir().join("disk2-p5-341-0027.bin")), std::fs::read(dir().join("disk2-p6-341-0028.bin"))) else {
            eprintln!("disk2 test: no card ROMs in roms/apple2/");
            return;
        };
        let mut card = Disk2::new(p5, &p6).unwrap();
        card.disk = Some(Disk::from_dsk(&vec![0; 143_360]).unwrap());
        // Half-track 1, between tracks 0 and 1: no flux changes, so the
        // amplifier's noise, and bytes of it.
        card.io(0xC0E3, None);
        card.io(0xC0E9, None);
        assert_eq!(card.half_track, 1);
        let (mut bytes, mut last) = (Vec::new(), 0u8);
        for _ in 0..50_000 * 4 {
            card.tick();
            if card.data & 0x80 != 0 && last & 0x80 == 0 {
                bytes.push(card.data);
            }
            last = card.data;
        }
        let mut kinds = bytes.clone();
        kinds.sort();
        kinds.dedup();
        assert!(bytes.len() > 1000, "{} bytes of noise in a turn", bytes.len());
        assert!(kinds.len() > 20, "{} kinds of byte in the noise", kinds.len());
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

    #[test]
    fn pac_man_boots_from_a_woz() {
        let dir = dir();
        let read = |name: &str| std::fs::read(dir.join(name)).ok();
        let (Some(rom), Some(p5), Some(p6), Some(woz)) = (
            read("Apple2_Plus.rom"),
            read("disk2-p5-341-0027.bin"),
            read("disk2-p6-341-0028.bin"),
            read("disks/Pac-Man (Atarisoft).woz"),
        ) else {
            eprintln!("disk2 test: no II+ ROM, card ROMs or Pac-Man .woz in roms/apple2/");
            return;
        };
        let mut card = Disk2::new(p5, &p6).unwrap();
        card.disk = Some(Disk::from_woz(&woz).unwrap());
        let mut apple = Apple2::new(rom);
        apple.disk2 = Some(card);
        let mut cpu = CPU::with_bus(apple);
        cpu.clock_hz = u32::MAX; // as fast as it goes
        cpu.reset();
        // DOS 3.3 loads it a sector a turn: half a minute.
        let menu = |cpu: &CPU<Apple2>| cpu.memory.text().iter().any(|line| line.contains("CHOOSE A MODE (1 - 2)"));
        while !menu(&cpu) && cpu.clock < 45 * CLOCK_HZ {
            cpu.execute(100_000);
        }
        assert!(menu(&cpu), "not at its menu in 45 seconds: {:#?}", cpu.memory.text());
    }
}
