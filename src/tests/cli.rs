//! The command line: as the desktop app, Android's game screen and a
//! launcher give it, and what it won't take.

use crate::cli::{Cli, Machine};
use crate::cpu::Variant;
use crate::host::machine::PowerOnRam;
use crate::machines::apple2::video::Monitor;
use crate::machines::region::Region;
use clap::Parser;
use clap::error::ErrorKind;

fn parse(line: &[&str]) -> Cli {
    let args = std::iter::once("cpu_emu").chain(line.iter().copied());
    Cli::try_parse_from(args).unwrap_or_else(|e| panic!("{line:?}: {e}"))
}

fn refused(line: &[&str]) -> ErrorKind {
    let args = std::iter::once("cpu_emu").chain(line.iter().copied());
    Cli::try_parse_from(args).map(|_| ()).unwrap_err().kind()
}

#[test]
fn nothing_but_its_name_is_this_ones_basic() {
    assert!(parse(&[]).machine.is_none());
}

#[test]
fn a_nes_game_as_the_desktop_app_starts_it() {
    let cli = parse(&[
        "nes",
        "/games/Kirby's Adventure (USA).zip",
        "--fast",
        "--ram=random",
        "--region=pal",
        "--console=famicom",
        "--port2=zapper",
        "--window=10,20,640,480",
        "--resume",
        "--no-rewind",
        "--no-drive-light",
        "--no-vsync",
    ]);
    let Some(Machine::Nes {
        rom,
        overscan,
        fast,
        ram,
        region,
        console,
        port2,
    }) = cli.machine
    else {
        panic!("not the NES: {cli:?}");
    };
    assert_eq!(rom, "/games/Kirby's Adventure (USA).zip");
    assert_eq!((overscan, fast, ram), (8, true, Some(PowerOnRam::Random)));
    assert_eq!(
        (region.region, console, port2),
        (Some(Region::Pal), Some(true), Some(true))
    );
    let host = cli.host;
    assert_eq!(host.window, Some([10, 20, 640, 480]));
    assert!(host.resume && host.no_rewind && host.no_drive_light && host.no_vsync);
    assert!(!host.fullscreen && !host.launched);
}

#[test]
fn a_launchers_line_its_options_anywhere() {
    let cli = parse(&["nes", "--fullscreen", "Game.zip", "--launched", "--fast"]);
    assert!(
        matches!(cli.machine, Some(Machine::Nes { ref rom, fast: true, .. }) if rom == "Game.zip")
    );
    assert!(cli.host.fullscreen && cli.host.launched);
    // As a space, not an equals sign, too.
    let cli = parse(&[
        "atari7800",
        "Game.a78",
        "--region",
        "ntsc",
        "--saves",
        "/data/saves",
    ]);
    assert!(
        matches!(cli.machine, Some(Machine::Atari7800 { region, .. }) if region.region == Some(Region::Ntsc))
    );
    assert_eq!(
        cli.host.saves.as_deref(),
        Some(std::path::Path::new("/data/saves"))
    );
}

#[test]
fn an_apple_with_its_disks_and_cards() {
    let cli = parse(&[
        "apple2e",
        "/library",
        "--unenhanced",
        "/disks/one.dsk",
        "/disks/two.woz",
        "--mockingboards=2",
        "--cpu=w65c02",
        "--monitor=green",
    ]);
    let Some(Machine::Apple2e {
        roms,
        unenhanced,
        apple,
    }) = cli.machine
    else {
        panic!("not a IIe: {cli:?}");
    };
    assert_eq!((roms.as_str(), unenhanced), ("/library", true));
    assert_eq!(apple.disks, ["/disks/one.dsk", "/disks/two.woz"]);
    assert_eq!(
        (apple.mockingboards, apple.cpu),
        (2, Some(Variant::Wdc65C02))
    );
    assert_eq!((apple.monitor, apple.text), (Monitor::Green, false));
    let cli = parse(&["apple2", "plus.rom"]);
    let Some(Machine::Apple2 { apple, .. }) = cli.machine else {
        panic!("not a II+: {cli:?}");
    };
    assert_eq!(
        (apple.mockingboards, apple.cpu, apple.monitor),
        (1, None, Monitor::Colour)
    );
}

#[test]
fn an_x16_game_on_its_own_card() {
    let cli = parse(&[
        "x16",
        "/library/x16-rom.bin",
        "--game=/games/Contra.zip",
        "--sd=/cards/Contra.img",
    ]);
    let Some(Machine::X16 { rom, game, sd }) = cli.machine else {
        panic!("not the X16: {cli:?}");
    };
    assert_eq!(rom.as_deref(), Some("/library/x16-rom.bin"));
    assert_eq!(game.as_deref(), Some("/games/Contra.zip"));
    assert_eq!(sd.as_deref(), Some("/cards/Contra.img"));
}

#[test]
fn the_apps_own_errands() {
    assert!(matches!(parse(&["pads"]).machine, Some(Machine::Pads)));
    assert!(matches!(parse(&["bound"]).machine, Some(Machine::Bound)));
    assert!(matches!(parse(&["facts"]).machine, Some(Machine::Facts)));
    assert!(matches!(
        parse(&["controls"]).machine,
        Some(Machine::Controls)
    ));
    assert!(matches!(
        parse(&["roms", "/library"]).machine,
        Some(Machine::Roms { .. })
    ));
}

#[test]
fn what_it_wont_take() {
    assert_eq!(
        refused(&["nes", "Game.zip", "--region=secam"]),
        ErrorKind::InvalidValue
    );
    assert_eq!(
        refused(&["nes", "Game.zip", "--overscan=40"]),
        ErrorKind::ValueValidation
    );
    assert_eq!(refused(&["nes"]), ErrorKind::MissingRequiredArgument);
    assert_eq!(
        refused(&["apple2", "plus.rom", "--mockingboards=3"]),
        ErrorKind::ValueValidation
    );
    assert_eq!(
        refused(&["nes", "Game.zip", "--window=1,2,3"]),
        ErrorKind::ValueValidation
    );
    assert_eq!(
        refused(&["gameboy", "Tetris.gb"]),
        ErrorKind::InvalidSubcommand
    );
}
