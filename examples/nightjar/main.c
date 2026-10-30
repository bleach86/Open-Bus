/*
 * Nightjar: a shoot-'em-up over the sea at night. The D-pad flies, B or A
 * held fires. Drifters weave down the screen, divers drop and then dive at
 * you, orbs stop and fire spreads. The waves go round faster each time;
 * three ships, and the high score kept in save memory.
 *
 * Plane A is the sea, scrolling down; plane B the clouds passing over it,
 * faster, and in its columns 40-63 the score bar. One raster interrupt
 * makes the split: lines 0-15 show the bar, unscrolled down, and the rest
 * the clouds. Sprites go to VRAM from a list in RAM, all at once in
 * vblank.
 */
#include <6502.h>
#include <stdlib.h>
#include <string.h>
#include "machine.h"
#include "music.h"

/* Art (art/), in banks 4-6 (the Makefile). */
extern const unsigned char art_sprites[], art_sprites_palette[];
extern const unsigned char art_sea[], art_sea_palette[];
extern const unsigned char art_clouds[], art_clouds_palette[];
extern const unsigned art_sea_count, art_sea_map[], art_clouds_count, art_clouds_map[];
#define BANK_SPRITES 4
#define BANK_SEA 5
#define BANK_CLOUDS 6

extern const Song song_flight, song_title, song_over, song_shot, song_boom, song_hit;
#define SFX_VOICE 3 /* the music leaves it free */

#define TILE_SPRITES 0x100
#define TILE_SEA 0x140
#define TILE_CLOUDS 0x1C0
enum {
    F_SHIP, F_LEFT, F_RIGHT, F_SHIP2, F_SHOT, F_DRIFTER, F_DRIFTER2, F_DIVER, F_DIVER2,
    F_ORB, F_ORB2, F_BULLET, F_BOOM1, F_BOOM2, F_BOOM3, F_LIFE
};
#define PAL_SEA 0
#define PAL_CLOUDS 1
#define PAL_SPRITES 2
#define PAL_TEXT 5
#define PAL_AMBER 6

#define HUD_COLUMN 40 /* plane B's columns 40-63: the score bar */
#define HUD_SCROLL 256 /* lines 0-15 show columns 32-71: the bar in the middle */
#define HUD_LINES 16

static unsigned held, pressed;
static unsigned char frames;

/* --- the screen ----------------------------------------------------------- */

static void text_at(unsigned char column, unsigned char row, unsigned char palette, const char *text)
{
    VADDR = VRAM_MAP_B + (row * MAP_COLUMNS + column) * 2;
    while (*text) {
        VDATA = *text++;
        VDATA = palette << 4;
    }
}

/* `n` in `width` digits, leading zeros. */
static const char *digits(unsigned n, unsigned char width)
{
    static char text[8];

    text[width] = '\0';
    while (width--) {
        text[width] = '0' + n % 10;
        n /= 10;
    }
    return text;
}

/* A tile map 40 cells across onto a plane, its tiles at `tile`. */
static void picture(unsigned plane, const unsigned *map, unsigned tile, unsigned char palette)
{
    unsigned char row, column;
    unsigned entry;

    for (row = 0; row < MAP_ROWS; ++row) {
        VADDR = plane + row * MAP_COLUMNS * 2;
        for (column = 0; column < 40; ++column) {
            entry = *map++ + tile + MAP_ENTRY(0, palette);
            VDATA = entry;
            VDATA = entry >> 8;
        }
    }
}

static const unsigned text_colors[] = {RGB(14, 15, 15), RGB(15, 11, 4)};

static void screen_start(void)
{
    unsigned char bank = BANK1;
    unsigned char i;

    VIRQ = 0;
    bios_videoreset();
    VCTRL = 0;
    vram_fill(VRAM_TILES, 0, 32);
    BANK1 = BANK_SPRITES;
    vram_write(VRAM_TILES + TILE_SPRITES * 32, art_sprites, 16 * 4 * 32);
    vram_write(VRAM_PALETTES + PAL_SPRITES * 32, art_sprites_palette, 32);
    BANK1 = BANK_SEA;
    vram_write(VRAM_TILES + TILE_SEA * 32, art_sea, art_sea_count * 32);
    vram_write(VRAM_PALETTES + PAL_SEA * 32, art_sea_palette, 32);
    picture(VRAM_MAP, art_sea_map, TILE_SEA, PAL_SEA);
    BANK1 = BANK_CLOUDS;
    vram_write(VRAM_TILES + TILE_CLOUDS * 32, art_clouds, art_clouds_count * 32);
    vram_write(VRAM_PALETTES + PAL_CLOUDS * 32, art_clouds_palette, 32);
    BANK1 = bank;
    for (i = 0; i < 2; ++i) {
        VADDR = VRAM_PALETTES + (PAL_TEXT + i) * 32 + 2;
        VDATA = text_colors[i];
        VDATA = text_colors[i] >> 8;
        VDATA = RGB(0, 1, 3) & 0xFF;
        VDATA = RGB(0, 1, 3) >> 8;
    }
    vram_fill(VRAM_MAP_B, 0, MAP_COLUMNS * MAP_ROWS * 2);
    SCROLLX = 0;
    SCROLLX2 = 0;
    SCROLLY2 = 0;
    VCTRL = VCTRL_DISPLAY | VCTRL_SPRITES | VCTRL_BIG | VCTRL_PLANE_B;
}

/* The clouds on plane B, the score bar beside them: for play. */
static void clouds_on(void)
{
    unsigned char bank = BANK1;

    BANK1 = BANK_CLOUDS;
    picture(VRAM_MAP_B, art_clouds_map, TILE_CLOUDS, PAL_CLOUDS);
    BANK1 = bank;
}

/* The split: lines 0-15 the bar, then the clouds. */
static unsigned char irq_stack[64];
static unsigned char cloud_y, next_cloud_y;

static unsigned char split(void)
{
    unsigned char status = VSTATUS;

    if (status & VIRQ_VBLANK) {
        SCROLLX2 = HUD_SCROLL;
        SCROLLY2 = 0;
        VSTATUS = VIRQ_VBLANK;
    }
    if (status & VIRQ_RASTER) {
        SCROLLX2 = 0;
        SCROLLY2 = cloud_y;
        VSTATUS = VIRQ_RASTER;
    }
    return IRQ_HANDLED;
}

static void split_on(void)
{
    RASTER = HUD_LINES;
    set_irq(split, irq_stack, sizeof irq_stack);
    VSTATUS = VIRQ_VBLANK | VIRQ_RASTER;
    VIRQ = VIRQ_VBLANK | VIRQ_RASTER;
}

static void split_off(void)
{
    VIRQ = 0;
    reset_irq();
    SCROLLX2 = 0;
    SCROLLY2 = 0;
}

/* Sprites: a list, to VRAM in vblank. */
static unsigned char list[64 * 4];
static unsigned char listed, shown;
static unsigned char sea_y;

static void sprite(int x, int y, unsigned char frame)
{
    unsigned char *s;
    unsigned tile;

    if (x <= -16 || x >= 320 || y < 0 || y >= 240 || listed == 64) {
        return;
    }
    tile = TILE_SPRITES + frame * 4;
    x &= 511;
    s = list + listed * 4;
    s[0] = y;
    s[1] = x;
    s[2] = tile;
    s[3] = SPRITE_PALETTE(PAL_SPRITES) | (x >> 8) | (tile >> 8 & 1) << 1;
    ++listed;
}

static void next_frame(void)
{
    unsigned char i;
    unsigned pad;

    bios_waitframe();
    SCROLLY = sea_y;
    SEI();
    cloud_y = next_cloud_y;
    CLI();
    vram_write(VRAM_SPRITES, list, listed * 4);
    for (i = listed; i < shown; ++i) {
        VADDR = VRAM_SPRITES + i * 4;
        VDATA = SPRITE_HIDDEN;
    }
    shown = listed;
    listed = 0;

    ++frames;
    music_tick();
    pad = PAD | KEYPAD;
    pressed = pad & ~held;
    held = pad;
    bios_getkey(); /* keys as typed: not wanted */
}

/* The sea and the clouds moving on: we fly up the screen. */
static void scroll_on(void)
{
    if (frames & 1) {
        --sea_y;
    }
    next_cloud_y -= 2;
}

/* --- the save: the high score ---------------------------------------------------- */

static struct {
    char magic[4];
    unsigned best; /* in tens */
} save;
static const char magic[4] = {'N', 'J', 'R', '1'};

static void load_save(void)
{
    save_read(0, &save, sizeof save);
    if (memcmp(save.magic, magic, sizeof magic) != 0) {
        memset(&save, 0, sizeof save);
        memcpy(save.magic, magic, sizeof magic);
    }
}

/* --- play ------------------------------------------------------------------ */

/* Positions in 16ths of a pixel, for the things that move at odd speeds. */
#define MAX_ENEMIES 10
#define MAX_BULLETS 14
#define MAX_SHOTS 6
#define MAX_BOOMS 6
enum { DRIFTER, DIVER, ORB };

static int ship_x, ship_y; /* pixels */
static unsigned char lives, invulnerable, respawn, fire_wait, round;
static unsigned score; /* in tens */
static unsigned char game_over;

static int shot_x[MAX_SHOTS], shot_y[MAX_SHOTS];
static unsigned char shot_on[MAX_SHOTS];

static int e_x[MAX_ENEMIES], e_y[MAX_ENEMIES], e_vx[MAX_ENEMIES], e_vy[MAX_ENEMIES];
static int e_base[MAX_ENEMIES];
static unsigned char e_type[MAX_ENEMIES], e_time[MAX_ENEMIES], e_hp[MAX_ENEMIES], e_on[MAX_ENEMIES];

static int b_x[MAX_BULLETS], b_y[MAX_BULLETS], b_vx[MAX_BULLETS], b_vy[MAX_BULLETS];
static unsigned char b_on[MAX_BULLETS];

static int boom_x[MAX_BOOMS], boom_y[MAX_BOOMS];
static unsigned char boom_t[MAX_BOOMS];

/* 24 sin(2 pi t / 64), a quarter of it. */
static const unsigned char quarter[17] = {0, 2, 5, 7, 9, 11, 13, 15, 17, 19, 20, 21, 22, 23, 24, 24, 24};

static int weave(unsigned char t)
{
    t &= 63;
    if (t < 16) {
        return quarter[t];
    }
    if (t < 32) {
        return quarter[32 - t];
    }
    if (t < 48) {
        return -(int)quarter[t - 32];
    }
    return -(int)quarter[64 - t];
}

/* The waves: after `gap` frames, an enemy of `type` comes in at x = 2 x. */
typedef struct {
    unsigned char gap, type, x;
} Spawn;

static const Spawn script[] = {
    /* A line of drifters. */
    {60, DRIFTER, 40}, {20, DRIFTER, 60}, {20, DRIFTER, 80}, {20, DRIFTER, 100}, {20, DRIFTER, 120},
    /* Down the left, then the right. */
    {80, DRIFTER, 30}, {16, DRIFTER, 30}, {16, DRIFTER, 30}, {16, DRIFTER, 30},
    {40, DRIFTER, 130}, {16, DRIFTER, 130}, {16, DRIFTER, 130}, {16, DRIFTER, 130},
    /* Divers. */
    {80, DIVER, 50}, {30, DIVER, 110}, {30, DIVER, 80},
    /* An orb, drifters about it. */
    {90, ORB, 80}, {30, DRIFTER, 30}, {0, DRIFTER, 130}, {30, DRIFTER, 45}, {0, DRIFTER, 115},
    /* Divers in pairs. */
    {120, DIVER, 40}, {0, DIVER, 120}, {40, DIVER, 60}, {0, DIVER, 100}, {40, DIVER, 80},
    /* Two orbs. */
    {100, ORB, 45}, {20, ORB, 115},
    /* A rush. */
    {120, DRIFTER, 20}, {10, DRIFTER, 50}, {10, DRIFTER, 80}, {10, DRIFTER, 110}, {10, DRIFTER, 140},
    {30, DIVER, 30}, {12, DIVER, 130}, {12, DIVER, 80},
    {60, ORB, 80}, {40, DRIFTER, 30}, {16, DRIFTER, 130}, {16, DRIFTER, 30}, {16, DRIFTER, 130},
    {120, DIVER, 60}, {10, DIVER, 100}, {10, DIVER, 40}, {10, DIVER, 120},
};
#define SCRIPT_LENGTH (sizeof script / sizeof script[0])
static unsigned char next_spawn, spawn_wait;

static void hud_score(void)
{
    text_at(HUD_COLUMN + 6, 0, PAL_TEXT, digits(score, 5));
    text_at(HUD_COLUMN + 11, 0, PAL_TEXT, "0");
    if (score > save.best) {
        text_at(HUD_COLUMN + 17, 0, PAL_AMBER, digits(score, 5));
        text_at(HUD_COLUMN + 22, 0, PAL_AMBER, "0");
    }
}

static void hud_lives(void)
{
    text_at(HUD_COLUMN + 6, 1, PAL_TEXT, digits(lives, 1));
}

static void hud_round(void)
{
    text_at(HUD_COLUMN + 23, 1, PAL_TEXT, digits(round + 1, 1));
}

static void hud(void)
{
    text_at(HUD_COLUMN, 0, PAL_AMBER, "SCORE 000000  HI 000000 ");
    text_at(HUD_COLUMN + 17, 0, PAL_AMBER, digits(save.best, 5));
    text_at(HUD_COLUMN + 22, 0, PAL_AMBER, "0");
    text_at(HUD_COLUMN, 1, PAL_AMBER, "SHIPS            ROUND  ");
    hud_score();
    hud_lives();
    hud_round();
}

static void boom(int x, int y)
{
    unsigned char i;

    for (i = 0; i < MAX_BOOMS; ++i) {
        if (!boom_t[i]) {
            boom_x[i] = x;
            boom_y[i] = y;
            boom_t[i] = 18;
            return;
        }
    }
}

/* A bullet from (x, y), pixels, at 16ths of a pixel a frame. */
static void fire(int x, int y, int vx, int vy)
{
    unsigned char i;

    for (i = 0; i < MAX_BULLETS; ++i) {
        if (!b_on[i]) {
            b_on[i] = 1;
            b_x[i] = x << 4;
            b_y[i] = y << 4;
            b_vx[i] = vx;
            b_vy[i] = vy;
            return;
        }
    }
}

/* Toward the ship, at `speed` 16ths of a pixel a frame. */
static void aim(int x, int y, int speed)
{
    int dx = ship_x - x;
    int dy = ship_y - y;
    int m = abs(dx) > abs(dy) ? abs(dx) : abs(dy);

    if (m == 0) {
        m = 1;
    }
    fire(x, y, dx * speed / m, dy * speed / m);
}

static void spawn(unsigned char type, unsigned char x)
{
    unsigned char i;

    for (i = 0; i < MAX_ENEMIES && e_on[i]; ++i) {
    }
    if (i == MAX_ENEMIES) {
        return;
    }
    e_on[i] = 1;
    e_type[i] = type;
    e_time[i] = 0;
    e_base[i] = x * 2;
    e_x[i] = (x * 2) << 4;
    e_y[i] = -16 << 4;
    e_vx[i] = 0;
    e_vy[i] = type == DRIFTER ? 16 + round * 4 : type == DIVER ? 36 + round * 4 : 16;
    e_hp[i] = type == ORB ? 6 : 1;
}

static void spawns(void)
{
    const Spawn *s;

    if (spawn_wait) {
        --spawn_wait;
        return;
    }
    do {
        s = &script[next_spawn];
        spawn(s->type, s->x);
        if (++next_spawn == SCRIPT_LENGTH) {
            next_spawn = 0;
            ++round;
            hud_round();
        }
        s = &script[next_spawn];
        spawn_wait = s->gap > round * 8 + 6 ? s->gap - round * 8 : s->gap ? 6 : 0;
    } while (!spawn_wait);
}

static void lose_ship(void)
{
    boom(ship_x, ship_y);
    sfx_play(SFX_VOICE, &song_hit);
    --lives;
    hud_lives();
    if (!lives) {
        game_over = 1;
        return;
    }
    respawn = 60;
}

static unsigned char hits_ship(int x, int y, unsigned char size)
{
    /* The ship's small heart: 4x6 pixels at its middle. */
    return !respawn && !invulnerable && !game_over
        && x + 8 + size > ship_x + 6 && x + 8 - size < ship_x + 10
        && y + 8 + size > ship_y + 5 && y + 8 - size < ship_y + 11;
}

static void enemies(void)
{
    unsigned char i, j, t, frame;
    int x, y;

    for (i = 0; i < MAX_ENEMIES; ++i) {
        if (!e_on[i]) {
            continue;
        }
        t = ++e_time[i];
        switch (e_type[i]) {
        case DRIFTER:
            e_y[i] += e_vy[i];
            e_x[i] = (e_base[i] + weave(t >> 1)) << 4;
            if (round && (t & 63) == 32 && (rand() & 3) == 0) {
                aim(e_x[i] >> 4, e_y[i] >> 4, 24);
            }
            frame = t & 8 ? F_DRIFTER : F_DRIFTER2;
            break;
        case DIVER:
            if (t == 40) {
                /* Down, then at the ship. */
                x = ship_x - (e_x[i] >> 4);
                y = ship_y - (e_y[i] >> 4);
                if (y == 0) {
                    y = 1;
                }
                j = abs(x) > abs(y) ? 0 : 1;
                e_vx[i] = j ? x * 40 / abs(y) : (x < 0 ? -40 : 40);
                e_vy[i] = j ? 40 + round * 4 : y * 40 / abs(x);
                if (e_vy[i] < 16) {
                    e_vy[i] = 16;
                }
            } else if (t < 40 && e_vy[i] > 8) {
                e_vy[i] -= 1; /* slowing to a stop before the dive */
            }
            e_x[i] += e_vx[i];
            e_y[i] += e_vy[i];
            frame = t < 40 ? F_DIVER : F_DIVER2;
            break;
        default: /* ORB */
            if ((e_y[i] >> 4) < 56) {
                e_y[i] += e_vy[i];
            } else if (t > 250) {
                e_y[i] += 24; /* gone on */
            } else if (t % 60 == 0) {
                x = e_x[i] >> 4;
                y = e_y[i] >> 4;
                fire(x, y, 0, 28);
                fire(x, y, -14, 24);
                fire(x, y, 14, 24);
                if (round) {
                    fire(x, y, -26, 14);
                    fire(x, y, 26, 14);
                }
            }
            frame = t & 16 ? F_ORB : F_ORB2;
            break;
        }
        x = e_x[i] >> 4;
        y = e_y[i] >> 4;
        if (y > 250 || x < -24 || x > 330) {
            e_on[i] = 0;
            continue;
        }

        /* Shots. */
        for (j = 0; j < MAX_SHOTS; ++j) {
            if (shot_on[j] && shot_x[j] + 10 > x + 2 && shot_x[j] + 6 < x + 14
                && shot_y[j] + 12 > y + 2 && shot_y[j] + 3 < y + 12) {
                shot_on[j] = 0;
                if (!--e_hp[i]) {
                    e_on[i] = 0;
                    boom(x, y);
                    sfx_play(SFX_VOICE, &song_boom);
                    score += e_type[i] == ORB ? 5 : e_type[i] == DIVER ? 2 : 1;
                    hud_score();
                    break;
                }
            }
        }
        if (!e_on[i]) {
            continue;
        }
        if (hits_ship(x, y, 6)) {
            e_on[i] = 0;
            boom(x, y);
            lose_ship();
            continue;
        }
        sprite(x, y + (e_hp[i] < 6 && e_type[i] == ORB && (t & 2)), frame);
    }
}

static void bullets(void)
{
    unsigned char i;
    int x, y;

    for (i = 0; i < MAX_BULLETS; ++i) {
        if (!b_on[i]) {
            continue;
        }
        b_x[i] += b_vx[i];
        b_y[i] += b_vy[i];
        x = b_x[i] >> 4;
        y = b_y[i] >> 4;
        if (y > 244 || y < -16 || x < -16 || x > 324) {
            b_on[i] = 0;
            continue;
        }
        if (hits_ship(x, y, 2)) {
            b_on[i] = 0;
            lose_ship();
            continue;
        }
        sprite(x, y, F_BULLET);
    }
}

static void ship(void)
{
    unsigned char i, frame = frames & 4 ? F_SHIP : F_SHIP2;

    if (respawn) {
        if (!--respawn) {
            ship_x = 152;
            ship_y = 200;
            invulnerable = 120;
        }
        return;
    }
    if ((held & PAD_LEFT) && ship_x > 0) {
        ship_x -= 2;
        frame = F_LEFT;
    } else if ((held & PAD_RIGHT) && ship_x < 304) {
        ship_x += 2;
        frame = F_RIGHT;
    }
    if ((held & PAD_UP) && ship_y > HUD_LINES + 4) {
        ship_y -= 2;
    } else if ((held & PAD_DOWN) && ship_y < 222) {
        ship_y += 2;
    }
    if (fire_wait) {
        --fire_wait;
    } else if (held & (PAD_A | PAD_B)) {
        for (i = 0; i < MAX_SHOTS && shot_on[i]; ++i) {
        }
        if (i < MAX_SHOTS) {
            shot_on[i] = 1;
            shot_x[i] = ship_x;
            shot_y[i] = ship_y - 10;
            fire_wait = 7;
            sfx_play(SFX_VOICE, &song_shot);
        }
    }
    if (invulnerable) {
        --invulnerable;
    }
    if (!(invulnerable & 4)) {
        sprite(ship_x, ship_y, frame);
    }
}

static void shots(void)
{
    unsigned char i;

    for (i = 0; i < MAX_SHOTS; ++i) {
        if (shot_on[i]) {
            shot_y[i] -= 6;
            if (shot_y[i] < 0) {
                shot_on[i] = 0;
            } else {
                sprite(shot_x[i], shot_y[i], F_SHOT);
            }
        }
    }
}

static void booms(void)
{
    unsigned char i;

    for (i = 0; i < MAX_BOOMS; ++i) {
        if (boom_t[i]) {
            --boom_t[i];
            sprite(boom_x[i], boom_y[i], boom_t[i] > 12 ? F_BOOM1 : boom_t[i] > 6 ? F_BOOM2 : F_BOOM3);
        }
    }
}

/* Start pauses; Start again goes on. */
static void pause(void)
{
    unsigned char voice;

    music_stop();
    for (voice = 0; voice < 4; ++voice) {
        SOUND_CTRL(voice) = 0;
    }
    text_at(HUD_COLUMN + 8, 1, PAL_AMBER, " PAUSED ");
    do {
        next_frame();
    } while (!(pressed & PAD_START));
    text_at(HUD_COLUMN + 8, 1, PAL_AMBER, "        ");
    music_play(&song_flight);
}

static void play(void)
{
    unsigned char i, after;

    memset(e_on, 0, sizeof e_on);
    memset(b_on, 0, sizeof b_on);
    memset(shot_on, 0, sizeof shot_on);
    memset(boom_t, 0, sizeof boom_t);
    ship_x = 152;
    ship_y = 200;
    lives = 3;
    invulnerable = 90;
    respawn = 0;
    score = 0;
    round = 0;
    game_over = 0;
    next_spawn = 0;
    spawn_wait = script[0].gap;
    clouds_on();
    hud();
    split_on();
    music_play(&song_flight);

    for (after = 0; after < 150;) {
        next_frame();
        if (game_over) {
            if (after == 0) {
                music_play(&song_over);
                /* GAME OVER across the clouds, where row 14 of the screen is
                 * now; they stop to show it. */
                i = (cloud_y >> 3) + 14 & 31;
                text_at(15, i, PAL_AMBER, " GAME OVER ");
            }
            ++after;
        } else {
            if (pressed & PAD_START) {
                pause();
            }
            scroll_on();
            spawns();
        }
        booms();
        ship();
        shots();
        bullets();
        enemies();
    }
    split_off();
    if (score > save.best) {
        save.best = score;
        save_write(0, &save, sizeof save);
    }
}

/* --- the title ---------------------------------------------------------------- */

static void title(void)
{
    screen_start();
    text_at(12, 6, PAL_AMBER, " N I G H T J A R ");
    text_at(13, 10, PAL_TEXT, " HI SCORE ");
    text_at(23, 10, PAL_AMBER, digits(save.best, 5));
    text_at(28, 10, PAL_AMBER, "0 ");
    text_at(8, 15, PAL_TEXT, " D-pad fly   B or A fire ");
    text_at(14, 20, PAL_AMBER, " START: FLY ");
    music_play(&song_title);
    do {
        next_frame();
        if (frames & 1) {
            --sea_y;
        }
        sprite(152, 200 - (frames >> 5 & 1), frames & 4 ? F_SHIP : F_SHIP2);
    } while (!(pressed & (PAD_START | PAD_A | PAD_B)));
    srand(frames ^ (unsigned)SOUND_OSC3 << 8);
    music_stop();
}

int main(void)
{
    load_save();
    held = PAD | KEYPAD;
    for (;;) {
        title();
        /* The title's text off plane B; the clouds come on. */
        vram_fill(VRAM_MAP_B, 0, MAP_COLUMNS * MAP_ROWS * 2);
        play();
    }
}
