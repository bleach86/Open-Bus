//! The library as the app keeps it, in `library.sqlite` in its data folder:
//! the folders added, the files hidden, and what's been found, every file in
//! `library` and in the folders, with its size; of the games, those that
//! are favourites, and those played, how often, for how long, and when last;
//! what's chosen of a game for itself, its own settings over the app's;
//! and what's known of each NES game's file without running it, as the
//! emulator says, for as long as it's the size it was and the emulator the
//! same. A change is to the rows it
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
    CREATE TABLE IF NOT EXISTS facts (
        path TEXT PRIMARY KEY,
        size INTEGER NOT NULL,
        crc TEXT NOT NULL,
        mapper TEXT NOT NULL,
        famicom INTEGER NOT NULL,
        zapper INTEGER NOT NULL,
        problem TEXT NOT NULL,
        emulator TEXT NOT NULL DEFAULT ''
    );
    CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, settings TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS games (
        key TEXT PRIMARY KEY,
        machine TEXT NOT NULL,
        favourite INTEGER NOT NULL DEFAULT 0,
        plays INTEGER NOT NULL DEFAULT 0,
        seconds INTEGER NOT NULL DEFAULT 0,
        last INTEGER
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
        // Facts kept before they said which emulator: none, so learned again.
        let has = connection
            .query_row(
                "SELECT 1 FROM pragma_table_info('facts') WHERE name = 'emulator'",
                [],
                |_| Ok(()),
            )
            .optional()
            .map_err(said)?
            .is_some();
        if !has {
            connection
                .execute_batch("ALTER TABLE facts ADD COLUMN emulator TEXT NOT NULL DEFAULT ''")
                .map_err(said)?;
        }
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

    /// `machine` played, started at `started` (seconds since 1970) and for
    /// `seconds`: once more, for longer, and how it was started kept.
    pub fn played(&self, machine: &Value, started: u64, seconds: u64) -> Result<(), String> {
        let Some((key, kept)) = game(machine) else {
            return Ok(());
        };
        let db = self.lock()?;
        db.execute(
            "INSERT INTO games (key, machine, plays, seconds, last) VALUES (?1, ?2, 1, ?3, ?4)
             ON CONFLICT (key) DO UPDATE SET
                 machine = ?2, plays = plays + 1, seconds = seconds + ?3, last = ?4",
            params![key, kept.to_string(), seconds as i64, started as i64],
        )
        .map(|_| ())
        .map_err(said)
    }

    /// `machine` a favourite, `on`, or not. How it was last started, if it
    /// has been, kept: a favourite started is started as that was.
    pub fn favour(&self, machine: &Value, on: bool) -> Result<(), String> {
        let Some((key, kept)) = game(machine) else {
            return Err("not a game".to_string());
        };
        let db = self.lock()?;
        db.execute(
            "INSERT INTO games (key, machine, favourite) VALUES (?1, ?2, ?3)
             ON CONFLICT (key) DO UPDATE SET favourite = ?3",
            params![key, kept.to_string(), on],
        )
        .map(|_| ())
        .map_err(said)
    }

    /// What's chosen of `machine`'s game for itself: its settings, as
    /// CHOICES has them, an object of those it has.
    pub fn settings(&self, machine: &Value) -> Result<Value, String> {
        let Some((key, _)) = game(machine) else {
            return Ok(json!({}));
        };
        let db = self.lock()?;
        let kept: Option<String> = db
            .query_row(
                "SELECT settings FROM settings WHERE key = ?1",
                [&key],
                |row| row.get(0),
            )
            .optional()
            .map_err(said)?;
        Ok(kept
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(json!({})))
    }

    /// `settings` chosen of `machine`'s game: those CHOICES has, as it has
    /// them; none, and it's as the app's settings have it.
    pub fn choose(&self, machine: &Value, settings: &Value) -> Result<(), String> {
        let Some((key, _)) = game(machine) else {
            return Err("not a game".to_string());
        };
        let mut chosen = json!({});
        for (name, values) in CHOICES {
            if let Some(value) = settings[name]
                .as_str()
                .filter(|value| values.contains(value))
            {
                chosen[name] = json!(value);
            }
        }
        let db = self.lock()?;
        if chosen.as_object().is_some_and(|chosen| chosen.is_empty()) {
            db.execute("DELETE FROM settings WHERE key = ?1", [&key])
        } else {
            db.execute(
                "INSERT OR REPLACE INTO settings (key, settings) VALUES (?1, ?2)",
                params![key, chosen.to_string()],
            )
        }
        .map(|_| ())
        .map_err(said)
    }

    /// The games that are favourites, or have been played: each, how it's
    /// started, whether it's a favourite, how often and how long it's been
    /// played, and when last; the last played first.
    pub fn games(&self) -> Result<Vec<Value>, String> {
        let db = self.lock()?;
        let mut statement = db
            .prepare(
                "SELECT machine, favourite, plays, seconds, last FROM games
                 WHERE favourite = 1 OR plays > 0 ORDER BY last DESC NULLS LAST, key",
            )
            .map_err(said)?;
        let rows = statement
            .query_map([], |row| {
                let machine: String = row.get(0)?;
                Ok(json!({
                    "machine": serde_json::from_str::<Value>(&machine).unwrap_or(Value::Null),
                    "favourite": row.get::<_, bool>(1)?,
                    "plays": row.get::<_, i64>(2)?,
                    "seconds": row.get::<_, i64>(3)?,
                    "last": row.get::<_, Option<i64>>(4)?,
                }))
            })
            .map_err(said)?;
        rows.collect::<Result<_, _>>().map_err(said)
    }

    /// The NES games found that nothing's known of by `emulator`, the one
    /// there is now, or not at the size they are now.
    pub fn unknown(&self, emulator: &str) -> Result<Vec<String>, String> {
        let db = self.lock()?;
        let mut statement = db
            .prepare(
                "SELECT files.path FROM files LEFT JOIN facts
                     ON facts.path = files.path AND facts.size = files.size AND facts.emulator = ?1
                 WHERE facts.path IS NULL ORDER BY files.id",
            )
            .map_err(said)?;
        let rows = statement
            .query_map([emulator], |row| row.get::<_, String>(0))
            .map_err(said)?;
        let paths: Vec<String> = rows.collect::<Result<_, _>>().map_err(said)?;
        Ok(paths.into_iter().filter(|path| is_nes(path)).collect())
    }

    /// What `emulator` said of games, a line each, as `cpu_emu facts` says
    /// it, kept for the size each is now. Whether any were.
    pub fn learned(&self, lines: &[String], emulator: &str) -> Result<bool, String> {
        let mut db = self.lock()?;
        let tx = db.transaction().map_err(said)?;
        let mut any = false;
        for line in lines {
            let fields: Vec<&str> = line.splitn(6, '\t').collect();
            let [crc, mapper, famicom, zapper, problem, path] = fields[..] else {
                continue;
            };
            let size: Option<i64> = tx
                .query_row("SELECT size FROM files WHERE path = ?1", [path], |row| {
                    row.get(0)
                })
                .optional()
                .map_err(said)?;
            let Some(size) = size else { continue };
            tx.execute(
                "INSERT OR REPLACE INTO facts
                     (path, size, crc, mapper, famicom, zapper, problem, emulator)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    path,
                    size,
                    crc,
                    mapper,
                    famicom == "1",
                    zapper == "1",
                    problem,
                    emulator
                ],
            )
            .map_err(said)?;
            any = true;
        }
        tx.commit().map_err(said)?;
        Ok(any)
    }

    /// What's known of the games found, as they are now: each's path, its
    /// CRC-32, its mapper, whether it's a Famicom's and a Zapper's, and why
    /// it won't load, if it won't.
    pub fn facts(&self) -> Result<Vec<Value>, String> {
        let db = self.lock()?;
        let mut statement = db
            .prepare(
                "SELECT DISTINCT facts.path, crc, mapper, famicom, zapper, problem FROM facts
                 JOIN files ON files.path = facts.path AND files.size = facts.size
                 ORDER BY facts.path",
            )
            .map_err(said)?;
        let rows = statement
            .query_map([], |row| {
                Ok(json!({
                    "path": row.get::<_, String>(0)?,
                    "crc": row.get::<_, String>(1)?,
                    "mapper": row.get::<_, String>(2)?,
                    "famicom": row.get::<_, bool>(3)?,
                    "zapper": row.get::<_, bool>(4)?,
                    "problem": row.get::<_, String>(5)?,
                }))
            })
            .map_err(said)?;
        rows.collect::<Result<_, _>>().map_err(said)
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

/// What a game can have of its own, and what each can be: as the emulator's
/// options have them, but quick resume's.
pub const CHOICES: [(&str, &[&str]); 5] = [
    ("region", &["ntsc", "pal"]),
    ("console", &["nes", "famicom"]),
    ("port2", &["controller", "zapper"]),
    ("ram", &["ones", "zeros", "random"]),
    ("resume", &["on", "off"]),
];

/// A game as it's kept, of what the page started, or would start: a NES's
/// game; an Apple's disks, and the model, its ROMs and Mockingboards it was
/// last started with, if it has been. By its files, that are what it is.
pub fn game(machine: &Value) -> Option<(String, Value)> {
    let text = |field: &str| machine[field].as_str().map(str::to_string);
    match machine["kind"].as_str()? {
        "nes" => {
            let game = text("game")?;
            Some((
                format!("nes\n{game}"),
                json!({ "kind": "nes", "game": game }),
            ))
        }
        "apple" => {
            let disks: Vec<String> = machine["disks"]
                .as_array()?
                .iter()
                .filter_map(|disk| disk.as_str().map(str::to_string))
                .collect();
            if disks.is_empty() {
                return None;
            }
            let mut kept = json!({ "kind": "apple", "disks": disks });
            for field in ["model", "roms", "mockingboards"] {
                if !machine[field].is_null() {
                    kept[field] = machine[field].clone();
                }
            }
            Some((format!("apple\n{}", disks.join("\n")), kept))
        }
        _ => None,
    }
}

/// Whether `path`'s a NES game's, as the library has them: an iNES image, a
/// Disk System's, or a zip of one.
fn is_nes(path: &str) -> bool {
    let kind = path.rsplit('.').next().unwrap_or_default().to_lowercase();
    ["nes", "fds", "zip"].contains(&kind.as_str())
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
    fn a_game_played_is_kept_with_how_often_how_long_and_when() {
        let library = Library::in_memory().unwrap();
        let nes = json!({ "kind": "nes", "game": "/g/a.zip", "fullscreen": true, "ram": "zeros" });
        library.played(&nes, 1000, 60).unwrap();
        library.played(&nes, 5000, 30).unwrap();
        let apple = json!({ "kind": "apple", "model": "apple2ee", "disks": ["/d/1.dsk", "/d/2.dsk"], "mockingboards": 1 });
        library.played(&apple, 3000, 10).unwrap();
        library
            .played(&json!({ "kind": "controls" }), 9000, 5)
            .unwrap();
        assert_eq!(
            library.games().unwrap(),
            [
                json!({ "machine": { "kind": "nes", "game": "/g/a.zip" }, "favourite": false, "plays": 2, "seconds": 90, "last": 5000 }),
                json!({ "machine": { "kind": "apple", "model": "apple2ee", "disks": ["/d/1.dsk", "/d/2.dsk"], "mockingboards": 1 }, "favourite": false, "plays": 1, "seconds": 10, "last": 3000 }),
            ],
            "the last played first, how each was started and none of its settings, and no controls"
        );
    }

    #[test]
    fn a_favourite_keeps_how_it_was_last_started() {
        let library = Library::in_memory().unwrap();
        let disks = json!({ "kind": "apple", "disks": ["/d/1.dsk"] });
        library.favour(&disks, true).unwrap();
        let games = library.games().unwrap();
        assert_eq!(
            (games.len(), &games[0]["last"]),
            (1, &json!(null)),
            "never played"
        );
        library
            .played(&json!({ "kind": "apple", "model": "apple2", "roms": "/r/plus.rom", "disks": ["/d/1.dsk"] }), 10, 1)
            .unwrap();
        library.favour(&disks, true).unwrap();
        let games = library.games().unwrap();
        assert_eq!(games[0]["machine"]["model"], "apple2", "as it was started");
        assert_eq!(games[0]["favourite"], true);
        library.favour(&disks, false).unwrap();
        assert_eq!(
            library.games().unwrap()[0]["favourite"],
            false,
            "still played"
        );
        library
            .favour(&json!({ "kind": "nes", "game": "/g/b.zip" }), false)
            .unwrap();
        assert_eq!(
            library.games().unwrap().len(),
            1,
            "neither a favourite nor played"
        );
        assert!(
            library
                .favour(&json!({ "kind": "controls" }), true)
                .is_err()
        );
    }

    #[test]
    fn a_games_facts_are_kept_while_its_the_size_it_was() {
        let library = Library::in_memory().unwrap();
        let files = vec![
            found("/g/a.zip", 10),
            found("/g/b.nes", 20),
            found("/d/c.dsk", 30),
        ];
        library
            .replace(vec![found("/data/library/disksys.rom", 8192)], files)
            .unwrap();
        assert_eq!(
            library.unknown("e1").unwrap(),
            ["/g/a.zip", "/g/b.nes"],
            "NES games only"
        );
        let lines = [
            "52E2B5E0\tMAPPER 4 MMC3\t1\t0\t\t/g/a.zip".to_string(),
            "\t\t0\t0\tmapper 19, which isn't here yet\t/g/b.nes".to_string(),
            "garbled".to_string(),
            "00000000\tMAPPER 0 NROM\t0\t0\t\t/g/gone.nes".to_string(),
        ];
        assert!(library.learned(&lines, "e1").unwrap());
        assert!(library.unknown("e1").unwrap().is_empty());
        assert_eq!(
            library.facts().unwrap(),
            [
                json!({ "path": "/g/a.zip", "crc": "52E2B5E0", "mapper": "MAPPER 4 MMC3", "famicom": true, "zapper": false, "problem": "" }),
                json!({ "path": "/g/b.nes", "crc": "", "mapper": "", "famicom": false, "zapper": false, "problem": "mapper 19, which isn't here yet" }),
            ],
            "a line that isn't one, and a file not found, left out"
        );
        // b.nes, a new dump: of a new size, not known.
        library
            .replace(vec![], vec![found("/g/a.zip", 10), found("/g/b.nes", 21)])
            .unwrap();
        assert_eq!(library.unknown("e1").unwrap(), ["/g/b.nes"]);
        assert_eq!(library.facts().unwrap().len(), 1);
        assert_eq!(
            library.unknown("e2").unwrap(),
            ["/g/a.zip", "/g/b.nes"],
            "a new emulator's to say it all again"
        );
    }

    #[test]
    fn a_games_settings_are_those_it_can_have_and_none_are_forgotten() {
        let library = Library::in_memory().unwrap();
        let nes = json!({ "kind": "nes", "game": "/g/a.zip", "fullscreen": true });
        assert_eq!(library.settings(&nes).unwrap(), json!({}), "none yet");
        library
            .choose(
                &nes,
                &json!({ "region": "pal", "console": "", "port2": "zapper", "ram": "lots", "colour": "red", "resume": "off" }),
            )
            .unwrap();
        assert_eq!(
            library
                .settings(&json!({ "kind": "nes", "game": "/g/a.zip" }))
                .unwrap(),
            json!({ "region": "pal", "port2": "zapper", "resume": "off" }),
            "by its game, not how it's started; what it can't have left out"
        );
        library.choose(&nes, &json!({})).unwrap();
        assert_eq!(library.settings(&nes).unwrap(), json!({}));
        assert!(
            library
                .choose(&json!({ "kind": "controls" }), &json!({}))
                .is_err()
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
