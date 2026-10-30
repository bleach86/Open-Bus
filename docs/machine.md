# The 6502 machine

A cartridge console in the style of the late 1970s and 80s. Everything here
has a period precedent; nothing is invented for a particular game.

**Rule: Rust emulates chips, and only chips.** The CPU, the bus and each
device below. Everything a machine's software does on real hardware —
finding carts, loading them, saving, menus, text output, mapping buttons —
is 6502 code: the BIOS, the system software or the cart.

## CPU

6502 (NMOS, documented opcodes) at 8 MHz. Video runs at 60 Hz: a frame is
133,333 cycles, 262 lines of 509.

Interrupts: the CPU samples IRQ between instructions and, with the I flag
clear, pushes the return address and the status and goes through `$FFFE`
(NMI through `$FFFA`; nothing raises it yet). The BIOS points both vectors
at RAM vectors (`NMI_VECTOR`, `IRQ_VECTOR` at `$0200`), which do nothing until
a program sets them; cc65's `set_irq` (with the SDK) handles IRQs in C. The
video chip is the only source of interrupts, and they are off at reset.

## Memory map

| Range         | Contents                                                      |
|---------------|---------------------------------------------------------------|
| `$0000-$00FF` | zero page; `$00-$0F` belong to the BIOS                        |
| `$0100-$01FF` | stack                                                         |
| `$0200-$02FF` | BIOS: interrupt vectors (`$0200` NMI, `$0202` IRQ), variables |
| `$0300-$7FFF` | RAM for the running software, cart or system: all of it       |
| `$8000-$9FFF` | cart window, slot 0: the bank `BANK0` selects                 |
| `$A000-$BFFF` | cart window, slot 1: the bank `BANK1` selects                 |
| `$C000-$FEFF` | system ROM: BIOS jump table at `$C000`, BIOS, system software |
| `$FF00-$FFF9` | I/O registers                                                 |
| `$FFFA-$FFFF` | NMI, RESET and IRQ vectors (system ROM)                       |

Power-on clears RAM. The reset button (Select+Start, or F12) does not: it
resets the CPU and the devices, as a reset line does.

## Boot

Reset enters the BIOS. It sets up the stack, the RAM interrupt vectors and
the video chip (palettes, font, clear screen). If the cart is enabled
(`CART_CTL`) and bank 0 starts with the program header (`"PRG"`, version,
entry address at `$8004`), it jumps to the entry: from then on the cart has
the machine. Otherwise it starts the system software: the menu.

## I/O registers

Each device has a 16-byte block.

### System, `$FF00`

| Reg     | Addr    | |
|---------|---------|-|
| `FRAME_WAIT`  | `$FF00` | write: halt the CPU until the next vblank or interrupt, whichever comes first |
| `FRAME_COUNT` | `$FF01` | read: frames since power-on, wrapping |
| `POWER`       | `$FF03` | write: power off; the value is the emulator's exit status |
| `HOST`        | `$FF04` | read: bit 0 set when a launcher acts on the `POWER` statuses below |
| `DEBUG_OUT`   | `$FF05` | write: a byte to the host's stdout |
| `RTC_LATCH`   | `$FF08` | write: latch the real-time clock into `RTC` |
| `RTC`         | `$FF08-$FF0F` | read: the latched time, ms since the Unix epoch, 64-bit little-endian |

`POWER` statuses a launcher (the RG351MP's) acts on: 10 games (ES-DE),
20 reboot, 21 power off. 0 is a plain power-off.

### Video, `$FF10`

320×240, 60 Hz. 64K of VRAM, reached through an address and a data port,
as on the TMS9918 and the NES PPU: set `VADDR`, then each access to `VDATA`
reads or writes VRAM at `VADDR` and adds one to it.

| Reg     | Addr    | |
|---------|---------|-|
| `VADDR`   | `$FF10-$FF11` | VRAM address, little-endian |
| `VDATA`   | `$FF12` | VRAM data at `VADDR`; `VADDR` then increments |
| `VCTRL`   | `$FF13` | bit 0 display on, bit 1 sprites on, bit 2 16×16 sprites, bit 3 plane B on |
| `SCROLLX` | `$FF14-$FF15` | background scroll, 0-511 |
| `SCROLLY` | `$FF16-$FF17` | background scroll, 0-255 |
| `BACKDROP`| `$FF18` | the colour (index into palette 0) behind everything |
| `VIRQ`    | `$FF19` | interrupts on: bit 0 vblank, bit 1 raster |
| `VSTATUS` | `$FF1A` | read: the interrupts that happened (bits as `VIRQ`); write: acknowledge those bits |
| `RASTER`  | `$FF1B` | the line (0-239) whose start raises the raster interrupt |
| `VLINE`   | `$FF1C` | read: the line being drawn, 0-239, or 240-261 in vblank |
| `SCROLLX2`| `$FF1D-$FF1E` | plane B's scroll, 0-511 |
| `SCROLLY2`| `$FF1F` | plane B's scroll, 0-255 |

The beam: each frame starts with vblank (lines 240-261, about 11,000
cycles), then draws lines 0-239. The chip keeps each line's scrolls as the
beam leaves it, so a scroll changed partway down the frame shows from that
line on: a raster interrupt at line N can scroll lines N and below
differently from those above (a status bar over a scrolling level, say).
Everything else shows as it stands at the end of the frame. An interrupt
stays raised until acknowledged in `VSTATUS`. `FRAME_WAIT` wakes for the
raster interrupt too; `BIOS_WAITFRAME` waits on for vblank.

VRAM layout:

| Range         | |
|---------------|-|
| `$0000-$7FFF` | 1024 tiles: 8×8 pixels, 4 bits each, two pixels a byte (high nibble left), 32 bytes a tile |
| `$8000-$8FFF` | background map: 64×32 entries of 2 bytes: tile (bits 0-9), flip X (10), flip Y (11), palette (12-15) |
| `$9000-$90FF` | 64 sprites of 4 bytes: Y, X (low 8 bits), tile (low 8 bits), attributes: X bit 8 (0), tile bit 8 (1), flip X (2), flip Y (3), palette (4-7) |
| `$9100-$92FF` | 16 palettes of 16 colours, 2 bytes each: `$0RGB`, 4 bits a channel, little-endian |
| `$9300-$933F` | each sprite's priority: 0 in front of both planes, 1 behind plane B, 2 behind both |
| `$A000-$AFFF` | plane B's map, as plane A's |

Two background planes: A, its map at `$8000`, and B, its map at `$A000`,
in front of A when `VCTRL` bit 3 is on; each scrolls on its own. Colour 0 of
every palette is see-through: through plane B to plane A, through plane A to
the backdrop. A 16×16 sprite is four tiles, `t`, `t+1` above `t+2`, `t+3`; a
sprite at Y 240 or more is hidden. Back to front: the backdrop, sprites of
priority 2, plane A, sprites of priority 1, plane B, sprites of priority 0.
Where sprites overlap, the frontmost one with a pixel there (sprite 0 on
top) decides, its priority with it, as on the NES.

### Input, `$FF20`

| Reg     | Addr    | |
|---------|---------|-|
| `PAD`        | `$FF20-$FF21` | read: buttons held, 16 bits (below) |
| `KEY_CODE`   | `$FF22` | read: the waiting key (ASCII; `$80-$83` up, down, left, right) |
| `KEY_STATUS` | `$FF23` | read: bit 0 a key is waiting; write: take it |

`PAD` bits: A 0, B 1, X 2, Y 3, L1 4, R1 5, L2 6, R2 7, Select 8, Start 9,
L3 10, R3 11, up 12, down 13, left 14, right 15. Buttons are only bits;
what they mean is up to the software.

### Cart, `$FF30`

The slot holds a flash cart, as an EverDrive does: 2M of RAM in 8K banks,
which the system software loads a cart file into, and 32K of non-volatile
save memory.

| Reg     | Addr    | |
|---------|---------|-|
| `BANK0`    | `$FF30` | the bank in slot 0, `$8000` (0 at reset) |
| `BANK1`    | `$FF31` | the bank in slot 1, `$A000` (1 at reset) |
| `CART_CTL` | `$FF32` | bit 0 enabled: the BIOS boots the cart; bit 1 writable: writes to the window go to the banks. Reset clears it. |
| `SAVE_ADDR`| `$FF34-$FF35` | save memory address, 0-32767 |
| `SAVE_DATA`| `$FF36` | save memory at `SAVE_ADDR`; `SAVE_ADDR` then increments |

### Drive, `$FF40`

A disk drive with its own DOS, like a 1541: the machine sends it commands
and reads or writes bytes. On the host it is a directory.

| Reg     | Addr    | |
|---------|---------|-|
| `DRIVE_NAME`   | `$FF40` | write: append a character to the file name |
| `DRIVE_CMD`    | `$FF41` | write: a command on that name, which is then cleared; read: status |
| `DRIVE_DATA`   | `$FF42` | the next byte of the open file |

Commands: 1 open to read, 2 open to write (replacing), 3 open the
directory (its names, each ending in a NUL), 4 close, 5 delete. Status:
bit 0 a byte can be read, bit 7 the last command failed.

### Status, `$FF50`

Power and system readings, as a handheld's fuel gauge gives them; they come
from the host and change once a second.

| Reg     | Addr    | |
|---------|---------|-|
| `BATTERY`      | `$FF50` | charge in percent, `$FF` with no battery |
| `POWER_STATUS` | `$FF51` | bit 0 on mains, bit 1 charging |
| `TEMPERATURE`  | `$FF52` | CPU temperature, °C |
| `IP`           | `$FF54-$FF57` | IPv4 address, most significant byte first; 0 offline |

### Sound, `$FF80`

Four voices after the C64's SID. Each is an oscillator shaped by an ADSR
envelope; any of them can go through a resonant filter; then the master
volume. At reset every register is 0 (silence, volume 0); the BIOS sets
the master volume to 15.

Voice `n` at `$FF80 + 8n`:

| offset | register |
|---|---|
| 0-1 | frequency, little-endian: Hz = value × 1 MHz / 2²⁴ (0.06 Hz steps, up to 3.9 kHz) |
| 2-3 | pulse width, 0-4095 of the cycle (2048: square) |
| 4 | control: bit 0 gate, 1 sync, 2 ring, 3 test, 4 triangle, 5 saw, 6 pulse, 7 noise |
| 5 | attack (bits 4-7), decay (0-3) |
| 6 | sustain level (bits 4-7), release (0-3) |
| 7 | volume, 0-15 |

- The gate going on starts the attack: a rise to full, then a decay to the
  sustain level, held while the gate stays on. The gate going off starts the
  release, to silence. Attack rates 0-15 take 2, 8, 16, 24, 38, 56, 68, 80,
  100, 250, 500, 800 ms, 1, 3, 5, 8 s; decay and release take three times as
  long.
- More than one wave bit ANDs the waves. Noise is a 23-bit shift register
  stepped 16 times a cycle.
- Sync restarts the oscillator each time the voice before's (voice 3 for
  voice 0) cycle does; ring inverts the triangle by that voice's top half.
- Test holds the oscillator at the start of its cycle, and silent.

Then:

| address | register |
|---|---|
| `$FFA0-$FFA1` | filter cutoff, 0-2047: 30 Hz to 12 kHz |
| `$FFA2` | resonance (bits 4-7); voices through the filter (bit n: voice n) |
| `$FFA3` | filter: bit 4 low-pass, 5 band-pass, 6 high-pass (summed); bit 7 voice 3 off; master volume (bits 0-3) |
| `$FFA4` | read: voice 3's oscillator, 0-255 |
| `$FFA5` | read: voice 3's envelope, 0-255 |

Voice 3 off takes voice 3 out of the output unless it goes through the
filter; it still runs, and reads back. On noise, its oscillator reads as
random bytes, as on the SID: a source of random numbers, set up once (a high
frequency, noise, voice 3 off) and read as often as wanted, each read
stepping the noise on. The oscillators and envelopes run whether anything is
heard or not.

## Cart files

`NAME.crt`: a 32-byte header, the icon, then the banks.

| Offset | |
|--------|-|
| 0  | `"6CRT"` |
| 4  | format version, 6 |
| 5  | number of 8K banks, 1-255 |
| 6  | 0 |
| 7  | save memory used, in K (0-32) |
| 8-31 | 0 |

At offset 32, the icon the menu shows (592 bytes): a 32×32 picture as 16
tiles (512 bytes, left to right, then down), two palettes of 16 colours (64
bytes), and the palette each tile takes, 0 or 1 (16 bytes). So an icon has
up to 30 colours, 15 to a tile. The banks follow at offset 624.

Bank 0 starts with the program header: `"PRG"`, version 1, entry address.
The name shown in the menu is the file name.

## System software

The menu (in the system ROM) lists the drive's `*.crt` files, each with its
icon. Choosing one loads its banks into the flash cart and `NAME.sav`, if
any, into save memory, enables the cart and resets into it. When the menu
starts and a cart with save memory was running, it writes save memory back
to that cart's `NAME.sav` (it notes the cart in `.last` on the drive). A
second page has the shell and the launcher's entries.

## Text

The BIOS's terminal (`BIOS_CHROUT`, `BIOS_PRINT`, ...) draws 40×30
characters on the background map: a character's code is its tile (the BIOS
loads the font into tiles `$20-$FF` at reset, pixels colour 1, background
colour 2) and the text colour is its palette. The default palettes put
colour n on black. It scrolls with `SCROLLY`, and the cursor is sprite 63.
Software that wants the screen for itself just takes it.

`BIOS_FONT` (`$C021`) draws the font's glyphs as tiles in any two colours:
with `VADDR` at the first tile, A the colours (the letters in bits 0-3, the
rest in bits 4-7, 0 for see-through), X the first character and Y how many.
One palette can then serve text in all its colours, with a copy of the font
for each.

## Making a cart

The SDK is `sdk/`: headers (`include/`), the C runtime (`lib/`), linker
configs (`cfg/`) and `cart.mk`. A cart is a directory anywhere with its
sources and a Makefile:

```make
NAME = pet
BANKS = 2              # 8K each: 2 (16K, never switched), 4, 8 or 16 (128K)
SAVE = 1               # K of save memory
RAM = yes              # run the program from RAM (4, 8 or 16 banks)
include /path/to/cpu_emu/sdk/cart.mk
```

`make` builds `pet.crt` with cc65; the linker writes the file header too.

With `RAM = yes` the program runs from RAM, as a program loaded from disk
would: banks 1-3 hold it (`CODE`, `RODATA` and `DATA`, up to 24K), and the
startup code in bank 0 copies it into RAM before `main`. RAM then holds the
program, its variables, the heap and the stack; banks 4 and up are free
(segments `BANK4` on), and with nothing of the program in the cart window,
both slots can switch. Without it, bank 0 stays at `$8000` and holds the startup code, the C runtime and the
`CODE`, `RODATA` and `DATA` segments; banks 1 and up take turns at `$A000`.
Code for bank N goes in segment `BANKN` (`#pragma code-name ("BANK3")`), and
calls into it go through the SDK's trampoline, declared with
`#pragma wrapped-call (push, bank_call, bank)`: cc65 takes the bank from
the linker config.

Pictures go in the cart's `art/` as PNGs, at most 16 colours each (colour 0,
or transparent pixels, is the see-through colour). `cart.mk` turns each
`art/NAME.png` into `art_NAME`, its 8×8 tiles (left to right, then down, 32
bytes each), and `art_NAME_palette`, 32 bytes, in segment `ART_SEGMENT`
(`RODATA` unless the Makefile says, say, `BANK1`, to keep bank 0 for code;
`ART_SEGMENT_NAME` puts one picture elsewhere, for art bigger than a bank).
PNGs in `art/16/` come out in 16×16 blocks, each one a 16×16 sprite's four
tiles in order, so sprite sheets are drawn as they look.
`vram_write` and `vram_fill` (`machine.h`) copy them to VRAM.

Read a multi-byte value from a port a byte at a time into variables of
their own (`low = SAVE_DATA; high = SAVE_DATA;`). cc65's optimizer can read
the port again where the code reads a variable just stored from it, so
`n = SAVE_DATA; n |= SAVE_DATA << 8;` comes out wrong.

A cart's icon is its `icon.png`, 32×32 in up to 30 colours, any PNG; the
SDK's converter shares its tiles between the two palettes. Without one, the
cart gets the SDK's `default-icon.png`.

## Debugging

`cpu_emu --monitor PORT` keeps a machine-language monitor on 127.0.0.1:PORT,
after VICE's. Attach with any line client (`nc localhost 6502`): the machine
stops, the screen keeps its last frame, and the monitor takes commands
(`h` lists them):

| | |
|---|---|
| `r` | the registers and flags, the beam's line, the cart's banks; `r a 12` sets one |
| `d [addr] [n]` | disassemble, from PC unless given |
| `m addr [to]`, `> addr bytes` | memory, and writing it |
| `v addr [n]` | VRAM |
| `io` | the chips' registers |
| `b [addr]`, `w addr`, `bc` | a breakpoint (again: gone; alone: the list), stop when addr is written, none |
| `s [n]`, `n`, `f` | step, step over a `jsr`, run to the next frame |
| `g [addr]` | go on |
| `l name` | a cart's labels: `name.lbl` on the drive |
| `q` | go on, without the monitor's breakpoints and watches |

Addresses are hex or labels. The system ROM's labels are built in; `cart.mk`
writes `NAME.lbl` beside `NAME.crt`, which `l NAME` loads, so a cart's C
functions are there by name (`b _main`, or `b main`). Looking changes
nothing: the monitor reads ports as they stand, without stepping `VDATA`,
the noise or the drive (a port it can't read so shows `--`). Typing while
the machine runs stops it; the client going away lets it go on.

## Music

Songs are text, one file each in a cart's `music/`; `cart.mk` turns
`music/NAME.song` into `song_NAME` (in `MUSIC_SEGMENT`, `RODATA` unless the
Makefile says) for the SDK's music driver (`music.h`):

```
# The room's tune.
tempo 132                                    # quarter notes a minute
filter 700 resonance 9 lowpass               # the sound chip's filter, if used
instrument lead pulse 1024 attack 1 decay 6 sustain 9 release 5 volume 10
instrument bass saw decay 5 sustain 7 release 3 volume 13 filter
voice 0 lead: o5 l8 L e g a g e d c d | e4 g4 a4. r8
voice 2 bass: o2 l8 L [c c > c < c]2 | [f f > f < f] [g g > g < g]
```

An instrument is waves (`triangle`, `saw`, `pulse` and a width, `noise`;
`sync`, `ring`), an envelope (`attack`, `decay`, `sustain`, `release`, 0-15
as the chip takes them), a `volume` and whether it goes through the
`filter`. A voice line (0-3) names its instrument and goes on in MML:

| | |
|---|---|
| `c` ... `b` | a note; `#` or `+` sharp, `-` flat; then a length, if not the default |
| `1` `2` `4` `8` `16` `32` `64`, `3` `6` `12` ... | whole, half, quarter ... notes, and triplets; `.` adds half |
| `r` | a rest, with a length too |
| `^` | the note before held a length longer: `c4^8` |
| `o4` `<` `>` | the octave (4 has middle C), one down, one up |
| `l8` | the length of notes written without one |
| `@name` | another instrument from here |
| `[` ... `]3` | that part three times |
| `L` | where the song goes back to at its end; without one it stops |
| `\|` | nothing: bar lines, for reading |

A song plays on the voices it has lines for and leaves the others alone.
In C:

```c
#include "music.h"
extern const Song song_room, song_coin;

music_play(&song_room);       /* from its start */
sfx_play(1, &song_coin);      /* song_coin's voice 0, on voice 1, over the music */
for (;;) {
    bios_waitframe();
    music_tick();             /* once a frame */
}
```

A sound effect plays on the voice it is given in place of the music's part
there, which goes on silently and is heard again when the effect ends.
`music_stop` stops the song; `music_note` says what a voice is sounding.

`examples/test` is the machine test: graphics from its PNGs, sprites,
scrolling under a split screen (a raster interrupt) and palette cycling;
every button, a voice on each of A, B, X and Y, and a song; save memory,
calls across banks, the clock and the status chip. `examples/sprout` is a
pet that lives on the real-time clock.
`examples/banks` switches through sixteen banks.
