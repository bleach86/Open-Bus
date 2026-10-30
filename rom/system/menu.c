/*
 * The menu: the system software the machine starts in when no cart is
 * enabled. A status bar along the top, then two pages, switched with L1/R1
 * (Tab on a keyboard): the carts on the drive, shown as cartridges on a
 * grid, and the system (the shell, and power).
 *
 * Choosing a cart works as a flash cart's menu does: load the file's banks
 * into the cart's RAM and its NAME.sav into save memory, note the cart in
 * .last, enable the cart and reset into it. The reset button brings the menu
 * back, and the menu then writes save memory out to that cart's NAME.sav.
 */
#include <stdlib.h>
#include <string.h>
#include "machine.h"
#include "system.h"

#define MAX_CARTS 64
#define NAME_SIZE 32 /* with the NUL; file names on the drive */
#define LAST ".last"

/* --- look ------------------------------------------------------------------ */

/* The PNGs in rom/art, converted by the build: each picture's tiles, then its
 * palette. All of them are drawn in one theme palette's colour numbers:
 *
 *   1 text     2 white    3 dim      4 accent   5 warning  6 border
 *   7 screen   8 card     9 green    10 red     11 yellow  12 grey
 *   13 dark grey          14 light grey
 *
 * so the menu needs just two palettes, the theme and the theme for the
 * selected card (text white, border accent), and the rest are free for six
 * carts' icons, two palettes each. Text in the theme's colours comes from
 * copies of the BIOS's font (bios_font), one for each colour it is drawn
 * in, so that one palette serves all the text. */
extern const unsigned char art_frame[], art_frame_palette[];
extern const unsigned char art_pill[], art_bracket[], art_buttons[];
extern const unsigned char art_status[], art_system[];

#define P_THEME 0
#define P_SELECTED 1
#define P_ICONS 2 /* two for each card shown */
#define C_TEXT 1
#define C_WHITE 2
#define C_DIM 3
#define C_ACCENT 4
#define C_WARN 5
#define C_BORDER 6
#define C_SCREEN 7
#define C_CARD 8 /* also the backdrop: a card's inside is empty cells */

/* Tiles. Sprites reach only the first 512, so the cursor is low; the
 * BIOS's own font stays at $20-$FF for the shell. */
#define TILE_BRACKET 0x02
#define TILE_FRAME 0x03   /* 3x3: corners, edges and an empty middle */
#define TILE_PILL 0x0C    /* a tab's left end; flipped, its right */
#define TILE_BUTTONS 0x0D /* A, B, then L1 and R1, two tiles each */
#define TILE_BUTTON_A (TILE_BUTTONS + 0)
#define TILE_BUTTON_L1 (TILE_BUTTONS + 2)
#define TILE_BUTTON_R1 (TILE_BUTTONS + 4)
#define TILE_STATUS 0x13 /* battery, bolt, thermometer, network */
#define TILE_BATTERY (TILE_STATUS + 0)
#define TILE_BOLT (TILE_STATUS + 1)
#define TILE_THERM (TILE_STATUS + 2)
#define TILE_NET (TILE_STATUS + 3)
#define TILE_FONTS 0x100  /* the font copies, $20-$7F each */
#define FONT_TILES 0x60
#define TILE_ICONS 0x340  /* 2x2 each: shell, games, reboot, power */
#define TILE_CARTS 0x350  /* 4x4 for each card shown */

/* A style: a font copy in a palette, as the map entry of character 0, so
 * that a character's entry is the style plus its code. */
#define FONT(n) (TILE_FONTS + (n) * FONT_TILES - 0x20)
#define STYLE_CARD MAP_ENTRY(FONT(0), P_THEME)    /* text on a card */
#define STYLE_CARD_ON MAP_ENTRY(FONT(0), P_SELECTED) /* white on the selected card */
#define STYLE_DIM MAP_ENTRY(FONT(1), P_THEME)     /* dim on a card */
#define STYLE_WARN MAP_ENTRY(FONT(2), P_THEME)    /* a warning on a card */
#define STYLE_SCREEN MAP_ENTRY(FONT(3), P_THEME)  /* dim on the screen */
#define STYLE_TAB MAP_ENTRY(FONT(4), P_THEME)     /* screen colour on the accent */
#define STYLE_BAR MAP_ENTRY(FONT(5), P_THEME)     /* white on the status bar */
/* Each copy's colours: the rest << 4 | the letters; 0 is see-through, and
 * shows the card colour behind. */
static const unsigned char fonts[6] = {
    C_TEXT, C_DIM, C_WARN, C_SCREEN << 4 | C_DIM, C_ACCENT << 4 | C_SCREEN, C_CARD << 4 | C_WHITE,
};

static void load_tiles(const unsigned char *tiles, unsigned char count, unsigned tile)
{
    vram_write(VRAM_TILES + tile * 32, tiles, count * 32u);
}

static void load_look(void)
{
    unsigned char i;

    load_tiles(art_bracket, 1, TILE_BRACKET);
    load_tiles(art_frame, 9, TILE_FRAME);
    load_tiles(art_pill, 1, TILE_PILL);
    load_tiles(art_buttons, 6, TILE_BUTTONS);
    load_tiles(art_status, 4, TILE_STATUS);
    load_tiles(art_system, 16, TILE_ICONS);
    for (i = 0; i < 6; ++i) {
        bios_font(VRAM_TILES + (TILE_FONTS + i * FONT_TILES) * 32, 0x20, FONT_TILES, fonts[i]);
    }
    vram_write(VRAM_PALETTES + P_THEME * 32, art_frame_palette, 32);
    vram_write(VRAM_PALETTES + P_SELECTED * 32, art_frame_palette, 32);
    VADDR = VRAM_PALETTES + P_SELECTED * 32 + C_TEXT * 2;
    VDATA = art_frame_palette[C_WHITE * 2];
    VDATA = art_frame_palette[C_WHITE * 2 + 1];
    VADDR = VRAM_PALETTES + P_SELECTED * 32 + C_BORDER * 2;
    VDATA = art_frame_palette[C_ACCENT * 2];
    VDATA = art_frame_palette[C_ACCENT * 2 + 1];
    BACKDROP = C_CARD;
}

/* --- drawing on the map ----------------------------------------------------- */

/* The menu draws on the map itself, so that a change is written well inside
 * one frame. The menu never scrolls. */
#define SPACE (STYLE_SCREEN + ' ')

static void cell(unsigned char column, unsigned char row)
{
    VADDR = VRAM_MAP + (row * MAP_COLUMNS + column) * 2;
}

static void put(unsigned entry)
{
    VDATA = entry;
    VDATA = entry >> 8;
}

static void put_at(unsigned char column, unsigned char row, unsigned entry)
{
    cell(column, row);
    put(entry);
}

/* `count` cells of `entry` from (column, row). */
static void fill(unsigned char column, unsigned char row, unsigned char count, unsigned entry)
{
    cell(column, row);
    while (count--) {
        put(entry);
    }
}

/* `text` from (column, row) in `style`; returns the column after it. */
static unsigned char text_at(unsigned char column, unsigned char row, unsigned style, const char *text)
{
    cell(column, row);
    while (*text) {
        put(style + (unsigned char)*text++);
        ++column;
    }
    return column;
}

/* A picture already in tiles, `columns` x `rows` of them from `tile`. */
static void put_image(unsigned char column, unsigned char row, unsigned tile,
                      unsigned char columns, unsigned char rows, unsigned char palette)
{
    unsigned char x;
    unsigned char y;

    for (y = 0; y < rows; ++y) {
        cell(column, row + y);
        for (x = 0; x < columns; ++x) {
            put(MAP_ENTRY(tile++, palette));
        }
    }
}

/* A card: the frame, `width` x `height` cells, its inside empty. */
static void draw_frame(unsigned char column, unsigned char row, unsigned char width,
                       unsigned char height, unsigned char palette)
{
    unsigned char y;
    unsigned char edge;

    for (y = 0; y < height; ++y) {
        edge = y == 0 ? 0 : y == height - 1 ? 6 : 3;
        cell(column, row + y);
        put(MAP_ENTRY(TILE_FRAME + edge, palette));
        fill(column + 1, row + y, width - 2, edge == 3 ? 0 : MAP_ENTRY(TILE_FRAME + edge + 1, palette));
        put(MAP_ENTRY(TILE_FRAME + edge + 2, palette));
    }
}

/* Rows `from` to `to` (inclusive) all screen. */
static void clear_rows(unsigned char from, unsigned char to)
{
    for (; from <= to; ++from) {
        fill(0, from, SCREEN_COLUMNS, SPACE);
    }
}

static void put_sprite(unsigned char n, unsigned x, unsigned char y, unsigned tile,
                       unsigned char palette, unsigned char flags)
{
    VADDR = VRAM_SPRITES + n * 4;
    VDATA = y;
    VDATA = x;
    VDATA = tile;
    VDATA = (x >> 8 & 1) | (tile >> 8 & 1) << 1 | flags | palette << 4;
}

/* --- the cursor ----------------------------------------------------------- */

/* Four brackets (sprites 0-3) around the selected card, gliding to it when
 * the selection moves. In pixels: where they are, and where they're going. */
static unsigned cursor[4];   /* x, y, width, height */
static unsigned heading[4];
static unsigned char cursor_shown;
static unsigned char cursor_drawn; /* the sprites show where the cursor is */

/* The cursor goes to the card at (column, row), `width` x `height` cells;
 * `jump` puts it there at once. */
static void aim_cursor(unsigned char column, unsigned char row, unsigned char width,
                       unsigned char height, unsigned char jump)
{
    unsigned char i;

    heading[0] = column * 8;
    heading[1] = row * 8;
    heading[2] = width * 8;
    heading[3] = height * 8;
    if (jump || !cursor_shown) {
        for (i = 0; i < 4; ++i) {
            cursor[i] = heading[i];
        }
    }
    cursor_shown = 1;
    cursor_drawn = 0;
}

/* Three quarters of the way there: it arrives in about four frames. */
static unsigned ease(unsigned now, unsigned to)
{
    unsigned left;

    if (now < to) {
        left = to - now;
        return now + left - left / 4;
    }
    left = now - to;
    return now - left + left / 4;
}

/* Once a frame: move the cursor along. Once it is there and drawn, this
 * does nothing at all. */
static void step_cursor(void)
{
    unsigned char i;
    unsigned right;
    unsigned char top;
    unsigned char bottom;

    if (cursor_drawn) {
        return;
    }
    cursor_drawn = 1;
    if (!cursor_shown) {
        for (i = 0; i < 4; ++i) {
            put_sprite(i, 0, SPRITE_HIDDEN, TILE_BRACKET, P_THEME, 0);
        }
        return;
    }
    for (i = 0; i < 4; ++i) {
        cursor[i] = ease(cursor[i], heading[i]);
        if (cursor[i] != heading[i]) {
            cursor_drawn = 0; /* still on its way */
        }
    }
    right = cursor[0] + cursor[2] - 5;
    top = cursor[1] - 3;
    bottom = cursor[1] + cursor[3] - 5;
    put_sprite(0, cursor[0] - 3, top, TILE_BRACKET, P_THEME, 0);
    put_sprite(1, right, top, TILE_BRACKET, P_THEME, SPRITE_FLIP_X);
    put_sprite(2, cursor[0] - 3, bottom, TILE_BRACKET, P_THEME, SPRITE_FLIP_Y);
    put_sprite(3, right, bottom, TILE_BRACKET, P_THEME, SPRITE_FLIP_X | SPRITE_FLIP_Y);
}

/* --- sound ---------------------------------------------------------------- */

/* The chip does the envelopes. Each of the menu's sounds is a voice set up
 * once; play() starts it, and with no sustain it dies away by itself,
 * wherever the menu is by then. */
#define VOICE_MOVE 0
#define VOICE_PAGE 1
#define VOICE_CHOOSE 2 /* and 3: a fifth */

static void sound_setup(void)
{
    SOUND_CUTOFF = 1000;              /* about 6 kHz */
    SOUND_FILTER = 0x20 | 0x02 | 0x04; /* a little resonance; voices 1 and 2 */
    SOUND_MODE = SOUND_LOWPASS | 15;

    /* Moving: a short, high triangle blip. */
    SOUND_FREQ(VOICE_MOVE) = SOUND_HZ(1047);
    SOUND_AD(VOICE_MOVE) = 0x02;
    SOUND_SR(VOICE_MOVE) = 0x00;
    SOUND_VOLUME(VOICE_MOVE) = 10;

    /* Turning the page: a thin pulse, a little longer. */
    SOUND_FREQ(VOICE_PAGE) = SOUND_HZ(659);
    SOUND_PULSE(VOICE_PAGE) = 1024;
    SOUND_AD(VOICE_PAGE) = 0x04;
    SOUND_SR(VOICE_PAGE) = 0x00;
    SOUND_VOLUME(VOICE_PAGE) = 8;

    /* Choosing: C and G together, square and triangle, ringing out. */
    SOUND_FREQ(VOICE_CHOOSE) = SOUND_HZ(523);
    SOUND_PULSE(VOICE_CHOOSE) = 2048;
    SOUND_AD(VOICE_CHOOSE) = 0x08;
    SOUND_SR(VOICE_CHOOSE) = 0x00;
    SOUND_VOLUME(VOICE_CHOOSE) = 8;
    SOUND_FREQ(VOICE_CHOOSE + 1) = SOUND_HZ(784);
    SOUND_AD(VOICE_CHOOSE + 1) = 0x08;
    SOUND_SR(VOICE_CHOOSE + 1) = 0x00;
    SOUND_VOLUME(VOICE_CHOOSE + 1) = 9;
}

/* Start `voice` again from its attack, as `wave`. */
static void play(unsigned char voice, unsigned char wave)
{
    SOUND_CTRL(voice) = wave;
    SOUND_CTRL(voice) = wave | SOUND_GATE;
}

/* --- drive ---------------------------------------------------------------- */

static unsigned char drive(unsigned char command, const char *file)
{
    while (*file) {
        DRIVE_NAME = *file++;
    }
    DRIVE_CMD = command;
    return !(DRIVE_CMD & DRIVE_FAILED);
}

static unsigned char readable(void)
{
    return DRIVE_CMD & DRIVE_READABLE;
}

/* --- carts ---------------------------------------------------------------- */

static char carts[MAX_CARTS][NAME_SIZE]; /* file names, NAME.crt */
static unsigned char cart_count;
static char name[NAME_SIZE + 4];
static CartHeader header;

static unsigned char is_cart_file(const char *file, unsigned char length)
{
    return length > 4 && length < NAME_SIZE && strcmp(file + length - 4, ".crt") == 0;
}

/* The drive's carts, in the order it lists them. */
static void find_carts(void)
{
    unsigned char length = 0;
    char c;

    cart_count = 0;
    if (!drive(DRIVE_OPEN_DIR, "")) {
        return;
    }
    while (readable()) {
        c = DRIVE_DATA;
        if (c != 0) {
            if (length < sizeof name - 1) {
                name[length++] = c;
            }
            continue;
        }
        name[length] = '\0';
        if (is_cart_file(name, length) && cart_count < MAX_CARTS) {
            strcpy(carts[cart_count++], name);
        }
        length = 0;
    }
    drive(DRIVE_CLOSE, "");
}

/* `file` with its extension replaced by `extension`, in `name`. */
static void with_extension(const char *file, const char *extension)
{
    strcpy(name, file);
    strcpy(strrchr(name, '.'), extension);
}

/* Read cart `file`'s header, leaving the file open; 0 if it is not a cart
 * this machine runs. */
static unsigned char read_header(const char *file)
{
    unsigned char *byte = (unsigned char *)&header;
    unsigned char i;

    if (!drive(DRIVE_OPEN_READ, file)) {
        return 0;
    }
    for (i = 0; i < sizeof header; ++i) {
        byte[i] = DRIVE_DATA;
    }
    return memcmp(header.magic, "6CRT", 4) == 0 && header.version == CART_VERSION &&
           header.bank_count != 0;
}

/* Write the save memory of the cart that ran last, if it had any, to its
 * NAME.sav. */
static void keep_last_save(void)
{
    unsigned char length = 0;
    unsigned count;
    char c;

    if (!drive(DRIVE_OPEN_READ, LAST)) {
        return;
    }
    while (readable() && (c = DRIVE_DATA) != 0 && length < NAME_SIZE - 1) {
        name[length++] = c;
    }
    name[length] = '\0';
    count = readable() ? DRIVE_DATA * 1024u : 0;
    drive(DRIVE_CLOSE, "");
    if (count != 0 && drive(DRIVE_OPEN_WRITE, name)) {
        SAVE_ADDR = 0;
        while (count--) {
            DRIVE_DATA = SAVE_DATA;
        }
        drive(DRIVE_CLOSE, "");
    }
    drive(DRIVE_DELETE, LAST);
}

static void message(const char *text);

/* Load cart `file` into the flash cart and reset into it. Returns only if
 * the file is not a cart. */
static void boot(const char *file)
{
    static unsigned char skipped;
    unsigned char bank;
    unsigned char *at;
    unsigned count;

    if (!read_header(file)) {
        drive(DRIVE_CLOSE, "");
        message("Not a cart for this machine.");
        return;
    }
    message("Loading...");
    for (count = CART_ICON_SIZE; count; --count) {
        skipped = DRIVE_DATA;
    }
    CART_CTL = CART_WRITABLE;
    for (bank = 0; bank < header.bank_count; ++bank) {
        BANK0 = bank;
        at = (unsigned char *)0x8000;
        count = 0x2000;
        do {
            *at++ = DRIVE_DATA;
        } while (--count);
    }
    drive(DRIVE_CLOSE, "");

    with_extension(file, ".sav");
    if (header.save_k != 0) {
        /* The cart's saved memory, or zeros if it has none yet. */
        drive(DRIVE_OPEN_READ, name);
        SAVE_ADDR = 0;
        for (count = header.save_k * 1024u; count; --count) {
            SAVE_DATA = readable() ? DRIVE_DATA : 0;
        }
        drive(DRIVE_CLOSE, "");
        if (drive(DRIVE_OPEN_WRITE, LAST)) {
            for (at = (unsigned char *)name; *at; ++at) {
                DRIVE_DATA = *at;
            }
            DRIVE_DATA = 0;
            DRIVE_DATA = header.save_k;
            drive(DRIVE_CLOSE, "");
        }
    }

    BANK0 = 0;
    BANK1 = 1;
    CART_CTL = CART_ENABLED;
    __asm__("jmp ($FFFC)");
}

/* --- system page ---------------------------------------------------------- */

#define DO_SHELL 0xFF

typedef struct {
    const char *label;
    const char *info;
    unsigned char icon; /* 0-3: shell, games, reboot, power */
    unsigned char power; /* POWER status, or DO_SHELL */
} SystemEntry;

static const SystemEntry with_launcher[] = {
    {"Shell", "The command line", 0, DO_SHELL},
    {"Games", "Play games with ES-DE", 1, POWER_GAMES},
    {"Reboot", "Restart the device", 2, POWER_REBOOT},
    {"Power off", "Shut the device down", 3, POWER_SHUTDOWN},
};

static const SystemEntry standalone[] = {
    {"Shell", "The command line", 0, DO_SHELL},
    {"Power off", "Turn the machine off", 3, POWER_OFF},
};

static const SystemEntry *system_entries;
static unsigned char system_count;

/* --- layout --------------------------------------------------------------- */

#define BAR_ROW 0
#define TAB_ROW 2
#define GRID_ROW 4
#define GRID_COLUMNS 3
#define GRID_ROWS 2
#define PER_SCREEN (GRID_COLUMNS * GRID_ROWS)
#define CARD_WIDTH 12
#define CARD_HEIGHT 8
#define CARD_STEP_X 13
#define CARD_STEP_Y 9
#define SYSTEM_ROW 4
#define SYSTEM_HEIGHT 4
#define SYSTEM_STEP 5
#define INFO_ROW 22
#define INFO_HEIGHT 5
#define INFO_DETAIL (INFO_ROW + 3)
#define HINT_ROW 28

#define PAGE_CARTS 0
#define PAGE_SYSTEM 1

static unsigned char page;
static unsigned char selected[2];
static unsigned char first_shown; /* the cart in the grid's top left */

/* `n` in decimal; the next call overwrites it. */
static const char *number(unsigned n)
{
    static char digits[6];

    return utoa(n, digits, 10);
}

static void message(const char *text)
{
    fill(3, INFO_DETAIL, SCREEN_COLUMNS - 6, 0);
    text_at(3, INFO_DETAIL, STYLE_WARN, text);
}

/* The status bar: the machine's name on the left; network, temperature,
 * charging and battery on the right, each after its icon. */
static void draw_status(void)
{
    static char text[16];
    unsigned char battery = BATTERY;
    unsigned char i;
    unsigned char x = SCREEN_COLUMNS - 1;

    fill(0, BAR_ROW, SCREEN_COLUMNS, STYLE_BAR + ' ');
    text_at(1, BAR_ROW, STYLE_BAR, "6502");
    if (battery != NO_BATTERY) {
        strcpy(text, number(battery));
        strcat(text, "%");
        x -= strlen(text);
        text_at(x, BAR_ROW, STYLE_BAR, text);
        put_at(--x, BAR_ROW, MAP_ENTRY(TILE_BATTERY, P_THEME));
        if (POWER_STATUS & POWER_CHARGING) {
            put_at(--x, BAR_ROW, MAP_ENTRY(TILE_BOLT, P_THEME));
        }
        x -= 2;
    }
    strcpy(text, number(TEMPERATURE));
    strcat(text, "C");
    x -= strlen(text);
    text_at(x, BAR_ROW, STYLE_BAR, text);
    put_at(--x, BAR_ROW, MAP_ENTRY(TILE_THERM, P_THEME));
    x -= 2;
    if (IP[0] == 0) {
        strcpy(text, "offline");
    } else {
        strcpy(text, number(IP[0]));
        for (i = 1; i < 4; ++i) {
            strcat(text, ".");
            strcat(text, number(IP[i]));
        }
    }
    x -= strlen(text);
    text_at(x, BAR_ROW, STYLE_BAR, text);
    put_at(--x, BAR_ROW, MAP_ENTRY(TILE_NET, P_THEME));
}

/* A tab: a pill when it is the page shown, plain dim text when not. */
static void draw_tab(unsigned char column, const char *label, unsigned char shown)
{
    if (shown) {
        put_at(column, TAB_ROW, MAP_ENTRY(TILE_PILL, P_THEME));
        put_at(text_at(column + 1, TAB_ROW, STYLE_TAB, label), TAB_ROW,
               MAP_ENTRY(TILE_PILL, P_THEME) | MAP_FLIP_X);
    } else {
        put_at(column, TAB_ROW, SPACE);
        put_at(text_at(column + 1, TAB_ROW, STYLE_SCREEN, label), TAB_ROW, SPACE);
    }
}

static void draw_tabs(void)
{
    draw_tab(1, "CARTS", page == PAGE_CARTS);
    draw_tab(9, "SYSTEM", page == PAGE_SYSTEM);
}

/* The button prompts along the bottom. */
static void draw_hint(void)
{
    unsigned char x;

    clear_rows(HINT_ROW, HINT_ROW);
    put_at(1, HINT_ROW, MAP_ENTRY(TILE_BUTTON_A, P_THEME));
    x = text_at(3, HINT_ROW, STYLE_SCREEN, page == PAGE_CARTS ? "Play" : "Select") + 2;
    put_image(x, HINT_ROW, TILE_BUTTON_L1, 2, 1, P_THEME);
    put_image(x + 2, HINT_ROW, TILE_BUTTON_R1, 2, 1, P_THEME);
    text_at(x + 5, HINT_ROW, STYLE_SCREEN, "Page");
}

/* Cart `i` (of those on the drive) without its .crt, at most `width`
 * characters, in `name`. */
static void cart_name(unsigned char i, unsigned char width)
{
    unsigned char length;

    strcpy(name, carts[i]);
    length = strlen(name) - 4;
    name[length < width ? length : width] = '\0';
}

/* The panel under the grid: the selected cart's name, and its size and
 * save. */
static void draw_info(void)
{
    static char text[SCREEN_COLUMNS];
    unsigned char i = selected[PAGE_CARTS];

    draw_frame(1, INFO_ROW, SCREEN_COLUMNS - 2, INFO_HEIGHT, P_THEME);
    if (cart_count == 0) {
        text_at(3, INFO_ROW + 1, STYLE_CARD, "No carts on the drive.");
        text_at(3, INFO_DETAIL, STYLE_DIM, "Copy .crt files to it.");
        return;
    }
    cart_name(i, 24);
    text_at(3, INFO_ROW + 1, STYLE_CARD_ON, name);
    strcpy(text, number(i + 1));
    strcat(text, " of ");
    strcat(text, number(cart_count));
    text_at(SCREEN_COLUMNS - 3 - strlen(text), INFO_ROW + 1, STYLE_DIM, text);
    if (!read_header(carts[i])) {
        strcpy(text, "Not a cart for this machine");
    } else {
        strcpy(text, number(header.bank_count * 8));
        strcat(text, "K cart");
        if (header.save_k != 0) {
            strcat(text, ", saves ");
            strcat(text, number(header.save_k));
            strcat(text, "K");
        }
    }
    drive(DRIVE_CLOSE, "");
    text_at(3, INFO_DETAIL, STYLE_DIM, text);
}

/* The icons of the carts shown: each slot's tiles and two palettes are in
 * VRAM, and here, the palette each tile takes. */
static unsigned char icon_palettes[PER_SCREEN][16];
static unsigned char icon_loaded[PER_SCREEN];

/* Cart `file`'s icon into VRAM for grid slot `slot`. */
static void load_icon(unsigned char slot, const char *file)
{
    unsigned count;
    unsigned char i;

    icon_loaded[slot] = read_header(file);
    if (icon_loaded[slot]) {
        VADDR = VRAM_TILES + (TILE_CARTS + slot * 16) * 32;
        for (count = 16 * 32; count; --count) {
            VDATA = DRIVE_DATA;
        }
        VADDR = VRAM_PALETTES + (P_ICONS + slot * 2) * 32;
        for (i = 0; i < 64; ++i) {
            VDATA = DRIVE_DATA;
        }
        for (i = 0; i < 16; ++i) {
            icon_palettes[slot][i] = DRIVE_DATA & 1;
        }
    }
    drive(DRIVE_CLOSE, "");
}

/* Grid slot `slot` of the screenful shown: its cart's card, if it has a
 * cart. */
static void draw_slot(unsigned char slot, unsigned char jump)
{
    unsigned char i = first_shown + slot;
    unsigned char column = 1 + slot % GRID_COLUMNS * CARD_STEP_X;
    unsigned char row = GRID_ROW + slot / GRID_COLUMNS * CARD_STEP_Y;
    unsigned char mine = i == selected[PAGE_CARTS];
    unsigned char length;
    unsigned char y;
    unsigned char x;
    const unsigned char *select = icon_palettes[slot];
    unsigned tile = TILE_CARTS + slot * 16;
    unsigned char palette = P_ICONS + slot * 2;

    if (i >= cart_count) {
        return; /* draw_carts cleared the grid */
    }
    draw_frame(column, row, CARD_WIDTH, CARD_HEIGHT, mine ? P_SELECTED : P_THEME);
    if (icon_loaded[slot]) {
        for (y = 0; y < 4; ++y) {
            cell(column + 4, row + 1 + y);
            for (x = 0; x < 4; ++x) {
                put(MAP_ENTRY(tile++, palette + *select++));
            }
        }
    }
    cart_name(i, CARD_WIDTH - 2);
    length = strlen(name);
    text_at(column + (CARD_WIDTH - length) / 2, row + 6, mine ? STYLE_CARD_ON : STYLE_CARD, name);
    if (mine) {
        aim_cursor(column, row, CARD_WIDTH, CARD_HEIGHT, jump);
    }
}

static void draw_carts(void)
{
    unsigned char slot;

    first_shown = selected[PAGE_CARTS] / PER_SCREEN * PER_SCREEN;
    cursor_shown = 0;
    cursor_drawn = 0;
    for (slot = 0; slot < PER_SCREEN; slot += GRID_COLUMNS) {
        clear_rows(GRID_ROW + slot / GRID_COLUMNS * CARD_STEP_Y,
                   GRID_ROW + slot / GRID_COLUMNS * CARD_STEP_Y + CARD_STEP_Y - 1);
    }
    for (slot = 0; slot < PER_SCREEN; ++slot) {
        if (first_shown + slot < cart_count) {
            load_icon(slot, carts[first_shown + slot]);
        }
        draw_slot(slot, 1);
    }
    draw_info();
}

static void draw_system_entry(unsigned char i, unsigned char jump)
{
    unsigned char row = SYSTEM_ROW + i * SYSTEM_STEP;
    unsigned char mine = i == selected[PAGE_SYSTEM];

    draw_frame(1, row, SCREEN_COLUMNS - 2, SYSTEM_HEIGHT, mine ? P_SELECTED : P_THEME);
    put_image(3, row + 1, TILE_ICONS + system_entries[i].icon * 4, 2, 2, P_THEME);
    text_at(7, row + 1, mine ? STYLE_CARD_ON : STYLE_CARD, system_entries[i].label);
    text_at(7, row + 2, STYLE_DIM, system_entries[i].info);
    if (mine) {
        aim_cursor(1, row, SCREEN_COLUMNS - 2, SYSTEM_HEIGHT, jump);
    }
}

static void draw_system(void)
{
    unsigned char i;

    clear_rows(TAB_ROW + 1, HINT_ROW - 1);
    cursor_shown = 0;
    cursor_drawn = 0;
    for (i = 0; i < system_count; ++i) {
        draw_system_entry(i, 1);
    }
}

/* The page: everything between the tabs and the button prompts. */
static void draw_page(void)
{
    if (page == PAGE_CARTS) {
        clear_rows(TAB_ROW + 1, GRID_ROW - 1);
        clear_rows(INFO_ROW + INFO_HEIGHT, HINT_ROW - 1);
        draw_carts();
    } else {
        draw_system();
    }
    draw_hint();
}

/* The selection moved from `from`: redraw the two cards and the info, or the
 * whole grid if it moved to another screenful. */
static void draw_move(unsigned char from)
{
    unsigned char to = selected[page];

    if (page == PAGE_SYSTEM) {
        draw_system_entry(from, 0);
        draw_system_entry(to, 0);
    } else if (to / PER_SCREEN != first_shown / PER_SCREEN) {
        draw_carts();
    } else {
        draw_slot(from - first_shown, 0);
        draw_slot(to - first_shown, 0);
        draw_info();
    }
}

static void draw(void)
{
    VCTRL = 0; /* drawn whole before it shows */
    SCROLLX = 0;
    SCROLLY = 0;
    load_look();
    clear_rows(0, MAP_ROWS - 1);
    draw_status();
    draw_tabs();
    draw_page();
    step_cursor();
    bios_waitframe();
    VCTRL = VCTRL_DISPLAY | VCTRL_SPRITES;
}

/* --- input ---------------------------------------------------------------- */

#define GO_UP 1
#define GO_DOWN 2
#define GO_LEFT 3
#define GO_RIGHT 4
#define GO_PAGE 5
#define GO_CHOOSE 6

/* Held this many frames, a direction starts repeating every REPEAT_RATE. */
#define REPEAT_DELAY 20
#define REPEAT_RATE 4
#define DIRECTIONS (PAD_UP | PAD_DOWN | PAD_LEFT | PAD_RIGHT)

static unsigned held;
static unsigned char held_frames;

/* What this frame's input asks for, or 0. */
static unsigned char next_action(void)
{
    unsigned pad = PAD;
    unsigned pressed = pad & ~held;
    unsigned char key = bios_getkey();

    held = pad;
    if (pad & DIRECTIONS) {
        ++held_frames;
        if (held_frames > REPEAT_DELAY && (held_frames - REPEAT_DELAY) % REPEAT_RATE == 0) {
            pressed |= pad & DIRECTIONS;
        }
    } else {
        held_frames = 0;
    }
    if ((pressed & PAD_UP) || key == KEY_UP) {
        return GO_UP;
    }
    if ((pressed & PAD_DOWN) || key == KEY_DOWN) {
        return GO_DOWN;
    }
    if ((pressed & PAD_LEFT) || key == KEY_LEFT) {
        return GO_LEFT;
    }
    if ((pressed & PAD_RIGHT) || key == KEY_RIGHT) {
        return GO_RIGHT;
    }
    if ((pressed & (PAD_L1 | PAD_R1)) || key == KEY_TAB) {
        return GO_PAGE;
    }
    if ((pressed & (PAD_A | PAD_START)) || key == KEY_RETURN) {
        return GO_CHOOSE;
    }
    return 0;
}

/* Move the selection by `step` within `count` entries; 0 if it cannot. */
static unsigned char move(signed char step, unsigned char count)
{
    int to = selected[page] + step;

    if (to < 0 || to >= count) {
        return 0;
    }
    selected[page] = to;
    return 1;
}

static void choose(void)
{
    unsigned char i = selected[page];
    unsigned char power;

    if (page == PAGE_CARTS) {
        if (i < cart_count) {
            boot(carts[i]); /* returns only for a bad file */
        }
        return;
    }
    power = system_entries[i].power;
    if (power == DO_SHELL) {
        bios_videoreset();
        shell();
        draw();
        return;
    }
    POWER = power;
    for (;;) {
        bios_waitframe(); /* the machine powers off at the end of the frame */
    }
}

int main(void)
{
    unsigned char action;
    unsigned char moved;
    unsigned char from;
    unsigned char count;

    if (HOST & HOST_LAUNCHER) {
        system_entries = with_launcher;
        system_count = sizeof with_launcher / sizeof with_launcher[0];
    } else {
        system_entries = standalone;
        system_count = sizeof standalone / sizeof standalone[0];
    }
    keep_last_save();
    find_carts();
    held = PAD; /* a button still down from the reset does nothing */
    sound_setup();
    draw();
    for (;;) {
        bios_waitframe();
        step_cursor();
        if (FRAME_COUNT % 60 == 0) {
            draw_status();
        }

        action = next_action();
        count = page == PAGE_CARTS ? cart_count : system_count;
        moved = 0;
        from = selected[page];
        if (action == GO_PAGE) {
            page ^= 1;
            play(VOICE_PAGE, SOUND_PULSE_WAVE);
            draw_tabs();
            draw_page();
        } else if (action == GO_CHOOSE) {
            /* Most of the chord rings out before the menu goes: a cart's
             * reset silences the chip. */
            play(VOICE_CHOOSE, SOUND_PULSE_WAVE);
            play(VOICE_CHOOSE + 1, SOUND_TRIANGLE);
            for (count = 0; count < 15; ++count) {
                bios_waitframe();
                step_cursor();
            }
            choose();
        } else if (action == GO_UP) {
            moved = move(page == PAGE_CARTS ? -GRID_COLUMNS : -1, count);
        } else if (action == GO_DOWN) {
            moved = move(page == PAGE_CARTS ? GRID_COLUMNS : 1, count);
        } else if (action == GO_LEFT && page == PAGE_CARTS) {
            moved = move(-1, count);
        } else if (action == GO_RIGHT && page == PAGE_CARTS) {
            moved = move(1, count);
        }
        if (moved) {
            play(VOICE_MOVE, SOUND_TRIANGLE);
            draw_move(from);
        }
    }
}
