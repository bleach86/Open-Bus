//! Where saves are kept: a battery's RAM, a save state, a game resumed, a
//! machine's own (the X16's clock's RAM). In a folder of their own, the
//! app's, not beside the games, whose folders are left as they are: each
//! under the whole of its game's path there, so that two of a name aren't
//! one. The program sets it as it starts (`--saves`, or the desktop app's
//! folder, `default_folder`); with none set (the tests, a web page), beside
//! the game. One saved beside its game before is read still, until there's
//! one kept apart.
//!
//! A disk's written back to its own image, as a real disk is written, but
//! where that can't be (a network share's, read only), or where a copy's
//! kept apart already: there.

use crate::host::files;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

static KEPT_IN: OnceLock<PathBuf> = OnceLock::new();

/// Saves kept in `folder`, from now on, not beside their games.
pub fn keep_in(folder: &Path) {
    let _ = KEPT_IN.set(folder.to_path_buf());
}

/// Whether saves are kept apart: written there, whatever can be written
/// beside their games.
pub fn kept_apart() -> bool {
    KEPT_IN.get().is_some()
}

/// Where the save that would be at `path`, beside what it's of, is kept.
pub fn place(path: &Path) -> PathBuf {
    place_in(KEPT_IN.get().map(PathBuf::as_path), path)
}

/// Where it's kept, saves kept in `folder`, if they're kept apart: a save
/// of what's there already (an X16 game's card, its state) where it is.
fn place_in(folder: Option<&Path>, path: &Path) -> PathBuf {
    let Some(folder) = folder else {
        return path.to_path_buf();
    };
    let whole = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    if whole.starts_with(folder) {
        return whole;
    }
    let parts = whole.components().filter_map(|part| match part {
        Component::Normal(part) => Some(part),
        _ => None,
    });
    let mut kept = folder.to_path_buf();
    kept.extend(parts);
    kept
}

/// The save that would be at `path`: where it's kept, or else beside what
/// it's of, as it was before saves were kept apart.
pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    files::read(&place(path)).or_else(|_| files::read(path))
}

/// A machine's own save, not a game's, by its `name`: in the folder of
/// saves; or, with none, at `beside`, as it was.
pub fn machine(name: &str, beside: &Path) -> PathBuf {
    KEPT_IN
        .get()
        .map_or_else(|| beside.to_path_buf(), |folder| folder.join(name))
}

/// Whether there's the save that would be at `path`, where it's kept or
/// beside what it's of.
pub fn exists(path: &Path) -> bool {
    files::exists(&place(path)) || files::exists(path)
}

/// Where the disk image at `path`, written, is saved: in itself, unless it
/// can't be written, or a copy's kept apart already, read in its place.
pub fn place_disk(path: &Path) -> PathBuf {
    place_disk_in(KEPT_IN.get().map(PathBuf::as_path), path)
}

/// Where it's saved, saves kept in `folder`, if they're kept apart.
fn place_disk_in(folder: Option<&Path>, path: &Path) -> PathBuf {
    let kept = place_in(folder, path);
    if kept != path && (files::exists(&kept) || files::read_only(path).unwrap_or(true)) {
        kept
    } else {
        path.to_path_buf()
    }
}

/// The desktop app's own folder of saves, as Tauri has its data folder for
/// the app (`app.openbus.desktop`), so that the program run by itself and
/// the app keep them in one place: on Linux in `$XDG_DATA_HOME` or
/// `~/.local/share`; on a Mac in `~/Library/Application Support`; on
/// Windows in `%APPDATA%`.
#[cfg(not(target_arch = "wasm32"))]
pub fn default_folder() -> Option<PathBuf> {
    let set = |name: &str| {
        std::env::var_os(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    let data = if cfg!(windows) {
        set("APPDATA")?
    } else if cfg!(target_os = "macos") {
        set("HOME")?.join("Library/Application Support")
    } else {
        set("XDG_DATA_HOME").or_else(|| set("HOME").map(|home| home.join(".local/share")))?
    };
    Some(data.join("app.openbus.desktop").join("saves"))
}

#[cfg(test)]
mod tests {
    use super::{place_disk_in, place_in};
    use std::path::Path;

    #[test]
    fn a_disk_is_written_in_itself_unless_it_can_t_be_or_a_copy_s_kept() {
        let root = std::env::temp_dir().join(format!("open-bus-saves-{}", std::process::id()));
        let (games, saves) = (root.join("games"), root.join("saves"));
        std::fs::create_dir_all(&games).unwrap();
        let disk = games.join("Game.dsk");
        std::fs::write(&disk, [0]).unwrap();
        assert_eq!(
            place_disk_in(Some(&saves), &disk),
            disk,
            "written in itself"
        );
        assert_eq!(place_disk_in(None, &disk), disk);

        let locked = games.join("Locked.dsk");
        std::fs::write(&locked, [0]).unwrap();
        let mut read_only = std::fs::metadata(&locked).unwrap().permissions();
        read_only.set_readonly(true);
        std::fs::set_permissions(&locked, read_only).unwrap();
        assert_eq!(
            place_disk_in(Some(&saves), &locked),
            place_in(Some(&saves), &locked),
            "not to be written: kept apart"
        );

        let kept = place_in(Some(&saves), &disk);
        std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
        std::fs::write(&kept, [1]).unwrap();
        assert_eq!(
            place_disk_in(Some(&saves), &disk),
            kept,
            "a copy kept apart already, read in its place: still there"
        );
        let mut writable = std::fs::metadata(&locked).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        writable.set_readonly(false);
        let _ = std::fs::set_permissions(&locked, writable);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_save_is_beside_its_game_unless_saves_are_kept_apart() {
        let save = Path::new("/games/nes/Zelda.srm");
        assert_eq!(place_in(None, save), save);
        assert_eq!(
            place_in(Some(Path::new("/app/saves")), save),
            Path::new("/app/saves/games/nes/Zelda.srm")
        );
    }

    #[test]
    fn a_save_of_what_s_kept_apart_already_is_where_it_is() {
        let card = Path::new("/app/saves/games/x16/Game.zip.img.state");
        assert_eq!(place_in(Some(Path::new("/app/saves")), card), card);
    }

    #[test]
    fn two_saves_of_a_name_kept_apart_are_still_two() {
        let kept = Some(Path::new("/app/saves"));
        assert_ne!(
            place_in(kept, Path::new("/nas/a/Game.srm")),
            place_in(kept, Path::new("/nas/b/Game.srm"))
        );
    }
}
