//! Where saves are kept: a battery's RAM, a save state, a game resumed, an
//! X16 game's card, a machine's own (the X16's clock's RAM). Not beside the
//! games, whose folders are left as they are, but under a root of their
//! own, as a MiSTer keeps them: what a game saves in `saves`, its states in
//! `states`, each in its system's folder, by the game's name without its
//! file's extension, as `saves/nes/Zelda.srm` and `states/nes/Zelda.state`.
//! Two games of a name, in two folders, share them, as on a MiSTer.
//!
//! The program sets the root as it starts (`--saves`, or the desktop app's
//! data folder, `default_root`), and the system it's running (`system`);
//! with no root (the tests, a web page), they're beside the game. What was
//! kept before is still read, until there's one kept here: beside the game,
//! or where the Android app kept them, under the whole of the game's path
//! in `saves`.
//!
//! A disk's written back to its own image, as a real disk is written, but
//! where that can't be (a network share's, read only), or where a copy's
//! kept apart already: there.

use crate::host::files;
use std::ffi::OsString;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

static ROOT: OnceLock<PathBuf> = OnceLock::new();
static SYSTEM: OnceLock<String> = OnceLock::new();

/// Saves kept under `root` from now on, not beside their games.
pub fn keep_in(root: &Path) {
    let _ = ROOT.set(root.to_path_buf());
}

/// The system saves are of from now on: its folder's name, as `nes`.
pub fn system(name: &str) {
    let _ = SYSTEM.set(name.to_string());
}

/// The root saves are kept under, if they're kept apart.
pub fn root() -> Option<&'static Path> {
    ROOT.get().map(PathBuf::as_path)
}

/// Whether saves are kept apart: written there, whatever can be written
/// beside their games.
pub fn kept_apart() -> bool {
    ROOT.get().is_some()
}

/// The kinds of save, as the app lists and clears them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A battery's RAM, a Disk System's disk, a 7800's high scores.
    Save,
    State,
    Resume,
    /// An X16 game's card, with what it's saved on it.
    Card,
    /// A disk's changes, kept apart as its own image couldn't be written.
    Disk,
}

/// The extensions of the disk images kept apart.
const DISKS: [&str; 7] = ["dsk", "do", "po", "nib", "woz", "hdv", "2mg"];

/// The extensions of games' files, left out of what's saved of them: a
/// state of `Zelda.nes` is `Zelda.state`.
const GAMES: [&str; 16] = [
    "nes", "fds", "a26", "a78", "bin", "zip", "prg", "rom", "img", "dsk", "do", "po", "nib", "woz",
    "hdv", "2mg",
];

impl Kind {
    pub const ALL: [Kind; 5] = [
        Kind::Save,
        Kind::State,
        Kind::Resume,
        Kind::Card,
        Kind::Disk,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Save => "save",
            Kind::State => "state",
            Kind::Resume => "resume",
            Kind::Card => "card",
            Kind::Disk => "disk",
        }
    }

    pub fn named(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// The kind of the save at `path`, by its file's extension.
    pub fn of(path: &Path) -> Option<Kind> {
        let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
        Some(match extension.as_str() {
            "srm" => Kind::Save,
            "state" => Kind::State,
            "resume" => Kind::Resume,
            "img" => Kind::Card,
            disk if DISKS.contains(&disk) => Kind::Disk,
            _ => return None,
        })
    }

    /// Whether it's kept with the states, not the saves.
    fn a_state(self) -> bool {
        matches!(self, Kind::State | Kind::Resume)
    }
}

/// The name the save at `path` is kept by: its game's without the game's
/// own extension, as `Zelda.nes.state`'s is `Zelda.state`. A disk's, its
/// own.
fn kept_name(path: &Path) -> Option<OsString> {
    let name = path.file_name()?;
    let kind = Kind::of(path);
    let inner = Path::new(path.file_stem()?);
    let game = inner.extension().is_some_and(|extension| {
        GAMES.contains(&extension.to_string_lossy().to_ascii_lowercase().as_str())
    });
    if !game || kind.is_none_or(|kind| kind == Kind::Disk) {
        return Some(name.to_os_string());
    }
    let mut kept = inner.file_stem()?.to_os_string();
    kept.push(".");
    kept.push(path.extension()?);
    Some(kept)
}

/// Where the save that would be at `path`, beside what it's of, is kept.
pub fn place(path: &Path) -> PathBuf {
    place_in(root(), SYSTEM.get().map(String::as_str), path)
}

/// Where it's kept, under `root`, if they're kept apart, of `system`.
fn place_in(root: Option<&Path>, system: Option<&str>, path: &Path) -> PathBuf {
    let (Some(root), Some(name)) = (root, kept_name(path)) else {
        return path.to_path_buf();
    };
    let folder = if Kind::of(path).is_some_and(Kind::a_state) {
        "states"
    } else {
        "saves"
    };
    root.join(folder).join(system.unwrap_or("other")).join(name)
}

/// Where the Android app kept it before: in `saves` under the root, under
/// the whole of its path.
fn before_in(root: Option<&Path>, path: &Path) -> Option<PathBuf> {
    let whole = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let parts = whole.components().filter_map(|part| match part {
        Component::Normal(part) => Some(part),
        _ => None,
    });
    let mut kept = root?.join("saves");
    kept.extend(parts);
    Some(kept)
}

/// Where the save at `path` might be: kept, as it was kept before, beside
/// it; the first there is to be read.
fn candidates(path: &Path) -> Vec<PathBuf> {
    let mut all = vec![place(path)];
    all.extend(before_in(root(), path));
    all.push(path.to_path_buf());
    all.dedup();
    all
}

/// The save that would be at `path`: where it's kept, or else as it was
/// kept before saves were kept apart.
pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    let mut error = None;
    for place in candidates(path) {
        match files::read(&place) {
            Ok(bytes) => return Ok(bytes),
            Err(e) => error = error.or(Some(e)),
        }
    }
    Err(error.unwrap_or_else(|| io::Error::from(io::ErrorKind::NotFound)))
}

/// Whether there's the save that would be at `path`, kept or as before.
pub fn exists(path: &Path) -> bool {
    candidates(path).iter().any(|place| files::exists(place))
}

/// A machine's own save, not a game's, by its `name`: with the system's
/// saves; or, with none kept apart, at `beside`, as it was.
pub fn machine(name: &str, beside: &Path) -> PathBuf {
    match root() {
        Some(root) => root
            .join("saves")
            .join(SYSTEM.get().map_or("other", String::as_str))
            .join(name),
        None => beside.to_path_buf(),
    }
}

/// Where the disk image at `path`, written, is saved: in itself, unless it
/// can't be written, or a copy's kept apart already, read in its place.
pub fn place_disk(path: &Path) -> PathBuf {
    place_disk_in(root(), SYSTEM.get().map(String::as_str), path)
}

/// Where it's saved, under `root`, if they're kept apart, of `system`.
fn place_disk_in(root: Option<&Path>, system: Option<&str>, path: &Path) -> PathBuf {
    let kept = place_in(root, system, path);
    let kept_before = before_in(root, path).is_some_and(|before| files::exists(&before));
    if kept != path
        && (files::exists(&kept) || kept_before || files::read_only(path).unwrap_or(true))
    {
        kept
    } else {
        path.to_path_buf()
    }
}

/// The saves there are of a game whose files are `files` (its ROM, its
/// disks, or the X16's card it's played from): where they're kept, and as
/// they were before; its card and disks only where they're kept apart, those
/// beside it being its own.
pub fn of_game(files: &[PathBuf]) -> Vec<(Kind, PathBuf)> {
    let mut found: Vec<(Kind, PathBuf)> = Vec::new();
    let mut add = |kind: Kind, path: PathBuf| {
        if files::exists(&path) && !found.iter().any(|(_, already)| *already == path) {
            found.push((kind, path));
        }
    };
    for file in files {
        let state = crate::host::save_state::path(file);
        for (kind, save) in [
            (Kind::Save, file.with_extension("srm")),
            (Kind::Resume, state.with_extension("resume")),
            (Kind::State, state),
        ] {
            for place in candidates(&save) {
                add(kind, place);
            }
        }
        // Its card, as the X16's made for it, or the card it is; or the
        // disk's changes kept apart.
        let mut card = file.as_os_str().to_owned();
        card.push(".img");
        add(Kind::Card, place(Path::new(&card)));
        let kept = place(file);
        match Kind::of(file) {
            Some(Kind::Card) if root().is_some_and(|root| file.starts_with(root)) => {
                add(Kind::Card, file.clone());
            }
            Some(Kind::Disk) if kept != *file => {
                add(Kind::Disk, kept);
                if let Some(before) = before_in(root(), file) {
                    add(Kind::Disk, before);
                }
            }
            _ => {}
        }
    }
    found
}

/// Every save there is under the root, of a kind it knows.
pub fn all() -> Vec<(Kind, PathBuf)> {
    fn walk(folder: &Path, found: &mut Vec<(Kind, PathBuf)>) {
        for path in files::list(folder).unwrap_or_default() {
            if files::is_folder(&path) {
                walk(&path, found);
            } else if let Some(kind) = Kind::of(&path) {
                found.push((kind, path));
            }
        }
    }
    let mut found = Vec::new();
    if let Some(root) = root() {
        walk(&root.join("saves"), &mut found);
        walk(&root.join("states"), &mut found);
    }
    found
}

/// The folders a save at `path` was in, left empty as it's gone, gone too,
/// up to the root, which stays.
#[cfg(not(target_arch = "wasm32"))]
pub fn tidy(path: &Path) {
    let Some(root) = root() else { return };
    let mut folder = path.parent();
    while let Some(here) = folder.filter(|here| here.starts_with(root) && *here != root) {
        if std::fs::remove_dir(here).is_err() {
            break;
        }
        folder = here.parent();
    }
}

/// The desktop app's data folder, the root its saves are kept under, as
/// Tauri has it for the app (`app.openbus.desktop`), so that the program
/// run by itself and the app keep them in one place: on Linux in
/// `$XDG_DATA_HOME` or `~/.local/share`; on a Mac in `~/Library/Application
/// Support`; on Windows in `%APPDATA%`.
#[cfg(not(target_arch = "wasm32"))]
pub fn default_root() -> Option<PathBuf> {
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
    Some(data.join("app.openbus.desktop"))
}

#[cfg(test)]
mod tests {
    use super::{place_disk_in, place_in};
    use std::path::Path;

    /// The root saves are kept under, in the tests.
    fn root() -> Option<&'static Path> {
        Some(Path::new("/app"))
    }

    #[test]
    fn a_save_is_beside_its_game_unless_saves_are_kept_apart() {
        let save = Path::new("/games/nes/Zelda.srm");
        assert_eq!(place_in(None, Some("nes"), save), save);
        assert_eq!(
            place_in(root(), Some("nes"), save),
            Path::new("/app/saves/nes/Zelda.srm")
        );
    }

    #[test]
    fn states_are_kept_apart_from_saves_by_the_game_s_name() {
        assert_eq!(
            place_in(root(), Some("nes"), Path::new("/games/Zelda.nes.state")),
            Path::new("/app/states/nes/Zelda.state")
        );
        assert_eq!(
            place_in(
                root(),
                Some("nes"),
                Path::new("/games/Super Mario Bros. (World).zip.resume")
            ),
            Path::new("/app/states/nes/Super Mario Bros. (World).resume")
        );
        assert_eq!(
            place_in(root(), Some("x16"), Path::new("/games/Mr. Do.state")),
            Path::new("/app/states/x16/Mr. Do.state"),
            "a game with no extension of its own keeps its name"
        );
    }

    #[test]
    fn an_x16_game_s_card_and_its_state_are_by_its_name_too() {
        let card = place_in(root(), Some("x16"), Path::new("/games/Wumpus.zip.img"));
        assert_eq!(card, Path::new("/app/saves/x16/Wumpus.img"));
        assert_eq!(place_in(root(), Some("x16"), &card), card, "where it is");
        assert_eq!(
            place_in(
                root(),
                Some("x16"),
                Path::new("/app/saves/x16/Wumpus.img.state")
            ),
            Path::new("/app/states/x16/Wumpus.state")
        );
    }

    #[test]
    fn a_disk_is_written_in_itself_unless_it_can_t_be_or_a_copy_s_kept() {
        let root = std::env::temp_dir().join(format!("open-bus-saves-{}", std::process::id()));
        let games = root.join("games");
        let apart = root.join("app");
        let kept = Some(apart.as_path());
        std::fs::create_dir_all(&games).unwrap();
        let disk = games.join("Game.dsk");
        std::fs::write(&disk, [0]).unwrap();
        assert_eq!(
            place_disk_in(kept, Some("apple2"), &disk),
            disk,
            "written in itself"
        );
        assert_eq!(place_disk_in(None, Some("apple2"), &disk), disk);

        let locked = games.join("Locked.dsk");
        std::fs::write(&locked, [0]).unwrap();
        let mut read_only = std::fs::metadata(&locked).unwrap().permissions();
        read_only.set_readonly(true);
        std::fs::set_permissions(&locked, read_only).unwrap();
        assert_eq!(
            place_disk_in(kept, Some("apple2"), &locked),
            apart.join("saves/apple2/Locked.dsk"),
            "not to be written: kept apart"
        );

        let copy = place_in(kept, Some("apple2"), &disk);
        std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
        std::fs::write(&copy, [1]).unwrap();
        assert_eq!(
            place_disk_in(kept, Some("apple2"), &disk),
            copy,
            "a copy kept apart already, read in its place: still there"
        );
        let mut writable = std::fs::metadata(&locked).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        writable.set_readonly(false);
        let _ = std::fs::set_permissions(&locked, writable);
        let _ = std::fs::remove_dir_all(&root);
    }
}
