# How Open Bus fits together

This is the map between the files. Each module starts with a doc comment on
its own part, such as how a chip behaves or what a register does, and
`cargo doc --open` collects them. This page covers what no single module can:
how the parts connect, and the rules that keep them working.

## The layers

```
 web/ (the library page)          desktop/ (Tauri app: desktop and Android)
        │                                   │ starts a process per game
        ▼                                   ▼
 src/web.rs ── wasm ──┐          src/cli.rs ── the command line
                      ▼                     ▼
               src/host/  ── window (SDL), terminal, or page
                      │  drives, a frame at a time
                      ▼
               src/machines/<machine>  ── implements Machine + Bus
                      │  is the bus of
                      ▼
               src/cpu.rs  ── the 6502 family
               src/chips/  ── chips shared between machines
```

Everything below `host/` knows nothing about windows, files or browsers. A
machine only emulates hardware, and it gets keys, controls and the Reset
button handed to it.

## The CPU and the bus

`CPU<B: Bus>` is one 6502-family core, generic over what it's wired to. Its
`Variant` picks the NMOS 6502 (undocumented opcodes included), the Synertek,
Rockwell or WDC 65C02, or the NES's Ricoh 2A03.

The CPU does one bus access per cycle, as the real chip does. Each access is a
`Bus::read` or `Bus::write` followed by `Bus::tick`, and a machine runs its
other chips in `tick`. The interrupt lines (`irq`, `nmi`), `reset_held`,
`halt` (RDY, for DMA) and `stolen` (cycles taken by DMA) are methods on the
bus too. The CPU polls them as the real one samples its pins, so interrupt
timing follows from the bus and needs no special cases.

`CPU::run_frame` runs instructions until `Bus::hand_back` says a frame has
ended. Then control returns to the host.

The CPU is checked against Tom Harte's SingleStepTests (every opcode, every
cycle's bus access) and Klaus Dormann's functional tests. See the README for
fetching them. A change to `src/cpu/` should pass both.

## Machines

A machine is a struct that implements `Bus` (its memory map) and `Machine`
(`src/host/machine.rs`, which is what the host needs between frames). The
`Machine` methods cover:

- the picture, the sound, and the title
- the keys and the controls
- `function_key`, for F7 to F10
- `menu`, for its own entries in the in-game menu
- `keep_saved`, for battery RAM and disks
- `identity` and `keep`, for save states (below)

Most methods have defaults, so a simple machine implements only a few.

A machine with a part another machine also has, such as a 6522 VIA, an
AY-3-8910 or a YM2151, uses the shared chip in `src/chips/`. Code that two
machines need goes in shared code, never copied between them.

### Adding a machine

1. Make `src/machines/<name>.rs`, with a struct implementing `Bus` and
   `Machine`, and register it in `lib.rs`.
2. Add a subcommand to `src/cli.rs`, the `Machine` enum, and its arm in
   `run()`.
3. For the library, teach `src/machines/facts.rs` to recognise its files, then
   add it to the page (`web/app.js`) and to `src/web.rs` if it runs in a
   browser.
4. Add tests beside it in `src/machines/<name>/tests/`.

## The host

`src/host/run.rs` holds `Host`, which drives any machine through any display.
Each frame it:

1. runs the machine's cycles
2. shows the picture, with any notice or debug panel over it
3. plays the sound
4. gives the machine the keys, controls and Reset button
5. handles F2 and F4 (save and load state), rewind, the menu and F6 (the
   controls screen)

`Host::frame` does one frame. `Host::run` loops it and keeps the machine to its
own clock.

A display (`src/host/display.rs`) is one of three things:

- `window.rs`: an SDL2 window with sound, gamepads and the clipboard
- `terminal.rs`: text machines in the terminal they were started from
- `page.rs`: the web build. It holds the frame and sound for the page to
  collect and takes the page's input. The page calls in through `src/web.rs`.

### Timing and sound

Machines make sound at `SAMPLE_RATE`. The window queues it to SDL and keeps
the queue near `AUDIO_CUSHION`.

When a machine's frame rate is within `IN_STEP` (1.5 Hz) of the display's
refresh rate, the host turns on vsync and lets presenting the frame set the
pace. A `Stretch` resampler (`host/sound.rs`) then speeds the sound up or
slows it down by up to 2%, steered by how full the queue is, so the sound
never runs dry or piles up. Time spent waiting for vsync isn't counted as host
load.

Otherwise the host sleeps to the machine's own clock. `--no-vsync` forces that.

### Files and saves

All file access goes through `host/files.rs`. The web build swaps in
`files_web.rs`, which has the same functions over files held in memory, so a
machine never knows the difference.

`host/saves.rs` decides where battery RAM, save states, resume files, X16
game cards and the X16's clock RAM go. They never go beside the games, so the
user's game folders stay as they are. They go under a root folder, laid out
the way a MiSTer does it: `saves/<system>/<game>.srm` for what a game saves,
`states/<system>/<game>.state` and `.resume` for states. The root is whatever
`--saves FOLDER` names, which the desktop and Android apps always pass (the
app's data folder, or the folder chosen in Settings → Saves), or by default the
desktop app's data folder. The program sets the system as it starts, from its
subcommand.

A save is named by its game's file without that file's extension, so
`Zelda.nes` gives `Zelda.srm` and `Zelda.state`. Saves made beside games by
older versions, and the Android app's older layout under the whole of a game's
path, are still read until a new one is written.

Disk images are the exception: a disk is written back into its own image, as a
real disk is. It only goes under the root when the image can't be written, such
as on a read-only network share.

`cpu_emu saves` lists or clears saves by kind, for one game (`--system` and its
files) or all of them. The desktop app's Settings → Saves and each game's
details use it.

## Save states and rewind

A state (`host/save_state.rs`) is a header, then the CPU and the machine,
serialised with serde into postcard. The header carries a format `VERSION` and
the machine's `identity()`: its make and its ROMs. A state is refused by a
different machine.

What a state leaves out, mainly ROMs, comes back from the running machine
through `Machine::keep`. Fields that are only caches use `#[serde(skip)]`.

Rewind keeps a ring of these states as a console runs.

### The rule about enums

Postcard stores an enum variant as its index, not its name. Inserting a
variant in the middle of a saved enum renumbers everything after it and breaks
every existing save state. So:

- **Add new variants at the end.** `nes::cartridge::Mapper` is the one this
  matters for most: new boards go after the last variant, whatever their
  mapper number.
- Don't reorder or remove fields in a saved struct. If one has to change shape,
  keep its old layout. When the MMC3's IRQ counter moved into the shared
  `Scanlines` struct, it kept the same fields in the same order.
- If a layout really must change, bump `VERSION` so old states are refused
  cleanly instead of loading as garbage.

## The NES

The NES is the most involved machine, and the one most likely to get new work.

### The PPU and owed dots

The PPU runs three dots per CPU cycle. To make that affordable, it doesn't
always run them immediately. In stretches where nothing outside the PPU can
observe its work, the dots are owed (`Ppu::owe`) and run later in a batch
(`Ppu::catch_up`). Those stretches are:

- the middle of a visible line, dots 2 to 255, while rendering is steady
- idle lines in vertical blank

The result must be bit-identical to running every dot as it comes. That holds
only if the PPU catches up before anything that could see its state, and
`Bus::tick` and `cpu_read`/`write` in `nes.rs` do this before:

- any PPU register access, and `$4014` (OAM DMA)
- the Zapper's read at `$4017`, which looks at the picture
- any cartridge register read or write, and any PRG read that isn't plain ROM
- DMA writes

When adding anything that reads or changes PPU state from the CPU side, catch
the PPU up first.

A mapper that watches the PPU's fetches, such as the MMC5 counting scanlines,
can also stop dots being owed. `Cartridge::reads_quietly` returns false while
the MMC5 has a line start due.

`ppu/fast.rs` is the separate performance mode (`--fast`). It draws a line at
a time and is knowingly less exact.

### The APU

Channels run lazily. A channel only does work when a timer it can be heard
through expires, and silent channels advance in closed form. Levels are cached
between changes. `Apu::catch_up` brings everything current before any register
access and at frame end, so save states see an exact APU.

### Checking a change is exact

NES speed work must not change output. `tools/nesbench` checks that. It runs
games headless (`examples/nesbench.rs`) for a fixed number of frames with
scripted input, and takes a checksum of every frame's picture and sound. With
`--base REF`, it builds another commit the same way and runs both:

```sh
tools/nesbench --base HEAD "Super Mario Bros. (World).zip" "Castlevania III.zip"
```

Each game gets both speeds and either `same` or `DIFFERENT`. Same checksums
mean the change is bit-identical. Pick games that use the mappers and effects
the change touches.

Add `--android arm64-v8a` to build for Android and run on the device `adb`
reaches. Speed gains on x86 often don't carry over to ARM, so check there
too. When runs differ, run the example by hand with `--log FILE` on both
builds to find the first frame where they part.

The NES test ROMs (nestest, blargg's suites, AccuracyCoin) run from
`src/machines/nes/tests/test_roms.rs` when they're present in
`roms/nes/tests/`.

### Cartridges and the game database

`nes/cartridge.rs` reads the iNES or NES 2.0 header. It then looks the ROM up
by CRC in Mesen's database (`nes/database.rs`, `MesenNesDB.txt`), whose entry
wins over a wrong or old header. The mapper number picks a `Mapper` variant.
Small boards live in `cartridge.rs`, larger chips in their own files (`vrc.rs`,
`namco163.rs`, `taito.rs`, and others).

PRG and CHR reads have a fast path. `prg_pages` and `chr_pages` cache where
each 4K or 1K page sits, any cartridge write clears them, and the next read
rebuilds them. A mapper whose banks change without a CPU write (from PPU
fetches, say, like the MMC2's latches) must be excluded in `chr_pages_now`, or
clear the cache itself.

### Adding a mapper

[Mesen2](https://github.com/SourMesen/Mesen2)'s `Core/NES/Mappers` is the
reference. Open Bus is GPL-3 as Mesen is, so following its code closely is
fine.

1. Put the chip in its own file under `src/machines/nes/` if it has more than
   a register or two: its state struct (`Serialize`, `Deserialize`), `write`,
   bank lookups, and `cycle` and `irq` if it counts.
2. Add a variant **at the end** of `Mapper`.
3. Map the mapper number to it in `Cartridge::read`, along with any RAM size
   or initial mirroring it needs.
4. Add its arms to the functions that need them:
   - `prg_offset` and `chr_offset`
   - `prg_ram_offset`, if it has odd RAM
   - `cpu_write`
   - `nametable_page`, if it controls nametables itself
   - `cycle` and `irq`, for CPU-cycle IRQs
   - `a12` / `ppu_bus`, for scanline IRQs (reuse `Scanlines`)
   - `mapper_name`
5. Test it in `nes/tests/boards.rs` (banking, mirroring, IRQ timing), and
   update `nes/tests/cartridge.rs` if it used that number as an "unsupported"
   example.
6. Load real games and look at them. Tests catch register mistakes, but a
   picture shows the rest.

## The app

The library is one page, `web/`, used in three places.

**In a browser**, it loads the emulator as WebAssembly (`tools/build-web` →
`web/pkg/`) and runs games in a canvas through `src/web.rs`. Its files live in
the browser's storage (`web/kept.js`).

**On the desktop**, `desktop/` is a Tauri app showing the same page.
`web/desktop.js` replaces the browser's file storage with Tauri commands
(`desktop/src/lib.rs`), and the library lives in `library.sqlite` in the app's
data folder. A game runs as a separate `cpu_emu` process, started with
`--window X,Y,W,H` where the library's window was. The library hides until
the game ends. The page also asks the emulator about games with
`cpu_emu facts`, about gamepads with `cpu_emu pads` and `cpu_emu bound`, and
sorts Apple ROMs with `cpu_emu roms`.

**On Android**, it's the same Tauri app, built for Android
(`desktop/gen/android`, `desktop/src/android.rs`). The emulator is built as a
library, `libcpu_emu.so`. Each game opens `GameActivity` in its own `:game`
process, which runs `cli::run` with the same command line the desktop would
use. The emulator is also shipped as a program (`libcpu_emu_exe.so`) for the
windowless subcommands.

On a touch screen with no gamepad plugged in, `GameActivity` handles Back
itself, with Android's own pause dialog. For the NES it also lays `TouchPad`,
an Android view, over SDL's surface. Its presses go through the JNI call
`GameActivity.touched` into `host::touch`, a fixed slot that the first player's
gamepad adds onto in `Window::gamepad`. They're not a control, so rebinding
keys or a gamepad never affects them. A TV remote's OK is kept out of the
controls the same way (`controls::REMOTE_OK`): it always chooses in the menus.

`web/app.js` checks `native` to tell the browser from the app, and
`native.android` to tell Android from the desktop.

## Before sending a change

- `cargo test` (and `--release` for the slow suites)
- `cargo fmt`, in the root and in `desktop/`
- Prettier on `web/`
- `tools/build-web` after any Rust change, since the page loads `web/pkg/`,
  which goes stale silently

[`.github/workflows/check.yml`](../.github/workflows/check.yml) runs these on
every push and pull request: formatting, clippy (no warnings allowed), the tests (with Klaus Dormann's,
fetched), the WebAssembly build, and a check of the desktop app.

## Releases

1. Bump `version` in `Cargo.toml` and `desktop/Cargo.toml`. The app shows it
   in Settings → System, and Android's version code comes from it.
2. Commit, then tag the commit `v<version>` and push the tag.

[`release.yml`](../.github/workflows/release.yml) refuses a tag that doesn't
match `desktop/Cargo.toml`. It builds the desktop installers on Linux, Windows
and macOS and the Android APK, then puts them all on a GitHub release for the
tag.

### The Android release key

Android only installs an update signed with the same key as the app already
there, so every release's APK must be signed with the one release key. Losing
the key means no update can install over the app again, so keep a backup of
it somewhere safe.

Locally, `tools/build-android` signs with the key that
`desktop/gen/android/keystore.properties` names. That file is gitignored:

```properties
storeFile=/absolute/path/to/open-bus-release.jks
storePassword=...
keyAlias=open-bus
keyPassword=...
```

Without the file, release builds are signed with the debug key and won't
install over a released APK. The script warns when that happens.

In CI, `release.yml` writes that file from four repository secrets:

- `ANDROID_KEYSTORE`: the keystore, as `base64 -w0 open-bus-release.jks`
- `ANDROID_KEYSTORE_PASSWORD`
- `ANDROID_KEY_ALIAS`
- `ANDROID_KEY_PASSWORD`

A tag's release fails without them, rather than publish an APK signed with
some other key.

[`pages.yml`](../.github/workflows/pages.yml) publishes the web build to
GitHub Pages whenever `main` moves.
