//! The machine to run: `cpu_emu` for this one, with BASIC; `cpu_emu apple2
//! ROM [DISK...]` for an Apple II+, ROM a dump of its ROM (its last 12K,
//! $D000 up), the Disk II's ROMs (disk2-p5-341-0027.bin and
//! disk2-p6-341-0028.bin) beside it; `cpu_emu apple2e ROMS [DISK...]` for
//! an enhanced Apple IIe, or with `--unenhanced` the first, ROMS the folder
//! its ROMs are in, by their names, and the Disk II's. The DISKs are .dsk,
//! .do, .po, .nib or .woz images, or folders of them (in their names'
//! order) or .m3u playlists, the first in drive 1, F9 and F10
//! changing drive 1's and drive 2's; hard disk volumes among them (.hdv,
//! .2mg, and .po bigger than a floppy), up to two, the hard disk card's,
//! in slot 7, booted first; a Mockingboard in slot 4, or with
//! `--mockingboards=N` none, or two (4 and 5); its CPU its own (the 6502,
//! the enhanced IIe's 65C02), or with `--cpu=` a 6502 or a 65C02
//! (`65c02`, the base one, as the enhanced IIe's; `r65c02`, Rockwell's;
//! `w65c02`, WDC's); in a window, with `--fullscreen` fullscreen (F11 or a
//! double click switching), its picture in colour or with
//! `--monitor=white`, `green` or `amber` monochrome (F8 switching); or
//! with `--text` its text screen in the terminal. F2 saves its state, F4
//! loads it back: beside the first DISK, or the ROM, its name and .state.
//! `cpu_emu nes ROM [--fullscreen] [--overscan=N]` for the NES, ROM an
//! iNES image or a zip of one, its picture's edges hidden as a TV hides
//! them, N dots or lines each (8, or 0 to see it all); or a Famicom Disk
//! System's .fds image, or a zip of one, disksys.rom, its BIOS, beside it
//! or in roms/nes/, and F9 changing the disk. `--fast` for performance mode,
//! for what's too slow otherwise: a line at a time, and less exact for it.
//! `cpu_emu atari2600 ROM [--fullscreen]` for the Atari 2600, ROM a .a26 or
//! a zip of one, F5 and F7 its difficulty switches and F8 its colour
//! switch; `cpu_emu atari7800 ROM [--fullscreen]` for the Atari 7800, ROM
//! an .a78, or a dump without its header, or a zip of one, F5 and F7 its
//! difficulty switches and F8 its Pause button, the High Score Cartridge
//! in it if highscre.bin's beside the game or in roms/atari7800/; or a
//! 2600's, played as a 2600, as the 7800 plays one. `cpu_emu skink [IMAGE]
//! [--disk=DISK] [--fullscreen]` for Skink, a 6502 computer of our own for
//! GeckOS, IMAGE its boot image, loaded to the top of its memory, and DISK
//! a disk's image, a FAT floppy's, say, written back as it's written:
//! roms/skink/rom.bin and roms/skink/geckos.img, if they're not given.
//! `cpu_emu x16 [ROM] [--game=GAME] [--sd=IMAGE] [--fullscreen]` for the
//! Commander X16, ROM its rom.bin, roms/x16/'s if it's not given (or its
//! release folder's there), and IMAGE its SD card's, written back as it's
//! written, made blank if it isn't there yet: roms/x16/sdcard.img, if it's
//! not given and it's there. GAME,
//! a game's zip, folder or program, is played from a card of its own,
//! IMAGE or beside it, as GAME.img: made, if it isn't there, to start it
//! as the X16's turned on, and kept. With any,
//! `--window=X,Y,WIDTH,HEIGHT` opens its window there, that size, to be
//! resized: as the desktop app has it, in place of its library.
//! `--ram=zeros` or `--ram=random` for its RAM at power-on all 0s, or as
//! random as a real console's, not all 1s ($FF). `--region=ntsc` or `pal`,
//! `--console=nes` or `famicom`, and `--port2=controller` or `zapper` for
//! those as chosen, not as the game's known to be.
//! `--no-rewind` for no rewinding: a console's last 30 seconds aren't kept
//! to go back to, held, as they are otherwise. `--no-drive-light` for no
//! light at the corner of the picture while a disk drive's busy.
//! `--launched` says that a launcher started it, as EmulationStation might,
//! for Select and Start held together to go back to. `--resume` keeps its
//! state as it's put away, beside it, and goes on from that next time.
//! `cpu_emu controls` for only the screen that changes the controls, as F6
//! has it in a game; they're for all of these.
//! `cpu_emu pads` for no window, and what the gamepads hold told on standard
//! output until standard input closes: the desktop app's library's.
//! `cpu_emu roms FOLDER` for the Apple's ROMs in the files named on standard
//! input written into FOLDER, under the names they're looked for by.
//! `cpu_emu facts` for what's known of each NES game named on standard
//! input, a line each: the desktop app's library's.

use crate::{cpu, machines};
use machines::apple2::video::Monitor;

/// The program run with `args`, its name first, as its command line has them.
pub fn run(args: Vec<String>) {
    if let Some(place) = args.iter().find_map(|arg| arg.strip_prefix("--window=")) {
        let numbers: Vec<i32> = place
            .split(',')
            .map_while(|number| number.parse().ok())
            .collect();
        let place: [i32; 4] = numbers.try_into().expect("--window=X,Y,WIDTH,HEIGHT");
        crate::host::window::PLACE.set(place).expect("one --window");
    }
    if args.iter().any(|arg| arg == "--resume") {
        crate::host::run::RESUME.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    if args.iter().any(|arg| arg == "--no-rewind") {
        crate::host::run::REWIND.store(false, std::sync::atomic::Ordering::Relaxed);
    }
    if args.iter().any(|arg| arg == "--no-drive-light") {
        crate::host::run::DRIVE_LIGHT.store(false, std::sync::atomic::Ordering::Relaxed);
    }
    if args.iter().any(|arg| arg == "--launched") {
        crate::host::window::LAUNCHED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let usage = "cpu_emu apple2 ROM | apple2e ROMS [--unenhanced] [DISK...] [--mockingboards=0|1|2] [--cpu=6502|65c02|r65c02|w65c02] [--monitor=colour|white|green|amber] [--fullscreen | --text] | nes ROM [--fullscreen] [--overscan=0..32] [--region=ntsc|pal] [--console=nes|famicom] [--port2=controller|zapper] | atari2600 ROM [--fullscreen] [--region=ntsc|pal] | atari7800 ROM [--fullscreen] [--region=ntsc|pal] | skink [IMAGE] [--disk=DISK] [--fullscreen]";
    match args.get(1).map(String::as_str) {
        Some("controls") => machines::nes::controls(),
        Some("pads") => crate::host::pads::report(),
        Some("facts") => machines::facts::report(),
        Some("roms") => {
            let out = args.get(2).expect("cpu_emu roms FOLDER");
            machines::apple2::rom_set::sort(std::path::Path::new(out))
                .expect("couldn't sort the ROMs");
        }
        Some("nes") => {
            let rom = args[2..]
                .iter()
                .find(|arg| !arg.starts_with("--"))
                .expect(usage);
            let overscan = match args.iter().find_map(|arg| arg.strip_prefix("--overscan=")) {
                Some(dots) => dots.parse().ok().filter(|&dots| dots <= 32).expect(usage),
                None => 8,
            };
            let region = args
                .iter()
                .find_map(|arg| arg.strip_prefix("--region="))
                .map(|name| machines::nes::region::Region::parse(name).expect(usage));
            let either = |name: &str, no: &str, yes: &str| {
                let prefix = format!("--{name}=");
                args.iter()
                    .find_map(|arg| arg.strip_prefix(&prefix))
                    .map(|said| match said {
                        _ if said == no => false,
                        _ if said == yes => true,
                        _ => panic!("{usage}"),
                    })
            };
            let chosen = machines::nes::Chosen {
                region,
                famicom: either("console", "nes", "famicom"),
                zapper: either("port2", "controller", "zapper"),
            };
            let fast = args.iter().any(|arg| arg == "--fast");
            let ram = args
                .iter()
                .find_map(|arg| arg.strip_prefix("--ram="))
                .map_or(Default::default(), |name| {
                    crate::host::machine::PowerOnRam::parse(name).expect(usage)
                });
            machines::nes::run(
                rom,
                args.iter().any(|arg| arg == "--fullscreen"),
                overscan,
                chosen,
                fast,
                ram,
            );
        }
        Some("atari2600") => {
            let rom = args[2..]
                .iter()
                .find(|arg| !arg.starts_with("--"))
                .expect(usage);
            let region = args
                .iter()
                .find_map(|arg| arg.strip_prefix("--region="))
                .map(|name| crate::machines::region::Region::parse(name).expect(usage));
            machines::atari2600::run(rom, args.iter().any(|arg| arg == "--fullscreen"), region);
        }
        Some("atari7800") => {
            let rom = args[2..]
                .iter()
                .find(|arg| !arg.starts_with("--"))
                .expect(usage);
            let region = args
                .iter()
                .find_map(|arg| arg.strip_prefix("--region="))
                .map(|name| crate::machines::region::Region::parse(name).expect(usage));
            let fullscreen = args.iter().any(|arg| arg == "--fullscreen");
            // A 2600's cartridge played as the 7800 plays one, as a 2600.
            let path = std::path::Path::new(rom);
            let bytes = crate::host::files::read(path).unwrap_or_default();
            match machines::facts::system_of(path, &bytes) {
                machines::facts::System::Atari2600 => {
                    machines::atari2600::run(rom, fullscreen, region)
                }
                _ => machines::atari7800::run(rom, fullscreen, region),
            }
        }
        Some("skink") => {
            // Its boot image and disk as they're given, else as GeckOS's
            // "make install" puts them, in roms/skink/.
            let image = args[2..]
                .iter()
                .find(|arg| !arg.starts_with("--"))
                .map_or(machines::skink::IMAGE_FILE, String::as_str);
            let disk = args
                .iter()
                .find_map(|arg| arg.strip_prefix("--disk="))
                .or_else(|| {
                    let disk = machines::skink::DISK_FILE;
                    std::path::Path::new(disk).exists().then_some(disk)
                });
            machines::skink::run(image, disk, args.iter().any(|arg| arg == "--fullscreen"));
        }
        Some("x16") => {
            // Its ROM as it's given, else as it's found in roms/x16/.
            let rom = args[2..]
                .iter()
                .find(|arg| !arg.starts_with("--"))
                .cloned()
                .or_else(|| machines::x16::rom_file().map(|rom| rom.to_string_lossy().into_owned()))
                .expect("cpu_emu x16 ROM: no rom.bin in roms/x16/");
            let said = |name: &str| {
                let prefix = format!("--{name}=");
                args[2..]
                    .iter()
                    .find_map(|arg| arg.strip_prefix(&prefix).map(str::to_string))
            };
            let game = said("game");
            let sd_card = match &game {
                Some(game) => {
                    let game = std::path::Path::new(&game);
                    let card = said("sd").unwrap_or_else(|| {
                        let plain = game.to_string_lossy();
                        format!("{}.img", plain.trim_end_matches(['/', '\\']))
                    });
                    machines::x16::make_card(game, std::path::Path::new(&card))
                        .unwrap_or_else(|e| panic!("{e}"));
                    Some(card)
                }
                None => match said("sd") {
                    Some(card) => {
                        machines::x16::blank_card(std::path::Path::new(&card))
                            .unwrap_or_else(|e| panic!("{e}"));
                        Some(card)
                    }
                    None => machines::x16::sd_card_file()
                        .map(|image| image.to_string_lossy().into_owned()),
                },
            };
            let fullscreen = args.iter().any(|arg| arg == "--fullscreen");
            machines::x16::run(&rom, sd_card.as_deref(), game.as_deref(), fullscreen);
        }
        Some(machine @ ("apple2" | "apple2e")) => {
            let files: Vec<&String> = args[2..]
                .iter()
                .filter(|arg| !arg.starts_with("--"))
                .collect();
            let rom = files.first().expect(usage);
            let text = args.iter().any(|arg| arg == "--text");
            let mockingboards = match args
                .iter()
                .find_map(|arg| arg.strip_prefix("--mockingboards="))
            {
                Some(count) => count.parse().ok().filter(|&count| count <= 2).expect(usage),
                None => 1,
            };
            let variant = match args.iter().find_map(|arg| arg.strip_prefix("--cpu=")) {
                None => None,
                Some("6502") => Some(cpu::Variant::Nmos6502),
                Some("65c02") => Some(cpu::Variant::Synertek65C02),
                Some("r65c02") => Some(cpu::Variant::Rockwell65C02),
                Some("w65c02") => Some(cpu::Variant::Wdc65C02),
                Some(_) => panic!("{usage}"),
            };
            let model = match (machine, args.iter().any(|arg| arg == "--unenhanced")) {
                ("apple2", _) => machines::apple2::Model::IIPlus,
                (_, true) => machines::apple2::Model::IIe,
                (_, false) => machines::apple2::Model::IIeEnhanced,
            };
            let disks: Vec<&str> = files[1..].iter().map(|disk| disk.as_str()).collect();
            let fullscreen = args.iter().any(|arg| arg == "--fullscreen");
            let monitor = match args.iter().find_map(|arg| arg.strip_prefix("--monitor=")) {
                Some(name) => *Monitor::ALL
                    .iter()
                    .find(|monitor| monitor.name() == name)
                    .expect(usage),
                None => Monitor::Colour,
            };
            machines::apple2::run(
                model,
                rom,
                &disks,
                mockingboards,
                variant,
                monitor,
                text,
                fullscreen,
            );
        }
        Some("basic") => {
            let files: Vec<&String> = args[2..]
                .iter()
                .filter(|arg| !arg.starts_with("--"))
                .collect();
            let rom = files.first().expect(usage);

            let with_terminal = !args.iter().any(|arg| arg == "--no-terminal");

            machines::basic::run(Some(rom), with_terminal);
        }
        _ => machines::basic::run(None, true),
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
