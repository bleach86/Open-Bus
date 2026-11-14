//! The Apple's ROMs found by what's in them: under any name, in a file of
//! several, in a zip, in a folder; the II+'s from its chips. With ROMs made
//! up here, and with the real ones renamed and rearranged as other programs
//! keep them, if they're in roms/apple2/ (not part of the repository;
//! without them, they say so and pass).

#[cfg(test)]
mod tests {
    use crate::host::zip::{crc32, stored};
    use crate::machines::apple2::rom_set::{
        self, DISK_II_P5, DISK_II_P6, Is, PLUS_ROM, Rom, Sorted, X16_ROM, rom,
    };
    use crate::machines::apple2::video::Monitor;
    use crate::machines::apple2::{self, Model, iie};
    use std::path::{Path, PathBuf};

    fn dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("roms/apple2")
    }

    /// `size` bytes, different for each `seed`.
    fn made_up(seed: u8, size: usize) -> Vec<u8> {
        (0..size)
            .map(|at| {
                (at as u8)
                    .wrapping_mul(seed)
                    .wrapping_add(seed ^ (at >> 8) as u8)
            })
            .collect()
    }

    /// Files sorted as the desktop app has them: which held ROMs and what
    /// was found told, the ROMs left where they were asked for, and a file
    /// that's too big, or isn't there, passed over.
    #[test]
    fn it_sorts_files_named_to_it_into_a_folder() {
        let out = std::env::temp_dir().join(format!("cpu_emu_sort_{}", std::process::id()));
        std::fs::create_dir_all(&out).unwrap();
        let (rom_bytes, other) = (made_up(7, 0x100), made_up(9, 0x100));
        let held = out.join("some name.bin");
        let not_rom = out.join("not one.bin");
        let too_big = out.join("too big.bin");
        std::fs::write(&held, &rom_bytes).unwrap();
        std::fs::write(&not_rom, &other).unwrap();
        std::fs::write(&too_big, made_up(7, 0x1_0001)).unwrap();
        let known = [rom(crc32(&rom_bytes), 0x100, Is::Named("wanted.bin"))];
        let paths = format!(
            "{}\n{}\n{}\n{}/gone.bin\n",
            held.display(),
            not_rom.display(),
            too_big.display(),
            out.display()
        );
        let mut told = Vec::new();
        rom_set::sort_of(&known, paths.as_bytes(), &out, &mut told).unwrap();
        let told = String::from_utf8(told).unwrap();
        assert_eq!(
            told,
            format!("held {}\nrom wanted.bin\nchips 0\n", held.display())
        );
        assert_eq!(std::fs::read(out.join("wanted.bin")).unwrap(), rom_bytes);
        std::fs::remove_dir_all(&out).unwrap();
    }

    fn known_of(pieces: &[(&[u8], Is)]) -> Vec<Rom> {
        pieces
            .iter()
            .map(|&(bytes, is)| rom(crc32(bytes), bytes.len(), is))
            .collect()
    }

    #[test]
    fn a_rom_is_known_by_whats_in_it_not_its_name() {
        let cd = made_up(3, 0x2000);
        let known = known_of(&[(&cd, Is::Named(iie::ROMS[0]))]);
        let mut sorted = Sorted::new();
        assert!(sorted.add_of(&known, "whatever you like.e8", &cd));
        assert_eq!(sorted.names(), [iie::ROMS[0]]);
        assert_eq!(sorted.bytes(iie::ROMS[0]).unwrap(), cd);
    }

    #[test]
    fn something_else_isnt_one() {
        let cd = made_up(3, 0x2000);
        let known = known_of(&[(&cd, Is::Named(iie::ROMS[0]))]);
        let mut sorted = Sorted::new();
        assert!(
            !sorted.add_of(&known, "Apple IIe.bin", &made_up(4, 0x2000)),
            "another ROM its size"
        );
        assert!(sorted.names().is_empty());
        assert!(sorted.bytes(iie::ROMS[0]).is_none());
    }

    #[test]
    fn a_file_of_several_has_each_of_them_found() {
        let (cd, ef) = (made_up(3, 0x2000), made_up(5, 0x2000));
        let known = known_of(&[
            (&cd, Is::Named(iie::ROMS[0])),
            (&ef, Is::Named(iie::ROMS[1])),
        ]);
        let mut sorted = Sorted::new();
        assert!(sorted.add_of(&known, "Apple2e.rom", &[&cd[..], &ef[..]].concat()));
        assert_eq!(sorted.bytes(iie::ROMS[0]).unwrap(), cd);
        assert_eq!(sorted.bytes(iie::ROMS[1]).unwrap(), ef);
    }

    #[test]
    fn a_rom_twice_over_is_the_one() {
        let video = made_up(7, 0x1000);
        let known = known_of(&[(&video, Is::Named(iie::ROMS[2]))]);
        let mut sorted = Sorted::new();
        assert!(sorted.add_of(&known, "video.bin", &[&video[..], &video[..]].concat()));
        assert_eq!(sorted.names().len(), 1);
        assert_eq!(sorted.bytes(iie::ROMS[2]).unwrap(), video);
    }

    #[test]
    fn a_zip_has_whats_in_it_found_and_a_game_isnt_one() {
        let (cd, p5) = (made_up(3, 0x2000), made_up(9, 0x100));
        let known = known_of(&[(&cd, Is::Named(iie::ROMS[0])), (&p5, Is::Named(DISK_II_P5))]);
        let mut sorted = Sorted::new();
        let set = stored(&[
            ("342-0135-a.e8", &cd),
            ("readme.txt", b"hello"),
            ("341-0027.p5", &p5),
        ]);
        assert!(sorted.add_of(&known, "apple2e.zip", &set));
        assert_eq!(sorted.names().len(), 2);

        // An NES game's zip: nothing in it, too big to be looked in.
        let mut sorted = Sorted::new();
        let game = stored(&[("Some Game.nes", &vec![0xEA; 0x4_0000])]);
        assert!(!sorted.add_of(&known, "Some Game.zip", &game));
        assert!(!sorted.add_of(&known, "not really a zip.zip", b"nonsense"));
    }

    #[test]
    fn the_ii_pluss_chips_make_its_rom_in_their_order_whichever_comes_first() {
        let chips: Vec<Vec<u8>> = (0..6)
            .map(|chip| made_up(11 + 2 * chip as u8, 0x800))
            .collect();
        let pieces: Vec<(&[u8], Is)> = chips
            .iter()
            .enumerate()
            .map(|(chip, bytes)| (&bytes[..], Is::PlusChip(chip)))
            .collect();
        let known = known_of(&pieces);
        let mut sorted = Sorted::new();
        for chip in [3, 0, 5, 1, 4] {
            sorted.add_of(&known, "a chip", &chips[chip]);
        }
        assert_eq!(sorted.plus_chips(), 5);
        assert!(sorted.bytes(PLUS_ROM).is_none(), "made of five");
        assert!(sorted.names().is_empty());

        sorted.add_of(&known, "the last", &chips[2]);
        assert_eq!(sorted.names(), [PLUS_ROM]);
        assert_eq!(sorted.bytes(PLUS_ROM).unwrap(), chips.concat());
    }

    #[test]
    fn a_ii_plus_chip_inside_a_bigger_rom_isnt_one() {
        let shared = made_up(13, 0x800);
        let cd = [&shared[..], &made_up(15, 0x1800)[..]].concat();
        let known = known_of(&[(&cd, Is::Named(iie::ROMS[0])), (&shared, Is::PlusChip(0))]);
        let mut sorted = Sorted::new();
        assert!(sorted.add_of(&known, "cd", &cd));
        assert_eq!(sorted.plus_chips(), 0, "the shared piece taken for a chip");
        assert!(sorted.add_of(&known, "a chip", &shared));
        assert_eq!(sorted.plus_chips(), 1, "the chip by itself not taken");
    }

    #[test]
    fn mames_sets_of_each_are_found_with_all_else_in_them_left() {
        let read = |name: &str| std::fs::read(dir().join(name)).ok();
        let (Some(plus), Some(old), Some(enhanced)) = (
            read("apple2.zip"),
            read("apple2e.zip"),
            read("apple2ee.zip"),
        ) else {
            eprintln!("rom set test: no MAME sets in roms/apple2/");
            return;
        };
        let mut sorted = Sorted::new();
        assert!(sorted.add("apple2.zip", &plus));
        assert!(
            sorted.names().contains(&PLUS_ROM.to_string()),
            "a set of dozens of machines'"
        );
        assert_eq!(sorted.bytes(PLUS_ROM).unwrap(), real(PLUS_ROM).unwrap());
        // A clone's floppy card in it has the Disk II's boot ROM in its own.
        assert_eq!(sorted.bytes(DISK_II_P5).unwrap(), real(DISK_II_P5).unwrap());

        let mut sorted = Sorted::new();
        assert!(sorted.add("apple2e.zip", &old));
        assert_eq!(sorted.names().len(), 3, "{:?}", sorted.names());
        assert!(iie::ROMS.iter().all(|name| sorted.bytes(name).is_some()));

        let mut sorted = Sorted::new();
        assert!(sorted.add("apple2ee.zip", &enhanced));
        assert_eq!(sorted.names().len(), 3, "{:?}", sorted.names());
        for name in iie::ENHANCED_ROMS {
            assert_eq!(sorted.bytes(name).unwrap(), real(name).unwrap(), "{name}");
        }
    }

    /// The real ROMs, by their names, if they're there.
    fn real(name: &str) -> Option<Vec<u8>> {
        std::fs::read(dir().join(name)).ok()
    }

    #[test]
    fn the_real_ones_are_known_rearranged_as_other_programs_have_them() {
        let (Some(plus), Some(p5), Some(p6)) = (real(PLUS_ROM), real(DISK_II_P5), real(DISK_II_P6))
        else {
            eprintln!("rom set test: no ROMs in roms/apple2/");
            return;
        };
        let [cd, ef, video] = iie::ENHANCED_ROMS.map(|name| real(name).unwrap());
        let [old_cd, old_ef, old_video] = iie::ROMS.map(|name| real(name).unwrap());

        // As AppleWin keeps a IIe's, its halves together; a video ROM doubled.
        let mut sorted = Sorted::new();
        assert!(sorted.add("Apple2e_Enhanced.rom", &[&cd[..], &ef[..]].concat()));
        assert!(sorted.add("video.bin", &[&video[..], &video[..]].concat()));
        assert_eq!(sorted.bytes(iie::ENHANCED_ROMS[0]).unwrap(), cd);
        assert_eq!(sorted.bytes(iie::ENHANCED_ROMS[1]).unwrap(), ef);
        assert_eq!(sorted.bytes(iie::ENHANCED_ROMS[2]).unwrap(), video);

        // As MAME keeps them: a zip of a board's chips.
        let mut sorted = Sorted::new();
        let chips: Vec<&[u8]> = plus.chunks(0x800).collect();
        let names = [
            "341-0011.d0",
            "341-0012.d8",
            "341-0013.e0",
            "341-0014.e8",
            "341-0015.f0",
            "341-0020-00.f8",
        ];
        let set: Vec<(&str, &[u8])> = names.iter().copied().zip(chips).collect();
        assert!(sorted.add("apple2p.zip", &stored(&set)));
        assert_eq!(sorted.bytes(PLUS_ROM).unwrap(), plus);

        let mut sorted = Sorted::new();
        assert!(sorted.add(
            "apple2e.zip",
            &stored(&[
                ("a", &old_cd),
                ("b", &old_ef),
                ("c", &old_video),
                ("d", &p5),
                ("e", &p6)
            ])
        ));
        assert_eq!(sorted.names().len(), 5);
    }

    #[test]
    fn a_folder_of_any_names_or_a_zip_starts_the_machine() {
        let (Some(p5), Some(p6)) = (real(DISK_II_P5), real(DISK_II_P6)) else {
            eprintln!("rom set test: no ROMs in roms/apple2/");
            return;
        };
        let [cd, ef, video] = iie::ENHANCED_ROMS.map(|name| real(name).unwrap());
        let folder = std::env::temp_dir().join(format!("cpu_emu_{}_rom_set", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let disk = folder.join("disk.dsk");
        std::fs::write(&disk, vec![0; 143_360]).unwrap();
        for (name, bytes) in [
            ("one", &cd),
            ("two.e10", &ef),
            ("three.chr", &video),
            ("four", &p5),
            ("five", &p6),
        ] {
            std::fs::write(folder.join(name), bytes).unwrap();
        }
        let disks = [disk.to_str().unwrap()];
        let built = apple2::build(
            Model::IIeEnhanced,
            folder.to_str().unwrap(),
            &disks,
            0,
            None,
            Monitor::Colour,
        );
        assert!(
            built.is_ok(),
            "from a folder of other names: {:?}",
            built.err()
        );

        // The same in a zip, the disk beside it.
        for name in ["one", "two.e10", "three.chr", "four", "five"] {
            std::fs::remove_file(folder.join(name)).unwrap();
        }
        let set = folder.join("apple2ee.zip");
        std::fs::write(
            &set,
            stored(&[
                ("a", &cd),
                ("b", &ef),
                ("c", &video),
                ("d", &p5),
                ("e", &p6),
            ]),
        )
        .unwrap();
        let built = apple2::build(
            Model::IIeEnhanced,
            set.to_str().unwrap(),
            &disks,
            0,
            None,
            Monitor::Colour,
        );
        assert!(built.is_ok(), "from a zip: {:?}", built.err());
        assert!(
            rom_set::scan(&set)
                .names()
                .contains(&DISK_II_P5.to_string())
        );

        // And no ROMs to be found, an error that says so.
        std::fs::remove_file(&set).unwrap();
        let error = apple2::build(
            Model::IIeEnhanced,
            folder.to_str().unwrap(),
            &[],
            0,
            None,
            Monitor::Colour,
        )
        .err()
        .unwrap();
        assert!(error.starts_with("no Apple IIe"), "{error}");
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn the_x16s_rom_is_known_by_its_banks_and_banner_alone_or_zipped() {
        let mut rom = vec![0xFF; 0x8000];
        rom[0x6000..0x600D].copy_from_slice(b"COMMANDER X16");
        let mut sorted = Sorted::new();
        assert!(sorted.add("rom.bin", &rom));
        assert_eq!(sorted.bytes(X16_ROM), Some(rom.clone()));
        let mut sorted = Sorted::new();
        assert!(sorted.add("Release.R49.ROM.Image.zip", &stored(&[("rom.bin", &rom)])));
        assert_eq!(sorted.names(), [X16_ROM]);
        // Not whole banks; no banner.
        let mut sorted = Sorted::new();
        assert!(!sorted.add("rom.bin", &rom[..0x7FFF]));
        assert!(!sorted.add("game.bin", &vec![0xFF; 0x8000]));
    }
}
