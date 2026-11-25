//! Open Bus as an app, on a desktop or on Android: the web page's library,
//! in a window of its own, its files on the disk rather than in a browser's storage. A folder
//! added is only remembered, in `library.sqlite` in the app's data folder
//! (library.rs), and looked through each time: its games stay where they are, and a game
//! removed from the library is hidden, its file never touched. Files added
//! one by one are copied to `library` there, and may be deleted; the ROMs a
//! machine needs too. The covers and what's known of the games are in `art`.
//!
//! The page's only way to the disk is these commands: a path it gives one
//! is checked to be in `library` or in a folder that's been added.
//!
//! Looking through the folders takes a while, on a NAS, so what it found is
//! kept there too, for the page to show at once when it starts: it looks
//! again behind that, and only says if anything's changed.
//!
//! A game's the native emulator, `cpu_emu`, as a process of its own, as
//! EmulationStation has it: it opens where the window is, that going away
//! until the game's over. Not in the same process, each wanting the main
//! thread as they do on a Mac; nor in the same window, there being no one
//! way to on the three systems. A game that fails leaves the library alone.

#[cfg(target_os = "android")]
mod android;
mod library;

use library::{Found, Library, Section, entry};
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{self, Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
#[cfg(desktop)]
use tauri::{LogicalSize, PhysicalPosition, PhysicalSize};
#[cfg(desktop)]
use tauri_plugin_dialog::DialogExt;

/// How long the library stays up after a game starts, for its window to be
/// there to take over: there's no flash of the desktop between the two.
#[cfg(desktop)]
const OVERLAP: Duration = Duration::from_millis(350);
/// Windows's process flag for that.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A folder of the app's, made if it isn't there yet.
fn folder(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let folder = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join(name);
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    Ok(folder)
}

/// Where the page's `name` is: only ever under `library`.
fn place(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let path = Path::new(name);
    let plain = path
        .components()
        .all(|part| matches!(part, Component::Normal(_)));
    if name.is_empty() || !plain || name.contains('\\') {
        return Err(format!("not a file's name: {name}"));
    }
    Ok(folder(app, "library")?.join(path))
}

/// `bytes` put at `path` whole or not at all: a save's the last thing to
/// leave half written.
fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut unfinished = path.as_os_str().to_owned();
    unfinished.push(".part");
    fs::write(&unfinished, bytes).map_err(|e| e.to_string())?;
    fs::rename(&unfinished, path).map_err(|e| e.to_string())
}

/// A request's header, as the page encodes it.
fn header(request: &Request<'_>, name: &str) -> Result<String, String> {
    let value = request.headers().get(name).ok_or(format!("no {name}"))?;
    let value = value.to_str().map_err(|e| e.to_string())?;
    decode(value)
}

/// What encodeURIComponent made.
fn decode(text: &str) -> Result<String, String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = text.get(at + 1..at + 3).ok_or("a bad escape")?;
            decoded.push(u8::from_str_radix(hex, 16).map_err(|e| e.to_string())?);
            at += 3;
        } else {
            decoded.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(decoded).map_err(|e| e.to_string())
}

/// A request's bytes: as they're sent, on a desktop; on Android, whose
/// webview sends only JSON, as base64, as the page sends them there; or an
/// array of them, or a typed array's, by index.
fn body<'a>(request: &'a Request<'a>) -> Result<Cow<'a, [u8]>, String> {
    let byte = |value: &Value| {
        value
            .as_u64()
            .and_then(|byte| u8::try_from(byte).ok())
            .ok_or("not a byte".to_string())
    };
    match request.body() {
        InvokeBody::Raw(bytes) => Ok(Cow::Borrowed(bytes)),
        InvokeBody::Json(Value::String(text)) => {
            use base64::Engine;
            let bytes = base64::engine::general_purpose::STANDARD.decode(text);
            Ok(Cow::Owned(bytes.map_err(|e| e.to_string())?))
        }
        InvokeBody::Json(Value::Array(bytes)) => bytes.iter().map(byte).collect(),
        InvokeBody::Json(Value::Object(bytes)) => {
            let mut all = vec![0; bytes.len()];
            for (at, value) in bytes {
                let at: usize = at.parse().map_err(|_| "not a byte's place".to_string())?;
                *all.get_mut(at).ok_or("a byte out of place")? = byte(value)?;
            }
            Ok(Cow::Owned(all))
        }
        InvokeBody::Json(_) => Err("expected bytes".to_string()),
    }
}

#[tauri::command(async)]
fn file_bytes(app: AppHandle, path: String) -> Result<Response, String> {
    let bytes = fs::read(allowed(&app, &path)?).map_err(|e| e.to_string())?;
    Ok(Response::new(bytes))
}

/// A file put in `library`: where it is. It's put with what's been found,
/// and what's known of it learned, as a file a watcher sees come is: the
/// library's own folder isn't watched.
#[tauri::command]
fn keep_file(app: AppHandle, request: Request<'_>) -> Result<String, String> {
    let path = place(&app, &header(&request, "name")?)?;
    let bytes = body(&request)?;
    let size = bytes.len() as u64;
    write(&path, &bytes)?;
    let path = path.to_string_lossy().into_owned();
    library(&app).kept(&path, size)?;
    learn_facts(&app);
    Ok(path)
}

/// Where the page may go, as they really are: the folders added, and
/// `library`.
fn places(app: &AppHandle) -> Result<Vec<PathBuf>, String> {
    let mut places: Vec<PathBuf> = library(app)
        .folders()?
        .into_iter()
        .map(PathBuf::from)
        .collect();
    places.push(folder(app, "library")?);
    Ok(places
        .into_iter()
        .filter_map(|place| fs::canonicalize(place).ok())
        .collect())
}

/// Files dropped on the window, that the page may read.
#[derive(Default)]
struct Dropped(Mutex<Vec<PathBuf>>);

/// What the app keeps of its library.
fn library(app: &AppHandle) -> tauri::State<'_, Library> {
    app.state::<Library>()
}

/// A file found as the page has one: its path, and its size.
fn as_entry((path, size): &Found) -> Value {
    json!({ "path": path, "size": size })
}

/// `path`, if it's somewhere the page may go: `library`, or a folder added.
fn allowed(app: &AppHandle, path: &str) -> Result<PathBuf, String> {
    let real = fs::canonicalize(path).map_err(|e| format!("{path}: {e}"))?;
    let inside = |place: &PathBuf| real.starts_with(place);
    let places = places(app)?;
    let dropped = app
        .state::<Dropped>()
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .contains(&real);
    if dropped || places.iter().any(inside) {
        Ok(real)
    } else {
        Err(format!("not in the library: {path}"))
    }
}

/// All the files under `folder`, with their sizes; not those in folders
/// that aren't anyone's games: a hidden one, or a NAS's own, as @eaDir is.
/// A folder that's an X16 game's, its program alone: the rest is its data.
fn walk(folder: &Path, found: &mut Vec<Found>) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    let entries: Vec<_> = entries.flatten().collect();
    let paths: Vec<PathBuf> = entries.iter().map(|entry| entry.path()).collect();
    if let Some(program) = x16_program(&paths) {
        let size = fs::metadata(&program).map_or(0, |data| data.len());
        found.push((program.to_string_lossy().into_owned(), size));
        return;
    }
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with('.') && !name.starts_with('@') {
                walk(&path, found);
            }
        } else if !path.to_string_lossy().ends_with(".part") {
            let size = entry.metadata().map_or(0, |data| data.len());
            found.push((path.to_string_lossy().into_owned(), size));
        }
    }
}

/// The program that makes a folder of `paths` an X16 game's, if one does:
/// the first .PRG that loads where BASIC's do, $0801, with none of another
/// machine's games beside it. Its game's the emulator's to say.
fn x16_program(paths: &[PathBuf]) -> Option<PathBuf> {
    const OTHERS: [&str; 12] = [
        "nes", "fds", "a26", "a78", "zip", "dsk", "do", "po", "nib", "woz", "hdv", "2mg",
    ];
    let ending = |path: &Path| {
        path.extension()
            .map(|ending| ending.to_string_lossy().to_lowercase())
    };
    if paths
        .iter()
        .any(|path| ending(path).is_some_and(|end| OTHERS.contains(&end.as_str())))
    {
        return None;
    }
    let loads_at_0801 = |path: &&PathBuf| {
        let mut start = [0u8; 2];
        fs::File::open(path)
            .and_then(|mut file| file.read_exact(&mut start))
            .is_ok_and(|_| start == [0x01, 0x08])
    };
    let mut programs: Vec<&PathBuf> = paths
        .iter()
        .filter(|path| ending(path).as_deref() == Some("prg"))
        .filter(loads_at_0801)
        .collect();
    programs.sort();
    programs.first().map(|program| program.to_path_buf())
}

/// `folder` remembered.
fn remember(app: &AppHandle, folder: &str) -> Result<(), String> {
    if library(app).add_folder(folder)? {
        watch(app);
    }
    Ok(())
}

/// The folders watched, as `watch_folders` has them, on a thread of its
/// own: watching a big folder takes a while, and nothing waits for it.
fn watch(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || watch_folders(&app));
}

/// What Settings shows: the folders, the hidden, and the app's own folder.
#[tauri::command(async)]
fn library_settings(app: AppHandle) -> Result<Value, String> {
    let kept = library(&app);
    let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(
        json!({ "folders": kept.folders()?, "hidden": kept.hidden()?, "data": data.to_string_lossy() }),
    )
}

/// The kinds of file the library has, a game's or a disk's: what's worth a
/// watcher's noticing. Saves and states beside them change as games are
/// played, and don't matter to it.
const GAMES: [&str; 14] = [
    "nes", "fds", "a26", "a78", "bin", "zip", "prg", "dsk", "do", "po", "nib", "woz", "hdv", "2mg",
];

/// The folders added, watched for games coming and going, and the watcher.
#[derive(Default)]
struct Watching(Mutex<Option<Debouncer<RecommendedWatcher>>>);

/// The folders added watched, as they are now: what was watching them before
/// stopped.
fn watch_folders(app: &AppHandle) {
    let folders = library(app).folders().unwrap_or_default();
    let seeing = app.clone();
    let watching = new_debouncer(
        Duration::from_millis(800),
        move |events: DebounceEventResult| {
            if let Ok(events) = events {
                seen(
                    &seeing,
                    events.into_iter().map(|event| event.path).collect(),
                );
            }
        },
    );
    let Ok(mut watcher) = watching else { return };
    // Not those on other computers: looked over every so often instead.
    for folder in folders.iter().filter(|folder| !on_network(folder)) {
        let _ = watcher
            .watcher()
            .watch(Path::new(folder), RecursiveMode::Recursive);
    }
    if let Ok(mut watched) = app.state::<Watching>().0.lock() {
        *watched = Some(watcher);
    }
}

/// Whether `path`, under one of `folders`, is somewhere `walk` doesn't go: a
/// hidden folder, a NAS's own, a download not finished.
fn passed_over(path: &Path, folders: &[String]) -> bool {
    let Some(within) = folders
        .iter()
        .find_map(|folder| path.strip_prefix(folder).ok())
    else {
        return true;
    };
    let hidden = within.components().any(|part| {
        let part = part.as_os_str().to_string_lossy();
        part.starts_with('.') || part.starts_with('@')
    });
    hidden || path.to_string_lossy().ends_with(".part")
}

/// What a watcher saw change: each of `paths` that's a game now, or a folder
/// with games in it, put with what's been found; each that's gone, and all
/// that was under it, taken out. The page told, if anything changed.
fn seen(app: &AppHandle, paths: Vec<PathBuf>) {
    let kept = library(app);
    let Ok(folders) = kept.folders() else { return };
    let game = |path: &Path| {
        path.extension()
            .is_some_and(|kind| GAMES.contains(&kind.to_string_lossy().to_lowercase().as_str()))
    };
    let mut changed = false;
    for path in paths.iter().filter(|path| !passed_over(path, &folders)) {
        let mut came = Vec::new();
        if path.is_dir() {
            walk(path, &mut came);
        } else if path.is_file() {
            let size = fs::metadata(path).map_or(0, |data| data.len());
            came.push((path.to_string_lossy().into_owned(), size));
        } else {
            changed |= kept.forget_under(path).unwrap_or(false);
            continue;
        }
        came.retain(|(file, _)| game(Path::new(file)));
        changed |= kept
            .add(Section::Folders, came)
            .is_ok_and(|added| !added.is_empty());
    }
    if changed {
        learn_facts(app);
        let _ = app.emit("files-changed", ());
    }
}

/// Whether `folder`'s on another computer whose changes a watcher isn't told
/// of, only of what's done from here: an NFS, SMB or SSH mount's, on Linux,
/// as /proc/mounts has them. Windows asks an SMB server to tell it of changes
/// there, by anyone, so its shares are watched as well as its own folders.
/// On Android, a drive's or a Shield's network share's, beside the device's
/// own storage: all of them mounted alike, through Android's own layer,
/// and only its own storage watched, a share's taking minutes to watch.
fn on_network(folder: &str) -> bool {
    #[cfg(target_os = "android")]
    if folder.starts_with("/storage/") && !folder.starts_with("/storage/emulated/") {
        return true;
    }
    let Ok(mounts) = fs::read_to_string("/proc/mounts") else {
        return false;
    };
    let mount = mounts
        .lines()
        .filter_map(|line| {
            let mut parts = line.split(' ');
            let (_, place, kind) = (
                parts.next()?,
                parts.next()?.replace("\\040", " "),
                parts.next()?,
            );
            Path::new(folder)
                .starts_with(&place)
                .then(|| (place.len(), kind.to_string()))
        })
        .max();
    mount.is_some_and(|(_, kind)| {
        ["nfs", "nfs4", "cifs", "smb3", "smbfs", "fuse.sshfs"].contains(&kind.as_str())
    })
}

/// How often the folders on other computers are looked through again.
const NETWORK_LOOK: Duration = Duration::from_secs(300);

/// The folders looked through again, if any of them are on another computer,
/// each NETWORK_LOOK: the page told if what's there has changed.
fn look_over_network(app: AppHandle) {
    thread::spawn(move || {
        loop {
            thread::sleep(NETWORK_LOOK);
            let folders = library(&app).folders().unwrap_or_default();
            if !folders.iter().any(|folder| on_network(folder)) {
                continue;
            }
            if look_again(&app).is_ok_and(|changed| changed) {
                let _ = app.emit("files-changed", ());
            }
        }
    });
}

/// A folder's files, as `folder_files` found them, ROMs taken out, now with
/// the rest: the folder isn't looked through again for them.
#[tauri::command(async)]
fn add_folder_files(app: AppHandle, entries: Vec<Value>) -> Result<Vec<Value>, String> {
    // Only kept, not read: each by its path, plainly in a place the page may
    // go, not each looked up as `allowed` has it, thousands of them on a
    // network share. What's read later's looked up then.
    let places = places(&app)?;
    let plainly_in = |path: &str| {
        let path = Path::new(path);
        let plain = path.components().all(|part| {
            matches!(
                part,
                Component::Prefix(_) | Component::RootDir | Component::Normal(_)
            )
        });
        plain && path.is_absolute() && places.iter().any(|place| path.starts_with(place))
    };
    let allowed_entries = entries
        .iter()
        .filter_map(entry)
        .filter(|(path, _)| plainly_in(path))
        .collect();
    let added = library(&app).add(Section::Folders, allowed_entries)?;
    learn_facts(&app);
    Ok(added.iter().map(as_entry).collect())
}

/// A folder no longer looked in: its files are left as they are, and what was
/// found in it's forgotten, not all looked for again.
#[tauri::command(async)]
fn remove_folder(app: AppHandle, path: String) -> Result<(), String> {
    library(&app).remove_folder(&path)?;
    watch(&app);
    Ok(())
}

/// Games hidden, back: no longer hidden, and with what's been found, as a
/// folder's are, without its folders being looked through again. What's still there of them, as `all_files` has a
/// folder's files.
#[tauri::command(async)]
fn restore_hidden(app: AppHandle, paths: Vec<String>) -> Result<Vec<Value>, String> {
    let kept = library(&app);
    kept.unhide(&paths)?;
    let back = paths
        .iter()
        .filter_map(|path| {
            fs::metadata(path)
                .ok()
                .map(|file| (path.clone(), file.len()))
        })
        .collect();
    let added = kept.add(Section::Folders, back)?;
    learn_facts(&app);
    Ok(added.iter().map(as_entry).collect())
}

/// A folder chosen, and remembered: its path, if one was.
#[cfg(desktop)]
#[tauri::command(async)]
fn pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    let Some(chosen) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let chosen = chosen
        .into_path()
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .into_owned();
    remember(&app, &chosen)?;
    Ok(Some(chosen))
}

/// No folders yet, on Android.
#[cfg(mobile)]
#[tauri::command(async)]
fn pick_folder(_app: AppHandle) -> Result<Option<String>, String> {
    Err("folders can't be added on Android yet".to_string())
}

/// Where folders are, to look through, in the page's own list of them:
/// on Android, the device's storage and its drives; on a desktop, the home
/// folder and the top.
fn storage_places() -> Vec<(String, PathBuf)> {
    #[cfg(target_os = "android")]
    return android::storage_places();
    #[cfg(not(target_os = "android"))]
    return [
        ("Home", env::var_os("HOME").map(PathBuf::from)),
        ("Everything", Some(PathBuf::from("/"))),
    ]
    .into_iter()
    .filter_map(|(name, path)| Some((name.to_string(), path?)))
    .collect();
}

/// The folders in `path`, for the page's own list of them, as Android has
/// no folder picker fit for a TV: their names and paths, by name, and the
/// folder above, if it's not where it started; with no `path`, where
/// folders are, to start.
#[tauri::command(async)]
fn folders_in(path: Option<String>) -> Result<Value, String> {
    let places = storage_places();
    let Some(path) = path else {
        let folders: Vec<Value> = places
            .iter()
            .map(|(name, path)| json!({ "name": name, "path": path }))
            .collect();
        return Ok(json!({ "folders": folders, "up": null }));
    };
    let folder = PathBuf::from(&path);
    let mut folders: Vec<(String, PathBuf)> = fs::read_dir(&folder)
        .map_err(|e| format!("{path}: {e}"))?
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                entry.path(),
            )
        })
        .filter(|(name, _)| !name.starts_with('.'))
        .collect();
    folders.sort_by_key(|(name, _)| name.to_lowercase());
    let started = places.iter().any(|(_, place)| *place == folder);
    let up = (!started).then(|| folder.parent()).flatten();
    let folders: Vec<Value> = folders
        .iter()
        .map(|(name, path)| json!({ "name": name, "path": path }))
        .collect();
    Ok(json!({ "folders": folders, "up": up }))
}

/// Whether the app may read folders by their paths: on Android, as it's
/// been let; on a desktop, always.
#[tauri::command(async)]
fn storage_access(window: WebviewWindow) -> Result<bool, String> {
    #[cfg(target_os = "android")]
    return android::storage_access(&window);
    #[cfg(not(target_os = "android"))]
    {
        let _ = window;
        Ok(true)
    }
}

/// That asked for, on Android.
#[tauri::command(async)]
fn ask_storage_access(window: WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "android")]
    return android::ask_storage_access(&window);
    #[cfg(not(target_os = "android"))]
    {
        let _ = window;
        Ok(())
    }
}

/// A folder chosen in the page's own list, remembered: its path.
#[tauri::command(async)]
fn add_folder(app: AppHandle, path: String) -> Result<String, String> {
    let folder = fs::canonicalize(&path).map_err(|e| format!("{path}: {e}"))?;
    if !folder.is_dir() {
        return Err(format!("{path} isn't a folder"));
    }
    let folder = folder.to_string_lossy().into_owned();
    remember(&app, &folder)?;
    Ok(folder)
}

/// What was dropped on the window, by the window's own say: its folders
/// remembered, and its files open to the page, to add as any are.
#[tauri::command(async)]
fn take_dropped(app: AppHandle, paths: Vec<String>) -> Result<Value, String> {
    let (mut folders, mut files) = (Vec::new(), Vec::new());
    for path in paths {
        if Path::new(&path).is_dir() {
            remember(&app, &path)?;
            folders.push(path);
        } else {
            let real = fs::canonicalize(&path).map_err(|e| e.to_string())?;
            app.state::<Dropped>()
                .0
                .lock()
                .map_err(|e| e.to_string())?
                .push(real);
            files.push(path);
        }
    }
    Ok(json!({ "folders": folders, "files": files }))
}

/// All the files of a folder that's been added.
#[tauri::command(async)]
fn folder_files(app: AppHandle, folder: String) -> Result<Vec<Value>, String> {
    let mut found = Vec::new();
    walk(&allowed(&app, &folder)?, &mut found);
    Ok(found.iter().map(as_entry).collect())
}

/// Every file there is: `library`'s, and the added folders', the hidden left
/// out, as paths: which first, and whether it's a folder's, for the page.
#[tauri::command(async)]
fn all_files(app: AppHandle) -> Result<Value, String> {
    look_again(&app)?;
    known_files(app)
}

/// Everything looked through again, and kept: whether it's changed.
fn look_again(app: &AppHandle) -> Result<bool, String> {
    let mut own = Vec::new();
    let root = folder(app, "library")?;
    walk(&root, &mut own);
    own.retain(|(path, _)| !machines_own(&root, Path::new(path)));
    let mut added = Vec::new();
    for place in library(app).folders()? {
        walk(Path::new(&place), &mut added);
    }
    let changed = library(app).replace(own, added)?;
    learn_facts(app);
    Ok(changed)
}

/// Whether `path`, in the library's folder `root`, is one of the machines'
/// own files there from before saves had a folder of their own, under
/// roms/: the X16's clock's RAM (roms/x16/nvram.bin), still read if there's
/// none where saves are. Not a game: its .bin had it on the NES's shelf.
fn machines_own(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root)
        .is_ok_and(|within| within.starts_with("roms"))
}

/// The games that are favourites, or have been played, as the library keeps
/// them.
#[tauri::command(async)]
fn kept_games(app: AppHandle) -> Result<Vec<Value>, String> {
    library(&app).games()
}

/// `machine`, a game as the page would start it, a favourite, `on`, or not.
#[tauri::command(async)]
fn favourite(app: AppHandle, machine: Value, on: bool) -> Result<(), String> {
    library(&app).favour(&machine, on)
}

/// What's been found, as it was last looked for.
#[tauri::command(async)]
fn known_files(app: AppHandle) -> Result<Value, String> {
    library(&app).found(&folder(&app, "library")?.to_string_lossy())
}

/// A file of `library` deleted; one of a folder's, only hidden: it's yours.
#[tauri::command(async)]
fn forget_path(app: AppHandle, path: String) -> Result<(), String> {
    let real = allowed(&app, &path)?;
    if fs::canonicalize(folder(&app, "library")?).is_ok_and(|library| real.starts_with(library)) {
        fs::remove_file(&real).map_err(|e| e.to_string())?;
        // Not found any more either: the library's folder isn't watched,
        // and a row left would keep what was known of it, or be learned of
        // as "No such file" and keep that for the next file of its size.
        library(&app).forget_own(&path)?;
        if let Some(parent) = real.parent() {
            let _ = fs::remove_dir(parent); // refused if there's anything in it
        }
        return Ok(());
    }
    library(&app).hide(&path)
}

/// A game's art's file, by a hash of its key, which can be any text.
fn art_file(app: &AppHandle, key: &str) -> Result<PathBuf, String> {
    let hash = key.bytes().fold(0xCBF2_9CE4_8422_2325u64, |hash, byte| {
        (hash ^ byte as u64).wrapping_mul(0x100_0000_01B3)
    });
    Ok(folder(app, "art")?.join(format!("{hash:016x}")))
}

/// What's known of a game, and its cover: the entry `key`, its text in
/// `known`, as JSON, and its cover the bytes sent, if there are any. A file
/// each: a length, that text, and the cover.
#[tauri::command]
fn keep_art(app: AppHandle, request: Request<'_>) -> Result<(), String> {
    let (key, known) = (header(&request, "key")?, header(&request, "known")?);
    let entry = serde_json::json!({ "key": key, "known": serde_json::from_str::<Value>(&known).map_err(|e| e.to_string())? }).to_string();
    let mut file = (entry.len() as u32).to_le_bytes().to_vec();
    file.extend(entry.as_bytes());
    file.extend_from_slice(&body(&request)?);
    write(&art_file(&app, &key)?, &file)
}

/// All the art's files, one after another, each after its length.
#[tauri::command]
fn all_art(app: AppHandle) -> Result<Response, String> {
    let mut all = Vec::new();
    for entry in fs::read_dir(folder(&app, "art")?).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().is_none() {
            let file = fs::read(path).map_err(|e| e.to_string())?;
            all.extend((file.len() as u32).to_le_bytes());
            all.extend(file);
        }
    }
    Ok(Response::new(all))
}

/// The emulator: beside this program, as it's installed; or built in the
/// repository, in development.
#[cfg(desktop)]
fn emulator() -> Result<PathBuf, String> {
    let name = format!("cpu_emu{}", std::env::consts::EXE_SUFFIX);
    let beside = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name(&name);
    let built = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/release")
        .join(&name);
    [beside, built]
        .into_iter()
        .find(|path| path.exists())
        .ok_or("the emulator, cpu_emu, isn't here".to_string())
}

/// The emulator, on Android: among the app's libraries.
#[cfg(target_os = "android")]
fn emulator() -> Result<PathBuf, String> {
    android::emulator()
}

/// The gamepads, for the library: a webview's own miss what its gamepad
/// library doesn't know, so they're read as a game reads them, by the
/// emulator with no window, what's held told to the page. It's stopped for a
/// game, which has the gamepads to itself.
#[derive(Default)]
struct Pads(Mutex<Option<Child>>);

fn start_pads(app: &AppHandle) {
    // On Android, the webview has them all.
    if cfg!(mobile) {
        return;
    }
    let Ok(mut command) = emulator_command(app) else {
        return;
    };
    command
        .arg("pads")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let Ok(mut pads) = command.spawn() else {
        return;
    };
    let Some(said) = pads.stdout.take() else {
        return;
    };
    *app.state::<Pads>().0.lock().unwrap() = Some(pads);
    let app = app.clone();
    thread::spawn(move || {
        for line in BufReader::new(said).lines().map_while(Result::ok) {
            let _ = app.emit("pads", line);
        }
    });
}

/// Told to go, by its input closing, and made to if it doesn't.
#[cfg(desktop)]
fn stop_pads(app: &AppHandle) {
    let Some(mut pads) = app.state::<Pads>().0.lock().unwrap().take() else {
        return;
    };
    drop(pads.stdin.take());
    for _ in 0..50 {
        if !matches!(pads.try_wait(), Ok(None)) {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let _ = pads.kill();
    let _ = pads.wait();
}

/// The emulator to run, without a console flashing up behind it, on Windows.
fn emulator_command(app: &AppHandle) -> Result<Command, String> {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut command = Command::new(emulator()?);
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut command, CREATE_NO_WINDOW);
    // On Android, SDL beside it, among the app's libraries; and its
    // settings where the game screen has them, the app's files, as Android
    // has no home.
    #[cfg(target_os = "android")]
    {
        command.env("LD_LIBRARY_PATH", android::libraries()?);
        let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
        command.env("XDG_CONFIG_HOME", data.join("files"));
    }
    #[cfg(not(target_os = "android"))]
    let _ = app;
    Ok(command)
}

/// The menus' choosing and going back, as the emulator has them bound for
/// the first player, by its names for them: `{ accept, back }`, a list
/// each. For the library to choose and go back by, on Android, where it
/// has the gamepads itself; and to name them, anywhere.
#[tauri::command(async)]
fn menu_buttons(app: AppHandle) -> Result<Value, String> {
    let said = emulator_command(&app)?
        .arg("bound")
        .output()
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&said.stdout);
    let of = |word: &str| {
        let start = format!("1 {word} ");
        text.lines()
            .filter_map(|line| line.strip_prefix(&start))
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    Ok(json!({ "accept": of("menu-accept"), "back": of("menu-back") }))
}

/// Whether the emulator's learning what's known of the games, and whether
/// it's been asked to again, for what came while it was.
#[derive(Default)]
struct Learning(Mutex<(bool, bool)>);

/// What's known of the NES games found learned, those nothing's known of
/// yet, by the emulator, in the background: kept as it says each, the page
/// told once it's done, if anything was. Asked while it's at it, it looks
/// again after; and once is all, for any it says nothing of.
fn learn_facts(app: &AppHandle) {
    {
        let state = app.state::<Learning>();
        let Ok(mut learning) = state.0.lock() else {
            return;
        };
        if learning.0 {
            learning.1 = true;
            return;
        }
        *learning = (true, false);
    }
    let app = app.clone();
    thread::spawn(move || {
        loop {
            let (learned, changed) = learn_unknown(&app).unwrap_or((false, false));
            if learned {
                let _ = app.emit("facts-changed", ());
            }
            let state = app.state::<Learning>();
            let Ok(mut learning) = state.0.lock() else {
                return;
            };
            if !learning.1 && !changed {
                learning.0 = false;
                return;
            }
            learning.1 = false;
        }
    });
}

/// The games nothing's known of told to `cpu_emu facts`, and what it says
/// kept, a hundred at a time: whether anything was; and whether any were
/// written while it was at it, what it said of them not kept, to be learned
/// again.
fn learn_unknown(app: &AppHandle) -> Result<(bool, bool), String> {
    let began = SystemTime::now();
    let emulator = which_emulator()?;
    let unknown = library(app).unknown(&emulator)?;
    if unknown.is_empty() {
        return Ok((false, false));
    }
    let list: String = unknown
        .iter()
        .filter(|one| !one.path.contains('\n'))
        .map(|one| format!("{}\n", one.path))
        .collect();
    let listed: HashMap<String, (i64, i64)> =
        unknown.into_iter().map(|one| (one.path, one.row)).collect();
    let mut learner = emulator_command(app)?
        .arg("facts")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut input = learner.stdin.take().ok_or("no input")?;
    thread::spawn(move || input.write_all(list.as_bytes()));
    let output = learner.stdout.take().ok_or("no output")?;
    let (mut lines, mut any, mut changed) = (Vec::new(), false, false);
    for line in BufReader::new(output).lines().map_while(Result::ok) {
        // One gone: nothing to know of it ("No such file" isn't). One
        // removed and fetched again, say, while the list was gone through:
        // read, maybe, while it wasn't there, so learned again.
        let path = line.splitn(6, '\t').nth(5).unwrap_or_default();
        if !Path::new(path).exists() {
            continue;
        }
        if written_since(path, began) {
            changed = true;
            continue;
        }
        lines.push(line);
        if lines.len() == 100 {
            any |= library(app).learned(&std::mem::take(&mut lines), &emulator, &listed)?;
        }
    }
    any |= library(app).learned(&lines, &emulator, &listed)?;
    let _ = learner.wait();
    Ok((any, changed))
}

/// Whether the file at `path` was written since `began` (and not, by a
/// clock that's wrong, in the future, which would have it learned forever).
fn written_since(path: &str, began: SystemTime) -> bool {
    fs::metadata(path)
        .and_then(|file| file.modified())
        .is_ok_and(|at| at >= began && at <= SystemTime::now())
}

/// Which emulator there is, as what it says of games goes: its size and
/// when it was built, which a new one changes. What an old one said is
/// learned again, as a mapper added makes a game playable.
fn which_emulator() -> Result<String, String> {
    let file = fs::metadata(emulator()?).map_err(|e| e.to_string())?;
    let built = file
        .modified()
        .ok()
        .and_then(|at| at.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |at| at.as_secs());
    Ok(format!("{} {built}", file.len()))
}

/// What's known of the games found, for the page.
#[tauri::command(async)]
fn game_facts(app: AppHandle) -> Result<Vec<Value>, String> {
    library(&app).facts()
}

/// What `machine`, as the page says, is on the emulator's command line.
fn command_line(app: &AppHandle, machine: &Value) -> Result<Vec<String>, String> {
    let text = |field: &str| machine[field].as_str().ok_or(format!("no {field}"));
    let file = |path: &str| Ok::<_, String>(allowed(app, path)?.to_string_lossy().into_owned());
    if text("kind")? == "controls" {
        return Ok(vec!["controls".to_string()]);
    }
    if text("kind")? == "nes" {
        let fast = machine["fast"]
            .as_bool()
            .unwrap_or(false)
            .then(|| "--fast".to_string());
        // Its own settings, over the app's: what its RAM holds at power-on,
        // if it's not the default; and its region, its console and what's
        // in its second port, if they're not as it's known to be.
        let chosen = library(app).settings(machine)?;
        let ram = chosen["ram"]
            .as_str()
            .or(machine["ram"].as_str())
            .filter(|ram| ["zeros", "random"].contains(ram))
            .map(|ram| format!("--ram={ram}"));
        let options = ["region", "console", "port2"]
            .into_iter()
            .filter_map(|name| Some(format!("--{name}={}", chosen[name].as_str()?)));
        return Ok(["nes".to_string(), file(text("game")?)?]
            .into_iter()
            .chain(fast)
            .chain(ram)
            .chain(options)
            .collect());
    }
    if let kind @ ("atari2600" | "atari7800") = text("kind")? {
        // Its region, if it's not as it's known to be.
        let chosen = library(app).settings(machine)?;
        let region = chosen["region"]
            .as_str()
            .map(|region| format!("--region={region}"));
        return Ok([kind.to_string(), file(text("game")?)?]
            .into_iter()
            .chain(region)
            .collect());
    }
    if text("kind")? == "x16" {
        // Its ROM, as it's been sorted into the library; the game on a card
        // of its own, with the saves, made the first time and kept, with
        // what it saves. With no game, the X16 itself, to BASIC, with a card
        // of its own, blank to start with.
        let rom = folder(app, "library")?.join("x16-rom.bin");
        let mut line = vec!["x16".to_string(), rom.to_string_lossy().into_owned()];
        if let Some(game) = machine["game"].as_str() {
            line.push(format!("--game={}", file(game)?));
        }
        // Its card where the emulator keeps it, one from before moved there.
        let card = x16_card(app, machine)?;
        line.push(format!("--sd={}", card.to_string_lossy()));
        // The X16's files, on its card as FILES.
        line.push(format!(
            "--files={}",
            x16_files_folder(app)?.to_string_lossy()
        ));
        return Ok(line);
    }
    let library = folder(app, "library")?.to_string_lossy().into_owned();
    let mut line = match text("model")? {
        "apple2" => vec!["apple2".to_string(), file(text("roms")?)?],
        "apple2e" => vec!["apple2e".to_string(), library, "--unenhanced".to_string()],
        _ => vec!["apple2e".to_string(), library],
    };
    for disk in machine["disks"].as_array().ok_or("no disks")? {
        line.push(file(disk.as_str().ok_or("a bad disk")?)?);
    }
    line.push(format!(
        "--mockingboards={}",
        machine["mockingboards"].as_u64().unwrap_or(1)
    ));
    Ok(line)
}

/// The X16's own card, for BASIC and what's saved from it.
const X16_CARD: &str = "Commander X16.img";

/// An X16 game's card's file, by its game's path: its name, and a hash of
/// the whole (FNV-1a), for two of a name.
fn card_name(game: &str) -> String {
    let hash = game.bytes().fold(0xCBF2_9CE4_8422_2325u64, |hash, byte| {
        (hash ^ byte as u64).wrapping_mul(0x0100_0000_01B3)
    });
    let name: String = Path::new(game)
        .file_stem()
        .map_or("game".into(), |name| name.to_string_lossy())
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " -_".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{name} {:08x}.img", hash as u32)
}

/// The root saves are kept under, in its `saves` and `states` folders: where
/// Settings has put them, else the app's own folder.
fn saves_root(app: &AppHandle) -> Result<PathBuf, String> {
    let root = match library(app).app_setting("saves")? {
        Some(chosen) => PathBuf::from(chosen),
        None => app.path().app_data_dir().map_err(|e| e.to_string())?,
    };
    fs::create_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    Ok(root)
}

/// The extensions of games' files, left out of what's saved of them, as the
/// emulator has them (`host::saves`).
const GAME_EXTENSIONS: [&str; 16] = [
    "nes", "fds", "a26", "a78", "bin", "zip", "prg", "rom", "img", "dsk", "do", "po", "nib", "woz",
    "hdv", "2mg",
];

/// The X16's card `machine`'s played from, as the emulator keeps it, with
/// what's saved on it: its game's, by the game's name, its extension left
/// out; or, with no game, the X16's own, for BASIC. One made before saves
/// had a place of their own, in the app's `x16 cards`, moved there the
/// first time it's wanted.
fn x16_card(app: &AppHandle, machine: &Value) -> Result<PathBuf, String> {
    let (name, before) = match machine["game"].as_str() {
        Some(game) => {
            let game = allowed(app, game)?;
            let named = game
                .file_name()
                .map_or("game".into(), |name| name.to_string_lossy());
            let inner = Path::new(named.as_ref());
            let known = inner.extension().is_some_and(|extension| {
                GAME_EXTENSIONS.contains(&extension.to_string_lossy().to_ascii_lowercase().as_str())
            });
            let name = match inner.file_stem() {
                Some(stem) if known => stem.to_string_lossy().into_owned(),
                _ => named.into_owned(),
            };
            (format!("{name}.img"), card_name(&game.to_string_lossy()))
        }
        None => (X16_CARD.to_string(), X16_CARD.to_string()),
    };
    let card = saves_root(app)?.join("saves").join("x16").join(name);
    let before = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("x16 cards")
        .join(before);
    if !card.exists() && before.exists() {
        move_file(&before, &card)?;
    }
    Ok(card)
}

/// The X16's files: one folder on this computer, on every X16 program's
/// card as its FILES folder (the emulator's `--files`): as Settings has
/// chosen it, else "Open Bus/X16 Files" in Documents (or, with none, the
/// app's own folder). On Android, the device's own Documents, not the one
/// Tauri means, in the app's folder in Android/data, which since Android 11
/// no file manager or other app can reach.
fn x16_files_folder(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(chosen) = library(app).app_setting("x16 files")? {
        return Ok(PathBuf::from(chosen));
    }
    #[cfg(target_os = "android")]
    let base = PathBuf::from(android::OWN_STORAGE).join("Documents");
    #[cfg(not(target_os = "android"))]
    let base = app
        .path()
        .document_dir()
        .or_else(|_| app.path().app_data_dir())
        .map_err(|e| e.to_string())?;
    Ok(base.join("Open Bus").join("X16 Files"))
}

/// The saves' folder and the X16's files' kept apart, neither in the
/// other: the X16's files are put on every X16 card, and every card's among
/// the saves.
fn apart(saves: &Path, x16_files: &Path) -> Result<(), String> {
    if saves.starts_with(x16_files) || x16_files.starts_with(saves) {
        return Err(format!(
            "The saves and the X16's files need folders of their own: {} and {} are one in the other.",
            saves.display(),
            x16_files.display()
        ));
    }
    Ok(())
}

/// The X16's files folder: `{ folder, chosen }`.
#[tauri::command(async)]
fn x16_files(app: AppHandle) -> Result<Value, String> {
    Ok(json!({
        "folder": x16_files_folder(&app)?.to_string_lossy(),
        "chosen": library(&app).app_setting("x16 files")?.is_some(),
    }))
}

/// The X16's files folder chosen (`None`, the default again): what's in
/// the one before stays there. `{ folder, chosen }` after.
#[tauri::command(async)]
fn set_x16_files(app: AppHandle, path: Option<String>) -> Result<Value, String> {
    if let Some(path) = &path {
        apart(&saves_root(&app)?, Path::new(path))?;
    }
    library(&app).set_app_setting("x16 files", path.as_deref())?;
    x16_files(app)
}

/// The X16's files folder opened in the system's file manager, made if it
/// isn't there yet.
#[tauri::command(async)]
fn open_x16_files(app: AppHandle) -> Result<(), String> {
    let folder = x16_files_folder(&app)?;
    fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let opener = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    Command::new(opener)
        .arg(&folder)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{opener}: {e}"))
}

/// How long the update check may take before it's given up. It's a few
/// KB, and nothing waits for it: this only stops a server that answers and
/// then says nothing from holding it for ever.
#[cfg(desktop)]
const UPDATE_CHECK_TIME: Duration = Duration::from_secs(30);
/// How long the update's download may go with nothing coming before it's
/// given up. However long the whole takes, on a slow line, is fine.
#[cfg(desktop)]
const UPDATE_STALL_TIME: Duration = Duration::from_secs(60);

/// The newer Open Bus, if the latest release on GitHub is one. Its
/// `latest.json` says, for each system and kind of install, where the update
/// is and its signature, which the app's key (tauri.conf.json's
/// `plugins.updater.pubkey`) has to check.
#[cfg(desktop)]
async fn newer(app: &AppHandle) -> Result<Option<tauri_plugin_updater::Update>, String> {
    use tauri_plugin_updater::UpdaterExt;
    app.updater_builder()
        .timeout(UPDATE_CHECK_TIME)
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())
}

/// A newer Open Bus: `{ version, notes }`, or null. Android has no updates
/// of Tauri's: null.
#[tauri::command(async)]
async fn update_check(app: AppHandle) -> Result<Value, String> {
    #[cfg(desktop)]
    {
        Ok(newer(&app).await?.map_or(
            Value::Null,
            |update| json!({ "version": update.version, "notes": update.body }),
        ))
    }
    #[cfg(not(desktop))]
    {
        let _ = app;
        Ok(Value::Null)
    }
}

/// The newer Open Bus downloaded, its signature checked, installed, and
/// started in this one's place. On Windows its installer finishes it, this
/// app having closed. The download's progress goes to the page
/// (`update-progress`: `{ got, total }`, in bytes, the total if it's known);
/// a download that stalls is given up, but not the install, which on Linux
/// may wait for the user's password.
#[tauri::command(async)]
async fn update_install(app: AppHandle) -> Result<(), String> {
    #[cfg(desktop)]
    {
        let mut update = newer(&app).await?.ok_or("there's no newer version now")?;
        // The check's time limit isn't the download's: that has none, only
        // the stall's.
        update.timeout = None;
        let heard = std::sync::Arc::new(Mutex::new(Instant::now()));
        let (seen, told) = (heard.clone(), app.clone());
        let (mut got, mut said) = (0u64, None);
        let download = update.download(
            move |chunk, total| {
                *seen.lock().unwrap() = Instant::now();
                got += chunk as u64;
                // Each percent, or each megabyte without a total.
                let step = total.map_or(got >> 20, |total| got * 100 / total.max(1));
                if said != Some(step) {
                    said = Some(step);
                    let _ = told.emit("update-progress", json!({ "got": got, "total": total }));
                }
            },
            || {},
        );
        tokio::pin!(download);
        let bytes = loop {
            tokio::select! {
                bytes = &mut download => break bytes.map_err(|e| e.to_string())?,
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    if heard.lock().unwrap().elapsed() > UPDATE_STALL_TIME {
                        return Err("the download stopped: nothing came for a minute".into());
                    }
                }
            }
        };
        update.install(bytes).map_err(|e| e.to_string())?;
        app.restart()
    }
    #[cfg(not(desktop))]
    {
        let _ = app;
        Err("there are no updates on Android: install the new APK".into())
    }
}

/// `from` moved to `to`: renamed, or, from one disk to another, copied and
/// the first removed.
fn move_file(from: &Path, to: &Path) -> Result<(), String> {
    let said = |e: std::io::Error| format!("{}: {e}", from.display());
    if let Some(folder) = to.parent() {
        fs::create_dir_all(folder).map_err(said)?;
    }
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    fs::copy(from, to).map_err(said)?;
    fs::remove_file(from).map_err(said)
}

/// Everything in the folder `from` moved into `to`, as it's laid out; of
/// two of a place, the one last written kept. The folders left empty, gone.
fn move_folder(from: &Path, to: &Path) -> Result<(), String> {
    let Ok(entries) = fs::read_dir(from) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let (path, there) = (entry.path(), to.join(entry.file_name()));
        if path.is_dir() {
            move_folder(&path, &there)?;
            let _ = fs::remove_dir(&path);
            continue;
        }
        let written = |path: &Path| fs::metadata(path).and_then(|about| about.modified()).ok();
        if there.exists() && written(&there) >= written(&path) {
            let _ = fs::remove_file(&path);
        } else {
            move_file(&path, &there)?;
        }
    }
    Ok(())
}

/// The system `machine`'s saves are kept under, as the emulator names it.
fn save_system(machine: &Value) -> Option<&'static str> {
    Some(match machine["kind"].as_str()? {
        "nes" => "nes",
        "atari2600" => "atari2600",
        "atari7800" => "atari7800",
        "apple" => "apple2",
        "x16" => "x16",
        _ => return None,
    })
}

/// The files of `machine`'s game its saves are named for: its ROM, its
/// disks, or, the X16 with no game, its own card.
fn game_files(app: &AppHandle, machine: &Value) -> Result<Vec<String>, String> {
    let file = |path: &str| Ok::<_, String>(allowed(app, path)?.to_string_lossy().into_owned());
    Ok(match machine["kind"].as_str().unwrap_or_default() {
        "nes" | "atari2600" | "atari7800" => {
            vec![file(machine["game"].as_str().ok_or("no game")?)?]
        }
        // Its card moved from where it was kept before, as it's played, so
        // it's found with the rest.
        "x16" => {
            let card = x16_card(app, machine)?;
            match machine["game"].as_str() {
                Some(game) => vec![file(game)?],
                None => vec![card.to_string_lossy().into_owned()],
            }
        }
        "apple" => machine["disks"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(file)
            .collect::<Result<_, _>>()?,
        _ => Vec::new(),
    })
}

/// The saves of a game of `system` whose files are `files`, or, with none,
/// all of them, as the emulator finds them: each `{ kind, bytes, written,
/// path }`; those of the kinds `clear` names deleted first.
fn saves_listed(
    app: &AppHandle,
    system: Option<&str>,
    files: &[String],
    clear: &[String],
) -> Result<Vec<Value>, String> {
    let mut command = emulator_command(app)?;
    command
        .arg(format!("--saves={}", saves_root(app)?.to_string_lossy()))
        .arg("saves");
    if let Some(system) = system {
        command.arg(format!("--system={system}"));
    }
    if !clear.is_empty() {
        command.arg(format!("--clear={}", clear.join(",")));
    }
    let said = command
        .args(files)
        .stderr(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&said.stdout)
        .lines()
        .filter_map(|line| {
            let [kind, bytes, written, path] = line.splitn(4, '\t').collect::<Vec<_>>()[..] else {
                return None;
            };
            Some(json!({
                "kind": kind,
                "bytes": bytes.parse::<u64>().ok()?,
                "written": written.parse::<u64>().ok()?,
                "path": path,
            }))
        })
        .collect())
}

/// What Settings shows of the saves: where they're kept, whether that's
/// been chosen, and of each kind, how many and how big.
#[tauri::command(async)]
fn saves_overview(app: AppHandle) -> Result<Value, String> {
    let mut kinds = json!({});
    for save in saves_listed(&app, None, &[], &[])? {
        let kind = &mut kinds[save["kind"].as_str().unwrap_or_default()];
        kind["count"] = json!(kind["count"].as_u64().unwrap_or(0) + 1);
        kind["bytes"] =
            json!(kind["bytes"].as_u64().unwrap_or(0) + save["bytes"].as_u64().unwrap_or(0));
    }
    Ok(json!({
        "folder": saves_root(&app)?.to_string_lossy(),
        "chosen": library(&app).app_setting("saves")?.is_some(),
        "kinds": kinds,
    }))
}

/// All saves of the `kinds` deleted.
#[tauri::command(async)]
fn clear_saves(app: AppHandle, kinds: Vec<String>) -> Result<Value, String> {
    if !kinds.is_empty() {
        saves_listed(&app, None, &[], &kinds)?;
    }
    saves_overview(app)
}

/// `machine`'s game's saves.
#[tauri::command(async)]
fn game_saves(app: AppHandle, machine: Value) -> Result<Vec<Value>, String> {
    let files = game_files(&app, &machine)?;
    if files.is_empty() {
        return Ok(Vec::new());
    }
    saves_listed(&app, save_system(&machine), &files, &[])
}

/// `machine`'s game's saves of the `kinds` deleted: what's left of them.
#[tauri::command(async)]
fn clear_game_saves(
    app: AppHandle,
    machine: Value,
    kinds: Vec<String>,
) -> Result<Vec<Value>, String> {
    let files = game_files(&app, &machine)?;
    if !files.is_empty() && !kinds.is_empty() {
        saves_listed(&app, save_system(&machine), &files, &kinds)?;
    }
    game_saves(app, machine)
}

/// Saves kept under `path` from now on, or, with none, the app's own folder
/// again: its `saves` and `states` moved there.
#[tauri::command(async)]
fn set_saves_folder(app: AppHandle, path: Option<String>) -> Result<Value, String> {
    let from = saves_root(&app)?;
    let default = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let to = path.map_or_else(|| default.clone(), PathBuf::from);
    apart(&to, &x16_files_folder(&app)?)?;
    if to != from {
        let inside = |root: &Path, other: &Path| {
            ["saves", "states"]
                .iter()
                .any(|sub| other.starts_with(root.join(sub)))
        };
        if inside(&from, &to) || inside(&to, &from) {
            return Err("The saves can't be moved into the folder they're in.".into());
        }
        fs::create_dir_all(&to).map_err(|e| format!("{}: {e}", to.display()))?;
        let trying = to.join(".open-bus-saves");
        fs::write(&trying, b"")
            .map_err(|e| format!("{}: can't be written to: {e}", to.display()))?;
        let _ = fs::remove_file(&trying);
        for sub in ["saves", "states"] {
            move_folder(&from.join(sub), &to.join(sub))?;
            let _ = fs::remove_dir(from.join(sub));
        }
    }
    let chosen = (to != default).then(|| to.to_string_lossy().into_owned());
    library(&app).set_app_setting("saves", chosen.as_deref())?;
    saves_overview(app)
}

/// `--window=`, for the emulator to open where the library is.
#[cfg(desktop)]
fn window_place(window: &WebviewWindow) -> Result<String, String> {
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let at = window
        .inner_position()
        .map_err(|e| e.to_string())?
        .to_logical::<i32>(scale);
    let size = window
        .inner_size()
        .map_err(|e| e.to_string())?
        .to_logical::<u32>(scale);
    Ok(format!(
        "--window={},{},{},{}",
        at.x, at.y, size.width, size.height
    ))
}

/// What a failed game had to say: a panic's message, without its place in
/// the code, or the folders of the game it names.
fn complaint(said: &str, game: &str) -> String {
    let useful = |line: &&str| {
        !line.starts_with("thread ") && !line.starts_with("note:") && !line.is_empty()
    };
    let line = said.lines().find(useful).unwrap_or("the game stopped");
    let name = Path::new(game)
        .file_name()
        .map_or(game.into(), |name| name.to_string_lossy());
    line.replace(game, &name)
}

/// Whether `machine`'s game's put away to resume from: as it's chosen of it,
/// or as the app's settings have it.
fn resumed(app: &AppHandle, machine: &Value) -> bool {
    let chosen = library(app).settings(machine).unwrap_or_default();
    match chosen["resume"].as_str() {
        Some("on") => true,
        Some("off") => false,
        _ => machine["resume"].as_bool().unwrap_or(false),
    }
}

/// What's chosen of a game for itself, its own settings, for the page.
#[tauri::command(async)]
fn game_settings(app: AppHandle, machine: Value) -> Result<Value, String> {
    library(&app).settings(&machine)
}

/// `settings` chosen of a game, for itself.
#[tauri::command(async)]
fn choose_settings(app: AppHandle, machine: Value, settings: Value) -> Result<(), String> {
    library(&app).choose(&machine, &settings)
}

/// The game that's running, as `play` was given it, till it's over. On
/// Android the library can be come back to while it runs (Home, then Open
/// Bus), and there's only ever one game screen.
static RUNNING: Mutex<Option<Value>> = Mutex::new(None);

/// Whether `a` and `b` are the same game: the same machine, with the same
/// game or disks in it.
fn same_game(a: &Value, b: &Value) -> bool {
    ["kind", "game", "disks"].iter().all(|key| a[key] == b[key])
}

/// Whether a game's running: none (null), `machine` itself (true), or
/// another (false).
#[tauri::command]
fn running(machine: Value) -> Option<bool> {
    let running = RUNNING.lock().unwrap();
    running.as_ref().map(|running| same_game(running, &machine))
}

/// The running game brought back as it is, on Android: its game screen in
/// front again. On a desktop, its window's already in front.
#[tauri::command(async)]
fn return_to_game(window: WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "android")]
    return android::bring_back(&window, false);
    #[cfg(not(target_os = "android"))]
    {
        let _ = window;
        Ok(())
    }
}

/// The running game quit, on Android, as its own Quit does: put away to
/// resume, its saves kept. `game-ended` says when it's over.
#[tauri::command(async)]
fn quit_game(window: WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "android")]
    return android::bring_back(&window, true);
    #[cfg(not(target_os = "android"))]
    {
        let _ = window;
        Err("a game's quit from its own window here".to_string())
    }
}

/// A game played, the window away until it's over. Not on the main thread:
/// on Android, starting the game screen's done there, and waited for. Not
/// while another's running: that's to be quit first.
#[tauri::command(async)]
fn play(app: AppHandle, window: WebviewWindow, machine: Value) -> Result<(), String> {
    if RUNNING.lock().unwrap().is_some() {
        return Err("a game's still running: quit it first".to_string());
    }
    let library = folder(&app, "library")?;
    let line = command_line(&app, &machine)?;
    let first_file = match machine["game"].as_str() {
        Some(game) => game.to_string(),
        None => line.get(1).cloned().unwrap_or_default(),
    };
    let mut line: Vec<String> = line
        .into_iter()
        .chain(
            [
                machine["fullscreen"]
                    .as_bool()
                    .unwrap_or(false)
                    .then_some("--fullscreen"),
                resumed(&app, &machine).then_some("--resume"),
                (machine["rewind"] == false).then_some("--no-rewind"),
                (machine["driveLight"] == false).then_some("--no-drive-light"),
                (machine["vsync"] == false).then_some("--no-vsync"),
            ]
            .into_iter()
            .flatten()
            .map(str::to_string),
        )
        .collect();
    // Its saves in the app's own folder, never beside the games: their
    // folders are left as they are (and may be a network share's, not to be
    // written to).
    line.push(format!("--saves={}", saves_root(&app)?.to_string_lossy()));
    let began = (SystemTime::now(), Instant::now());
    #[cfg(desktop)]
    {
        line.push(window_place(&window)?);
        let mut command = emulator_command(&app)?;
        command
            .args(line)
            .current_dir(&library)
            .stderr(Stdio::piped());
        stop_pads(&app);
        let mut game = command.spawn().map_err(|e| {
            start_pads(&app);
            e.to_string()
        })?;
        *RUNNING.lock().unwrap() = Some(machine.clone());
        thread::spawn(move || {
            thread::sleep(OVERLAP);
            if matches!(game.try_wait(), Ok(None)) {
                let _ = window.hide();
            }
            let mut said = String::new();
            if let Some(mut stderr) = game.stderr.take() {
                let _ = stderr.read_to_string(&mut said);
            }
            let failed = !game.wait().is_ok_and(|status| status.success());
            let problem = failed.then(|| complaint(&said, &first_file));
            game_over(&app, &window, &machine, began, problem);
        });
    }
    // On Android, the game screen: what it says, kept in a file, as it
    // has no output of its own to read; a panic's, as it fails.
    #[cfg(target_os = "android")]
    {
        let log = app
            .path()
            .app_cache_dir()
            .map_err(|e| e.to_string())?
            .join("game.log");
        let _ = fs::remove_file(&log);
        android::start(&window, line, &library, &log)?;
        *RUNNING.lock().unwrap() = Some(machine.clone());
        thread::spawn(move || {
            let ran = android::wait(&window, &log);
            let said = fs::read_to_string(&log).unwrap_or_default();
            let problem = if !ran {
                Some("the game didn't start".to_string())
            } else {
                said.find("panicked").map(|at| {
                    let line = said[..at].rfind('\n').map_or(0, |end| end + 1);
                    complaint(&said[line..], &first_file)
                })
            };
            game_over(&app, &window, &machine, began, problem);
        });
    }
    Ok(())
}

/// A game over, `problem` if it failed: played if it didn't, once more and
/// for so long; the library back, and told.
fn game_over(
    app: &AppHandle,
    window: &WebviewWindow,
    machine: &Value,
    (started, since): (SystemTime, Instant),
    problem: Option<String>,
) {
    *RUNNING.lock().unwrap() = None;
    if problem.is_none() {
        let at = started
            .duration_since(UNIX_EPOCH)
            .map_or(0, |at| at.as_secs());
        let _ = self::library(app).played(machine, at, since.elapsed().as_secs());
    }
    #[cfg(desktop)]
    {
        let _ = window.show();
        let _ = window.set_focus();
    }
    #[cfg(mobile)]
    let _ = window;
    let _ = app.emit("game-ended", problem);
    start_pads(app);
}

/// The machines' ROMs among the files at `paths`, into `library`, by the
/// emulator, which knows them: which held any, the ROMs, and how many of the
/// II+'s chips.
#[tauri::command(async)]
fn sort_roms(app: AppHandle, paths: Vec<String>) -> Result<Value, String> {
    let wanted = paths
        .iter()
        .filter(|path| !path.contains('\n') && allowed(&app, path).is_ok());
    sort_into_library(&app, wanted.map(|path| format!("{path}\n")).collect())
}

/// The ROMs in the files `list`ed, a path a line, sorted into `library`,
/// as `sort_roms` has it.
fn sort_into_library(app: &AppHandle, list: String) -> Result<Value, String> {
    let app = app.clone();
    let mut command = emulator_command(&app)?;
    let library = folder(&app, "library")?;
    let mut sorter = command
        .arg("roms")
        .arg(library)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut input = sorter.stdin.take().ok_or("no input")?;
    thread::spawn(move || input.write_all(list.as_bytes()));
    let finished = sorter.wait_with_output().map_err(|e| e.to_string())?;
    if !finished.status.success() {
        return Err(String::from_utf8_lossy(&finished.stderr).into_owned());
    }
    let said = String::from_utf8_lossy(&finished.stdout);
    let of = |word: &str| {
        said.lines()
            .filter_map(|line| line.strip_prefix(word).map(str::to_string))
            .collect::<Vec<_>>()
    };
    let chips: usize = of("chips ")
        .first()
        .and_then(|count| count.parse().ok())
        .unwrap_or(0);
    // The ROMs written into the library, with what was found there, for it
    // not to be looked through again for them.
    let library = folder(&app, "library")?;
    let written = of("rom ")
        .iter()
        .filter_map(|name| {
            let path = library.join(name);
            fs::metadata(&path)
                .ok()
                .map(|file| (path.to_string_lossy().into_owned(), file.len()))
        })
        .collect();
    let kept: Vec<Value> = self::library(&app)
        .add(Section::Library, written)?
        .iter()
        .map(as_entry)
        .collect();
    Ok(json!({ "held": of("held "), "roms": of("rom "), "chips": chips, "kept": kept }))
}

/// The X16's ROM as its makers release it: R49's zip, on GitHub, and its
/// SHA-256, for anything else that comes to be turned away.
const X16_ROM_ZIP: &str =
    "https://github.com/X16Community/x16-rom/releases/download/r49/Release.R49.ROM.Image.zip";
const X16_ROM_SHA256: &str = "95b4e3566fa44f23dda9c8295b18a5fa9ea297e5db49dc978ae241e6114e39aa";
/// Longer than that, and it's not coming.
const DOWNLOAD_TIME: Duration = Duration::from_secs(120);

/// The X16's ROM, asked for: its release downloaded, checked and sorted
/// into the library, as `sort_roms` has it.
#[tauri::command(async)]
fn fetch_x16_rom(app: AppHandle) -> Result<Value, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(DOWNLOAD_TIME))
        .build()
        .into();
    let zip = agent
        .get(X16_ROM_ZIP)
        .call()
        .and_then(|mut response| response.body_mut().read_to_vec())
        .map_err(|e| format!("the download failed: {e}"))?;
    if !is_x16_rom_zip(&zip) {
        return Err("what came wasn't the release's zip".to_string());
    }
    let path = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join(format!("open-bus-x16-rom-{}.zip", process::id()));
    write(&path, &zip)?;
    let sorted = sort_into_library(&app, format!("{}\n", path.to_string_lossy()));
    let _ = fs::remove_file(&path);
    sorted
}

/// busterm, a BBS terminal for the X16 and its network card
/// (https://github.com/bleach86/busterm): its releases, each with a zip of
/// its files (busterm-X.Y.Z.zip), which the library keeps as an X16 game,
/// as it is: what's in it goes on its card, however many files busterm
/// comes as. Its name, BUSTERM.zip, names its card BUSTERM.img, as the
/// BUSTERM.PRG kept before 0.4.3 did, so what busterm's saved on it stays.
const BUSTERM_RELEASES: &str = "https://github.com/bleach86/busterm/releases";
const BUSTERM_ZIP: &str = "BUSTERM.zip";
/// What Open Bus 0.4.2 kept: the program alone, gone when the zip comes.
const BUSTERM_PRG: &str = "BUSTERM.PRG";
/// The app's setting: the version of the zip it fetched, as its zip doesn't
/// say it where it can be read without unpacking it.
const BUSTERM_VERSION: &str = "busterm";
/// How long finding the latest release may take.
const BUSTERM_CHECK_TIME: Duration = Duration::from_secs(15);

/// busterm's latest release's version: GitHub sends ".../latest" on to its
/// page, ".../tag/vX.Y.Z".
fn latest_busterm() -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(BUSTERM_CHECK_TIME))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into();
    let response = agent
        .get(&format!("{BUSTERM_RELEASES}/latest"))
        .call()
        .map_err(|e| format!("the check failed: {e}"))?;
    response
        .headers()
        .get("location")
        .and_then(|place| place.to_str().ok())
        .and_then(|place| place.rsplit_once("/tag/v"))
        .map(|(_, version)| version.to_string())
        .filter(|version| is_version(version))
        .ok_or_else(|| "GitHub didn't say busterm's latest release".to_string())
}

/// busterm, asked for: its latest release's zip downloaded into the
/// library as BUSTERM.zip, if it's a zip with BUSTERM.PRG in it. With
/// `replace`, over the library's copy, a newer release's; Open Bus 0.4.2's
/// BUSTERM.PRG goes. `{ kept, gone }`, as `sort_roms` has it.
#[tauri::command(async)]
fn fetch_busterm(app: AppHandle, replace: bool) -> Result<Value, String> {
    let version = latest_busterm()?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(DOWNLOAD_TIME))
        .build()
        .into();
    let zip = agent
        .get(&format!(
            "{BUSTERM_RELEASES}/download/v{version}/busterm-{version}.zip"
        ))
        .call()
        .and_then(|mut response| response.body_mut().read_to_vec())
        .map_err(|e| format!("the download failed: {e}"))?;
    if !zip.starts_with(b"PK\x03\x04")
        || !zip.windows(11).any(|name| name == BUSTERM_PRG.as_bytes())
    {
        return Err("what came wasn't busterm's zip".to_string());
    }
    let root = folder(&app, "library")?;
    let path = root.join(BUSTERM_ZIP);
    if path.exists() && !replace {
        return Err("busterm's in the library already".to_string());
    }
    write(&path, &zip)?;
    let entry = (path.to_string_lossy().into_owned(), zip.len() as u64);
    // Found, if it wasn't, and either way its size now and nothing known of
    // it till it's learned: not what was of a file before at its path.
    library(&app).add(Section::Library, vec![entry.clone()])?;
    library(&app).resize(&entry.0, entry.1)?;
    library(&app).set_app_setting(BUSTERM_VERSION, Some(&version))?;
    // What the emulator says of it, learned again: what was known was the
    // old file's, of its size. Till it's said, the zip's a NES's to the page
    // (as an unknown zip is), and on the wrong shelf.
    learn_facts(&app);
    // Open Bus 0.4.2's busterm, the program alone: this in its place.
    let before = root.join(BUSTERM_PRG);
    let mut gone = Vec::new();
    if before.exists() {
        fs::remove_file(&before).map_err(|e| format!("{}: {e}", before.display()))?;
        let before = before.to_string_lossy().into_owned();
        library(&app).forget_own(&before)?;
        gone.push(before);
    }
    Ok(json!({ "kept": [as_entry(&entry)], "gone": gone }))
}

/// A newer busterm than the library's: `{ have, latest }`, its version and
/// the latest release's; or null, with none in the library, or none newer.
/// The library's is the zip's, as fetched; or, Open Bus 0.4.2's program's,
/// as its start screen says.
#[tauri::command(async)]
fn busterm_update(app: AppHandle) -> Result<Value, String> {
    let root = folder(&app, "library")?;
    let have = if root.join(BUSTERM_ZIP).exists() {
        library(&app).app_setting(BUSTERM_VERSION)?
    } else {
        fs::read(root.join(BUSTERM_PRG))
            .ok()
            .and_then(|bytes| busterm_version(&bytes))
    };
    let Some(have) = have else {
        return Ok(Value::Null);
    };
    let latest = latest_busterm()?;
    Ok(if newer_version(&latest, &have) {
        json!({ "have": have, "latest": latest })
    } else {
        Value::Null
    })
}

/// busterm's version, as its start screen says it: "busterm 0.1.1".
fn busterm_version(program: &[u8]) -> Option<String> {
    const SAID: &[u8] = b"busterm ";
    program
        .windows(SAID.len())
        .enumerate()
        .find_map(|(at, here)| {
            if here != SAID {
                return None;
            }
            let rest = &program[at + SAID.len()..];
            let end = rest
                .iter()
                .position(|byte| !(byte.is_ascii_digit() || *byte == b'.'))
                .unwrap_or(rest.len());
            let version = std::str::from_utf8(&rest[..end])
                .ok()?
                .trim_end_matches('.');
            is_version(version).then(|| version.to_string())
        })
}

/// A version: numbers between dots, "0.1.1".
fn is_version(version: &str) -> bool {
    !version.is_empty()
        && version
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Whether version `a` comes after `b`, number by number: 0.1.10 after 0.1.9.
fn newer_version(a: &str, b: &str) -> bool {
    let numbers = |version: &str| -> Vec<u64> {
        version
            .split('.')
            .map(|part| part.parse().unwrap_or(0))
            .collect()
    };
    numbers(a) > numbers(b)
}

/// Whether `bytes` are R49's zip, as its hash says.
fn is_x16_rom_zip(bytes: &[u8]) -> bool {
    let hash: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    hash == X16_ROM_SHA256
}

/// A title bar's height, about, for a window whose frame isn't known yet;
/// and how long it takes to be known, once the window's shown.
#[cfg(desktop)]
const TITLE_BAR: u32 = 40;
#[cfg(desktop)]
const FRAME_KNOWN: Duration = Duration::from_millis(500);

/// Where a window of `outer` size goes, framed `outer - inner`, in the part of
/// a screen at `at` and `area` that's free, a taskbar's left out: its `inner`
/// size no more than leaves it all there, and the place that centres it. Its
/// frame's not known yet, on Linux, its `outer` size being a stand-in, less
/// than `inner`: it's taken as a title bar's, till it's fitted again, once
/// it's on the screen.
#[cfg(desktop)]
fn fitted(
    inner: (u32, u32),
    outer: (u32, u32),
    at: (i32, i32),
    area: (u32, u32),
) -> ((u32, u32), (i32, i32)) {
    let outer = if outer.0 < inner.0 || outer.1 < inner.1 {
        (inner.0, inner.1 + TITLE_BAR)
    } else {
        outer
    };
    let frame = (outer.0 - inner.0, outer.1 - inner.1);
    let (width, height) = (outer.0.min(area.0), outer.1.min(area.1));
    let size = (
        width.saturating_sub(frame.0),
        height.saturating_sub(frame.1),
    );
    (
        size,
        (
            at.0 + (area.0 - width) as i32 / 2,
            at.1 + (area.1 - height) as i32 / 2,
        ),
    )
}

/// The window's place and size as it was before it went fullscreen, to go
/// back to after.
#[cfg(desktop)]
#[derive(Default)]
struct Windowed(Mutex<Option<(PhysicalPosition<i32>, PhysicalSize<u32>)>>);

/// The window fullscreen, as the TV layout has it, or back as it was. Hidden
/// and shown again for a game, a window manager can forget what it was, so
/// it's kept here; and put back a moment after, once the window manager's done
/// with putting back what it remembers. With nothing kept, it's as it first
/// opens.
#[cfg(desktop)]
#[tauri::command]
fn fullscreen(app: AppHandle, window: WebviewWindow, on: bool) -> Result<(), String> {
    let kept = app.state::<Windowed>();
    let mut kept = kept.0.lock().map_err(|e| e.to_string())?;
    if on {
        if kept.is_none() && !window.is_fullscreen().map_err(|e| e.to_string())? {
            *kept = Some((
                window.outer_position().map_err(|e| e.to_string())?,
                window.inner_size().map_err(|e| e.to_string())?,
            ));
        }
        return window.set_fullscreen(true).map_err(|e| e.to_string());
    }
    window.set_fullscreen(false).map_err(|e| e.to_string())?;
    let was = kept.take();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        match was {
            Some((place, size)) => {
                let _ = window.set_size(size);
                let _ = window.set_position(place);
            }
            None => {
                let _ = window.set_size(LogicalSize::new(1280.0, 800.0));
                thread::sleep(Duration::from_millis(100));
                let _ = fit_to_screen(&window);
            }
        }
    });
    Ok(())
}

/// On Android, it's always fullscreen.
#[cfg(mobile)]
#[tauri::command]
fn fullscreen(_on: bool) {}

/// The app closed: by Ctrl+Q, or for a gamepad, from Settings.
#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

/// The window all on the screen it's opening on, however big that is, and
/// whatever a system's scaling makes of the size it asks for.
#[cfg(desktop)]
fn fit_to_screen(window: &WebviewWindow) -> tauri::Result<()> {
    let Some(monitor) = window.current_monitor()? else {
        return Ok(());
    };
    let area = monitor.work_area();
    let (inner, outer) = (window.inner_size()?, window.outer_size()?);
    let (size, place) = fitted(
        (inner.width, inner.height),
        (outer.width, outer.height),
        (area.position.x, area.position.y),
        (area.size.width, area.size.height),
    );
    if size != (inner.width, inner.height) {
        window.set_size(PhysicalSize::new(size.0, size.1))?;
    }
    window.set_position(PhysicalPosition::new(place.0, place.1))
}

/// The app, run: on a desktop, by `main`; on Android, by its activity.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = builder.manage(Windowed::default());
    // Updates, from the releases on GitHub (`update_check`).
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    builder
        .setup(|app| {
            // The page glides a mouse wheel's steps itself. WebKitGTK's own
            // smooth scrolling runs ahead of drawing, and the sidebar and
            // toolbar jump about.
            #[cfg(target_os = "linux")]
            app.get_webview_window("main")
                .ok_or("no window")?
                .with_webview(|webview| {
                    use webkit2gtk::{SettingsExt, WebViewExt};
                    if let Some(settings) = WebViewExt::settings(&webview.inner()) {
                        settings.set_enable_smooth_scrolling(false);
                    }
                })?;
            let data = app.path().app_data_dir()?;
            app.manage(Library::open(&data)?);
            learn_facts(app.handle());
            start_pads(app.handle());
            watch(app.handle());
            look_over_network(app.handle().clone());
            #[cfg(desktop)]
            {
                let window = app.get_webview_window("main").ok_or("no window")?;
                fit_to_screen(&window)?;
                // Fitted again once it's on the screen, its frame known then.
                thread::spawn(move || {
                    thread::sleep(FRAME_KNOWN);
                    let shown = window.clone();
                    let _ = window.run_on_main_thread(move || {
                        let _ = fit_to_screen(&shown);
                    });
                });
            }
            Ok(())
        })
        .manage(Dropped::default())
        .manage(Pads::default())
        .manage(Watching::default())
        .manage(Learning::default())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            update_check,
            update_install,
            x16_files,
            set_x16_files,
            open_x16_files,
            all_files,
            known_files,
            file_bytes,
            keep_file,
            forget_path,
            keep_art,
            all_art,
            pick_folder,
            folders_in,
            storage_access,
            ask_storage_access,
            add_folder,
            menu_buttons,
            take_dropped,
            folder_files,
            play,
            running,
            return_to_game,
            quit_game,
            library_settings,
            remove_folder,
            restore_hidden,
            sort_roms,
            fetch_x16_rom,
            fetch_busterm,
            busterm_update,
            fullscreen,
            quit,
            add_folder_files,
            kept_games,
            favourite,
            game_facts,
            game_settings,
            choose_settings,
            saves_overview,
            clear_saves,
            game_saves,
            clear_game_saves,
            set_saves_folder
        ])
        .run(tauri::generate_context!())
        .expect("couldn't open the window");
}

#[cfg(test)]
mod tests {
    use super::{
        X16_ROM_SHA256, busterm_version, card_name, fitted, is_x16_rom_zip, machines_own,
        newer_version, walk, written_since,
    };
    use std::path::Path;

    #[test]
    fn the_x16s_old_clock_ram_in_the_library_is_not_a_game() {
        let root = Path::new("/data/library");
        assert!(machines_own(root, &root.join("roms/x16/nvram.bin")));
        assert!(!machines_own(root, &root.join("Galaga.bin")));
        assert!(!machines_own(root, &root.join("roms.zip")));
        assert!(
            !machines_own(root, Path::new("/games/roms/Pitfall.bin")),
            "a folder of yours called roms"
        );
    }
    use std::time::{Duration, SystemTime};

    #[test]
    fn a_file_written_since_a_time_is_told_from_one_written_before() {
        let path = std::env::temp_dir().join(format!("open-bus-written-{}", std::process::id()));
        std::fs::write(&path, b"old").unwrap();
        let file = path.to_string_lossy().into_owned();
        let after = SystemTime::now() + Duration::from_millis(50);
        assert!(!written_since(&file, after), "written before");
        let before = SystemTime::now() - Duration::from_secs(5);
        assert!(written_since(&file, before), "written since");
        std::fs::remove_file(&path).unwrap();
        assert!(!written_since(&file, before), "not there");
    }

    #[test]
    fn busterms_version_is_read_from_its_start_screen_not_its_status_line() {
        let program = b"\x01\x08... busterm \x10 ...\x1b[1;36mbusterm 0.1.1\x1b[0m  a BBS terminal";
        assert_eq!(busterm_version(program), Some("0.1.1".to_string()));
        assert_eq!(busterm_version(b"\x01\x08busterm \x10 nothing"), None);
    }

    #[test]
    fn versions_are_compared_number_by_number() {
        assert!(newer_version("0.1.10", "0.1.9"));
        assert!(newer_version("0.2", "0.1.9"));
        assert!(newer_version("0.1.1", "0.1"));
        assert!(!newer_version("0.1.1", "0.1.1"));
        assert!(!newer_version("0.1.0", "0.1.1"));
    }
    use std::fs;

    /// 1280 by 800 at 125%, on a 1080p screen with a taskbar: too tall by
    /// its title bar and menu, as much taken off the page.
    #[test]
    fn a_window_too_big_for_the_screen_is_made_to_fit_it() {
        assert_eq!(
            fitted((1600, 1000), (1616, 1079), (0, 0), (1920, 1032)),
            ((1600, 953), (152, 0))
        );
    }

    #[test]
    fn a_window_that_fits_stays_its_size_and_is_centred_in_whats_free() {
        assert_eq!(
            fitted((1280, 800), (1296, 860), (0, 0), (1920, 1040)),
            ((1280, 800), (312, 90))
        );
    }

    /// As Linux has it, before the window's first drawn.
    #[test]
    fn an_outer_size_less_than_the_inner_isnt_believed_but_a_title_bars_left() {
        assert_eq!(
            fitted((1280, 800), (320, 158), (0, 36), (1920, 1044)),
            ((1280, 800), (320, 138))
        );
        // A laptop's 1366 by 768, all of it free: the window under it, its
        // title bar on the screen too.
        assert_eq!(
            fitted((1280, 800), (1280, 800), (0, 0), (1366, 768)),
            ((1280, 768), (43, 0))
        );
        assert_eq!(
            fitted((1280, 800), (320, 158), (0, 0), (1366, 768)),
            ((1280, 728), (43, 0))
        );
    }

    /// On a second screen, the place is that screen's.
    #[test]
    fn its_place_is_on_the_screen_its_opening_on() {
        assert_eq!(
            fitted((800, 600), (800, 600), (1920, 100), (1600, 900)),
            ((800, 600), (2320, 250))
        );
    }

    #[test]
    fn an_x16_games_card_is_named_by_it_and_hashed_by_its_whole_path() {
        let one = card_name("/games/x16/Galax 16: Deluxe.zip");
        assert!(
            one.starts_with("Galax 16_ Deluxe ") && one.ends_with(".img"),
            "{one}"
        );
        assert_eq!(
            one,
            card_name("/games/x16/Galax 16: Deluxe.zip"),
            "the same each time"
        );
        assert_ne!(
            one,
            card_name("/other/Galax 16: Deluxe.zip"),
            "two of a name"
        );
    }

    #[test]
    fn a_folder_of_an_x16_game_is_found_as_its_program_alone() {
        let root = std::env::temp_dir().join(format!("open_bus_{}_walk", std::process::id()));
        let (game, nes) = (root.join("x16-contra"), root.join("nes"));
        fs::create_dir_all(game.join("data")).unwrap();
        fs::create_dir_all(&nes).unwrap();
        fs::write(game.join("CONTRA.PRG"), [0x01, 0x08, 1]).unwrap();
        fs::write(game.join("NOTES.PRG"), b"text").unwrap();
        fs::write(game.join("BANK-0-1.BIN"), [0, 0xA0]).unwrap();
        fs::write(game.join("data/MAP.BIN"), [0, 0xA0]).unwrap();
        // A program among a NES's games isn't its folder's game.
        fs::write(nes.join("STRAY.PRG"), [0x01, 0x08]).unwrap();
        fs::write(nes.join("Game.nes"), b"NES").unwrap();
        let mut found = Vec::new();
        walk(&root, &mut found);
        let mut names: Vec<String> = found
            .iter()
            .map(|(path, _)| path[root.to_string_lossy().len() + 1..].to_string())
            .collect();
        names.sort();
        assert_eq!(
            names,
            ["nes/Game.nes", "nes/STRAY.PRG", "x16-contra/CONTRA.PRG"]
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn only_the_x16_releases_own_zip_is_taken() {
        assert!(!is_x16_rom_zip(b""));
        assert!(!is_x16_rom_zip(b"PK\x03\x04 something else"));
        assert_eq!(X16_ROM_SHA256.len(), 64);
    }
}
