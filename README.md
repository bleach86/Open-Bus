# 6502 Virtual Console

A fantasy console: a 6502 at 8 MHz with 32K of RAM, a tile-and-sprite video
chip, a SID-style sound chip, a flash cart for game carts and a disk drive.
Rust emulates the chips; everything you see runs on the 6502: the BIOS, the
menu and the shell (`rom/`), and the carts. `docs/machine.md` describes the
machine and how to make carts for it.

## Building and running

You need Rust and [cc65](https://cc65.github.io/) (a recent build: set
`CC65_HOME` if it isn't in `~/cc65` or installed system-wide).

```sh
cargo build --release          # the emulator, with the system ROM
make -C examples               # the example carts, onto drive/
cargo run --release -- --drive drive
```

The menu lists the carts on the drive. Without `--drive`, the emulator uses
a `drive` directory next to its executable.

A gamepad is the machine's pad. The keyboard is the machine's keyboard,
which the menu and Sprout read as well: arrows, Return to choose, Escape to
go back, Tab for the menu's next page. Games also read the keys held as
pad buttons (`KEYPAD`), laid out as in SNES emulators: the arrows are the
D-pad, Z is B, X is A, A is Y, S is X and Return is Start, so Lamplight,
the platformer, plays on either. F11 is full
screen, and F12 (or Select+Start on the pad) is the reset button. `cargo test` runs the machine
tests.

`--monitor 6502` opens a machine-language monitor on 127.0.0.1:6502 (attach
with `nc localhost 6502`): breakpoints, stepping, disassembly with labels,
memory, VRAM and the chips' registers. See "Debugging" in `docs/machine.md`.

## Making a cart

A cart is a directory with its C and assembly sources, its art as PNGs and
a short Makefile that includes `sdk/cart.mk`; see `examples/` and
`docs/machine.md`.
