//! Open Bus as a desktop app: the web page's library, in a window of its
//! own, its files on the disk rather than in a browser's storage. Those are
//! in the app's data folder, the games, disks and ROMs in `library`, as the
//! page names them, and the covers and what's known of the games in `art`.
//!
//! The page's only way to the disk is these commands, each name it gives
//! one in `library`, checked to stay there.
//!
//! A game's the native emulator, `cpu_emu`, as a process of its own, as
//! EmulationStation has it: it opens where the window is, that going away
//! until the game's over. Not in the same process, each wanting the main
//! thread as they do on a Mac; nor in the same window, there being no one
//! way to on the three systems. A game that fails leaves the library alone.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde_json::Value;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

/// How long the library stays up after a game starts, for its window to be
/// there to take over: there's no flash of the desktop between the two.
const OVERLAP: Duration = Duration::from_millis(350);

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

/// The names of all under `folder`, those in folders with `/`.
fn names_under(folder: &Path, prefix: &str, names: &mut Vec<String>) -> Result<(), String> {
    for entry in fs::read_dir(folder).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
        if entry.path().is_dir() {
            names_under(&entry.path(), &format!("{name}/"), names)?;
        } else if !name.ends_with(".part") {
            names.push(name);
        }
    }
    Ok(())
}

#[tauri::command]
fn file_names(app: AppHandle) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    names_under(&folder(&app, "library")?, "", &mut names)?;
    Ok(names)
}

#[tauri::command]
fn file_bytes(app: AppHandle, name: String) -> Result<Response, String> {
    let bytes = fs::read(place(&app, &name)?).map_err(|e| e.to_string())?;
    Ok(Response::new(bytes))
}

#[tauri::command]
fn keep_file(app: AppHandle, request: Request<'_>) -> Result<(), String> {
    write(&place(&app, &header(&request, "name")?)?, body(&request)?)
}

/// A file gone, and the folder it was in if that's left empty.
#[tauri::command]
fn forget_file(app: AppHandle, name: String) -> Result<(), String> {
    let path = place(&app, &name)?;
    fs::remove_file(&path).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        let _ = fs::remove_dir(parent); // refused if there's anything in it
    }
    Ok(())
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
    let file = |name: &str| Ok::<_, String>(place(app, name)?.to_string_lossy().into_owned());
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
/// the code or where the library is.
fn complaint(said: &str, library: &Path) -> String {
    let useful = |line: &&str| !line.starts_with("thread ") && !line.starts_with("note:") && !line.is_empty();
    let line = said.lines().find(useful).unwrap_or("the game stopped");
    let folder = format!("{}{}", library.display(), std::path::MAIN_SEPARATOR);
    line.replace(&folder, "")
}

/// A game played, the window away until it's over.
#[tauri::command]
fn play(app: AppHandle, window: WebviewWindow, machine: Value) -> Result<(), String> {
    let library = folder(&app, "library")?;
    let mut game = Command::new(emulator()?)
        .args(command_line(&app, &machine)?)
        .arg(window_place(&window)?)
        .current_dir(&library)
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
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
        let _ = app.emit("game-ended", failed.then(|| complaint(&said, &library)));
    });
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![file_names, file_bytes, keep_file, forget_file, keep_art, all_art, play])
        .run(tauri::generate_context!())
        .expect("couldn't open the window");
}
