//! The app on Android: a game's on the game screen, `GameActivity`, in a
//! process of its own (":game"), the emulator's library run as the program
//! is on a desktop, with the same command line; and the emulator's program
//! itself, for what it's asked without a window (what's known of the games,
//! the ROMs sorted), among the app's native libraries, as Android lets an
//! app run only what it was installed with. Folders are read by their paths,
//! as on a desktop, once the app's been let: Android's All files access.

use jni::JNIEnv;
use jni::objects::{JObject, JValue};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use tauri::WebviewWindow;

/// The emulator's program, as tools/build-android names it: a library's
/// name, for it to be installed with them.
const PROGRAM: &str = "libcpu_emu_exe.so";
/// The game's process, as the manifest names it.
const GAME: &str = "app.openbus:game";
/// How often it's looked for; and how long it has to start.
const LOOK_EVERY: Duration = Duration::from_millis(250);
const STARTING: Duration = Duration::from_secs(10);
/// How long the activity has to answer.
const ANSWER: Duration = Duration::from_secs(5);
/// Reading what's stored, as Android before 11 lets an app.
const READ_STORAGE: &str = "android.permission.READ_EXTERNAL_STORAGE";
/// The device's own storage, as an app has it.
const OWN_STORAGE: &str = "/storage/emulated/0";

/// Where the app's native libraries are: this one's folder, as it's mapped.
pub fn libraries() -> Result<PathBuf, String> {
    let maps = fs::read_to_string("/proc/self/maps").map_err(|e| e.to_string())?;
    maps.lines()
        .filter_map(|line| line.split_whitespace().nth(5))
        .find(|path| path.ends_with("/libopen_bus_lib.so"))
        .and_then(|path| Path::new(path).parent())
        .map(Path::to_path_buf)
        .ok_or("the app's libraries aren't to be found".to_string())
}

/// The emulator's program.
pub fn emulator() -> Result<PathBuf, String> {
    let program = libraries()?.join(PROGRAM);
    program
        .exists()
        .then_some(program)
        .ok_or("the emulator, cpu_emu, isn't here".to_string())
}

/// `work` done with the activity, on its own thread, as Android has it:
/// what it came to.
fn on_activity<T: Send + 'static>(
    window: &WebviewWindow,
    work: impl FnOnce(&mut JNIEnv, &JObject) -> jni::errors::Result<T> + Send + 'static,
) -> Result<T, String> {
    let (answer, answered) = mpsc::channel();
    window
        .with_webview(move |webview| {
            webview.jni_handle().exec(move |env, activity, _| {
                let done = work(env, activity);
                if done.is_err() {
                    let _ = env.exception_clear();
                }
                let _ = answer.send(done.map_err(|e| e.to_string()));
            });
        })
        .map_err(|e| e.to_string())?;
    answered.recv_timeout(ANSWER).map_err(|e| e.to_string())?
}

/// Android's version, as its API level.
fn level(env: &mut JNIEnv) -> jni::errors::Result<i32> {
    env.get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?
        .i()
}

/// The game started, with `line` as its command line, in `folder`, what it
/// says written to `log`.
pub fn start(
    window: &WebviewWindow,
    line: Vec<String>,
    folder: &Path,
    log: &Path,
) -> Result<(), String> {
    let (folder, log) = (
        folder.to_string_lossy().into_owned(),
        log.to_string_lossy().into_owned(),
    );
    on_activity(window, move |env, activity| {
        start_activity(env, activity, &line, &folder, &log)
    })
    .map_err(|e| format!("the game didn't start: {e}"))
}

/// Whether the app may read the folders stored: from Android 11, All files
/// access; before, reading what's stored.
pub fn storage_access(window: &WebviewWindow) -> Result<bool, String> {
    on_activity(window, |env, activity| {
        if level(env)? >= 30 {
            return env
                .call_static_method(
                    "android/os/Environment",
                    "isExternalStorageManager",
                    "()Z",
                    &[],
                )?
                .z();
        }
        let wanted = env.new_string(READ_STORAGE)?;
        let granted = env
            .call_method(
                activity,
                "checkSelfPermission",
                "(Ljava/lang/String;)I",
                &[JValue::Object(&wanted)],
            )?
            .i()?;
        Ok(granted == 0)
    })
}

/// That asked for: from Android 11, its settings for the app opened, for
/// it to be turned on there; before, asked as a permission is.
pub fn ask_storage_access(window: &WebviewWindow) -> Result<(), String> {
    on_activity(window, |env, activity| {
        if level(env)? >= 30 {
            let package = env
                .call_method(activity, "getPackageName", "()Ljava/lang/String;", &[])?
                .l()?;
            let package = env
                .get_string(&package.into())?
                .to_str()
                .unwrap_or_default()
                .to_string();
            let action =
                env.new_string("android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION")?;
            let text = env.new_string(format!("package:{package}"))?;
            let uri = env
                .call_static_method(
                    "android/net/Uri",
                    "parse",
                    "(Ljava/lang/String;)Landroid/net/Uri;",
                    &[JValue::Object(&text)],
                )?
                .l()?;
            let intent = env.new_object(
                "android/content/Intent",
                "(Ljava/lang/String;Landroid/net/Uri;)V",
                &[JValue::Object(&action), JValue::Object(&uri)],
            )?;
            env.call_method(
                activity,
                "startActivity",
                "(Landroid/content/Intent;)V",
                &[JValue::Object(&intent)],
            )?;
        } else {
            let wanted = env.new_object_array(1, "java/lang/String", JObject::null())?;
            let read = env.new_string(READ_STORAGE)?;
            env.set_object_array_element(&wanted, 0, read)?;
            env.call_method(
                activity,
                "requestPermissions",
                "([Ljava/lang/String;I)V",
                &[JValue::Object(&wanted), JValue::Int(1)],
            )?;
        }
        Ok(())
    })
}

/// Where folders are, to look through: the device's own storage; and by
/// its name, each drive put in, and each place a Shield has network shares
/// from, as they're mounted: /storage itself isn't the app's to look in.
pub fn storage_places() -> Vec<(String, PathBuf)> {
    let mounts = fs::read_to_string("/proc/self/mounts").unwrap_or_default();
    let mut mounted: Vec<&str> = mounts
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1)?.strip_prefix("/storage/"))
        .filter_map(|under| under.split('/').next())
        .filter(|top| !top.is_empty() && !["emulated", "self"].contains(top))
        .collect();
    mounted.sort_unstable();
    mounted.dedup();
    [("This device".to_string(), PathBuf::from(OWN_STORAGE))]
        .into_iter()
        .chain(
            mounted
                .into_iter()
                .map(|top| (top.to_string(), Path::new("/storage").join(top))),
        )
        .collect()
}

/// `new Intent(activity, GameActivity.class)`, its extras the command
/// line, the folder and the log; started.
fn start_activity(
    env: &mut JNIEnv,
    activity: &JObject,
    line: &[String],
    folder: &str,
    log: &str,
) -> jni::errors::Result<()> {
    // By the app's own class loader: this thread's is the system's.
    let loader = env
        .call_method(activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?
        .l()?;
    let name = env.new_string("app.openbus.GameActivity")?;
    let class = env
        .call_method(
            &loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&name)],
        )?
        .l()?;
    let intent = env.new_object(
        "android/content/Intent",
        "(Landroid/content/Context;Ljava/lang/Class;)V",
        &[JValue::Object(activity), JValue::Object(&class)],
    )?;
    let args = env.new_object_array(line.len() as i32, "java/lang/String", JObject::null())?;
    for (n, arg) in line.iter().enumerate() {
        let arg = env.new_string(arg)?;
        env.set_object_array_element(&args, n as i32, arg)?;
    }
    let extra = |env: &mut JNIEnv, key: &str, value: &JObject| -> jni::errors::Result<()> {
        let key = env.new_string(key)?;
        let signature = if env.is_instance_of(value, "java/lang/String")? {
            "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;"
        } else {
            "(Ljava/lang/String;[Ljava/lang/String;)Landroid/content/Intent;"
        };
        env.call_method(
            &intent,
            "putExtra",
            signature,
            &[JValue::Object(&key), JValue::Object(value)],
        )?;
        Ok(())
    };
    extra(env, "args", &args)?;
    let folder = JObject::from(env.new_string(folder)?);
    extra(env, "folder", &folder)?;
    let log = JObject::from(env.new_string(log)?);
    extra(env, "log", &log)?;
    env.call_method(
        activity,
        "startActivity",
        "(Landroid/content/Intent;)V",
        &[JValue::Object(&intent)],
    )?;
    Ok(())
}

/// Whether the game's process is running.
fn game_running() -> bool {
    let Ok(processes) = fs::read_dir("/proc") else {
        return false;
    };
    let numbered = |name: &str| name.bytes().all(|byte| byte.is_ascii_digit());
    processes.flatten().any(|process| {
        process.file_name().to_str().is_some_and(numbered)
            && fs::read(process.path().join("cmdline"))
                .is_ok_and(|line| line.split(|&byte| byte == 0).next() == Some(GAME.as_bytes()))
    })
}

/// Until the game's over: its process started, and gone. Whether it ran.
pub fn wait() -> bool {
    let starting = Instant::now();
    while !game_running() {
        if starting.elapsed() > STARTING {
            return false;
        }
        thread::sleep(LOOK_EVERY);
    }
    while game_running() {
        thread::sleep(LOOK_EVERY);
    }
    true
}
