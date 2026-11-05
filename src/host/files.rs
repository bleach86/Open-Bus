//! Files the machines keep: a disk image, a save state, a battery's RAM.

use std::io;
use std::path::Path;

/// `bytes` saved to `path`: to a file beside it, then put in its place, so
/// that it's never half saved.
pub fn save(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut saving = path.as_os_str().to_owned();
    saving.push(".saving");
    std::fs::write(&saving, bytes)?;
    std::fs::rename(&saving, path)
}
