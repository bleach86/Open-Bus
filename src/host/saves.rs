//! Where saves are kept: a battery's RAM, a save state, a game resumed.
//! Beside what they're of, as a rule; or, told so, in a folder of their
//! own, as an app's are when it can't write beside its games (a network
//! share's, read only, on Android), each under the whole of its path there,
//! so that two of a name aren't one. One saved beside its game before is
//! read still, until there's one kept apart.

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

/// Where it's kept, saves kept in `folder`, if they're kept apart.
fn place_in(folder: Option<&Path>, path: &Path) -> PathBuf {
    let Some(folder) = folder else {
        return path.to_path_buf();
    };
    let whole = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
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

#[cfg(test)]
mod tests {
    use super::place_in;
    use std::path::Path;

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
    fn two_saves_of_a_name_kept_apart_are_still_two() {
        let kept = Some(Path::new("/app/saves"));
        assert_ne!(
            place_in(kept, Path::new("/nas/a/Game.srm")),
            place_in(kept, Path::new("/nas/b/Game.srm"))
        );
    }
}
