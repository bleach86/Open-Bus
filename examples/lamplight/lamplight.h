/*
 * Lamplight: a lamplighter's round at dusk. Three levels of fields, a quarry
 * and a hill, scrolling sideways over hills that scroll slower behind them;
 * embers to gather, lamps to light (each one a place to start again from),
 * beetles and moths in the way, and a door at the end.
 *
 * main.c: the title, the results and the save. play.c: a level. levels.c:
 * the levels, drawn as text. video.c: the screen. The art is the PNGs in
 * art/, the music the songs in music/.
 */
#ifndef LAMPLIGHT_H
#define LAMPLIGHT_H

#include "machine.h"

#define LEVELS 3
#define LEVEL_W 128 /* cells of 16x16 pixels */
#define LEVEL_H 14

/* What a level's cells hold: art/16/level.png's blocks, in order. */
enum {
    AIR, GRASS, GRASS_L, GRASS_R, DIRT, DIRT_L, DIRT_R, STONE, PLANK, SPIKES,
    EMBER, LAMP, LAMP_LIT, DOOR_TOP, DOOR, TUFT, FLOWER
};

/* Tiles, above the BIOS's font. Sprites can only use the first 512. */
#define TILE_HERO 0x100    /* art/16/hero.png: 16x32 frames of 8 tiles */
#define TILE_SPRITES 0x140 /* art/16/sprites.png: 16x16 frames of 4 tiles */
#define TILE_LEVEL 0x170   /* art/16/level.png: a cell's 4 tiles */
#define TILE_HUD 0x1B8     /* art/hud.png */
#define TILE_SKY 0x1C0     /* art/map/sky.png */
#define TILE_FAR 0x210     /* art/map/far.png */
#define TILE_NEAR 0x260    /* art/map/near.png */

/* The lamplighter's frames, 16x32: the tiles of frame n start at
 * TILE_HERO + n * 8, the head's four, then the feet's. */
enum {
    HERO_IDLE, HERO_RUN1, HERO_RUN2, HERO_RUN3, HERO_SKID, HERO_JUMP, HERO_FALL, HERO_HURT
};
/* The other sprites' frames, 16x16: tiles from TILE_SPRITES + n * 4. */
enum {
    FRAME_BEETLE1, FRAME_BEETLE2, FRAME_BEETLE_FLAT, FRAME_MOTH1, FRAME_MOTH2,
    FRAME_PUFF1, FRAME_PUFF2, FRAME_SPARK1, FRAME_SPARK2
};

#define PAL_LEVEL 0   /* its colour 15 is the sky: the backdrop */
#define PAL_SCENERY 1 /* the three bands of plane A share one */
#define PAL_SPRITES 2
#define PAL_TEXT 5
#define PAL_AMBER 6
#define PAL_DIM 8
#define PAL_HUD 7
#define SKY_COLOR 15
#define EMBER_COLOR 11 /* 11-13 take turns: embers and lit lamps flicker */

/* Banks (the Makefile's ART_SEGMENTs, and levels.c). */
#define BANK_ART 4
#define BANK_LEVELS 5
#define BANK_SKY 6
#define BANK_NEAR 7

/* The screen: a HUD of two rows on plane B, the level under it. Plane A's
 * bands scroll at their own speeds from these lines on. */
#define HUD_LINES 16
#define FAR_LINE 112
#define NEAR_LINE 160

/* main.c */
extern unsigned held;
extern unsigned pressed;
extern unsigned char frames;
/* The next frame: the music, then the pad (and the keyboard held). */
void next_frame(void);
void __fastcall__ text_at(unsigned char column, unsigned char row, unsigned char palette, const char *text);
/* `seconds` as M:SS. */
const char *__fastcall__ clock_text(unsigned seconds);

/* video.c: the screen as play and the title share it. */
void video_start(void);
void video_stop(void);
/* Plane A's scenery, from the levels' camera position `x`. */
void scenery_at(unsigned x);
/* Plane B's scroll from the HUD down: the level's camera position. */
void level_at(unsigned x);
/* A 16x16 sprite at screen (x, y), its tiles from `tile`, for the next
 * frame; sprite() for the sheet's frames. */
void sprite_tile(int x, int y, unsigned tile, unsigned char attributes);
#define sprite(x, y, frame, attributes) sprite_tile(x, y, TILE_SPRITES + (frame) * 4, attributes)
/* The sprites given since the last, shown from the next frame on. */
void present(void);
/* Plane B: a box of `width` x `height` cells from (column, row), and
 * everything in it cleared. */
void panel(unsigned char column, unsigned char row, unsigned char width, unsigned char height);
void clear_b(void);

/* The outcome of a level played to its door. */
typedef struct {
    unsigned char embers;
    unsigned char total;
    unsigned char falls;
    unsigned seconds;
} Result;

/* play.c: level `n` until its door (1, and `result`) or quit (0). */
unsigned char __fastcall__ play(unsigned char n, Result *result);

/* levels.c, in bank BANK_LEVELS. */
extern const char level_rows[LEVELS][LEVEL_H][LEVEL_W + 1];
/* main.c */
extern const char *const level_names[LEVELS];

#endif
