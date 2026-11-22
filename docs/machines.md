# The machines

What each machine needs, how to play it, and what's missing. The
[README](../README.md) lists the keys every machine shares.

In the app, everything here is also in the game's menu: press F1, or tap
Select and Start together on a gamepad.

## Files you add

Open Bus comes with no games, BIOS files or system ROMs. In the app, add
system files under Settings → System. It recognises them by their contents,
not their names, so MAME's zips and differently named dumps work. From the
command line, put them where each section below says.

| For                    | File                                                              |
| ---------------------- | ----------------------------------------------------------------- |
| Apple II+              | The II+ ROM, whole or as its six 2K chips                         |
| Apple IIe              | The IIe's ROMs, original or enhanced                              |
| Apple disks            | The Disk II card's two ROMs (341-0027 and 341-0028)               |
| Famicom Disk System    | `disksys.rom`                                                     |
| Atari 7800 high scores | `highscre.bin` (optional)                                         |
| Commander X16          | `rom.bin` from an official release. The desktop app can fetch it. |

## Where saves go

Saves are kept in the app's own folder, never in your game folders, laid out
by system, as a MiSTer keeps them:

```
saves/nes/Super Mario Bros. 3 (USA).srm       what the game saves
states/nes/Super Mario Bros. 3 (USA).state    a save state (F2)
states/nes/Super Mario Bros. 3 (USA).resume   where it was left off
saves/x16/Wumpus.img                          an X16 game's card
```

By default the `saves` and `states` folders are in:

| System  | Folder                                              |
| ------- | --------------------------------------------------- |
| Linux   | `~/.local/share/app.openbus.desktop`                |
| macOS   | `~/Library/Application Support/app.openbus.desktop` |
| Windows | `%APPDATA%\app.openbus.desktop`                     |
| Android | the app's own storage                               |

**Settings → Saves** moves them somewhere else, shows how many of each kind
there are, and clears a kind. A game's details list its own saves, each to be
deleted. From the command line, `--saves FOLDER` keeps them under another
folder, and `cpu_emu saves` lists or clears them.

Saves are named by the game's file, without its extension. Two games with the
same name in different folders share their saves. Saves left beside games by
older versions are still loaded.

Disks are different: an Apple II disk or hard disk, or an SD card you give the
X16, is written back into its own image, as the real hardware writes to its
disk.

## NES and Famicom

Cartridges are `.nes` files (iNES or NES 2.0), or zips of one. Disk System
games are `.fds`.

- **NES or Famicom.** The game's name, or the database, decides. The two
  differ in small ways that a few games notice: how the PPU starts up and
  resets, and the Famicom's second controller staying connected next to a
  Zapper. Override with Console in the game's settings, or `--console`.
- **NTSC or PAL.** Taken from the name, such as "(Europe)" or "(E)". Override
  with `--region`.
- **The Zapper.** Plugged in for games known to use it. Aim with the mouse
  and click to fire. Override with Port 2 in the game's settings, or
  `--port2`.
- **The Disk System.** It needs `disksys.rom`. F9 (or Change Disk Side in the
  menu) flips the disk. What a game writes to its disk is saved like battery
  RAM.
- **Bad headers.** Mesen's game database corrects them, so most mislabelled
  ROMs just work.
- **Performance mode** (Settings → NES, or `--fast`) draws a line at a time,
  for slow computers. A few raster effects won't be exact.
- **RAM at power-on** (Settings → NES, or `--ram`) defaults to all $FF. A few
  games behave differently with other values, as they do on real consoles.

**Not yet supported:**

- Konami's VRC7 (mapper 85): Lagrange Point, and Tiny Toon Adventures 2 in
  Japan. It needs an FM synth.
- Sachen's 8259C board (mapper 139).
- Peripheral-only games: karaoke (mapper 188) and Oeka Kids (mapper 96).
- The speech chip on some Jaleco boards. Those games play, but without their
  voice clips.

**Not a bug:** Kyuukyoku Harikiri Koushien shows a "tournament called off"
message the first time it runs, with its save RAM blank. Press Reset once
and it starts normally, as on a real cartridge.

## Apple II+ and IIe

The machine is chosen from the library's Apple shelf, or with `apple2`
(II+) or `apple2e` (enhanced IIe; `--unenhanced` for the original).

- **Disks.** `.dsk`, `.do`, `.po`, `.nib` and `.woz`, or a folder or `.m3u`
  playlist of them. The first goes in drive 1. F9 and F10 move drives 1 and 2
  to their next disk. Copy-protected disks need `.woz` (or `.nib`) images.
- **Hard disks.** `.hdv`, `.2mg`, or a `.po` too big for a floppy, up to two
  32MB volumes. The card is in slot 7 and boots first.
- **Cards.**
  - Slot 1: a clock card, so ProDOS knows the date.
  - Slot 2: a Super Serial Card with a modem (below).
  - Slots 4 and 5: Mockingboard sound cards, one by default.
  - Slot 6: the Disk II.
- **Joystick.** A gamepad's stick or D-pad, with A and B as its buttons. On a
  keyboard, use the numeric keypad: 8, 2, 4 and 6 to move, 0 and . for the
  buttons.
- **Monitor.** F8 cycles colour, white, green and amber.
- **Reset.** F12 is the Reset key. Ctrl+Shift+V pastes text in as typing, and
  Ctrl+Shift+C copies the text screen.
- **CPU.** `--cpu` swaps in a different 6502 family chip, for software that
  wants one.

### The modem

The serial card's modem dials internet addresses instead of phone numbers, as
a WiFi modem does, so terminal programs can reach the BBSes still running
today. In a terminal program set to 9600 baud, 8N1:

```
ATDT bbs.example.org:23
```

Port 23 (telnet) is assumed if none is given. `+++` returns to command mode
and `ATH` hangs up. The web version can't open network connections, so there
the modem never connects.

## Atari 2600

`.a26` or `.bin` files, or zips of one. The bank-switching scheme, NTSC or
PAL, and the controllers are worked out from the game, using Stella's game
properties.

- **Controllers.** Joystick, paddles (the mouse), driving controller or
  keypad, chosen per game. One keypad is the keyboard's 1 to 9, \*, 0 and #.
  With two, the left is 123, QWE, ASD and ZXC, and the right is 890, IOP, KL;
  and ,./.
- **Console switches.** Start is Reset and Select is Select. F7 and F8 flip
  the left and right difficulty switches, and F9 the colour/black-and-white
  switch.
- **Supercharger.** Starpath tape games load without the Supercharger's BIOS,
  using Stella's stand-in.

## Atari 7800

`.a78` files, or zips of one. 2600 games run too, as the 7800 plays them.

- **POKEY sound** is on for cartridges that carry one.
- **Light gun games** aim with the mouse.
- **Console switches.** F7 and F8 are the difficulty switches, and F9 is
  Pause.
- **High Score Cartridge.** With `highscre.bin` added, games that support it
  keep their high score tables. The scores are shared by all games, as on the real
  cartridge, and loading a save state doesn't change them. It's left out for
  Xenophobe and Scrapyard Dog, which would overwrite it.

## Commander X16

It needs the X16's `rom.bin` from an
[official release](https://github.com/X16Community/x16-rom/releases). The
desktop app can download it. On the command line it's read from `roms/x16/`.

- **Games.** A `.zip`, a folder, or a `.PRG`. Each game gets an SD card image
  of its own, made the first time it runs, which boots straight into it. What
  the game saves goes on its card. When the game's files change, say for a new
  version, the card is made again: the game's new files, plus everything else
  that was on it. Deleting the card in the game's saves starts it afresh.
- **SD card.** `--sd IMAGE` uses a card image of your own, written back as the
  machine writes to it. `tools/make-x16-sd` makes one or adds files to it.
- **Keyboard.** Keys go to the X16 by position, so its own layout applies.
  Pasted text is typed in.
- **Mouse.** Click the picture to capture the mouse, and press Ctrl+M to let it
  go.
- **Controllers.** Gamepads are its SNES controllers 1 and 2.
- **Clock.** It starts from the host's time. Its NVRAM is kept with your
  saves.
- **Network card.** TexElec's Serial & ESP32 Network Card is in its expansion
  slot, at `$9FE0` (below).

**Not yet supported:** NTSC and 240p video output. VGA output works.

### The network card

The card has two 16550 UARTs. The one at `$9FE0` goes to its WiFi modem, and
the one at `$9FE8` is its RS-232 port, with nothing plugged in. The modem runs
Zimodem's commands, as the real card's ESP32 does, and its WiFi is your
computer's network, so there's nothing to set up.

[ROMTERM](https://texelec.com/product/commander-x16-serial-network-card/),
David Murray's terminal for the card, finds it by itself: press Return at its
setup screen to keep 115,200 baud and port `$9FE0`, then dial:

```
atdt"bbs.example.org:23"
```

The address goes in quotes. `atdt` speaks telnet, which most BBSes want, and
`atd` opens a plain TCP connection. `+++` hangs up, as it does on Zimodem.
`ati` says which modem it is. Programs that use the card directly, such as
[x16net](https://github.com/retrodores/x16net), work too.

Zimodem's own extras aren't there: its phone book, listening for calls, its
packet commands, `AT&G` downloads, SSH and PETSCII translation. The web
version can't open network connections, so there the modem never connects.

## Skink and BASIC

Skink is a small 6502 computer designed for this project, built to run the
GeckOS operating system: 80×25 text, a 6522 VIA, a 6551 serial chip, and a
floppy image. It boots `roms/skink/rom.bin` and uses `roms/skink/geckos.img`
when it's there.

BASIC is a bare 6502 at 1 MHz running Microsoft BASIC in your terminal. Build
its ROM with `tools/fetch-msbasic`. Letters you type come out in upper case,
as on the machines BASIC first ran on. Ctrl-C stops a program. `QUIT` (or
`Q`) at BASIC's prompt, or Ctrl-D, turns the machine off.

Both are mostly for development, and aren't in the library.
