//! The emulator as a program: which machine to run, with what, as its
//! command line says. `cpu_emu --help` says it all, and `cpu_emu MACHINE
//! --help` all of a machine's; with no machine, this one's BASIC.
//!
//! The desktop app runs games so, its window's place given it, and asks it
//! what the gamepads hold, what's bound, what's known of its games and
//! where the Apple's ROMs are; Android's game screen gives it the same
//! command line; a launcher, as EmulationStation, can too.

use crate::host::machine::PowerOnRam;
use crate::machines::apple2::video::Monitor;
use crate::machines::region::Region;
use crate::{cpu, host, machines};
use clap::builder::{PossibleValuesParser, TypedValueParser};
use clap::{Args, Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

#[derive(Debug, Parser)]
#[command(name = "cpu_emu", version, about = "Open Bus: old machines, emulated")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) machine: Option<Machine>,
    #[command(flatten)]
    pub(crate) host: Host,
}

/// What any machine has of the host's: its window, its saves, and what's
/// kept and shown as it runs.
#[derive(Debug, Args)]
pub(crate) struct Host {
    /// Fullscreen, not in a window: F11, or a double click, switches.
    #[arg(long, global = true)]
    pub(crate) fullscreen: bool,
    /// Its window there, that size, to be resized: as the desktop app has
    /// it, in place of its library.
    #[arg(long, global = true, value_name = "X,Y,WIDTH,HEIGHT", value_parser = place)]
    pub(crate) window: Option<[i32; 4]>,
    /// Its state kept as it's put away, beside it, and gone on from next
    /// time.
    #[arg(long, global = true)]
    pub(crate) resume: bool,
    /// No rewinding: a console's last 30 seconds not kept to go back to.
    #[arg(long, global = true)]
    pub(crate) no_rewind: bool,
    /// No light at the corner of the picture while a disk drive's busy.
    #[arg(long, global = true)]
    pub(crate) no_drive_light: bool,
    /// Frames kept by the machine's clock alone, never in step with the
    /// display's refresh.
    #[arg(long, global = true)]
    pub(crate) no_vsync: bool,
    /// Started by a launcher, as EmulationStation might: Select and Start
    /// held together go back to it.
    #[arg(long, global = true)]
    pub(crate) launched: bool,
    /// Its saves kept under FOLDER, not beside the game: what it saves in
    /// saves/SYSTEM, its states in states/SYSTEM, by its name, as
    /// saves/nes/Zelda.srm and states/nes/Zelda.state; under the desktop
    /// app's data folder if it's not given.
    ///
    /// On Linux, ~/.local/share/app.openbus.desktop; on a Mac,
    /// ~/Library/Application Support/app.openbus.desktop; on Windows,
    /// %APPDATA%\app.openbus.desktop.
    #[arg(long, global = true, value_name = "FOLDER")]
    pub(crate) saves: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Machine {
    /// This one, a 6502 computer of our own, with BASIC.
    ///
    /// Its terminal the one it's run in, unless it's said.
    Basic {
        /// Its ROM, for BASIC's own.
        rom: Option<String>,
        /// Its screen alone, not the terminal.
        #[arg(long)]
        no_terminal: bool,
    },
    /// An Apple II+, with its disks.
    Apple2 {
        /// A dump of its ROM, its last 12K, $D000 up; the Disk II's ROMs
        /// (disk2-p5-341-0027.bin and disk2-p6-341-0028.bin) beside it.
        rom: String,
        #[command(flatten)]
        apple: Apple,
    },
    /// An enhanced Apple IIe, with its disks.
    Apple2e {
        /// The folder its ROMs are in, by their names, and the Disk II's.
        roms: String,
        /// The first IIe, not the enhanced.
        #[arg(long)]
        unenhanced: bool,
        #[command(flatten)]
        apple: Apple,
    },
    /// The NES, or a Famicom: a cartridge's game, or the Disk System's.
    Nes {
        /// An iNES image, or a zip of one; or a Disk System's .fds image, or
        /// a zip of one, disksys.rom, its BIOS, beside it or in roms/nes/,
        /// F9 changing the disk.
        rom: String,
        /// The picture's edges hidden as a TV hides them, so many dots or
        /// lines each, 0 to 32.
        #[arg(long, default_value_t = 8, value_parser = clap::value_parser!(u8).range(0..=32))]
        overscan: u8,
        /// Performance mode, for what's too slow otherwise: a line drawn at
        /// a time, less exact for it.
        #[arg(long)]
        fast: bool,
        /// Its RAM at power-on: all 1s, $FF, as most have it; all 0s; or as
        /// random as a real console's.
        #[arg(long, value_parser = choice(["ones", "zeros", "random"], PowerOnRam::parse))]
        ram: Option<PowerOnRam>,
        #[command(flatten)]
        region: TvRegion,
        /// Its console, not as the game's known to be.
        #[arg(long, value_parser = choice(["nes", "famicom"], |name| Some(name == "famicom")))]
        console: Option<bool>,
        /// What's in its second port, not as the game's known to be.
        #[arg(long, value_parser = choice(["controller", "zapper"], |name| Some(name == "zapper")))]
        port2: Option<bool>,
    },
    /// The Atari 2600.
    ///
    /// F7 and F8 its difficulty switches, F9 its colour switch.
    Atari2600 {
        /// A .a26, or a zip of one.
        rom: String,
        #[command(flatten)]
        region: TvRegion,
    },
    /// The Atari 7800, and the 2600's games as it plays them.
    ///
    /// F7 and F8 its difficulty switches, F9 its Pause button; the High
    /// Score Cartridge in it if highscre.bin's beside the game or in
    /// roms/atari7800/. A 2600's game played as a 2600, as the 7800 plays
    /// one.
    Atari7800 {
        /// An .a78, or a dump without its header, or a zip of one.
        rom: String,
        #[command(flatten)]
        region: TvRegion,
    },
    /// Skink, a 6502 computer of our own for GeckOS.
    Skink {
        /// Its boot image, loaded to the top of its memory:
        /// roms/skink/rom.bin, if it's not given.
        image: Option<String>,
        /// A disk's image, a FAT floppy's, say, written back as it's
        /// written: roms/skink/geckos.img, if it's not given and it's
        /// there.
        #[arg(long)]
        disk: Option<String>,
    },
    /// The Commander X16.
    X16 {
        /// Its rom.bin: roms/x16/'s, or its release folder's there, if it's
        /// not given.
        rom: Option<String>,
        /// A game's zip, folder or program, played from a card of its own,
        /// the SD card's or GAME.img where saves are kept (or beside it, if
        /// one was made there before): made, if it isn't there, to start it
        /// as the X16's turned on, and kept, with what the game saves.
        #[arg(long)]
        game: Option<String>,
        /// Its SD card's image, written back as it's written, made blank if
        /// it isn't there yet: roms/x16/sdcard.img, if it's not given and
        /// it's there.
        #[arg(long, value_name = "IMAGE")]
        sd: Option<String>,
        /// A folder on this computer, the X16's files: on its card as its
        /// FILES folder, and what's saved there back in it as it's stopped.
        #[arg(long, value_name = "FOLDER")]
        files: Option<String>,
    },
    /// The screen that changes the controls, alone.
    ///
    /// As F6 has it in a game: they're for every machine.
    Controls,
    /// What the gamepads hold, told as it changes: for the app's library.
    ///
    /// No window: a line on standard output for each change, till standard
    /// input closes.
    Pads,
    /// The controls as they're bound: for the app's library.
    ///
    /// As they're kept, a line each, for the library to choose and go back
    /// by the menus'.
    Bound,
    /// What's known of games: for the app's library.
    ///
    /// Each named on standard input, a line each: its CRC, mapper or
    /// system, console and controller, and anything that keeps it from
    /// running.
    Facts,
    /// The Apple's ROMs sorted into a folder: for the app's library.
    ///
    /// Those among the files named on standard input, written into FOLDER
    /// under the names they're looked for by.
    Roms { folder: PathBuf },
    /// The saves kept, a line each (their kind, bytes, when they were last
    /// written, in seconds since 1970, and where they are), of a game, by
    /// its files (its ROM, its disks, or the X16's card it's played from),
    /// or, with none, all of them: for the app's library
    Saves {
        /// Those of these kinds deleted, and said: save, state, resume,
        /// card or disk, a comma between
        #[arg(long, value_name = "KINDS")]
        clear: Option<String>,
        /// The game's system, as its saves' folders are named: nes,
        /// atari2600, atari7800, apple2, x16
        #[arg(long)]
        system: Option<String>,
        files: Vec<PathBuf>,
    },
}

/// An Apple's disks, cards, CPU and monitor.
#[derive(Debug, Args)]
pub(crate) struct Apple {
    /// .dsk, .do, .po, .nib or .woz images, or folders of them (in their
    /// names' order) or .m3u playlists: the first in drive 1, F9 and F10
    /// changing drive 1's and drive 2's. Hard disk volumes among them
    /// (.hdv, .2mg, and a .po bigger than a floppy), up to two, the hard
    /// disk card's, in slot 7, booted first.
    pub(crate) disks: Vec<String>,
    /// Mockingboards: one, in slot 4; none; or two, in 4 and 5.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(0..=2))]
    pub(crate) mockingboards: u8,
    /// Its CPU, not its own (a II+'s 6502, an enhanced IIe's 65C02): a
    /// 6502; a 65C02, the base one, as the enhanced IIe's; Rockwell's; or
    /// WDC's.
    #[arg(long, value_parser = choice(["6502", "65c02", "r65c02", "w65c02"], cpu_named))]
    pub(crate) cpu: Option<cpu::Variant>,
    /// Its picture in colour, or a monochrome monitor's: F8 switches.
    #[arg(long, default_value = "colour", value_parser = choice(Monitor::ALL.map(Monitor::name), monitor_named))]
    pub(crate) monitor: Monitor,
    /// Its text screen in the terminal, not a window.
    #[arg(long)]
    pub(crate) text: bool,
}

/// A console's TV standard, chosen.
#[derive(Debug, Args)]
pub(crate) struct TvRegion {
    /// Its TV standard, not as the game's known to be.
    #[arg(long, value_parser = choice(["ntsc", "pal"], Region::parse))]
    pub(crate) region: Option<Region>,
}

/// A choice of `names`, each what `parse` makes of it: they're what's
/// shown, and taken.
fn choice<const N: usize, T: Clone + Send + Sync + 'static>(
    names: [&'static str; N],
    parse: fn(&str) -> Option<T>,
) -> impl TypedValueParser<Value = T> {
    PossibleValuesParser::new(names).map(move |name| parse(&name).expect("one of the names"))
}

fn cpu_named(name: &str) -> Option<cpu::Variant> {
    match name {
        "6502" => Some(cpu::Variant::Nmos6502),
        "65c02" => Some(cpu::Variant::Synertek65C02),
        "r65c02" => Some(cpu::Variant::Rockwell65C02),
        "w65c02" => Some(cpu::Variant::Wdc65C02),
        _ => None,
    }
}

fn monitor_named(name: &str) -> Option<Monitor> {
    Monitor::ALL
        .into_iter()
        .find(|monitor| monitor.name() == name)
}

/// `--window=`'s X,Y,WIDTH,HEIGHT.
fn place(said: &str) -> Result<[i32; 4], String> {
    let numbers: Result<Vec<i32>, _> = said.split(',').map(str::parse).collect();
    numbers
        .ok()
        .and_then(|numbers| numbers.try_into().ok())
        .ok_or_else(|| "four numbers, X,Y,WIDTH,HEIGHT".to_string())
}

/// The program run with `args`, its name first, as its command line has them.
/// One that's wrong is told, and the program stopped; a panic, on Android,
/// for its library to tell.
pub fn run(args: Vec<String>) {
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) if cfg!(target_os = "android") && error.use_stderr() => {
            panic!("{}", error.render())
        }
        Err(error) => error.exit(),
    };
    let host = cli.host;
    // Saves never beside the games, whose folders are left as they are.
    if let Some(root) = host.saves.clone().or_else(host::saves::default_root) {
        host::saves::keep_in(&root);
    }
    if let Some(system) = cli.machine.as_ref().and_then(Machine::system) {
        host::saves::system(system);
    }
    if let Some(place) = host.window {
        host::window::PLACE.set(place).expect("one --window");
    }
    host::run::RESUME.store(host.resume, Ordering::Relaxed);
    host::run::REWIND.store(!host.no_rewind, Ordering::Relaxed);
    host::run::DRIVE_LIGHT.store(!host.no_drive_light, Ordering::Relaxed);
    host::run::VSYNC.store(!host.no_vsync, Ordering::Relaxed);
    host::window::LAUNCHED.store(host.launched, Ordering::Relaxed);
    let fullscreen = host.fullscreen;
    match cli.machine {
        None => machines::basic::run(None, true),
        Some(Machine::Basic { rom, no_terminal }) => {
            machines::basic::run(rom.as_deref(), !no_terminal)
        }
        Some(Machine::Apple2 { rom, apple }) => {
            apple.run(machines::apple2::Model::IIPlus, &rom, fullscreen)
        }
        Some(Machine::Apple2e {
            roms,
            unenhanced,
            apple,
        }) => {
            let model = if unenhanced {
                machines::apple2::Model::IIe
            } else {
                machines::apple2::Model::IIeEnhanced
            };
            apple.run(model, &roms, fullscreen)
        }
        Some(Machine::Nes {
            rom,
            overscan,
            fast,
            ram,
            region,
            console,
            port2,
        }) => {
            let chosen = machines::nes::Chosen {
                region: region.region,
                famicom: console,
                zapper: port2,
            };
            let ram = ram.unwrap_or_default();
            machines::nes::run(&rom, fullscreen, overscan as usize, chosen, fast, ram);
        }
        Some(Machine::Atari2600 { rom, region }) => {
            machines::atari2600::run(&rom, fullscreen, region.region)
        }
        Some(Machine::Atari7800 { rom, region }) => {
            // A 2600's cartridge played as the 7800 plays one, as a 2600.
            let path = Path::new(&rom);
            let bytes = host::files::read(path).unwrap_or_default();
            match machines::facts::system_of(path, &bytes) {
                machines::facts::System::Atari2600 => {
                    machines::atari2600::run(&rom, fullscreen, region.region)
                }
                _ => machines::atari7800::run(&rom, fullscreen, region.region),
            }
        }
        Some(Machine::Skink { image, disk }) => {
            // Its boot image and disk as they're given, else as GeckOS's
            // "make install" puts them, in roms/skink/.
            let image = image.as_deref().unwrap_or(machines::skink::IMAGE_FILE);
            let disk = disk.as_deref().or_else(|| {
                let disk = machines::skink::DISK_FILE;
                Path::new(disk).exists().then_some(disk)
            });
            machines::skink::run(image, disk, fullscreen);
        }
        Some(Machine::X16 {
            rom,
            game,
            sd,
            files,
        }) => x16(rom, game, sd, files, fullscreen),
        Some(Machine::Controls) => machines::nes::controls(),
        Some(Machine::Pads) => host::pads::report(),
        Some(Machine::Bound) => print!("{}", host::controls::Controls::load().text()),
        Some(Machine::Facts) => machines::facts::report(),
        Some(Machine::Saves { clear, files, .. }) => saves_report(clear.as_deref(), &files),
        Some(Machine::Roms { folder }) => {
            machines::apple2::rom_set::sort(&folder).expect("couldn't sort the ROMs")
        }
    }
}

impl Apple {
    /// The Apple `model`, its ROM, or its ROMs' folder, `rom`.
    fn run(self, model: machines::apple2::Model, rom: &str, fullscreen: bool) {
        let disks: Vec<&str> = self.disks.iter().map(String::as_str).collect();
        let built = machines::apple2::build(
            model,
            rom,
            &disks,
            self.mockingboards as usize,
            self.cpu,
            self.monitor,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        machines::apple2::run(model, built, self.text, fullscreen);
    }
}

impl Machine {
    /// The folder its saves are kept in, under saves/ and states/.
    fn system(&self) -> Option<&str> {
        Some(match self {
            Machine::Basic { .. } => "basic",
            Machine::Apple2 { .. } | Machine::Apple2e { .. } => "apple2",
            Machine::Nes { .. } => "nes",
            Machine::Atari2600 { .. } => "atari2600",
            Machine::Atari7800 { .. } => "atari7800",
            Machine::Skink { .. } => "skink",
            Machine::X16 { .. } => "x16",
            Machine::Saves { system, .. } => return system.as_deref(),
            _ => return None,
        })
    }
}

/// `cpu_emu saves`: each save of `files`' game, or all, a line each; those
/// of the kinds `clear` names deleted first, and said.
fn saves_report(clear: Option<&str>, files: &[PathBuf]) {
    use std::io::Write;
    let kinds: Option<Vec<host::saves::Kind>> = clear.map(|kinds| {
        kinds
            .split(',')
            .map(|name| {
                host::saves::Kind::named(name.trim())
                    .unwrap_or_else(|| panic!("{name}: not a kind of save"))
            })
            .collect()
    });
    let found = if files.is_empty() {
        host::saves::all()
    } else {
        host::saves::of_game(files)
    };
    let mut out = std::io::stdout().lock();
    for (kind, path) in found {
        let Ok(about) = std::fs::metadata(&path) else {
            continue;
        };
        if let Some(kinds) = &kinds {
            if !kinds.contains(&kind) {
                continue;
            }
            if let Err(e) = std::fs::remove_file(&path) {
                eprintln!("{}: not deleted: {e}", path.display());
                continue;
            }
            host::saves::tidy(&path);
        }
        let written = about
            .modified()
            .ok()
            .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |since| since.as_secs());
        let _ = writeln!(
            out,
            "{}\t{}\t{written}\t{}",
            kind.name(),
            about.len(),
            path.display()
        );
    }
}

/// The X16: its ROM as it's given, else as it's found in roms/x16/; its SD
/// card a game's own, made from it, or as it's given, or roms/x16/'s; the
/// X16's files, a folder of the host's, on it as FILES (`shared_folder`).
fn x16(
    rom: Option<String>,
    game: Option<String>,
    sd: Option<String>,
    files: Option<String>,
    fullscreen: bool,
) {
    let rom = rom
        .or_else(|| machines::x16::rom_file().map(|rom| rom.to_string_lossy().into_owned()))
        .expect("cpu_emu x16 ROM: no rom.bin in roms/x16/");
    let sd_card = match &game {
        Some(game) => {
            let card = sd.unwrap_or_else(|| {
                let beside = PathBuf::from(format!("{}.img", game.trim_end_matches(['/', '\\'])));
                let card = if beside.exists() {
                    beside
                } else {
                    host::saves::place(&beside)
                };
                card.to_string_lossy().into_owned()
            });
            machines::x16::make_card(Path::new(game), Path::new(&card))
                .unwrap_or_else(|e| panic!("{e}"));
            Some(card)
        }
        None => match sd {
            Some(card) => {
                machines::x16::blank_card(Path::new(&card)).unwrap_or_else(|e| panic!("{e}"));
                Some(card)
            }
            None => machines::x16::sd_card_file().map(|image| image.to_string_lossy().into_owned()),
        },
    };
    // The X16's files, a folder of the host's: on its card as it starts,
    // and what it saved there brought back as it stops.
    let shared = sd_card
        .as_deref()
        .zip(files.as_deref())
        .and_then(|(card, files)| {
            machines::x16::shared_folder::share_in(Path::new(card), Path::new(files))
        });
    machines::x16::run(&rom, sd_card.as_deref(), game.as_deref(), fullscreen);
    if let Some(shared) = &shared {
        machines::x16::shared_folder::share_out(shared);
    }
}

/// Android's way in: SDL's activity calls this, with the arguments it's
/// given, as a program's are given them, the first its name.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn SDL_main(
    argc: std::ffi::c_int,
    argv: *const *const std::ffi::c_char,
) -> std::ffi::c_int {
    to_log();
    // Where the library has it run, as a desktop's does: in its folder;
    // and a panic told it in the file it says, as it has no output of the
    // game's to read.
    if let Ok(folder) = std::env::var("OPENBUS_FOLDER") {
        let _ = std::env::set_current_dir(folder);
    }
    if let Ok(log) = std::env::var("OPENBUS_LOG") {
        let told = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = std::fs::write(&log, format!("thread 'main' {info}\n"));
            told(info);
        }));
    }
    let args = (0..argc.max(0) as usize)
        .map(|n| {
            // SAFETY: SDL hands over argc arguments, each a C string.
            let arg = unsafe { std::ffi::CStr::from_ptr(*argv.add(n)) };
            arg.to_string_lossy().into_owned()
        })
        .collect();
    run(args);
    0
}

/// What the emulator writes, its standard output and error, panics too, to
/// Android's log, a line at a time, tagged "OpenBus": Android has no
/// terminal for them.
#[cfg(target_os = "android")]
fn to_log() {
    use std::ffi::{CString, c_char, c_int};
    use std::io::{BufRead, BufReader};
    use std::os::fd::FromRawFd;
    unsafe extern "C" {
        fn pipe(fds: *mut c_int) -> c_int;
        fn dup2(from: c_int, to: c_int) -> c_int;
        fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
    }
    const INFO: c_int = 4;
    let mut fds = [0 as c_int; 2];
    // SAFETY: a pipe made, its writing end standing for standard output and
    // error from here on; its reading end the thread's alone.
    let reading = unsafe {
        if pipe(fds.as_mut_ptr()) != 0 {
            return;
        }
        dup2(fds[1], 1);
        dup2(fds[1], 2);
        std::fs::File::from_raw_fd(fds[0])
    };
    std::thread::spawn(move || {
        let tag = CString::new("OpenBus").unwrap();
        for line in BufReader::new(reading).lines().map_while(Result::ok) {
            if let Ok(text) = CString::new(line) {
                // SAFETY: both strings end in their NULs.
                unsafe { __android_log_write(INFO, tag.as_ptr(), text.as_ptr()) };
            }
        }
    });
}
