; machine.inc: the machine, as seen from 6502 assembly. machine.h has the
; same definitions for C. docs/machine.md describes it all.
;
; Memory map
;
;   $0000-$000F   BIOS zero page
;   $0010-$00FF   zero page for the running software
;   $0100-$01FF   stack
;   $0200-$02FF   BIOS: RAM interrupt vectors, variables
;   $0300-$7FFF   RAM for the running software, cart or system
;   $8000-$9FFF   cart window, slot 0: the bank BANK0 selects
;   $A000-$BFFF   cart window, slot 1: the bank BANK1 selects
;   $C000-$FEFF   system ROM: BIOS jump table at $C000, BIOS, system software
;   $FF00-$FFF9   I/O registers
;   $FFFA-$FFFF   NMI, RESET and IRQ vectors

.ifndef MACHINE_INC
MACHINE_INC = 1

; --- RAM ----------------------------------------------------------------------

NMI_VECTOR      = $0200         ; RAM vectors the hardware vectors jump through
IRQ_VECTOR      = $0202

; --- system, $FF00 ------------------------------------------------------------

FRAME_WAIT      = $FF00         ; write: halt until the next vblank or interrupt
FRAME_COUNT     = $FF01         ; read: frames since power-on, wrapping
POWER           = $FF03         ; write: power off; the value is the exit status
HOST            = $FF04         ; read: bits below
HOST_LAUNCHER   = $01           ; a launcher acts on the POWER statuses below
DEBUG_OUT       = $FF05         ; write: a byte to the host's stdout
RTC_LATCH       = $FF08         ; write: latch the real-time clock into RTC
RTC             = $FF08         ; read: 8 bytes, ms since the Unix epoch, LE

POWER_OFF       = 0             ; POWER statuses
POWER_GAMES     = 10            ; with a launcher: run ES-DE
POWER_REBOOT    = 20
POWER_SHUTDOWN  = 21

; --- video, $FF10 -------------------------------------------------------------

VADDR           = $FF10         ; VRAM address, 2 bytes
VDATA           = $FF12         ; VRAM at VADDR; VADDR then increments
VCTRL           = $FF13         ; bits below
VCTRL_DISPLAY   = $01
VCTRL_SPRITES   = $02
VCTRL_BIG       = $04           ; 16x16 sprites
SCROLLX         = $FF14         ; 2 bytes, 0-511
SCROLLY         = $FF16         ; 2 bytes, 0-255
BACKDROP        = $FF18         ; colour behind everything: palette 0's index
VIRQ            = $FF19         ; interrupts on: VIRQ_ bits
VSTATUS         = $FF1A         ; read: VIRQ_ bits that happened; write: acknowledge them
RASTER          = $FF1B         ; the line (0-239) of the raster interrupt
VLINE           = $FF1C         ; read: the line being drawn (240-261: vblank)
SCROLLX2        = $FF1D         ; plane B's scroll: X (word, 0-511)
SCROLLY2        = $FF1F         ; and Y (0-255)
VCTRL_PLANE_B   = $08           ; plane B on, in front of plane A
VRAM_PRIORITY   = $9300         ; a byte for each sprite: 0 front, 1 behind B, 2 behind both
VRAM_MAP_B      = $A000         ; plane B's map
VIRQ_VBLANK     = $01
VIRQ_RASTER     = $02

VRAM_TILES      = $0000         ; 1024 tiles, 32 bytes each, 4 bits a pixel
VRAM_MAP        = $8000         ; 64x32 entries, 2 bytes: tile, flips, palette
VRAM_SPRITES    = $9000         ; 64 sprites: Y, X, tile, attributes
VRAM_PALETTES   = $9100         ; 16 palettes of 16 colours, $0RGB, LE

MAP_COLUMNS     = 64
MAP_ROWS        = 32

; --- input, $FF20 -------------------------------------------------------------

PAD             = $FF20         ; read: buttons held, 2 bytes, bits below
KEY_CODE        = $FF22         ; read: the waiting key
KEY_STATUS      = $FF23         ; read: bit 0 a key is waiting; write: take it
KEY_WAITING     = $01

PAD_A           = $0001
PAD_B           = $0002
PAD_X           = $0004
PAD_Y           = $0008
PAD_L1          = $0010
PAD_R1          = $0020
PAD_L2          = $0040
PAD_R2          = $0080
PAD_SELECT      = $0100
PAD_START       = $0200
PAD_L3          = $0400
PAD_R3          = $0800
PAD_UP          = $1000
PAD_DOWN        = $2000
PAD_LEFT        = $4000
PAD_RIGHT       = $8000

; --- cart, $FF30 --------------------------------------------------------------

BANK0           = $FF30         ; the bank at $8000
BANK1           = $FF31         ; the bank at $A000
CART_CTL        = $FF32         ; bits below; reset clears them
CART_ENABLED    = $01           ; the BIOS boots the cart
CART_WRITABLE   = $02           ; writes to the window go to the banks
SAVE_ADDR       = $FF34         ; save memory address, 2 bytes
SAVE_DATA       = $FF36         ; save memory at SAVE_ADDR; it then increments
SAVE_SIZE       = $8000

; --- drive, $FF40 -------------------------------------------------------------

DRIVE_NAME      = $FF40         ; write: append a character to the file name
DRIVE_CMD       = $FF41         ; write: a command; read: status, below
DRIVE_DATA      = $FF42         ; the next byte of the open file

DRIVE_OPEN_READ = 1
DRIVE_OPEN_WRITE = 2
DRIVE_OPEN_DIR  = 3             ; the names, each ending in a NUL
DRIVE_CLOSE     = 4
DRIVE_DELETE    = 5
DRIVE_READABLE  = $01           ; status: a byte can be read
DRIVE_FAILED    = $80           ; status: the last command failed

; --- status, $FF50 ------------------------------------------------------------

BATTERY         = $FF50         ; read: charge in percent, NO_BATTERY if none
NO_BATTERY      = $FF
POWER_STATUS    = $FF51         ; read: bits below
POWER_MAINS     = $01
POWER_CHARGING  = $02
TEMPERATURE     = $FF52         ; read: CPU temperature, degrees C
IP              = $FF54         ; read: IPv4 address, 4 bytes, MSB first; 0 offline

; --- sound, $FF80 -------------------------------------------------------------
; Four voices after the C64's SID (machine.h and docs/machine.md say more).
; Voice n at SOUND + 8n: frequency (word; Hz = value x 1 MHz / 2^24), pulse
; width (word, 0-4095), control, attack << 4 | decay, sustain << 4 |
; release, volume 0-15. Then the filter and master volume.

SOUND           = $FF80
SOUND_CUTOFF    = $FFA0         ; word, 0-2047: 30 Hz-12 kHz
SOUND_FILTER    = $FFA2         ; resonance << 4 | voices filtered
SOUND_MODE      = $FFA3         ; pass bits | master volume
SOUND_OSC3      = $FFA4         ; read: voice 3's oscillator, 0-255 (noise: random)
SOUND_ENV3      = $FFA5         ; read: voice 3's envelope, 0-255

SOUND_GATE      = $01
SOUND_SYNC      = $02
SOUND_RING      = $04
SOUND_TEST      = $08
SOUND_TRIANGLE  = $10
SOUND_SAW       = $20
SOUND_PULSE_WAVE = $40
SOUND_NOISE     = $80
SOUND_LOWPASS   = $10
SOUND_BANDPASS  = $20
SOUND_HIGHPASS  = $40
SOUND_VOICE3_OFF = $80          ; voice 3 out of the output (unless filtered)

; --- text ---------------------------------------------------------------------
; The BIOS's terminal: 40x30 characters on the background map, one font tile
; per character code ($20-$FF; pixels colour 1, background colour 2), in the
; palette of the text colour. The default palettes put colour n on black.

SCREEN_COLUMNS  = 40
SCREEN_ROWS     = 30

COLOR_BLACK     = $0
COLOR_RED       = $1
COLOR_GREEN     = $2
COLOR_YELLOW    = $3
COLOR_BLUE      = $4
COLOR_MAGENTA   = $5
COLOR_CYAN      = $6
COLOR_GREY      = $7
COLOR_DARKGREY  = $8
COLOR_LIGHTRED  = $9
COLOR_LIGHTGREEN = $A
COLOR_LIGHTYELLOW = $B
COLOR_LIGHTBLUE = $C
COLOR_LIGHTMAGENTA = $D
COLOR_LIGHTCYAN = $E
COLOR_WHITE     = $F
DEFAULT_COLOR   = COLOR_GREY

; Font: $20-$7E ASCII, $7F outlined block, $80-$9F block elements
; (U+2580-U+259F), $A0-$FF box drawing (U+2500-U+255F).
GLYPH_OUTLINE   = $7F
GLYPH_BLOCK     = $88
CURSOR_TILE     = $01           ; the cursor sprite's tile
CURSOR_SPRITE   = 63
SPRITE_HIDDEN   = 240           ; a sprite at this Y or below is not drawn

; --- keyboard -----------------------------------------------------------------

KEY_BACKSPACE   = $08
KEY_TAB         = $09
KEY_RETURN      = $0D
KEY_ESCAPE      = $1B
KEY_UP          = $80
KEY_DOWN        = $81
KEY_LEFT        = $82
KEY_RIGHT       = $83

; --- BIOS ---------------------------------------------------------------------
; Jump table: call these addresses, never the routines behind them. A, X and
; Y are not preserved. Byte results come back in A with X = 0, so C can call
; the routines directly (see machine.h).

BIOS_VERSION    = $C000         ; -> A: BIOS version
BIOS_CHROUT     = $C003         ; A: byte to the terminal (see below)
BIOS_PRINT      = $C006         ; A/X: NUL-terminated text to the terminal
BIOS_GETKEY     = $C009         ; -> A: waiting key, or 0 if none
BIOS_WAITKEY    = $C00C         ; -> A: next key; shows the cursor while waiting
BIOS_SETCURSOR  = $C00F         ; X: column, Y: row; carry set if off screen
BIOS_GETCURSOR  = $C012         ; -> X: column, Y: row
BIOS_SETCOLOR   = $C015         ; A: text colour (0-15) -> A: the previous one
BIOS_CLEAR      = $C018         ; clear the screen, cursor home
BIOS_WAITFRAME  = $C01B         ; halt until the next vblank
BIOS_VIDEORESET = $C01E         ; video chip back to the BIOS's set-up, clear screen
BIOS_FONT       = $C021         ; glyphs X..X+Y-1 as tiles at VADDR; A: rest << 4 | set pixels

; Terminal control bytes for BIOS_CHROUT; $20-$FF draw that glyph.
CH_BACKSPACE    = $08           ; cursor left (does not erase)
CH_NEWLINE      = $0A           ; start of the next line, scrolling at the end
CH_CLEAR        = $0C           ; clear the screen
CH_RETURN       = $0D           ; start of this line

; --- carts --------------------------------------------------------------------
; A cart file is a 32-byte header, then its 8K banks (docs/machine.md). Bank 0
; starts with the program header, written by the SDK's crt0.s: at reset, the
; BIOS jumps to the entry point of an enabled cart.

CART_MAGIC_SIZE = 4             ; "6CRT"
CART_VERSION    = 6
CART_HEADER_SIZE = 32
CART_ICON_SIZE  = 592           ; after the header: 16 tiles, 2 palettes, 16 palette choices
CART_H_VERSION  = 4             ; header offsets
CART_H_BANKS    = 5
CART_H_SAVE     = 7             ; K of save memory used

PROGRAM_HEADER  = $8000
.define PROGRAM_MAGIC "PRG"
PROGRAM_VERSION = 1             ; at $8003
PROGRAM_ENTRY   = $8004         ; where the BIOS jumps at reset

.endif
