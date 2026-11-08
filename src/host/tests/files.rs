//! The files, on the host's disk and in a web page's memory, each doing as
//! the other does; a web page's own, what it's written kept for it.

#[cfg(test)]
#[path = "../files_web.rs"]
mod web;

#[cfg(test)]
mod tests {
    use super::web;
    use crate::host::files;
    use std::path::Path;

    #[test]
    fn a_file_saved_is_read_back_and_written_over_in_place() {
        let folder = std::env::temp_dir().join(format!("cpu_emu_{}_files", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("a");
        files::save(&file, &[1, 2, 3, 4]).unwrap();
        files::write_at(&file, 1, &[9, 8]).unwrap();
        assert_eq!(files::read(&file).unwrap(), [1, 9, 8, 4]);
        assert_eq!(files::size(&file), Some(4));
        assert!(files::exists(&file) && !files::read_only(&file).unwrap());
        assert!(files::is_folder(&folder) && !files::is_folder(&file));
        assert_eq!(
            files::list(&folder).unwrap(),
            [file.clone()],
            "a file left beside it"
        );
        std::fs::remove_dir_all(&folder).unwrap();
        assert!(!files::exists(&file) && files::size(&file).is_none());
        assert!(files::read(&file).is_err() && files::read_only(&file).is_err());
    }

    #[test]
    fn a_file_saved_later_is_saved_its_latest() {
        let file = std::env::temp_dir().join(format!("cpu_emu_{}_later", std::process::id()));
        for byte in 0..100 {
            files::save_later(&file, vec![byte; 0x2000]);
        }
        files::all_saved();
        assert_eq!(files::read(&file).unwrap(), [99; 0x2000]);
        std::fs::remove_file(&file).unwrap();
        files::save_later(Path::new("/no/such/folder/a"), vec![1]);
        files::all_saved();

        web::save_later(Path::new("later.srm"), vec![7]);
        web::all_saved();
        assert_eq!(web::read(Path::new("later.srm")).unwrap(), [7], "the web's");
    }

    #[test]
    fn a_web_pages_files_do_the_same() {
        let (folder, file) = (Path::new("disks"), Path::new("disks/a"));
        assert!(web::read(file).is_err() && !web::exists(file) && web::read_only(file).is_err());
        web::put(file, vec![1, 2, 3, 4]);
        assert!(
            web::written().is_empty(),
            "what the page put in called written"
        );
        web::write_at(file, 1, &[9, 8]).unwrap();
        assert_eq!(web::read(file).unwrap(), [1, 9, 8, 4]);
        assert_eq!(
            (web::size(file), web::read_only(file).unwrap()),
            (Some(4), false)
        );
        assert!(web::is_folder(folder) && !web::is_folder(file));
        assert_eq!(web::list(folder).unwrap(), [file.to_path_buf()]);
        assert_eq!(web::canonical(file), file);
        assert!(web::write_at(Path::new("no/such"), 0, &[1]).is_err());
    }

    #[test]
    fn a_web_page_is_told_what_was_written_once() {
        let file = Path::new("game.srm");
        web::save(file, &[5]).unwrap();
        web::save(file, &[6]).unwrap();
        assert_eq!(
            web::written(),
            [file.to_path_buf()],
            "saved twice, told once"
        );
        assert!(web::written().is_empty(), "told again");
        assert_eq!(web::read(file).unwrap(), [6]);
    }

    #[test]
    fn a_web_page_is_told_only_the_parts_written_over() {
        let file = Path::new("volume.hdv");
        web::put(file, vec![0; 2 * web::PART + 100]);

        // A write across the first two parts, and one in the short last.
        web::write_at(file, web::PART as u64 - 2, &[1, 2, 3, 4]).unwrap();
        web::write_at(file, 2 * web::PART as u64 + 10, &[5]).unwrap();
        assert!(
            web::written().is_empty(),
            "the whole file told, for parts of it"
        );
        assert_eq!(web::changed(), [file.to_path_buf()]);
        let offsets = [0, web::PART as u32, 2 * web::PART as u32];
        assert_eq!(web::take_changed_parts(file), offsets);
        assert!(
            web::changed().is_empty() && web::take_changed_parts(file).is_empty(),
            "told again"
        );

        let first = web::read_part(file, 0).unwrap();
        let second = web::read_part(file, web::PART as u32).unwrap();
        let last = web::read_part(file, 2 * web::PART as u32).unwrap();
        assert_eq!(
            (first.len(), second.len(), last.len()),
            (web::PART, web::PART, 100)
        );
        assert_eq!(
            (&first[web::PART - 2..], &second[..2], last[10]),
            (&[1, 2][..], &[3, 4][..], 5)
        );
        assert!(web::read_part(Path::new("no/such"), 0).is_none());
    }

    #[test]
    fn a_file_saved_whole_is_told_whole_not_in_parts() {
        let (file, other) = (Path::new("a.hdv"), Path::new("b.hdv"));
        web::put(file, vec![0; web::PART]);
        web::write_at(file, 0, &[1]).unwrap();
        web::save(file, &[2, 3]).unwrap();
        assert!(web::changed().is_empty(), "parts of what was saved whole");

        web::save(other, &[0; 4]).unwrap();
        web::write_at(other, 1, &[1]).unwrap();
        assert!(
            web::changed().is_empty(),
            "parts of what's already to be saved whole"
        );
        assert_eq!(web::written(), [file.to_path_buf(), other.to_path_buf()]);
        assert_eq!(web::read(other).unwrap(), [0, 1, 0, 0]);
    }
}
