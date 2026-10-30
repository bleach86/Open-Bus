/*
 * The machine test cart: three pages, L1 and R1 (Tab on a keyboard) to go
 * between them. Video: a landscape drawn from the cart's PNGs scrolling under
 * a title row that stays put (a raster interrupt splits the screen), a
 * 16x16 hero on the D-pad walking behind a foreground of pillars (plane B,
 * scrolling faster), bouncing balls and a palette-cycled pool. Input
 * and sound (sound.c): every button, live, and a voice on each of A, B, X
 * and Y. System (system.c): save memory, the banks, the clock, the status
 * chip.
 */
#include <6502.h>
#include <stdlib.h>
#include <string.h>
#include "music.h"
#include "test.h"

unsigned held;
unsigned pressed;
unsigned frames;

/* --- text --------------------------------------------------------------- */

void __fastcall__ text_at(unsigned char column, unsigned char row, unsigned char color, const char *text)
{
    VADDR = VRAM_MAP + (row * MAP_COLUMNS + column) * 2;
    while (*text) {
        VDATA = *text++;
        VDATA = color << 4;
    }
}

void number_at(unsigned char column, unsigned char row, unsigned char color, unsigned n, unsigned char width)
{
    static char digits[8];
    unsigned char length;

    utoa(n, digits, 10);
    length = strlen(digits);
    while (width-- > length) {
        text_at(column++, row, color, " ");
    }
    text_at(column, row, color, digits);
}

static const char *const pages[] = {"VIDEO", "INPUT/SOUND", "SYSTEM"};
#define PAGES 3
static unsigned char page;

void plain_screen(const char *title)
{
    bios_videoreset();
    SCROLLX = 0;
    SCROLLY = 0;
    text_at(1, 0, COLOR_WHITE, "MACHINE TEST");
    text_at(15, 0, COLOR_YELLOW, title);
    text_at(1, 29, COLOR_DARKGREY, "L1 R1 page");
}

/* --- video page ----------------------------------------------------------- */

/* In bank 4 (ART_SEGMENT). */
extern const unsigned char art_tiles[], art_tiles_palette[];
extern const unsigned char art_hero[], art_hero_palette[];
extern const unsigned char art_ball[], art_ball_palette[];
#define ART_BANK 4

/* Tiles above the BIOS's font. */
#define TILE_LAND 0x100 /* sky, grass, dirt, brick, water, cloud, cloud, star, pillar */
#define SKY 0
#define GRASS 1
#define DIRT 2
#define BRICK 3
#define WATER 4
#define CLOUD 5
#define STAR 7
#define PILLAR_TOP 8 /* and its right half */
#define PILLAR 10    /* likewise */
#define TILE_HERO 0x10C /* 16x16: four tiles */
#define TILE_BALL 0x110

#define PAL_LAND 0
#define PAL_HERO 1
#define PAL_BALL 2
#define PAL_TEXT 3
#define SKY_COLOR 15  /* in the land's palette */
#define WATER_COLOR 9 /* 9-11 cycle */

#define BALLS 8
static unsigned ball_x[BALLS];
static unsigned char ball_y[BALLS];
static signed char ball_dx[BALLS];
static signed char ball_dy[BALLS];
static unsigned hero_x;
static unsigned char hero_y;
static unsigned char hero_flip;
static unsigned scroll;

static void load_art(void)
{
    unsigned char bank = BANK1;

    BANK1 = ART_BANK;
    vram_write(VRAM_TILES + TILE_LAND * 32, art_tiles, 12 * 32);
    vram_write(VRAM_TILES + TILE_HERO * 32, art_hero, 4 * 32);
    vram_write(VRAM_TILES + TILE_BALL * 32, art_ball, 4 * 32);
    vram_write(VRAM_PALETTES + PAL_LAND * 32, art_tiles_palette, 32);
    vram_write(VRAM_PALETTES + PAL_HERO * 32, art_hero_palette, 32);
    vram_write(VRAM_PALETTES + PAL_BALL * 32, art_ball_palette, 32);
    BANK1 = bank;
    /* Text: white on dark blue. */
    VADDR = VRAM_PALETTES + PAL_TEXT * 32 + 2;
    VDATA = 0xFF;
    VDATA = 0x0F;
    VDATA = RGB(1, 1, 4) & 0xFF;
    VDATA = RGB(1, 1, 4) >> 8;
}

/* `count` cells of land tile `tile` from (column, row). */
static void land(unsigned char column, unsigned char row, unsigned char count, unsigned char tile)
{
    unsigned entry = MAP_ENTRY(TILE_LAND + tile, PAL_LAND);

    VADDR = VRAM_MAP + (row * MAP_COLUMNS + column) * 2;
    while (count--) {
        VDATA = entry;
        VDATA = entry >> 8;
    }
}

static const char title[] = " VIDEO     A fast  B stop   L1 R1 page";

/* The split: the title row (lines 0-7) unscrolled, set in vblank; from
 * line 8 on, the landscape's scroll, set by the raster interrupt. */
#define SPLIT_LINE 8
static unsigned char irq_stack[128];

static unsigned char split(void)
{
    SCROLLX = scroll & 511;
    VSTATUS = VIRQ_RASTER;
    return IRQ_HANDLED;
}

static void video_show(void)
{
    unsigned char row;
    unsigned char i;

    VCTRL = 0;
    load_art();
    BACKDROP = SKY_COLOR;
    text_at(0, 0, PAL_TEXT, title);
    for (i = sizeof title - 1; i < SCREEN_COLUMNS; ++i) {
        text_at(i, 0, PAL_TEXT, " ");
    }
    for (row = 1; row < MAP_ROWS; ++row) {
        land(0, row, MAP_COLUMNS, SKY);
    }
    for (i = 0; i < 6; ++i) {
        land(4 + i * 11, 3 + i % 3 * 2, 1, CLOUD);
        land(5 + i * 11, 3 + i % 3 * 2, 1, CLOUD + 1);
        land(9 + i * 10, 2 + i % 4, 1, STAR);
    }
    land(10, 17, 8, BRICK);
    land(40, 15, 6, BRICK);
    land(0, 22, MAP_COLUMNS, GRASS);
    for (row = 23; row < MAP_ROWS; ++row) {
        land(0, row, MAP_COLUMNS, DIRT);
    }
    for (row = 22; row < 25; ++row) {
        land(26, row, 8, WATER);
    }

    /* Plane B: a foreground of pillars, which the hero walks behind. */
    vram_fill(VRAM_MAP_B, 0, MAP_COLUMNS * MAP_ROWS * 2);
    for (i = 0; i < 4; ++i) {
        VADDR = VRAM_MAP_B + (12 * MAP_COLUMNS + 6 + i * 16) * 2;
        VDATA = TILE_LAND + PILLAR_TOP;
        VDATA = (TILE_LAND + PILLAR_TOP) >> 8;
        VDATA = TILE_LAND + PILLAR_TOP + 1;
        VDATA = (TILE_LAND + PILLAR_TOP + 1) >> 8;
        for (row = 13; row < 22; ++row) {
            VADDR = VRAM_MAP_B + (row * MAP_COLUMNS + 6 + i * 16) * 2;
            VDATA = TILE_LAND + PILLAR;
            VDATA = (TILE_LAND + PILLAR) >> 8;
            VDATA = TILE_LAND + PILLAR + 1;
            VDATA = (TILE_LAND + PILLAR + 1) >> 8;
        }
    }
    VADDR = VRAM_PRIORITY;
    VDATA = SPRITE_BEHIND_B; /* the hero */
    for (i = 1; i <= BALLS; ++i) {
        VDATA = SPRITE_FRONT;
    }

    hero_x = 152;
    hero_y = 160;
    for (i = 0; i < BALLS; ++i) {
        ball_x[i] = 20 + i * 36;
        ball_y[i] = 30 + i * 17;
        ball_dx[i] = i & 1 ? 1 : -2;
        ball_dy[i] = i & 2 ? 2 : -1;
    }
    VCTRL = VCTRL_DISPLAY | VCTRL_SPRITES | VCTRL_BIG | VCTRL_PLANE_B;
    RASTER = SPLIT_LINE;
    VSTATUS = VIRQ_RASTER;
    VIRQ = VIRQ_RASTER;
    set_irq(split, irq_stack, sizeof irq_stack);
}

static void sprite(unsigned char n, unsigned x, unsigned char y, unsigned tile, unsigned char attributes)
{
    VADDR = VRAM_SPRITES + n * 4;
    VDATA = y;
    VDATA = x;
    VDATA = tile;
    VDATA = attributes | (x >> 8 & 1) | (tile >> 8 & 1) << 1;
}

static void video_step(void)
{
    unsigned char i;
    unsigned char t;
    unsigned colors[3];

    SCROLLX = 0; /* in vblank: the title row; the split moves it after */
    SCROLLX2 = (scroll + scroll / 2) & 511; /* the foreground, nearer: faster */

    if (!(held & PAD_B)) {
        scroll += held & PAD_A ? 3 : 1;
    }

    if ((held & PAD_LEFT) && hero_x > 0) {
        hero_x -= 2;
        hero_flip = SPRITE_FLIP_X;
    }
    if ((held & PAD_RIGHT) && hero_x < 320 - 16) {
        hero_x += 2;
        hero_flip = 0;
    }
    if ((held & PAD_UP) && hero_y > 8) {
        hero_y -= 2;
    }
    if ((held & PAD_DOWN) && hero_y < 240 - 16) {
        hero_y += 2;
    }
    sprite(0, hero_x, hero_y, TILE_HERO, SPRITE_PALETTE(PAL_HERO) | hero_flip);

    for (i = 0; i < BALLS; ++i) {
        ball_x[i] += ball_dx[i];
        ball_y[i] += ball_dy[i];
        if (ball_x[i] < 2 || ball_x[i] > 320 - 18) {
            ball_dx[i] = -ball_dx[i];
        }
        if (ball_y[i] < 10 || ball_y[i] > 240 - 18) {
            ball_dy[i] = -ball_dy[i];
        }
        sprite(1 + i, ball_x[i], ball_y[i], TILE_BALL, SPRITE_PALETTE(PAL_BALL));
    }

    /* The pool's three blues take turns. */
    if ((frames & 7) == 0) {
        t = frames >> 3;
        VADDR = VRAM_PALETTES + PAL_LAND * 32 + WATER_COLOR * 2;
        colors[t % 3] = RGB(2, 4, 10);
        colors[(t + 1) % 3] = RGB(4, 8, 13);
        colors[(t + 2) % 3] = RGB(8, 12, 15);
        for (i = 0; i < 3; ++i) {
            VDATA = colors[i];
            VDATA = colors[i] >> 8;
        }
    }
}

/* --- pages ---------------------------------------------------------------- */

static void show(void)
{
    unsigned char voice;

    VIRQ = 0; /* the video page's split, off */
    music_stop();
    for (voice = 0; voice < 4; ++voice) {
        SOUND_CTRL(voice) = 0;
    }
    if (page == 0) {
        video_show();
    } else if (page == 1) {
        plain_screen(pages[1]);
        sound_show();
    } else {
        plain_screen(pages[2]);
        system_show();
    }
}

int main(void)
{
    unsigned pad;
    unsigned char key;

    system_boot();
    held = PAD;
    show();
    for (;;) {
        bios_waitframe();
        ++frames;
        music_tick();
        pad = PAD;
        pressed = pad & ~held;
        held = pad;
        key = bios_getkey();
        if (key == KEY_TAB) {
            pressed |= PAD_R1;
        }
        if (pressed & (PAD_L1 | PAD_R1)) {
            page = (pressed & PAD_R1) ? (page + 1) % PAGES : (page + PAGES - 1) % PAGES;
            show();
        } else if (page == 0) {
            video_step();
        } else if (page == 1) {
            sound_step();
        } else {
            system_step();
        }
    }
}
