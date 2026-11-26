//! The X16's cards, made: AUTOBOOT.X16 as the X16 saves it; a game's
//! files and program, found; a card read back as FAT32 has it, its long
//! and short names, its folders and its files' clusters; and one booted,
//! its program run (with its ROM, in roms/x16/). A zip of a program BASIC
//! loads, an X16 game, as its facts say.

#[cfg(test)]
mod tests {
    use crate::host::clock::DateTime;
    use crate::host::zip::stored;
    use crate::machines::facts::{System, facts_of, system_of};
    use crate::machines::x16::card::{self, CardFile};
    use std::path::Path;

    const WHEN: DateTime = DateTime {
        year: 2026,
        month: 11,
        day: 13,
        weekday: 5,
        hour: 19,
        minute: 12,
        second: 30,
    };

    #[test]
    fn autoboot_is_a_line_of_basic_as_the_x16_saves_it() {
        // `10 LOAD"GALAX16.PRG"`, SAVEd as AUTOBOOT.X16 by the X16 itself.
        let saved = [
            0x01, 0x08, 0x14, 0x08, 0x0A, 0x00, 0x93, 0x22, b'G', b'A', b'L', b'A', b'X', b'1',
            b'6', b'.', b'P', b'R', b'G', 0x22, 0x00, 0x00, 0x00,
        ];
        assert_eq!(card::autoboot("galax16.prg"), saved);
    }

    #[test]
    fn a_folders_autoboot_goes_into_it_as_the_x16_saves_it() {
        // `10 DOS"CD:VERANDA":LOAD"VERANDA.PRG"`, SAVEd by x16emu R49.
        let mut saved = vec![0x01, 0x08, 0x23, 0x08, 0x0A, 0x00, 0xCE, 0x81];
        saved.extend(b"\"CD:VERANDA\":\x93\"VERANDA.PRG\"\0\0\0");
        assert_eq!(card::autoboot_in("Veranda", "veranda.prg"), saved);
    }

    #[test]
    fn a_whole_cards_program_is_in_its_folder_run_from_there() {
        // Veranda's zip: its folders, for the top of a card.
        let game = card::game(
            "veranda-1.0.1-sdcard",
            vec![
                file("DOCUMENTS/Garden.brd", b"brd"),
                file("INSTALL.txt", b"copy the folders"),
                file("PC/web_proxy.py", b"py"),
                file("VERANDA/AUTOBOOT.X16", &[0x01, 0x08, 1]),
                file("VERANDA/RBOOT", &[0x01, 0x08, 2]),
                file("VERANDA/VERANDA.PRG", &[0x01, 0x08, 1]),
            ],
        )
        .unwrap();
        assert_eq!(game.program, "VERANDA/VERANDA.PRG", "in its name");
        let boot = game
            .files
            .iter()
            .find(|(path, _)| path == "AUTOBOOT.X16")
            .map(|(_, bytes)| bytes.clone());
        assert_eq!(boot, Some(card::autoboot_in("VERANDA", "VERANDA.PRG")));
        assert_eq!(game.files.len(), 7, "its files where they were, and that");
        let top = card::game(
            "x",
            vec![file("X.PRG", &[1, 8]), file("TOOLS/X.PRG", &[1, 8])],
        );
        assert_eq!(top.unwrap().program, "X.PRG", "one at the top first");
    }

    fn file(path: &str, bytes: &[u8]) -> CardFile {
        (path.to_string(), bytes.to_vec())
    }

    #[test]
    fn a_games_program_is_its_named_one_that_loads_at_0801() {
        let game = card::game(
            "X16Robots-12-19-2024",
            vec![
                file("robots/", b""),
                file("robots/ROBOTSMIDI.PRG", &[0x01, 0x08, 1]),
                file("robots/ROBOTS.PRG", &[0x01, 0x08, 2]),
                file("robots/A.PRG", &[0x01, 0x08, 3]),
                file("robots/LEVEL-A", &[0x00, 0xA0, 4]),
            ],
        )
        .unwrap();
        assert_eq!(game.program, "ROBOTS.PRG", "in its name; then the shortest");
        let paths: Vec<&str> = game.files.iter().map(|(path, _)| path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "ROBOTSMIDI.PRG",
                "ROBOTS.PRG",
                "A.PRG",
                "LEVEL-A",
                "AUTOBOOT.X16"
            ],
            "its one folder its files; AUTOBOOT.X16 added"
        );
        let own = card::game(
            "x",
            vec![file("AUTOBOOT.X16", b"its"), file("X.PRG", &[1, 8])],
        );
        assert_eq!(own.unwrap().files.len(), 2, "its own AUTOBOOT.X16 kept");
        assert!(card::game("x", vec![file("DATA.BIN", &[0, 0xA0])]).is_err());
        // A card's image it comes with, for x16emu: left off.
        let mut image = vec![0; 0x10_0000];
        image[450] = 0x0C;
        image[510..512].copy_from_slice(&[0x55, 0xAA]);
        let with = card::game("x", vec![file("X.PRG", &[1, 8]), file("x.img", &image)]);
        assert_eq!(with.unwrap().files.len(), 2, "X.PRG and AUTOBOOT.X16");
    }

    /// A card read as FAT32 has it: enough of it to check what's on it.
    struct Reader<'a> {
        card: &'a [u8],
        data: usize,
        fat: usize,
        /// A cluster's bytes.
        cluster: usize,
    }

    impl Reader<'_> {
        fn new(card: &[u8]) -> Reader<'_> {
            let u32_at = |at: usize| u32::from_le_bytes(card[at..at + 4].try_into().unwrap());
            let u16_at = |at: usize| u16::from_le_bytes(card[at..at + 2].try_into().unwrap());
            assert_eq!(card[510..512], [0x55, 0xAA], "the MBR's signature");
            assert_eq!(card[450], 0x0C, "FAT32, by LBA");
            let start = u32_at(454) as usize * 512;
            let boot = &card[start..];
            assert_eq!(&boot[82..90], b"FAT32   ");
            assert_eq!(u16_at(start + 11), 512);
            assert_eq!(u32_at(start + 44), 2, "the root at cluster 2");
            let reserved = u16_at(start + 14) as usize;
            let fat_size = u32_at(start + 36) as usize;
            let fat = start + reserved * 512;
            Reader {
                card,
                fat,
                data: fat + boot[16] as usize * fat_size * 512,
                cluster: boot[13] as usize * 512,
            }
        }

        /// A chain of clusters' bytes, from `first`.
        fn chain(&self, first: u32) -> Vec<u8> {
            let mut bytes = Vec::new();
            let mut cluster = first;
            while cluster < 0x0FFF_FFF8 {
                let at = self.data + (cluster as usize - 2) * self.cluster;
                bytes.extend(&self.card[at..at + self.cluster]);
                let entry = self.fat + cluster as usize * 4;
                cluster = u32::from_le_bytes(self.card[entry..entry + 4].try_into().unwrap());
            }
            bytes
        }

        /// A folder's entries: each one's long name (or short), short
        /// name, attributes, first cluster and size.
        fn folder(&self, cluster: u32) -> Vec<(String, String, u8, u32, u32)> {
            let bytes = self.chain(cluster);
            let mut found = Vec::new();
            let mut long: Vec<(u8, Vec<u16>, u8)> = Vec::new();
            for entry in bytes.chunks(32).take_while(|entry| entry[0] != 0) {
                if entry[11] == 0x0F {
                    let places = (1..11).step_by(2).chain((14..26).step_by(2));
                    let chars = places
                        .chain((28..32).step_by(2))
                        .map(|at| u16::from_le_bytes([entry[at], entry[at + 1]]))
                        .collect();
                    long.push((entry[0] & 0x3F, chars, entry[13]));
                    continue;
                }
                let short: String = String::from_utf8_lossy(&entry[..11]).into_owned();
                let sum = entry[..11]
                    .iter()
                    .fold(0u8, |sum, &c| sum.rotate_right(1).wrapping_add(c));
                long.sort_by_key(|(n, ..)| *n);
                assert!(
                    long.iter().all(|(.., check)| *check == sum),
                    "{short}'s checksum"
                );
                let name = if long.is_empty() {
                    let (base, ext) = short.split_at(8);
                    let (base, ext) = (base.trim_end(), ext.trim_end());
                    if ext.is_empty() {
                        base.to_string()
                    } else {
                        format!("{base}.{ext}")
                    }
                } else {
                    let chars: Vec<u16> = long
                        .iter()
                        .flat_map(|(_, chars, _)| chars.clone())
                        .collect();
                    let chars = chars.into_iter().take_while(|&c| c != 0 && c != 0xFFFF);
                    String::from_utf16(&chars.collect::<Vec<_>>()).unwrap()
                };
                long.clear();
                let cluster = u16::from_le_bytes([entry[26], entry[27]]) as u32
                    | (u16::from_le_bytes([entry[20], entry[21]]) as u32) << 16;
                let size = u32::from_le_bytes(entry[28..32].try_into().unwrap());
                found.push((name, short, entry[11], cluster, size));
            }
            found
        }
    }

    #[test]
    fn a_card_is_fat32_its_names_long_and_short_its_files_whole() {
        let big: Vec<u8> = (0..70_000).map(|n| (n % 251) as u8).collect();
        let files = vec![
            file("HELLO.PRG", &[0x01, 0x08, 0, 0]),
            file("BlueShips.bin", &big),
            file("BLUESHIP.PAL", b"pal"),
            file("blueships two.bin", b"two"),
            file("EMPTY", b""),
            file("maps/level one.map", b"level"),
        ];
        let image = card::card(&files, WHEN).unwrap();
        assert_eq!(image.len(), 40 << 20, "40M, the least");
        let card = Reader::new(&image);
        let root = card.folder(2);
        let names: Vec<(&str, &str)> = root
            .iter()
            .map(|(n, s, ..)| (n.as_str(), s.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("X16", "X16        "),
                ("BLUESHIP.PAL", "BLUESHIPPAL"),
                ("BlueShips.bin", "BLUESH~1BIN"),
                ("EMPTY", "EMPTY      "),
                ("HELLO.PRG", "HELLO   PRG"),
                ("blueships two.bin", "BLUESH~2BIN"),
                ("maps", "MAPS~1     "),
            ]
        );
        let (.., cluster, size) = root[2];
        assert_eq!(size, 70_000);
        assert_eq!(card.chain(cluster)[..70_000], big[..], "across 3 clusters");
        assert_eq!(card.cluster, 32 << 10, "of 32K, as an SD card's");
        assert_eq!(root[3].3, 0, "empty: no cluster");
        let maps = card.folder(root[6].3);
        assert_eq!(maps[0].1, ".          ");
        assert_eq!(
            (maps[1].1.as_str(), maps[1].3),
            ("..         ", 0),
            "its parent the root"
        );
        assert_eq!(maps[2].0, "level one.map");
        assert_eq!(&card.chain(maps[2].3)[..5], b"level");
    }

    #[test]
    fn a_zip_of_a_program_basic_loads_is_an_x16_game_and_its_facts_say_so() {
        let path = Path::new("Galaxy.zip");
        let game = stored(&[
            ("GALAXY.PRG", &[0x01, 0x08, 0x0B]),
            ("SHIPS.BIN", &[0, 0xA0]),
        ]);
        assert_eq!(system_of(path, &game), System::X16);
        let facts = facts_of(path, game);
        assert!(
            facts.ends_with("\tCOMMANDER X16 GALAXY.PRG\t0\t0\t\tGalaxy.zip"),
            "{facts}"
        );
        // A 2600's zip, of a .bin; a .bin that loads at $0801 isn't a program.
        let atari = stored(&[("Game.bin", &[0x01, 0x08, 0, 0])]);
        assert_eq!(system_of(Path::new("Game.zip"), &atari), System::Atari2600);
        let nes = stored(&[("Game.nes", b"NES\x1A")]);
        assert_eq!(system_of(Path::new("Game.zip"), &nes), System::Nes);
    }

    #[test]
    fn a_programs_game_is_its_folder_unless_others_games_are_in_it() {
        use crate::machines::x16;
        let root = std::env::temp_dir().join(format!("cpu_emu_{}_x16_folders", std::process::id()));
        let contra = root.join("x16-contra");
        std::fs::create_dir_all(contra.join("data")).unwrap();
        std::fs::write(contra.join("CONTRA.PRG"), [0x01, 0x08, 1]).unwrap();
        std::fs::write(contra.join("BANK-0-1.BIN"), [0, 0xA0]).unwrap();
        std::fs::write(contra.join("data/MAP.BIN"), [0, 0xA0]).unwrap();
        assert_eq!(x16::game_of(&contra.join("CONTRA.PRG")), contra);
        let facts = facts_of(&contra.join("CONTRA.PRG"), vec![0x01, 0x08, 1]);
        assert!(facts.contains("\tCOMMANDER X16 CONTRA.PRG\t"), "{facts}");
        // A program among a NES's games: the program alone.
        let nes = root.join("nes");
        std::fs::create_dir_all(&nes).unwrap();
        std::fs::write(nes.join("STRAY.PRG"), [0x01, 0x08, 2]).unwrap();
        std::fs::write(nes.join("Game.nes"), b"NES\x1A").unwrap();
        assert_eq!(x16::game_of(&nes.join("STRAY.PRG")), nes.join("STRAY.PRG"));
        assert_eq!(x16::game_of(&contra), contra, "a folder, itself");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_blank_card_is_made_once_and_kept() {
        use crate::machines::x16;
        let path = std::env::temp_dir().join(format!("cpu_emu_{}_blank.img", std::process::id()));
        let _ = std::fs::remove_file(&path);
        x16::blank_card(&path).unwrap();
        let image = std::fs::read(&path).unwrap();
        let root = Reader::new(&image).folder(2);
        assert_eq!(root.len(), 1, "its volume's label alone");
        std::fs::write(&path, b"kept").unwrap();
        x16::blank_card(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"kept", "not made again");
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_card_is_read_back_as_it_was_made() {
        let big: Vec<u8> = (0..70_000).map(|n| (n % 251) as u8).collect();
        let mut files = vec![
            file("HELLO.PRG", &[0x01, 0x08, 0, 0]),
            file("BlueShips.bin", &big),
            file("EMPTY", b""),
            file("maps/level one.map", b"level"),
            file("maps/deeper/X.BIN", b"x"),
        ];
        let image = card::card(&files, WHEN).unwrap();
        let mut read = card::files_of(&image).unwrap();
        read.sort();
        files.sort();
        assert_eq!(read, files);
        assert_eq!(card::made(&image), card::Made::Before, "no fingerprint");
        assert_eq!(card::made(&[0; 512]), card::Made::Other);
    }

    #[test]
    fn a_kept_file_keeps_its_place_but_not_a_new_version_of_the_games() {
        let game = vec![file("GAME.PRG", b"new"), file("Data/LEVELS.BIN", b"2")];
        let kept = vec![
            file("game.prg", b"old"),
            file("DATA/SAVE.DAT", b"saved"),
            file("SCORES", b"99"),
        ];
        let mut both = card::with_kept(game, kept);
        both.sort();
        assert_eq!(
            both,
            [
                file("Data/LEVELS.BIN", b"2"),
                file("Data/SAVE.DAT", b"saved"),
                file("GAME.PRG", b"new"),
                file("SCORES", b"99"),
            ]
        );
    }

    #[test]
    fn a_games_card_is_made_again_when_the_game_changes_its_saves_kept() {
        use crate::machines::x16;
        let root = std::env::temp_dir().join(format!("cpu_emu_{}_x16_again", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let game = root.join("Galaxy");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("GALAXY.PRG"), [0x01, 0x08, 1]).unwrap();
        std::fs::write(game.join("SCORES"), b"none").unwrap();
        let path = root.join("Galaxy.img");
        x16::make_card(&game, &path).unwrap();
        let on = |path: &Path| {
            let mut files = card::files_of(&std::fs::read(path).unwrap()).unwrap();
            files.sort();
            files
        };
        assert!(matches!(
            card::made(&std::fs::read(&path).unwrap()),
            card::Made::Game(_)
        ));
        // The game saves on it, as its DOS would: the boot sector as it was.
        let saved = |path: &Path, file: CardFile| {
            let image = std::fs::read(path).unwrap();
            let mut files = card::files_of(&image).unwrap();
            files.retain(|(name, _)| *name != file.0);
            files.push(file);
            let mut again = card::card(&files, WHEN).unwrap();
            let start = 2048 * 512;
            again[start..start + 512].copy_from_slice(&image[start..start + 512]);
            std::fs::write(path, again).unwrap();
        };
        saved(&path, file("SAVE.DAT", b"level 3"));
        saved(&path, file("SCORES", b"1000"));
        let before = std::fs::read(&path).unwrap();
        x16::make_card(&game, &path).unwrap();
        assert!(
            std::fs::read(&path).unwrap() == before,
            "the game the same: left as it is"
        );
        // A new version: its files, and what was saved.
        std::fs::write(game.join("GALAXY.PRG"), [0x01, 0x08, 2]).unwrap();
        x16::make_card(&game, &path).unwrap();
        assert_eq!(
            on(&path),
            [
                file("AUTOBOOT.X16", &card::autoboot("GALAXY.PRG")),
                file("GALAXY.PRG", &[0x01, 0x08, 2]),
                file("SAVE.DAT", b"level 3"),
                file("SCORES", b"none"),
            ],
            "the game's own files its new ones"
        );
        // A card from before fingerprints, its program the game's: stamped,
        // its files as they were, its scores kept.
        let old = vec![
            file("AUTOBOOT.X16", &card::autoboot("GALAXY.PRG")),
            file("GALAXY.PRG", &[0x01, 0x08, 2]),
            file("SCORES", b"500"),
        ];
        std::fs::write(&path, card::card(&old, WHEN).unwrap()).unwrap();
        x16::make_card(&game, &path).unwrap();
        assert!(matches!(
            card::made(&std::fs::read(&path).unwrap()),
            card::Made::Game(_)
        ));
        assert_eq!(on(&path), old);
        // Its program not the game's: made again.
        let old = vec![file("GALAXY.PRG", &[0x01, 0x08, 1]), file("SCORES", b"500")];
        std::fs::write(&path, card::card(&old, WHEN).unwrap()).unwrap();
        x16::make_card(&game, &path).unwrap();
        assert_eq!(on(&path)[1], file("GALAXY.PRG", &[0x01, 0x08, 2]));
        // Someone else's card: left alone.
        let mut other = card::card(&old, WHEN).unwrap();
        other[2048 * 512 + 71..2048 * 512 + 82].copy_from_slice(b"MINE       ");
        std::fs::write(&path, &other).unwrap();
        x16::make_card(&game, &path).unwrap();
        assert!(std::fs::read(&path).unwrap() == other);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
