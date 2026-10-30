/*
 * machine.h: the machine, as seen from C. machine.inc has the same
 * definitions for assembly, and the memory map; docs/machine.md describes
 * it all.
 */
#ifndef MACHINE_H
#define MACHINE_H

#define IO(address) (*(volatile unsigned char *)(address))
#define IO16(address) (*(volatile unsigned int *)(address))

/* --- system, $FF00 -------------------------------------------------------- */

/* Write: halt until the next vblank or interrupt, whichever comes first
 * (bios_waitframe waits for vblank itself). */
#define FRAME_WAIT IO(0xFF00)
/* Read: frames since power-on, wrapping. */
#define FRAME_COUNT IO(0xFF01)
/* Write: power off; the value is the emulator's exit status. */
#define POWER IO(0xFF03)
/* Read: HOST_ bits. */
#define HOST IO(0xFF04)
#define HOST_LAUNCHER 0x01 /* a launcher acts on the POWER_ statuses */
/* Write: a byte to the host's stdout. */
#define DEBUG_OUT IO(0xFF05)
/* Write: latch the real-time clock; read RTC after. */
#define RTC_LATCH IO(0xFF08)
/* Read: the latched time, ms since the Unix epoch, 64-bit little-endian. */
#define RTC ((volatile const unsigned char *)0xFF08)
/* The clock in whole seconds since the Unix epoch (sdk/lib/rtc.c). */
unsigned long rtc_seconds(void);

#define POWER_OFF 0
#define POWER_GAMES 10 /* with a launcher: run ES-DE */
#define POWER_REBOOT 20
#define POWER_SHUTDOWN 21

/* --- video, $FF10 --------------------------------------------------------- */

/* VRAM address; each VDATA access reads or writes there, then adds one. */
#define VADDR IO16(0xFF10)
#define VDATA IO(0xFF12)
#define VCTRL IO(0xFF13)
#define VCTRL_DISPLAY 0x01
#define VCTRL_SPRITES 0x02
#define VCTRL_BIG 0x04 /* 16x16 sprites */
#define VCTRL_PLANE_B 0x08 /* plane B, in front of plane A */
#define SCROLLX IO16(0xFF14)
#define SCROLLY IO16(0xFF16)
/* The colour behind everything: an index into palette 0. */
#define BACKDROP IO(0xFF18)
/* Interrupts (IRQ), as VIRQ_ bits: on in VIRQ; what happened in VSTATUS,
 * which a write acknowledges (write the bits handled). The raster interrupt
 * comes as the beam starts line RASTER (0-239); setting RASTER from the
 * handler, to a line further down, interrupts again this frame. A frame is
 * 262 lines: vblank (VLINE 240-261) first, then lines 0-239. Handlers in C:
 * cc65's set_irq (6502.h). */
#define VIRQ IO(0xFF19)
#define VSTATUS IO(0xFF1A)
#define RASTER IO(0xFF1B)
#define VLINE IO(0xFF1C) /* read: the line being drawn */
/* Plane B's scroll: X 0-511, Y 0-255. */
#define SCROLLX2 IO16(0xFF1D)
#define SCROLLY2 IO(0xFF1F)
#define VIRQ_VBLANK 0x01
#define VIRQ_RASTER 0x02

/* VRAM layout. */
#define VRAM_TILES 0x0000    /* 1024 tiles: 8x8, 4 bits a pixel, 32 bytes */
#define VRAM_MAP 0x8000      /* 64x32 entries: see MAP_ENTRY */
#define VRAM_SPRITES 0x9000  /* 64 sprites: Y, X, tile, attributes */
#define VRAM_PALETTES 0x9100 /* 16 palettes of 16 colours, $0RGB */
#define VRAM_PRIORITY 0x9300 /* a byte for each sprite: SPRITE_ priorities */
#define VRAM_MAP_B 0xA000    /* plane B's map, as plane A's */
#define MAP_COLUMNS 64
#define MAP_ROWS 32

/* A background map entry: tile 0-1023, palette 0-15. */
#define MAP_ENTRY(tile, palette) ((tile) | (unsigned)(palette) << 12)
#define MAP_FLIP_X 0x0400
#define MAP_FLIP_Y 0x0800
/* Sprite attributes. */
#define SPRITE_X_HIGH 0x01
#define SPRITE_TILE_HIGH 0x02
#define SPRITE_FLIP_X 0x04
#define SPRITE_FLIP_Y 0x08
#define SPRITE_PALETTE(palette) ((palette) << 4)
#define SPRITE_HIDDEN 240 /* a Y this low or lower hides the sprite */
/* Sprite priorities (VRAM_PRIORITY): in front of both planes, behind plane
 * B, behind both (seen where both are see-through). */
#define SPRITE_FRONT 0
#define SPRITE_BEHIND_B 1
#define SPRITE_BEHIND 2
/* A colour: 4 bits each of red, green and blue. */
#define RGB(r, g, b) ((unsigned)(r) << 8 | (g) << 4 | (b))

/* --- input, $FF20 --------------------------------------------------------- */

/* Buttons held, one PAD_ bit each. */
#define PAD IO16(0xFF20)
/* The waiting key, while KEY_STATUS has KEY_WAITING; write KEY_STATUS to
 * take it. */
#define KEY_CODE IO(0xFF22)
#define KEY_STATUS IO(0xFF23)
#define KEY_WAITING 0x01
/* Keys held, as the PAD bits they stand for, laid out as SNES emulators do:
 * arrows the D-pad, Z B, X A, A Y, S X, Q L1, W R1, Return Start, right
 * Shift Select. A game reads PAD | KEYPAD. */
#define KEYPAD IO16(0xFF24)

#define PAD_A 0x0001
#define PAD_B 0x0002
#define PAD_X 0x0004
#define PAD_Y 0x0008
#define PAD_L1 0x0010
#define PAD_R1 0x0020
#define PAD_L2 0x0040
#define PAD_R2 0x0080
#define PAD_SELECT 0x0100
#define PAD_START 0x0200
#define PAD_L3 0x0400
#define PAD_R3 0x0800
#define PAD_UP 0x1000
#define PAD_DOWN 0x2000
#define PAD_LEFT 0x4000
#define PAD_RIGHT 0x8000

#define KEY_BACKSPACE 0x08
#define KEY_TAB 0x09
#define KEY_RETURN 0x0D
#define KEY_ESCAPE 0x1B
#define KEY_UP 0x80
#define KEY_DOWN 0x81
#define KEY_LEFT 0x82
#define KEY_RIGHT 0x83

/* --- cart, $FF30 ---------------------------------------------------------- */

/* The banks in the cart window's two slots, $8000 and $A000. */
#define BANK0 IO(0xFF30)
#define BANK1 IO(0xFF31)
#define CART_CTL IO(0xFF32)
#define CART_ENABLED 0x01  /* the BIOS boots the cart */
#define CART_WRITABLE 0x02 /* writes to the window go to the banks */
/* Save memory: 32K that keeps its contents with the power off. Each
 * SAVE_DATA access reads or writes at SAVE_ADDR, then adds one. */
#define SAVE_ADDR IO16(0xFF34)
#define SAVE_DATA IO(0xFF36)
#define SAVE_SIZE 0x8000u
/* `length` bytes of save memory from `address` on, to or from `data`
 * (sdk/lib/save.s). Use these rather than SAVE_DATA in C: cc65's optimizer
 * may reuse a byte it read from the port instead of reading it again. */
void __fastcall__ save_read(unsigned address, void *data, unsigned length);
void __fastcall__ save_write(unsigned address, const void *data, unsigned length);

/* --- drive, $FF40 --------------------------------------------------------- */

/* Write the file name a character at a time, then a command. */
#define DRIVE_NAME IO(0xFF40)
#define DRIVE_CMD IO(0xFF41)
#define DRIVE_DATA IO(0xFF42)

#define DRIVE_OPEN_READ 1
#define DRIVE_OPEN_WRITE 2
#define DRIVE_OPEN_DIR 3 /* the names, each ending in a NUL */
#define DRIVE_CLOSE 4
#define DRIVE_DELETE 5
#define DRIVE_READABLE 0x01 /* status: a byte can be read */
#define DRIVE_FAILED 0x80   /* status: the last command failed */

/* --- status, $FF50 -------------------------------------------------------- */

/* Read: battery charge in percent, NO_BATTERY if there is none. */
#define BATTERY IO(0xFF50)
#define NO_BATTERY 0xFF
/* Read: POWER_ bits. */
#define POWER_STATUS IO(0xFF51)
#define POWER_MAINS 0x01
#define POWER_CHARGING 0x02
/* Read: CPU temperature in degrees C. */
#define TEMPERATURE IO(0xFF52)
/* Read: IPv4 address, most significant byte first; 0.0.0.0 offline. */
#define IP ((volatile const unsigned char *)0xFF54)

/* --- sound, $FF80 --------------------------------------------------------- */

/* Four voices after the C64's SID. Voice n: an oscillator at SOUND_FREQ(n)
 * (SOUND_HZ makes the value) with the waveforms SOUND_CTRL picks, shaped by
 * an ADSR envelope that SOUND_GATE starts (on) and releases (off); then the
 * filter, which the voices in SOUND_FILTER's low bits go through, and the
 * master volume. At reset every register is 0; the BIOS sets the master
 * volume to 15. */
#define SOUND_FREQ(n) IO16(0xFF80 + 8 * (n))  /* Hz = value x 1 MHz / 2^24 */
#define SOUND_PULSE(n) IO16(0xFF82 + 8 * (n)) /* pulse width, 0-4095: 2048 square */
#define SOUND_CTRL(n) IO(0xFF84 + 8 * (n))
#define SOUND_AD(n) IO(0xFF85 + 8 * (n))      /* attack << 4 | decay */
#define SOUND_SR(n) IO(0xFF86 + 8 * (n))      /* sustain level << 4 | release */
#define SOUND_VOLUME(n) IO(0xFF87 + 8 * (n))  /* 0-15 */
#define SOUND_CUTOFF IO16(0xFFA0)             /* 0-2047: 30 Hz-12 kHz */
#define SOUND_FILTER IO(0xFFA2)               /* resonance << 4 | voices filtered */
#define SOUND_MODE IO(0xFFA3)                 /* SOUND_ pass bits | master volume */
/* Read: voice 3's oscillator, 0-255. On noise (with SOUND_VOICE3_OFF in the
 * mode, unheard), a byte of random bits; each read steps the noise on. */
#define SOUND_OSC3 IO(0xFFA4)
#define SOUND_ENV3 IO(0xFFA5)                 /* read: voice 3's envelope, 0-255 */

#define SOUND_HZ(hz) ((unsigned)((hz) * 16777UL / 1000))

/* SOUND_CTRL bits. More than one wave ANDs them, as the SID does. */
#define SOUND_GATE 0x01
#define SOUND_SYNC 0x02 /* restart with the voice before's cycle */
#define SOUND_RING 0x04 /* triangle: ring-modulated by the voice before */
#define SOUND_TEST 0x08 /* hold the oscillator at the start */
#define SOUND_TRIANGLE 0x10
#define SOUND_SAW 0x20
#define SOUND_PULSE_WAVE 0x40
#define SOUND_NOISE 0x80

/* SOUND_MODE bits. */
#define SOUND_LOWPASS 0x10
#define SOUND_BANDPASS 0x20
#define SOUND_HIGHPASS 0x40
#define SOUND_VOICE3_OFF 0x80 /* voice 3 out of the output (unless filtered) */

/* Envelope rates, 0-15: attack 2, 8, 16, 24, 38, 56, 68, 80, 100, 250, 500,
 * 800 ms, 1, 3, 5, 8 s from silence to full; decay and release take three
 * times as long to fall away. */

/* --- text ----------------------------------------------------------------- */

/* The BIOS's terminal: 40x30 characters on the background map, one font
 * tile per character code ($20-$FF; pixels colour 1, background colour 2),
 * in the palette of the text colour. The default palettes put colour n on
 * black. */
#define SCREEN_COLUMNS 40
#define SCREEN_ROWS 30

#define COLOR_BLACK 0x0
#define COLOR_RED 0x1
#define COLOR_GREEN 0x2
#define COLOR_YELLOW 0x3
#define COLOR_BLUE 0x4
#define COLOR_MAGENTA 0x5
#define COLOR_CYAN 0x6
#define COLOR_GREY 0x7
#define COLOR_DARKGREY 0x8
#define COLOR_LIGHTRED 0x9
#define COLOR_LIGHTGREEN 0xA
#define COLOR_LIGHTYELLOW 0xB
#define COLOR_LIGHTBLUE 0xC
#define COLOR_LIGHTMAGENTA 0xD
#define COLOR_LIGHTCYAN 0xE
#define COLOR_WHITE 0xF
#define DEFAULT_COLOR COLOR_GREY

/* Font glyphs beyond ASCII: block elements at $80-$9F (Unicode U+2580 on)
 * and box drawing at $A0-$FF (U+2500 on). */
#define CH_UPPER_HALF 0x80
#define CH_LOWER_HALF 0x84
#define CH_BLOCK 0x88
#define CH_LEFT_HALF 0x8C
#define CH_RIGHT_HALF 0x90
#define CH_SHADE_LIGHT 0x91
#define CH_SHADE_MEDIUM 0x92
#define CH_SHADE_DARK 0x93
#define CH_HLINE 0xA0
#define CH_VLINE 0xA2
#define CH_TOP_LEFT 0xAC
#define CH_TOP_RIGHT 0xB0
#define CH_BOTTOM_LEFT 0xB4
#define CH_BOTTOM_RIGHT 0xB8
#define CH_TEE_RIGHT 0xBC /* ├ */
#define CH_TEE_LEFT 0xC4  /* ┤ */
#define CH_TEE_DOWN 0xCC  /* ┬ */
#define CH_TEE_UP 0xD4    /* ┴ */
#define CH_CROSS 0xDC
#define CH_DOUBLE_HLINE 0xF0
#define CH_DOUBLE_VLINE 0xF1
#define CH_DOUBLE_TOP_LEFT 0xF4
#define CH_DOUBLE_TOP_RIGHT 0xF7
#define CH_DOUBLE_BOTTOM_LEFT 0xFA
#define CH_DOUBLE_BOTTOM_RIGHT 0xFD

/* --- BIOS ----------------------------------------------------------------- */

unsigned char bios_version(void);
/* Terminal output: '\n' starts the next line, '\r' this one, '\b' moves
 * left, '\f' clears the screen. */
void __fastcall__ bios_chrout(char c);
void __fastcall__ bios_print(const char *text);
/* The waiting key, or 0 if none. */
unsigned char bios_getkey(void);
/* The next key; the cursor shows while waiting. */
unsigned char bios_waitkey(void);
void __fastcall__ bios_setcursor(unsigned char column, unsigned char row);
unsigned char bios_column(void);
unsigned char bios_row(void);
/* Set the colour for new text, 0-15; returns the previous one. */
unsigned char __fastcall__ bios_setcolor(unsigned char color);
void bios_clear(void);
void bios_waitframe(void);
/* The video chip back as the BIOS sets it up at reset: default palettes,
 * the font, sprites hidden, a clear screen. */
void bios_videoreset(void);
/* The font's glyphs `first` to `first + count - 1` ($20-$FF) as tiles from
 * VRAM `address` on: set pixels in colour number `colors & 15`, the rest in
 * `colors >> 4` (0 for see-through). Text in more colours than one palette
 * per colour: say, a copy of the font for each colour of one palette. */
void __fastcall__ bios_font(unsigned address, unsigned char first, unsigned char count, unsigned char colors);

/* --- VRAM (sdk/lib/vram.s) ------------------------------------------------- */

/* Copy `length` bytes to VRAM from `address` on: tiles, a palette, map
 * entries. */
void __fastcall__ vram_write(unsigned address, const void *data, unsigned length);
/* `length` bytes of `value` in VRAM from `address` on. */
void __fastcall__ vram_fill(unsigned address, unsigned char value, unsigned length);

/* A cart's art (its art/ PNGs, converted by cart.mk): each picture's tiles,
 * 32 bytes each, left to right, then down, and its palette, 32 bytes. Declare
 * a picture art/hero.png as
 *
 *     extern const unsigned char art_hero[], art_hero_palette[];
 *
 * A picture in art/map/ comes as a tile map: art_NAME holds its distinct
 * tiles (art_NAME_count of them), and art_NAME_map an entry for each 8x8
 * cell, left to right, then down: the tile, counting from art_NAME's first,
 * and MAP_FLIP_ bits. Add the tile the first one was loaded at, and a
 * palette, to put it on a plane:
 *
 *     extern const unsigned art_hills_count, art_hills_map[];
 *     entry = art_hills_map[i] + TILE_HILLS + MAP_ENTRY(0, PAL_HILLS);
 */

/* --- carts ---------------------------------------------------------------- */

/* A cart file's header (docs/machine.md). */
#define CART_VERSION 6
/* The icon after the header: a 32x32 picture as 16 tiles (512 bytes, left to
 * right, then down), two palettes (64), and each tile's palette, 0 or 1
 * (16). */
#define CART_ICON_SIZE 592
typedef struct {
    char magic[4]; /* "6CRT" */
    unsigned char version;
    unsigned char bank_count; /* 8K banks */
    unsigned char reserved;
    unsigned char save_k; /* K of save memory used, 0-32 */
    unsigned char unused[24];
} CartHeader;

/* Calls into another bank go through the SDK's trampoline:
 *
 *     #pragma wrapped-call (push, bank_call, 3)
 *     void level_three(void);
 *     #pragma wrapped-call (pop)
 *
 * maps bank 3 at $A000 for the call and puts the caller's bank back after.
 * The function itself is compiled into that bank's segment:
 *
 *     #pragma code-name (push, "BANK3")
 */
void bank_call(void);

#endif
