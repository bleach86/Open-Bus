//! Zip archives: a file found by its name, stored or deflated; one not
//! there, one damaged, and what isn't a zip at all.

#[cfg(test)]
mod tests {
    use crate::host::zip::{crc32, file};

    /// A zip of `files`, each its name, its bytes and whether it's deflated.
    fn zip(files: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let (mut archive, mut directory) = (Vec::new(), Vec::new());
        for &(name, bytes, deflated) in files {
            let data = if deflated {
                miniz_oxide::deflate::compress_to_vec(bytes, 6)
            } else {
                bytes.to_vec()
            };
            let method: u16 = if deflated { 8 } else { 0 };
            let sizes = [crc32(bytes), data.len() as u32, bytes.len() as u32];
            let local = archive.len() as u32;
            archive.extend(0x0403_4B50u32.to_le_bytes());
            archive.extend([20, 0, 0, 0]); // the version needed; flags
            archive.extend(method.to_le_bytes());
            archive.extend([0; 4]); // its time and date
            sizes
                .iter()
                .for_each(|size| archive.extend(size.to_le_bytes()));
            archive.extend((name.len() as u16).to_le_bytes());
            archive.extend([0, 0]); // no extra field
            archive.extend(name.as_bytes());
            archive.extend(&data);

            directory.extend(0x0201_4B50u32.to_le_bytes());
            directory.extend([20, 0, 20, 0, 0, 0]); // made by; needed; flags
            directory.extend(method.to_le_bytes());
            directory.extend([0; 4]);
            sizes
                .iter()
                .for_each(|size| directory.extend(size.to_le_bytes()));
            directory.extend((name.len() as u16).to_le_bytes());
            directory.extend([0; 8]); // extra, comment, disk, internal attributes
            directory.extend([0; 4]); // external attributes
            directory.extend(local.to_le_bytes());
            directory.extend(name.as_bytes());
        }
        let start = archive.len() as u32;
        archive.extend(&directory);
        archive.extend(0x0605_4B50u32.to_le_bytes());
        archive.extend([0; 4]); // this disk, the directory's
        archive.extend((files.len() as u16).to_le_bytes());
        archive.extend((files.len() as u16).to_le_bytes());
        archive.extend((directory.len() as u32).to_le_bytes());
        archive.extend(start.to_le_bytes());
        archive.extend(7u16.to_le_bytes());
        archive.extend(b"comment"); // a comment after it all
        archive
    }

    #[test]
    fn a_file_is_found_by_its_name() {
        let game: Vec<u8> = (0..40_000).map(|n| (n % 251) as u8).collect();
        let archive = zip(&[
            ("readme.txt", b"hello", false),
            ("Game (USA).nes", &game, true),
        ]);
        let nes = |name: &str| name.to_lowercase().ends_with(".nes");
        assert_eq!(
            file(&archive, nes).unwrap(),
            ("Game (USA).nes".to_string(), game.clone()),
            "deflated"
        );
        assert_eq!(
            file(&archive, |name| name == "readme.txt").unwrap().1,
            b"hello",
            "stored"
        );
        assert!(
            file(&archive, |name| name.ends_with(".fds")).is_err(),
            "none such"
        );
    }

    #[test]
    fn what_isnt_as_it_was_zipped_is_refused() {
        let game = vec![0x5A; 1000];
        let mut archive = zip(&[("game.nes", &game, false)]);
        archive[30 + "game.nes".len() + 10] ^= 1; // a byte of it changed
        let refused = file(&archive, |_| true).unwrap_err();
        assert!(refused.contains("CRC"), "{refused}");
        assert_eq!(
            file(b"not a zip at all, not one", |_| true).unwrap_err(),
            "not a zip"
        );
        assert_eq!(file(&[], |_| true).unwrap_err(), "not a zip");
    }
}
