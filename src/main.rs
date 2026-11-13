/// The machine to run: `cpu_emu` for this one, with BASIC; `cpu_emu apple2
/// ROM [DISK...]` for an Apple II+, ROM a dump of its ROM (its last 12K,
/// $D000 up), the Disk II's ROMs (disk2-p5-341-0027.bin and
/// disk2-p6-341-0028.bin) beside it; `cpu_emu apple2e ROMS [DISK...]` for
/// an enhanced Apple IIe, or with `--unenhanced` the first, ROMS the folder
/// its ROMs are in, by their names, and the Disk II's. The DISKs are .dsk,
/// .do, .po, .nib or .woz images, or folders of them (in their names'
/// order) or .m3u playlists, the first in drive 1, F9 and F10
/// changing drive 1's and drive 2's; hard disk volumes among them (.hdv,
/// .2mg, and .po bigger than a floppy), up to two, the hard disk card's,
/// in slot 7, booted first; a Mockingboard in slot 4, or with
/// `--mockingboards=N` none, or two (4 and 5); its CPU its own (the 6502,
/// the enhanced IIe's 65C02), or with `--cpu=` a 6502 or a 65C02
/// (`65c02`, the base one, as the enhanced IIe's; `r65c02`, Rockwell's;
/// `w65c02`, WDC's); in a window, with `--fullscreen` fullscreen (F11 or a
/// double click switching), its picture in colour or with
/// `--monitor=white`, `green` or `amber` monochrome (F8 switching); or
/// with `--text` its text screen in the terminal. F2 saves its state, F4
/// loads it back: beside the first DISK, or the ROM, its name and .state.
/// `cpu_emu nes ROM [--fullscreen] [--overscan=N]` for the NES, ROM an
/// iNES image or a zip of one, its picture's edges hidden as a TV hides
/// them, N dots or lines each (8, or 0 to see it all); or a Famicom Disk
/// System's .fds image, or a zip of one, disksys.rom, its BIOS, beside it
/// or in roms/nes/, and F9 changing the disk. `--fast` for performance mode,
/// for what's too slow otherwise: a line at a time, and less exact for it.
/// With any, `--window=X,Y,WIDTH,HEIGHT` opens its window there, that size,
/// to be resized: as the desktop app has it, in place of its library.
/// `--ram=zeros` or `--ram=random` for its RAM at power-on all 0s, or as
/// random as a real console's, not all 1s ($FF). `--region=ntsc` or `pal`,
/// `--console=nes` or `famicom`, and `--port2=controller` or `zapper` for
/// those as chosen, not as the game's known to be.
/// `--no-rewind` for no rewinding: a console's last 30 seconds aren't kept
/// to go back to, held, as they are otherwise. `--no-drive-light` for no
/// light at the corner of the picture while a disk drive's busy.
/// `--launched` says that a launcher started it, as EmulationStation might,
/// for Select and Start held together to go back to. `--resume` keeps its
/// state as it's put away, beside it, and goes on from that next time.
/// `cpu_emu controls` for only the screen that changes the controls, as F6
/// has it in a game; they're for all of these.
/// `cpu_emu pads` for no window, and what the gamepads hold told on standard
/// output until standard input closes: the desktop app's library's.
/// `cpu_emu roms FOLDER` for the Apple's ROMs in the files named on standard
/// input written into FOLDER, under the names they're looked for by.
/// `cpu_emu facts` for what's known of each NES game named on standard
/// input, a line each: the desktop app's library's.
use cpu_emu::{cpu, machines};
use machines::apple2::video::Monitor;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(place) = args.iter().find_map(|arg| arg.strip_prefix("--window=")) {
        let numbers: Vec<i32> = place
            .split(',')
            .map_while(|number| number.parse().ok())
            .collect();
        let place: [i32; 4] = numbers.try_into().expect("--window=X,Y,WIDTH,HEIGHT");
        cpu_emu::host::window::PLACE
            .set(place)
            .expect("one --window");
    }
    if args.iter().any(|arg| arg == "--resume") {
        cpu_emu::host::run::RESUME.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    if args.iter().any(|arg| arg == "--no-rewind") {
        cpu_emu::host::run::REWIND.store(false, std::sync::atomic::Ordering::Relaxed);
    }
    if args.iter().any(|arg| arg == "--no-drive-light") {
        cpu_emu::host::run::DRIVE_LIGHT.store(false, std::sync::atomic::Ordering::Relaxed);
    }
    if args.iter().any(|arg| arg == "--launched") {
        cpu_emu::host::window::LAUNCHED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let usage = "cpu_emu apple2 ROM | apple2e ROMS [--unenhanced] [DISK...] [--mockingboards=0|1|2] [--cpu=6502|65c02|r65c02|w65c02] [--monitor=colour|white|green|amber] [--fullscreen | --text] | nes ROM [--fullscreen] [--overscan=0..32] [--region=ntsc|pal] [--console=nes|famicom] [--port2=controller|zapper] | atari2600 ROM [--fullscreen] [--region=ntsc|pal]";
    match args.get(1).map(String::as_str) {
        Some("controls") => machines::nes::controls(),
        Some("pads") => cpu_emu::host::pads::report(),
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
                    cpu_emu::host::machine::PowerOnRam::parse(name).expect(usage)
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
                .map(|name| cpu_emu::machines::region::Region::parse(name).expect(usage));
            machines::atari2600::run(rom, args.iter().any(|arg| arg == "--fullscreen"), region);
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
        _ => machines::basic::run(),
    }
}
