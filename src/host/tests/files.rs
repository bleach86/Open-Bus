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
        assert_eq!(files::list(&folder).unwrap(), [file.clone()], "a file left beside it");
        std::fs::remove_dir_all(&folder).unwrap();
        assert!(!files::exists(&file) && files::size(&file).is_none());
        assert!(files::read(&file).is_err() && files::read_only(&file).is_err());
    }

    #[test]
    fn a_web_pages_files_do_the_same() {
        let (folder, file) = (Path::new("disks"), Path::new("disks/a"));
        assert!(web::read(file).is_err() && !web::exists(file) && web::read_only(file).is_err());
        web::put(file, vec![1, 2, 3, 4]);
        assert!(web::written().is_empty(), "what the page put in called written");
        web::write_at(file, 1, &[9, 8]).unwrap();
        assert_eq!(web::read(file).unwrap(), [1, 9, 8, 4]);
        assert_eq!((web::size(file), web::read_only(file).unwrap()), (Some(4), false));
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
        assert_eq!(web::written(), [file.to_path_buf()], "saved twice, told once");
        assert!(web::written().is_empty(), "told again");
        assert_eq!(web::read(file).unwrap(), [6]);
    }
}
