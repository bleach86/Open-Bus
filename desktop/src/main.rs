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
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
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

/// A folder no longer looked in: its files are left as they are.
#[tauri::command(async)]
fn remove_folder(app: AppHandle, path: String) -> Result<(), String> {
    take_out(&app, "folders", &path)
}

/// A game hidden, back.
#[tauri::command(async)]
fn restore_hidden(app: AppHandle, path: String) -> Result<(), String> {
    take_out(&app, "hidden", &path)
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

/// What `machine`, as the page says, is on the emulator's command line.
fn command_line(app: &AppHandle, machine: &Value) -> Result<Vec<String>, String> {
    let text = |field: &str| machine[field].as_str().ok_or(format!("no {field}"));
    let file = |path: &str| Ok::<_, String>(allowed(app, path)?.to_string_lossy().into_owned());
    if text("kind")? == "controls" {
        return Ok(vec!["controls".to_string()]);
    }
    if text("kind")? == "nes" {
        return Ok(vec!["nes".to_string(), file(text("game")?)?]);
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
    let mut command = Command::new(emulator()?);
    command
        .args(line)
        .arg(window_place(&window)?)
        .current_dir(&library)
        .stderr(Stdio::piped());
    // Without a console flashing up behind it, on Windows.
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut command, CREATE_NO_WINDOW);
    let mut game = command.spawn().map_err(|e| e.to_string())?;
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
    });
    Ok(())
}

/// The app's menu: its own items, told to the page, and the system's.
fn menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let item = |id: &str, text: &str, keys: &str| MenuItem::with_id(app, id, text, true, Some(keys));
    let library = Submenu::with_items(
        app,
        "Library",
        true,
        &[
            &item("add-folder", "Add Folder…", "CmdOrCtrl+O")?,
            &item("rescan", "Look for New Games", "CmdOrCtrl+R")?,
            &item("find", "Search", "CmdOrCtrl+F")?,
            &PredefinedMenuItem::separator(app)?,
            &item("settings", "Settings…", "CmdOrCtrl+,")?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    Menu::with_items(app, &[&library, &edit])
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
            app.set_menu(menu(app.handle())?)?;
            app.on_menu_event(|app, event| {
                let _ = app.emit("menu", event.id().0.clone());
            });
            Ok(())
        })
        .manage(Dropped::default())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            all_files, known_files, file_bytes, keep_file, forget_path, keep_art, all_art, pick_folder, take_dropped, folder_files, play,
            library_settings, remove_folder, restore_hidden
        ])
        .run(tauri::generate_context!())
        .expect("couldn't open the window");
}
