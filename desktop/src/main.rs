//! Open Bus as a desktop app: the web page's library, in a window of its
//! own, its files on the disk rather than in a browser's storage. A folder
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

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod library;

use library::{Found, Library, Section, entry};
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};
use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, WebviewWindow,
};
use tauri_plugin_dialog::DialogExt;

/// How long the library stays up after a game starts, for its window to be
/// there to take over: there's no flash of the desktop between the two.
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

fn body<'a>(request: &'a Request<'a>) -> Result<&'a [u8], String> {
    match request.body() {
        InvokeBody::Raw(bytes) => Ok(bytes),
        InvokeBody::Json(_) => Err("expected bytes".to_string()),
    }
}

#[tauri::command(async)]
fn file_bytes(app: AppHandle, path: String) -> Result<Response, String> {
    let bytes = fs::read(allowed(&app, &path)?).map_err(|e| e.to_string())?;
    Ok(Response::new(bytes))
}

/// A file put in `library`: where it is.
#[tauri::command]
fn keep_file(app: AppHandle, request: Request<'_>) -> Result<String, String> {
    let path = place(&app, &header(&request, "name")?)?;
    write(&path, body(&request)?)?;
    Ok(path.to_string_lossy().into_owned())
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
    let mut places = library(app)
        .folders()?
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    places.push(folder(app, "library")?);
    let inside =
        |place: &PathBuf| fs::canonicalize(place).is_ok_and(|place| real.starts_with(place));
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
fn walk(folder: &Path, found: &mut Vec<Found>) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
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

/// `folder` remembered.
fn remember(app: &AppHandle, folder: &str) -> Result<(), String> {
    if library(app).add_folder(folder)? {
        watch_folders(app);
    }
    Ok(())
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
const GAMES: [&str; 13] = [
    "nes", "fds", "a26", "a78", "bin", "zip", "dsk", "do", "po", "nib", "woz", "hdv", "2mg",
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
    for folder in &folders {
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
fn on_network(folder: &str) -> bool {
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
    let allowed_entries = entries
        .iter()
        .filter_map(entry)
        .filter(|(path, _)| allowed(&app, path).is_ok())
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
    watch_folders(&app);
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
    walk(&folder(app, "library")?, &mut own);
    let mut added = Vec::new();
    for place in library(app).folders()? {
        walk(Path::new(&place), &mut added);
    }
    let changed = library(app).replace(own, added)?;
    learn_facts(app);
    Ok(changed)
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
    file.extend(body(&request)?);
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

/// The gamepads, for the library: a webview's own miss what its gamepad
/// library doesn't know, so they're read as a game reads them, by the
/// emulator with no window, what's held told to the page. It's stopped for a
/// game, which has the gamepads to itself.
#[derive(Default)]
struct Pads(Mutex<Option<Child>>);

fn start_pads(app: &AppHandle) {
    let Ok(mut command) = emulator_command() else {
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
fn emulator_command() -> Result<Command, String> {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut command = Command::new(emulator()?);
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut command, CREATE_NO_WINDOW);
    Ok(command)
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
            if learn_unknown(&app).unwrap_or(false) {
                let _ = app.emit("facts-changed", ());
            }
            let state = app.state::<Learning>();
            let Ok(mut learning) = state.0.lock() else {
                return;
            };
            if !learning.1 {
                learning.0 = false;
                return;
            }
            learning.1 = false;
        }
    });
}

/// The games nothing's known of told to `cpu_emu facts`, and what it says
/// kept, a hundred at a time: whether anything was.
fn learn_unknown(app: &AppHandle) -> Result<bool, String> {
    let emulator = which_emulator()?;
    let unknown = library(app).unknown(&emulator)?;
    if unknown.is_empty() {
        return Ok(false);
    }
    let list: String = unknown
        .iter()
        .filter(|path| !path.contains('\n'))
        .map(|path| format!("{path}\n"))
        .collect();
    let mut learner = emulator_command()?
        .arg("facts")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut input = learner.stdin.take().ok_or("no input")?;
    thread::spawn(move || input.write_all(list.as_bytes()));
    let output = learner.stdout.take().ok_or("no output")?;
    let (mut lines, mut any) = (Vec::new(), false);
    for line in BufReader::new(output).lines().map_while(Result::ok) {
        lines.push(line);
        if lines.len() == 100 {
            any |= library(app).learned(&std::mem::take(&mut lines), &emulator)?;
        }
    }
    any |= library(app).learned(&lines, &emulator)?;
    let _ = learner.wait();
    Ok(any)
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

/// `--window=`, for the emulator to open where the library is.
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

/// A game played, the window away until it's over.
#[tauri::command]
fn play(app: AppHandle, window: WebviewWindow, machine: Value) -> Result<(), String> {
    let library = folder(&app, "library")?;
    let line = command_line(&app, &machine)?;
    let first_file = line.get(1).cloned().unwrap_or_default();
    let mut command = emulator_command()?;
    command
        .args(line)
        .args(
            machine["fullscreen"]
                .as_bool()
                .unwrap_or(false)
                .then_some("--fullscreen"),
        )
        .args(resumed(&app, &machine).then_some("--resume"))
        .args((machine["rewind"] == false).then_some("--no-rewind"))
        .args((machine["driveLight"] == false).then_some("--no-drive-light"))
        .arg(window_place(&window)?)
        .current_dir(&library)
        .stderr(Stdio::piped());
    stop_pads(&app);
    let mut game = command.spawn().map_err(|e| {
        start_pads(&app);
        e.to_string()
    })?;
    let (started, since) = (SystemTime::now(), Instant::now());
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
        // Played, if it didn't fail: once more, and for so long.
        if !failed {
            let at = started
                .duration_since(UNIX_EPOCH)
                .map_or(0, |at| at.as_secs());
            let _ = self::library(&app).played(&machine, at, since.elapsed().as_secs());
        }
        let _ = window.show();
        let _ = window.set_focus();
        let _ = app.emit("game-ended", failed.then(|| complaint(&said, &first_file)));
        start_pads(&app);
    });
    Ok(())
}

/// The machines' ROMs among the files at `paths`, into `library`, by the
/// emulator, which knows them: which held any, the ROMs, and how many of the
/// II+'s chips.
#[tauri::command(async)]
fn sort_roms(app: AppHandle, paths: Vec<String>) -> Result<Value, String> {
    let wanted = paths
        .iter()
        .filter(|path| !path.contains('\n') && allowed(&app, path).is_ok());
    let list: String = wanted.map(|path| format!("{path}\n")).collect();
    let mut command = emulator_command()?;
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

/// Where a window of `outer` size goes, framed `outer - inner`, in the part of
/// a screen at `at` and `area` that's free, a taskbar's left out: its `inner`
/// size no more than leaves it all there, and the place that centres it. Its
/// frame's not known yet, on Linux, its `outer` size being a stand-in, less
/// than `inner`: it's taken as having none.
fn fitted(
    inner: (u32, u32),
    outer: (u32, u32),
    at: (i32, i32),
    area: (u32, u32),
) -> ((u32, u32), (i32, i32)) {
    let outer = (outer.0.max(inner.0), outer.1.max(inner.1));
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
#[derive(Default)]
struct Windowed(Mutex<Option<(PhysicalPosition<i32>, PhysicalSize<u32>)>>);

/// The window fullscreen, as the TV layout has it, or back as it was. Hidden
/// and shown again for a game, a window manager can forget what it was, so
/// it's kept here; and put back a moment after, once the window manager's done
/// with putting back what it remembers. With nothing kept, it's as it first
/// opens.
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

/// The app closed: by Ctrl+Q, or for a gamepad, from Settings.
#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

/// The window all on the screen it's opening on, however big that is, and
/// whatever a system's scaling makes of the size it asks for.
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

fn main() {
    tauri::Builder::default()
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
            watch_folders(app.handle());
            look_over_network(app.handle().clone());
            fit_to_screen(&app.get_webview_window("main").ok_or("no window")?)?;
            Ok(())
        })
        .manage(Dropped::default())
        .manage(Pads::default())
        .manage(Windowed::default())
        .manage(Watching::default())
        .manage(Learning::default())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            all_files,
            known_files,
            file_bytes,
            keep_file,
            forget_path,
            keep_art,
            all_art,
            pick_folder,
            take_dropped,
            folder_files,
            play,
            library_settings,
            remove_folder,
            restore_hidden,
            sort_roms,
            fullscreen,
            quit,
            add_folder_files,
            kept_games,
            favourite,
            game_facts,
            game_settings,
            choose_settings
        ])
        .run(tauri::generate_context!())
        .expect("couldn't open the window");
}

#[cfg(test)]
mod tests {
    use super::fitted;

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
    fn an_outer_size_less_than_the_inner_isnt_believed() {
        assert_eq!(
            fitted((1280, 800), (320, 158), (0, 36), (1920, 1044)),
            ((1280, 800), (320, 158))
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
}
