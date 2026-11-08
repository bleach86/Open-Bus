//! The library as the app keeps it, in `library.sqlite` in its data folder:
//! the folders added, the files hidden, and what's been found, every file in
//! `library` and in the folders, with its size. A change is to the rows it
//! changes, not all of them; and a look through the folders that finds them
//! as they were changes nothing. What was kept before, in `settings.json`
//! and `files.json`, is taken in the first time, and they're left as `.old`.

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

/// A file found: its path, and its size (kept as SQLite's integers are,
/// signed, as no file's too big for).
pub type Found = (String, u64);

/// Where a file was found: the app's own `library`, or a folder added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Library,
    Folders,
}

impl Section {
    /// As the files table has it, and the page.
    fn own(self) -> bool {
        self == Section::Library
    }
}

pub struct Library(Mutex<Connection>);

/// The folders, in the order they were added; the hidden; and the files, in
/// the order they were found, a library's and a folder's apart.
const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS folders (id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE);
    CREATE TABLE IF NOT EXISTS hidden (path TEXT PRIMARY KEY);
    CREATE TABLE IF NOT EXISTS files (
        id INTEGER PRIMARY KEY,
        path TEXT NOT NULL,
        size INTEGER NOT NULL,
        own INTEGER NOT NULL,
        UNIQUE (path, own)
    );
";

/// What the files kept before were taken in by, once: SQLite's own number
/// for it, 0 in a new one.
const TAKEN_IN: i32 = 1;

fn said(e: impl ToString) -> String {
    e.to_string()
}

impl Library {
    /// The library in the folder `data`, made if it isn't there yet, and
    /// what was kept there before taken in.
    pub fn open(data: &Path) -> Result<Library, String> {
        fs::create_dir_all(data).map_err(said)?;
        let library = Library::of(Connection::open(data.join("library.sqlite")).map_err(said)?)?;
        library.take_in(data)?;
        Ok(library)
    }

    /// One only in memory, for the tests.
    #[cfg(test)]
    pub fn in_memory() -> Result<Library, String> {
        Library::of(Connection::open_in_memory().map_err(said)?)
    }

    fn of(connection: Connection) -> Result<Library, String> {
        connection.execute_batch(SCHEMA).map_err(said)?;
        Ok(Library(Mutex::new(connection)))
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>, String> {
        self.0.lock().map_err(said)
    }

    /// `settings.json`'s folders and hidden, and `files.json`'s files, in
    /// `data`, if they're there and it's not been done: all or nothing, and
    /// they're then left as `.old`.
    pub fn take_in(&self, data: &Path) -> Result<(), String> {
        let mut db = self.lock()?;
        let done: i32 = db
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(said)?;
        if done >= TAKEN_IN {
            return Ok(());
        }
        let read = |name: &str| {
            fs::read(data.join(name))
                .ok()
                .and_then(|text| serde_json::from_slice::<Value>(&text).ok())
                .unwrap_or(Value::Null)
        };
        let (settings, found) = (read("settings.json"), read("files.json"));
        let strings = |field: &str| -> Vec<String> {
            settings[field].as_array().map_or(Vec::new(), |list| {
                list.iter()
                    .filter_map(|item| item.as_str().map(String::from))
                    .collect()
            })
        };
        let tx = db.transaction().map_err(said)?;
        for folder in strings("folders") {
            tx.execute(
                "INSERT OR IGNORE INTO folders (path) VALUES (?1)",
                [&folder],
            )
            .map_err(said)?;
        }
        for path in strings("hidden") {
            tx.execute("INSERT OR IGNORE INTO hidden (path) VALUES (?1)", [&path])
                .map_err(said)?;
        }
        for (field, section) in [("library", Section::Library), ("folders", Section::Folders)] {
            for (path, size) in found[field]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(entry)
            {
                tx.execute(
                    "INSERT OR IGNORE INTO files (path, size, own) VALUES (?1, ?2, ?3)",
                    params![path, size as i64, section.own()],
                )
                .map_err(said)?;
            }
        }
        tx.pragma_update(None, "user_version", TAKEN_IN)
            .map_err(said)?;
        tx.commit().map_err(said)?;
        for name in ["settings.json", "files.json"] {
            let file = data.join(name);
            if file.exists() {
                let _ = fs::rename(&file, data.join(format!("{name}.old")));
            }
        }
        Ok(())
    }

    /// The folders added, in the order they were.
    pub fn folders(&self) -> Result<Vec<String>, String> {
        self.paths("SELECT path FROM folders ORDER BY id")
    }

    /// The files hidden.
    pub fn hidden(&self) -> Result<Vec<String>, String> {
        self.paths("SELECT path FROM hidden ORDER BY path")
    }

    fn paths(&self, query: &str) -> Result<Vec<String>, String> {
        let db = self.lock()?;
        let mut statement = db.prepare(query).map_err(said)?;
        let rows = statement.query_map([], |row| row.get(0)).map_err(said)?;
        rows.collect::<Result<_, _>>().map_err(said)
    }

    /// `folder` added, if it isn't already: whether it's new.
    pub fn add_folder(&self, folder: &str) -> Result<bool, String> {
        let db = self.lock()?;
        db.execute("INSERT OR IGNORE INTO folders (path) VALUES (?1)", [folder])
            .map(|added| added == 1)
            .map_err(said)
    }

    /// `folder` no longer looked in, and what was found in it forgotten.
    pub fn remove_folder(&self, folder: &str) -> Result<(), String> {
        let mut db = self.lock()?;
        let tx = db.transaction().map_err(said)?;
        tx.execute("DELETE FROM folders WHERE path = ?1", [folder])
            .map_err(said)?;
        forget_under(&tx, Path::new(folder))?;
        tx.commit().map_err(said)
    }

    /// `path` hidden, and no longer among what's found.
    pub fn hide(&self, path: &str) -> Result<(), String> {
        let mut db = self.lock()?;
        let tx = db.transaction().map_err(said)?;
        tx.execute("INSERT OR IGNORE INTO hidden (path) VALUES (?1)", [path])
            .map_err(said)?;
        tx.execute("DELETE FROM files WHERE path = ?1 AND own = 0", [path])
            .map_err(said)?;
        tx.commit().map_err(said)
    }

    /// `paths` hidden no more.
    pub fn unhide(&self, paths: &[String]) -> Result<(), String> {
        let mut db = self.lock()?;
        let tx = db.transaction().map_err(said)?;
        for path in paths {
            tx.execute("DELETE FROM hidden WHERE path = ?1", [path])
                .map_err(said)?;
        }
        tx.commit().map_err(said)
    }

    /// `entries` found in `section`, put with what's been found: those
    /// hidden, or there already, left out. What's put there.
    pub fn add(&self, section: Section, entries: Vec<Found>) -> Result<Vec<Found>, String> {
        let mut db = self.lock()?;
        let tx = db.transaction().map_err(said)?;
        let mut added = Vec::new();
        for (path, size) in entries {
            let hidden = tx
                .query_row("SELECT 1 FROM hidden WHERE path = ?1", [&path], |_| Ok(()))
                .optional()
                .map_err(said)?
                .is_some();
            let put = !hidden
                && tx
                    .execute(
                        "INSERT OR IGNORE INTO files (path, size, own) VALUES (?1, ?2, ?3)",
                        params![path, size as i64, section.own()],
                    )
                    .map_err(said)?
                    == 1;
            if put {
                added.push((path, size));
            }
        }
        tx.commit().map_err(said)?;
        Ok(added)
    }

    /// Every file found in a folder under `path` forgotten, as when a
    /// watcher sees it's gone: whether there were any.
    pub fn forget_under(&self, path: &Path) -> Result<bool, String> {
        let mut db = self.lock()?;
        let tx = db.transaction().map_err(said)?;
        let forgot = forget_under(&tx, path)?;
        tx.commit().map_err(said)?;
        Ok(forgot)
    }

    /// What a look through everything found: `library`'s, and the folders',
    /// the hidden left out of them. In place of what was found before, if
    /// it's not the same: whether it wasn't.
    pub fn replace(&self, library: Vec<Found>, folders: Vec<Found>) -> Result<bool, String> {
        let hidden = self.hidden()?;
        let folders: Vec<Found> = folders
            .into_iter()
            .filter(|(path, _)| !hidden.contains(path))
            .collect();
        let mut db = self.lock()?;
        let tx = db.transaction().map_err(said)?;
        let mut changed = false;
        for (section, now) in [(Section::Library, library), (Section::Folders, folders)] {
            if section_files(&tx, section)? == now {
                continue;
            }
            changed = true;
            tx.execute("DELETE FROM files WHERE own = ?1", [section.own()])
                .map_err(said)?;
            for (path, size) in now {
                tx.execute(
                    "INSERT OR IGNORE INTO files (path, size, own) VALUES (?1, ?2, ?3)",
                    params![path, size as i64, section.own()],
                )
                .map_err(said)?;
            }
        }
        tx.commit().map_err(said)?;
        Ok(changed)
    }

    /// All that's been found, as the page has it: `root`, where the app's
    /// library is, and each section's files, a path and a size each.
    pub fn found(&self, root: &str) -> Result<Value, String> {
        let db = self.lock()?;
        let listed = |section| -> Result<Vec<Value>, String> {
            Ok(section_files(&db, section)?
                .into_iter()
                .map(|(path, size)| json!({ "path": path, "size": size }))
                .collect())
        };
        Ok(json!({
            "root": root,
            "library": listed(Section::Library)?,
            "folders": listed(Section::Folders)?,
        }))
    }
}

/// A file as the page has one: its path, and its size.
pub fn entry(value: &Value) -> Option<Found> {
    Some((
        value["path"].as_str()?.to_string(),
        value["size"].as_u64().unwrap_or(0),
    ))
}

/// `section`'s files, in the order they were found.
fn section_files(db: &Connection, section: Section) -> Result<Vec<Found>, String> {
    let mut statement = db
        .prepare("SELECT path, size FROM files WHERE own = ?1 ORDER BY id")
        .map_err(said)?;
    let rows = statement
        .query_map([section.own()], |row| {
            Ok((row.get(0)?, row.get::<_, i64>(1)? as u64))
        })
        .map_err(said)?;
    rows.collect::<Result<_, _>>().map_err(said)
}

/// The folders' files under `path`, or it, forgotten: whether there were
/// any. Under it as a path is, a part at a time, not as text.
fn forget_under(db: &Connection, path: &Path) -> Result<bool, String> {
    let gone: Vec<String> = section_files(db, Section::Folders)?
        .into_iter()
        .map(|(file, _)| file)
        .filter(|file| Path::new(file).starts_with(path))
        .collect();
    for file in &gone {
        db.execute("DELETE FROM files WHERE path = ?1 AND own = 0", [file])
            .map_err(said)?;
    }
    Ok(!gone.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{Found, Library, Section};
    use serde_json::json;
    use std::fs;
    use std::path::Path;

    fn found(path: &str, size: u64) -> Found {
        (path.to_string(), size)
    }

    /// A folder of its own for a test, empty.
    fn data(name: &str) -> std::path::PathBuf {
        let data = std::env::temp_dir().join(format!("open_bus_{}_{name}", std::process::id()));
        let _ = fs::remove_dir_all(&data);
        fs::create_dir_all(&data).unwrap();
        data
    }

    #[test]
    fn what_was_kept_before_is_taken_in_once() {
        let data = data("taken_in");
        let settings =
            json!({ "folders": ["/games/nes", "/games/apple"], "hidden": ["/games/nes/b.zip"] });
        let files = json!({
            "root": "/data/library",
            "library": [{ "path": "/data/library/disksys.rom", "size": 8192 }],
            "folders": [{ "path": "/games/nes/a.zip", "size": 10 }, { "path": "/games/nes/c.zip", "size": 30 }],
        });
        fs::write(data.join("settings.json"), settings.to_string()).unwrap();
        fs::write(data.join("files.json"), files.to_string()).unwrap();
        let library = Library::open(&data).unwrap();
        assert_eq!(library.folders().unwrap(), ["/games/nes", "/games/apple"]);
        assert_eq!(library.hidden().unwrap(), ["/games/nes/b.zip"]);
        assert_eq!(
            library.found("/data/library").unwrap(),
            files,
            "the page's, as it was"
        );
        assert!(!data.join("settings.json").exists() && data.join("settings.json.old").exists());
        assert!(!data.join("files.json").exists() && data.join("files.json.old").exists());
        drop(library);

        // A settings.json come back, as from an older app: not taken in again.
        fs::write(
            data.join("settings.json"),
            json!({ "folders": ["/elsewhere"] }).to_string(),
        )
        .unwrap();
        let library = Library::open(&data).unwrap();
        assert_eq!(library.folders().unwrap(), ["/games/nes", "/games/apple"]);
        drop(library);
        fs::remove_dir_all(&data).unwrap();
    }

    #[test]
    fn a_new_one_with_nothing_before_is_empty() {
        let data = data("empty");
        let library = Library::open(&data).unwrap();
        assert!(library.folders().unwrap().is_empty());
        assert_eq!(
            library.found("/root").unwrap(),
            json!({ "root": "/root", "library": [], "folders": [] })
        );
        drop(library);
        fs::remove_dir_all(&data).unwrap();
    }

    #[test]
    fn files_added_are_those_not_hidden_or_there_already() {
        let library = Library::in_memory().unwrap();
        library.hide("/g/hidden.zip").unwrap();
        library
            .add(Section::Folders, vec![found("/g/a.zip", 1)])
            .unwrap();
        let added = library
            .add(
                Section::Folders,
                vec![
                    found("/g/a.zip", 1),
                    found("/g/hidden.zip", 2),
                    found("/g/b.zip", 3),
                ],
            )
            .unwrap();
        assert_eq!(added, [found("/g/b.zip", 3)]);
        library.unhide(&["/g/hidden.zip".to_string()]).unwrap();
        assert_eq!(
            library
                .add(Section::Folders, vec![found("/g/hidden.zip", 2)])
                .unwrap()
                .len(),
            1,
            "back once it's not hidden"
        );
    }

    #[test]
    fn a_file_hidden_is_no_longer_found() {
        let library = Library::in_memory().unwrap();
        library
            .add(
                Section::Folders,
                vec![found("/g/a.zip", 1), found("/g/b.zip", 2)],
            )
            .unwrap();
        library.hide("/g/a.zip").unwrap();
        assert_eq!(
            library.found("/r").unwrap()["folders"],
            json!([{ "path": "/g/b.zip", "size": 2 }])
        );
        assert_eq!(library.hidden().unwrap(), ["/g/a.zip"]);
    }

    #[test]
    fn a_folder_removed_takes_its_files_and_no_others() {
        let library = Library::in_memory().unwrap();
        assert!(library.add_folder("/g/nes").unwrap());
        assert!(!library.add_folder("/g/nes").unwrap(), "added twice");
        library.add_folder("/g/nes2").unwrap();
        library
            .add(
                Section::Folders,
                vec![found("/g/nes/a.zip", 1), found("/g/nes2/b.zip", 2)],
            )
            .unwrap();
        library.remove_folder("/g/nes").unwrap();
        assert_eq!(library.folders().unwrap(), ["/g/nes2"]);
        assert_eq!(
            library.found("/r").unwrap()["folders"],
            json!([{ "path": "/g/nes2/b.zip", "size": 2 }]),
            "a folder whose name starts the same"
        );
        assert!(library.forget_under(Path::new("/g/nes2")).unwrap());
        assert!(
            !library.forget_under(Path::new("/g/nes2")).unwrap(),
            "nothing left"
        );
    }

    #[test]
    fn looked_through_again_its_only_changed_if_its_not_the_same() {
        let library = Library::in_memory().unwrap();
        library.hide("/g/hidden.zip").unwrap();
        let own = vec![found("/data/library/a.rom", 4)];
        let folders = vec![
            found("/g/b.zip", 2),
            found("/g/hidden.zip", 9),
            found("/g/a.zip", 1),
        ];
        assert!(
            library.replace(own.clone(), folders.clone()).unwrap(),
            "new"
        );
        assert!(
            !library.replace(own.clone(), folders.clone()).unwrap(),
            "the same"
        );
        assert_eq!(
            library.found("/data/library").unwrap()["folders"],
            json!([{ "path": "/g/b.zip", "size": 2 }, { "path": "/g/a.zip", "size": 1 }]),
            "in the order found, the hidden left out"
        );
        assert!(
            library
                .replace(own, vec![found("/g/b.zip", 3), found("/g/a.zip", 1)])
                .unwrap(),
            "a size changed"
        );
    }
}
