//! Open Bus as a desktop app: the web page's library, in a window of its
//! own, its files on the disk rather than in a browser's storage. A folder
//! added is only remembered, in `settings.json` in the app's data folder,
//! and looked through each time: its games stay where they are, and a game
//! removed from the library is hidden, its file never touched. Files added
//! one by one are copied to `library` there, and may be deleted; the ROMs a
//! machine needs too. The covers and what's known of the games are in `art`.
//!
//! The page's only way to the disk is these commands: a path it gives one
//! is checked to be in `library` or in a folder that's been added.
//!
//! Looking through the folders takes a while, on a NAS, so what it found is
//! kept, in `files.json`, for the page to show at once when it starts: it
//! looks again behind that, and only says if anything's changed.
//!
//! A game's the native emulator, `cpu_emu`, as a process of its own, as
//! EmulationStation has it: it opens where the window is, that going away
//! until the game's over. Not in the same process, each wanting the main
//! thread as they do on a Mac; nor in the same window, there being no one
//! way to on the three systems. A game that fails leaves the library alone.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

/// How long the library stays up after a game starts, for its window to be
/// there to take over: there's no flash of the desktop between the two.
const OVERLAP: Duration = Duration::from_millis(350);
/// Windows's process flag for that.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A folder of the app's, made if it isn't there yet.
fn folder(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let folder = app.path().app_data_dir().map_err(|e| e.to_string())?.join(name);
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    Ok(folder)
}

/// Where the page's `name` is: only ever under `library`.
fn place(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let path = Path::new(name);
    let plain = path.components().all(|part| matches!(part, Component::Normal(_)));
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

/// The folders added, and the files hidden: from `settings.json`.
fn settings(app: &AppHandle) -> Result<Value, String> {
    let path = app.path().app_data_dir().map_err(|e| e.to_string())?.join("settings.json");
    Ok(match fs::read(path) {
        Ok(text) => serde_json::from_slice(&text).map_err(|e| e.to_string())?,
        Err(_) => json!({ "folders": [], "hidden": [] }),
    })
}

fn save_settings(app: &AppHandle, settings: &Value) -> Result<(), String> {
    let path = app.path().app_data_dir().map_err(|e| e.to_string())?.join("settings.json");
    write(&path, settings.to_string().as_bytes())
}

fn listed(settings: &Value, field: &str) -> Vec<String> {
    settings[field].as_array().map_or(Vec::new(), |list| list.iter().filter_map(|item| item.as_str().map(String::from)).collect())
}

/// `path`, if it's somewhere the page may go: `library`, or a folder added.
fn allowed(app: &AppHandle, path: &str) -> Result<PathBuf, String> {
    let real = fs::canonicalize(path).map_err(|e| format!("{path}: {e}"))?;
    let mut places = listed(&settings(app)?, "folders").into_iter().map(PathBuf::from).collect::<Vec<_>>();
    places.push(folder(app, "library")?);
    let inside = |place: &PathBuf| fs::canonicalize(place).is_ok_and(|place| real.starts_with(place));
    let dropped = app.state::<Dropped>().0.lock().map_err(|e| e.to_string())?.contains(&real);
    if dropped || places.iter().any(inside) { Ok(real) } else { Err(format!("not in the library: {path}")) }
}

/// All the files under `folder`, with their sizes; not those in folders
/// that aren't anyone's games: a hidden one, or a NAS's own, as @eaDir is.
fn walk(folder: &Path, found: &mut Vec<Value>) {
    let Ok(entries) = fs::read_dir(folder) else { return };
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
            found.push(json!({ "path": path.to_string_lossy(), "size": size }));
        }
    }
}

/// `folder` remembered.
fn remember(app: &AppHandle, folder: &str) -> Result<(), String> {
    let mut kept = settings(app)?;
    if !listed(&kept, "folders").iter().any(|known| known == folder) {
        kept["folders"].as_array_mut().ok_or("bad settings")?.push(json!(folder));
        save_settings(app, &kept)?;
        watch_folders(app);
    }
    Ok(())
}

/// What Settings shows: the folders, the hidden, and the app's own folder.
#[tauri::command(async)]
fn library_settings(app: AppHandle) -> Result<Value, String> {
    let kept = settings(&app)?;
    let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(json!({ "folders": listed(&kept, "folders"), "hidden": listed(&kept, "hidden"), "data": data.to_string_lossy() }))
}

/// `path` gone from `field` of the settings.
fn take_out(app: &AppHandle, field: &str, path: &str) -> Result<(), String> {
    let mut kept = settings(app)?;
    kept[field].as_array_mut().ok_or("bad settings")?.retain(|item| item != path);
    save_settings(app, &kept)
}

/// The kinds of file the library has, a game's or a disk's: what's worth a
/// watcher's noticing. Saves and states beside them change as games are
/// played, and don't matter to it.
const GAMES: [&str; 10] = ["nes", "fds", "zip", "dsk", "do", "po", "nib", "woz", "hdv", "2mg"];

/// The folders added, watched for games coming and going, and the watcher.
#[derive(Default)]
struct Watching(Mutex<Option<Debouncer<RecommendedWatcher>>>);

/// The folders added watched, as they are now: what was watching them before
/// stopped.
fn watch_folders(app: &AppHandle) {
    let folders = settings(app).map(|kept| listed(&kept, "folders")).unwrap_or_default();
    let seeing = app.clone();
    let watching = new_debouncer(Duration::from_millis(800), move |events: DebounceEventResult| {
        if let Ok(events) = events {
            seen(&seeing, events.into_iter().map(|event| event.path).collect());
        }
    });
    let Ok(mut watcher) = watching else { return };
    for folder in &folders {
        let _ = watcher.watcher().watch(Path::new(folder), RecursiveMode::Recursive);
    }
    if let Ok(mut watched) = app.state::<Watching>().0.lock() {
        *watched = Some(watcher);
    }
}

/// Whether `path`, under one of `folders`, is somewhere `walk` doesn't go: a
/// hidden folder, a NAS's own, a download not finished.
fn passed_over(path: &Path, folders: &[String]) -> bool {
    let Some(within) = folders.iter().find_map(|folder| path.strip_prefix(folder).ok()) else { return true };
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
    let Ok(kept) = settings(app) else { return };
    let (folders, hidden) = (listed(&kept, "folders"), listed(&kept, "hidden"));
    let game = |path: &Path| path.extension().is_some_and(|kind| GAMES.contains(&kind.to_string_lossy().to_lowercase().as_str()));
    let mut changed = false;
    let _ = change_found(app, |found| {
        let Some(list) = found["folders"].as_array_mut() else { return };
        for path in paths.iter().filter(|path| !passed_over(path, &folders)) {
            let mut came = Vec::new();
            if path.is_dir() {
                walk(path, &mut came);
            } else if path.is_file() {
                if game(path) {
                    let size = fs::metadata(path).map_or(0, |data| data.len());
                    came.push(json!({ "path": path.to_string_lossy(), "size": size }));
                }
            } else {
                let before = list.len();
                list.retain(|entry| !entry["path"].as_str().is_some_and(|file| Path::new(file).starts_with(path)));
                changed |= list.len() != before;
                continue;
            }
            for entry in came {
                let file = entry["path"].as_str().unwrap_or_default().to_string();
                let known = list.iter().any(|known| known["path"] == entry["path"]);
                if game(Path::new(&file)) && !known && !hidden.contains(&file) {
                    list.push(entry);
                    changed = true;
                }
            }
        }
    });
    if changed {
        let _ = app.emit("files-changed", ());
    }
}

/// Whether `folder`'s on another computer, where a watcher's not told of
/// what's done there, only of what's done from here: an NFS, SMB or SSH
/// mount's, on Linux, as /proc/mounts has them; a share's, on Windows.
fn on_network(folder: &str) -> bool {
    if folder.starts_with(r"\\") {
        return true;
    }
    let Ok(mounts) = fs::read_to_string("/proc/mounts") else { return false };
    let mount = mounts
        .lines()
        .filter_map(|line| {
            let mut parts = line.split(' ');
            let (_, place, kind) = (parts.next()?, parts.next()?.replace("\\040", " "), parts.next()?);
            Path::new(folder).starts_with(&place).then(|| (place.len(), kind.to_string()))
        })
        .max();
    mount.is_some_and(|(_, kind)| ["nfs", "nfs4", "cifs", "smb3", "smbfs", "fuse.sshfs"].contains(&kind.as_str()))
}

/// How often the folders on other computers are looked through again.
const NETWORK_LOOK: Duration = Duration::from_secs(300);

/// The folders looked through again, if any of them are on another computer,
/// each NETWORK_LOOK: the page told if what's there has changed.
fn look_over_network(app: AppHandle) {
    thread::spawn(move || loop {
        thread::sleep(NETWORK_LOOK);
        let folders = settings(&app).map(|kept| listed(&kept, "folders")).unwrap_or_default();
        if !folders.iter().any(|folder| on_network(folder)) {
            continue;
        }
        let before = known_files(app.clone()).unwrap_or(Value::Null);
        if all_files(app.clone()).is_ok_and(|now| now != before) {
            let _ = app.emit("files-changed", ());
        }
    });
}

/// What `all_files` last found, to be changed by `change`, as one thing's
/// done that changes it, so that it's not all looked for again for that.
fn change_found(app: &AppHandle, change: impl FnOnce(&mut Value)) -> Result<(), String> {
    let file = found_files(app)?;
    let mut found = fs::read(&file).ok().and_then(|text| serde_json::from_slice(&text).ok()).unwrap_or_else(|| {
        json!({ "root": folder(app, "library").map(|path| path.to_string_lossy().into_owned()).unwrap_or_default(), "library": [], "folders": [] })
    });
    change(&mut found);
    write(&file, found.to_string().as_bytes())
}

/// `entries`, files found, with what was found before, in `section`, the
/// library's or the folders': those hidden, or that are there already, left
/// out. What's put there.
fn add_found(app: &AppHandle, section: &str, entries: Vec<Value>) -> Result<Vec<Value>, String> {
    let hidden = listed(&settings(app)?, "hidden");
    let mut added = Vec::new();
    change_found(app, |found| {
        let Some(list) = found[section].as_array_mut() else { return };
        for entry in entries {
            let path = entry["path"].as_str().unwrap_or_default();
            if !hidden.iter().any(|hidden| hidden == path) && !list.iter().any(|known| known["path"] == entry["path"]) {
                list.push(entry.clone());
                added.push(entry);
            }
        }
    })?;
    Ok(added)
}

/// A folder's files, as `folder_files` found them, ROMs taken out, now with
/// the rest: the folder isn't looked through again for them.
#[tauri::command(async)]
fn add_folder_files(app: AppHandle, entries: Vec<Value>) -> Result<Vec<Value>, String> {
    let allowed_entries = entries.into_iter().filter(|entry| entry["path"].as_str().is_some_and(|path| allowed(&app, path).is_ok())).collect();
    add_found(&app, "folders", allowed_entries)
}

/// A folder no longer looked in: its files are left as they are, and what was
/// found in it's forgotten, not all looked for again.
#[tauri::command(async)]
fn remove_folder(app: AppHandle, path: String) -> Result<(), String> {
    take_out(&app, "folders", &path)?;
    watch_folders(&app);
    let inside = Path::new(&path).to_path_buf();
    change_found(&app, |found| {
        if let Some(list) = found["folders"].as_array_mut() {
            list.retain(|entry| !entry["path"].as_str().is_some_and(|file| Path::new(file).starts_with(&inside)));
        }
    })
}

/// Games hidden, back: out of `settings.json`'s hidden, and into what's
/// been found, in `files.json`, as a folder's are, without its folders being
/// looked through again. What's still there of them, as `all_files` has a
/// folder's files.
#[tauri::command(async)]
fn restore_hidden(app: AppHandle, paths: Vec<String>) -> Result<Vec<Value>, String> {
    let mut kept = settings(&app)?;
    kept["hidden"].as_array_mut().ok_or("bad settings")?.retain(|item| !paths.iter().any(|path| item == path));
    save_settings(&app, &kept)?;
    let back: Vec<Value> = paths
        .iter()
        .filter_map(|path| fs::metadata(path).ok().map(|file| json!({ "path": path, "size": file.len() })))
        .collect();
    add_found(&app, "folders", back)
}

/// A folder chosen, and remembered: its path, if one was.
#[tauri::command(async)]
fn pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    let Some(chosen) = app.dialog().file().blocking_pick_folder() else { return Ok(None) };
    let chosen = chosen.into_path().map_err(|e| e.to_string())?.to_string_lossy().into_owned();
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
            app.state::<Dropped>().0.lock().map_err(|e| e.to_string())?.push(real);
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
    Ok(found)
}

/// Every file there is: `library`'s, and the added folders', the hidden left
/// out, as paths: which first, and whether it's a folder's, for the page.
#[tauri::command(async)]
fn all_files(app: AppHandle) -> Result<Value, String> {
    let kept = settings(&app)?;
    let hidden = listed(&kept, "hidden");
    let mut library = Vec::new();
    walk(&folder(&app, "library")?, &mut library);
    let mut added = Vec::new();
    for place in listed(&kept, "folders") {
        walk(Path::new(&place), &mut added);
    }
    added.retain(|file| !hidden.iter().any(|path| file["path"] == *path));
    let found = json!({ "root": folder(&app, "library")?.to_string_lossy(), "library": library, "folders": added });
    write(&found_files(&app)?, found.to_string().as_bytes())?;
    Ok(found)
}

fn found_files(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("files.json"))
}

/// What `all_files` last found, if it's looked yet.
#[tauri::command(async)]
fn known_files(app: AppHandle) -> Result<Value, String> {
    Ok(fs::read(found_files(&app)?).ok().and_then(|text| serde_json::from_slice(&text).ok()).unwrap_or(Value::Null))
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
    let mut kept = settings(&app)?;
    kept["hidden"].as_array_mut().ok_or("bad settings")?.push(json!(path));
    save_settings(&app, &kept)
}

/// A game's art's file, by a hash of its key, which can be any text.
fn art_file(app: &AppHandle, key: &str) -> Result<PathBuf, String> {
    let hash = key.bytes().fold(0xCBF2_9CE4_8422_2325u64, |hash, byte| (hash ^ byte as u64).wrapping_mul(0x100_0000_01B3));
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
    let beside = std::env::current_exe().map_err(|e| e.to_string())?.with_file_name(&name);
    let built = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/release").join(&name);
    [beside, built].into_iter().find(|path| path.exists()).ok_or("the emulator, cpu_emu, isn't here".to_string())
}

/// The gamepads, for the library: a webview's own miss what its gamepad
/// library doesn't know, so they're read as a game reads them, by the
/// emulator with no window, what's held told to the page. It's stopped for a
/// game, which has the gamepads to itself.
#[derive(Default)]
struct Pads(Mutex<Option<Child>>);

fn start_pads(app: &AppHandle) {
    let Ok(mut command) = emulator_command() else { return };
    command.arg("pads").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    let Ok(mut pads) = command.spawn() else { return };
    let Some(said) = pads.stdout.take() else { return };
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
    let Some(mut pads) = app.state::<Pads>().0.lock().unwrap().take() else { return };
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

/// What `machine`, as the page says, is on the emulator's command line.
fn command_line(app: &AppHandle, machine: &Value) -> Result<Vec<String>, String> {
    let text = |field: &str| machine[field].as_str().ok_or(format!("no {field}"));
    let file = |path: &str| Ok::<_, String>(allowed(app, path)?.to_string_lossy().into_owned());
    if text("kind")? == "controls" {
        return Ok(vec!["controls".to_string()]);
    }
    if text("kind")? == "nes" {
        let fast = machine["fast"].as_bool().unwrap_or(false).then(|| "--fast".to_string());
        return Ok(["nes".to_string(), file(text("game")?)?].into_iter().chain(fast).collect());
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
    line.push(format!("--mockingboards={}", machine["mockingboards"].as_u64().unwrap_or(1)));
    Ok(line)
}

/// `--window=`, for the emulator to open where the library is.
fn window_place(window: &WebviewWindow) -> Result<String, String> {
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let at = window.inner_position().map_err(|e| e.to_string())?.to_logical::<i32>(scale);
    let size = window.inner_size().map_err(|e| e.to_string())?.to_logical::<u32>(scale);
    Ok(format!("--window={},{},{},{}", at.x, at.y, size.width, size.height))
}

/// What a failed game had to say: a panic's message, without its place in
/// the code, or the folders of the game it names.
fn complaint(said: &str, game: &str) -> String {
    let useful = |line: &&str| !line.starts_with("thread ") && !line.starts_with("note:") && !line.is_empty();
    let line = said.lines().find(useful).unwrap_or("the game stopped");
    let name = Path::new(game).file_name().map_or(game.into(), |name| name.to_string_lossy());
    line.replace(game, &name)
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
        .args(machine["fullscreen"].as_bool().unwrap_or(false).then_some("--fullscreen"))
        .args(machine["resume"].as_bool().unwrap_or(false).then_some("--resume"))
        .arg(window_place(&window)?)
        .current_dir(&library)
        .stderr(Stdio::piped());
    stop_pads(&app);
    let mut game = command.spawn().map_err(|e| {
        start_pads(&app);
        e.to_string()
    })?;
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
    let wanted = paths.iter().filter(|path| !path.contains('\n') && allowed(&app, path).is_ok());
    let list: String = wanted.map(|path| format!("{path}\n")).collect();
    let mut command = emulator_command()?;
    let library = folder(&app, "library")?;
    let mut sorter = command.arg("roms").arg(library).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    let mut input = sorter.stdin.take().ok_or("no input")?;
    thread::spawn(move || input.write_all(list.as_bytes()));
    let finished = sorter.wait_with_output().map_err(|e| e.to_string())?;
    if !finished.status.success() {
        return Err(String::from_utf8_lossy(&finished.stderr).into_owned());
    }
    let said = String::from_utf8_lossy(&finished.stdout);
    let of = |word: &str| said.lines().filter_map(|line| line.strip_prefix(word).map(str::to_string)).collect::<Vec<_>>();
    let chips: usize = of("chips ").first().and_then(|count| count.parse().ok()).unwrap_or(0);
    // The ROMs written into the library, with what was found there, for it
    // not to be looked through again for them.
    let library = folder(&app, "library")?;
    let written = of("rom ")
        .iter()
        .filter_map(|name| {
            let path = library.join(name);
            fs::metadata(&path).ok().map(|file| json!({ "path": path.to_string_lossy(), "size": file.len() }))
        })
        .collect();
    let kept = add_found(&app, "library", written)?;
    Ok(json!({ "held": of("held "), "roms": of("rom "), "chips": chips, "kept": kept }))
}

/// Where a window of `outer` size goes, framed `outer - inner`, in the part of
/// a screen at `at` and `area` that's free, a taskbar's left out: its `inner`
/// size no more than leaves it all there, and the place that centres it. Its
/// frame's not known yet, on Linux, its `outer` size being a stand-in, less
/// than `inner`: it's taken as having none.
fn fitted(inner: (u32, u32), outer: (u32, u32), at: (i32, i32), area: (u32, u32)) -> ((u32, u32), (i32, i32)) {
    let outer = (outer.0.max(inner.0), outer.1.max(inner.1));
    let frame = (outer.0 - inner.0, outer.1 - inner.1);
    let (width, height) = (outer.0.min(area.0), outer.1.min(area.1));
    let size = (width.saturating_sub(frame.0), height.saturating_sub(frame.1));
    (size, (at.0 + (area.0 - width) as i32 / 2, at.1 + (area.1 - height) as i32 / 2))
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
            *kept = Some((window.outer_position().map_err(|e| e.to_string())?, window.inner_size().map_err(|e| e.to_string())?));
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
    let Some(monitor) = window.current_monitor()? else { return Ok(()) };
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
            // WebKitGTK alone doesn't animate a mouse wheel's steps.
            #[cfg(target_os = "linux")]
            app.get_webview_window("main").ok_or("no window")?.with_webview(|webview| {
                use webkit2gtk::{SettingsExt, WebViewExt};
                if let Some(settings) = WebViewExt::settings(&webview.inner()) {
                    settings.set_enable_smooth_scrolling(true);
                }
            })?;
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
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            all_files, known_files, file_bytes, keep_file, forget_path, keep_art, all_art, pick_folder, take_dropped, folder_files, play,
            library_settings, remove_folder, restore_hidden, sort_roms, fullscreen, quit, add_folder_files
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
        assert_eq!(fitted((1600, 1000), (1616, 1079), (0, 0), (1920, 1032)), ((1600, 953), (152, 0)));
    }

    #[test]
    fn a_window_that_fits_stays_its_size_and_is_centred_in_whats_free() {
        assert_eq!(fitted((1280, 800), (1296, 860), (0, 0), (1920, 1040)), ((1280, 800), (312, 90)));
    }

    /// As Linux has it, before the window's first drawn.
    #[test]
    fn an_outer_size_less_than_the_inner_isnt_believed() {
        assert_eq!(fitted((1280, 800), (320, 158), (0, 36), (1920, 1044)), ((1280, 800), (320, 158)));
    }

    /// On a second screen, the place is that screen's.
    #[test]
    fn its_place_is_on_the_screen_its_opening_on() {
        assert_eq!(fitted((800, 600), (800, 600), (1920, 100), (1600, 900)), ((800, 600), (2320, 250)));
    }
}
