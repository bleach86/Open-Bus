/*
 * A level. The lamplighter runs (Y or X held) and jumps (B or A; held,
 * higher), as on the SNES, with a moment's grace to jump after leaving an
 * edge and a jump pressed just before landing kept for it. Embers are gathered by touching them,
 * lamps lit the same way, and the last lamp lit is where a fall or a third
 * hurt starts things again. Beetles walk to and fro and turn at edges;
 * moths flutter. Either is beaten by landing on it and hurts otherwise.
 *
 * The level is 128 cells across; plane B's map is 32 cells, so the cells
 * come onto it a column at a time as the camera moves, just ahead of what
 * shows. Positions are in 16ths of a pixel: the lamplighter's is his
 * feet's 16x16 cell-sized box, his head the 16 pixels above it.
 */
#include <stdlib.h>
#include <string.h>
#include "music.h"
#include "lamplight.h"

extern const Song song_dusk, song_clear;
extern const Song song_jump, song_ember, song_stomp, song_hurt, song_lamp, song_fall;
#define SFX_VOICE 3 /* the music leaves it free */
#define JUMP_BUTTONS (PAD_A | PAD_B)
#define RUN_BUTTONS (PAD_X | PAD_Y)

static unsigned char level[LEVEL_H][LEVEL_W];
#define SOLID(m) ((unsigned char)((m) - GRASS) <= STONE - GRASS)

/* Moving, the way the classic console platformers do it. Speeds are in
 * 16ths of a pixel a frame, each with a byte of 256ths under it, so speed
 * builds slowly (most of a second to a run) and slides off as slowly; turning
 * against it skids at twice the rate. The speed at take-off picks the jump:
 * how hard up, and the pull while the jump is held on the way up (light)
 * and otherwise (heavy), in 256ths of a pixel a frame each frame. */
#define WALK 24
#define RUN 40
#define ACCEL_WALK 0x98
#define ACCEL_RUN 0xE4
#define RELEASE 0xD0
#define FAST 25 /* a running jump from here */
#define JUMP 64
#define JUMP_FAST 80
#define BOUNCE 48
#define FALL_MAX 64
#define COYOTE 6
#define JUMP_BUFFER 6
#define HEARTS 3

/* The camera: the lamplighter kept between these, across the screen. */
#define CAMERA_LEFT 112
#define CAMERA_RIGHT 176
#define CAMERA_MAX (LEVEL_W * 16 - 320)

static unsigned x16, y16;
static int vx, vy;
static unsigned char vx_sub, vy_sub; /* 256ths, 16ths: under vx, vy */
static unsigned char rise_pull, fall_pull;
static unsigned char facing; /* 0, or SPRITE_FLIP_X: left */
static unsigned char on_ground;
static unsigned char coyote, jump_buffer;
static unsigned char hearts, invulnerable, dying;
static unsigned char run_anim;
static unsigned char skidding;
static unsigned camera;
static unsigned char start_x, start_y; /* the cell to start again at */
static unsigned char embers, embers_total, falls;
static unsigned seconds;
static unsigned char second_frames;
static unsigned char finished;

/* --- the level -------------------------------------------------------------- */

#define MAX_SPAWNS 24
enum { BEETLE, MOTH };
enum { WAITING, OUT, GONE }; /* a spawn: to come on screen, on it, beaten */
static unsigned char spawn_x[MAX_SPAWNS], spawn_y[MAX_SPAWNS];
static unsigned char spawn_type[MAX_SPAWNS], spawn_state[MAX_SPAWNS];
static unsigned char spawns;

static void load(unsigned char n)
{
    const char (*rows)[LEVEL_W + 1];
    unsigned char bank = BANK1;
    unsigned char r, c, m;
    unsigned char left, right;
    char ch;

    BANK1 = BANK_LEVELS;
    rows = level_rows[n];
    spawns = 0;
    embers_total = 0;
    for (r = 0; r < LEVEL_H; ++r) {
        for (c = 0; c < LEVEL_W; ++c) {
            ch = rows[r][c];
            m = AIR;
            switch (ch) {
            case '#':
                left = c == 0 || rows[r][c - 1] == '#';
                right = c == LEVEL_W - 1 || rows[r][c + 1] == '#';
                if (r == 0 || rows[r - 1][c] != '#') {
                    m = !left ? GRASS_L : !right ? GRASS_R : GRASS;
                } else {
                    m = !left ? DIRT_L : !right ? DIRT_R : DIRT;
                }
                break;
            case 'B': m = STONE; break;
            case '-': m = PLANK; break;
            case '^': m = SPIKES; break;
            case 'o':
                m = EMBER;
                ++embers_total;
                break;
            case 'L': m = LAMP; break;
            case 'D':
                m = DOOR;
                if (r) {
                    level[r - 1][c] = DOOR_TOP;
                }
                break;
            case ',': m = TUFT; break;
            case '*': m = FLOWER; break;
            case 'P':
                start_x = c;
                start_y = r;
                break;
            case 'b':
            case 'm':
                if (spawns < MAX_SPAWNS) {
                    spawn_x[spawns] = c;
                    spawn_y[spawns] = r;
                    spawn_type[spawns] = ch == 'b' ? BEETLE : MOTH;
                    spawn_state[spawns] = WAITING;
                    ++spawns;
                }
                break;
            }
            level[r][c] = m; /* a door's top, in the row above, is done */
        }
    }
    BANK1 = bank;
}

/* The cell at pixel (x, y): the level's two ends are walls, and above and
 * below it, nothing. */
static unsigned char cell(unsigned x, unsigned y)
{
    if (y >= LEVEL_H * 16) {
        return AIR;
    }
    if (x >= LEVEL_W * 16) {
        return STONE;
    }
    return level[y >> 4][x >> 4];
}

/* --- plane B ---------------------------------------------------------------- */

#define HUD_ROWS 2
static unsigned char drawn_lo, drawn_hi; /* the columns on plane B */

static void draw_cell(unsigned char c, unsigned char r)
{
    unsigned addr = VRAM_MAP_B + ((HUD_ROWS + r * 2) * MAP_COLUMNS + (c & 31) * 2) * 2;
    unsigned tile = TILE_LEVEL + level[r][c] * 4;

    VADDR = addr;
    VDATA = tile;
    VDATA = tile >> 8;
    VDATA = tile + 1;
    VDATA = (tile + 1) >> 8;
    VADDR = addr + MAP_COLUMNS * 2;
    VDATA = tile + 2;
    VDATA = (tile + 2) >> 8;
    VDATA = tile + 3;
    VDATA = (tile + 3) >> 8;
}

static void draw_column(unsigned char c)
{
    unsigned char r;

    for (r = 0; r < LEVEL_H; ++r) {
        draw_cell(c, r);
    }
}

/* The columns from just left of the camera to just right of the screen,
 * drawn: what came into range since the last frame. */
static void stream(void)
{
    unsigned char first = camera >> 4;
    unsigned char lo = first > 2 ? first - 2 : 0;
    unsigned char hi = first + 22 > LEVEL_W ? LEVEL_W : first + 22;

    while (drawn_hi < hi) {
        draw_column(drawn_hi++);
        if (drawn_hi - drawn_lo > 32) {
            ++drawn_lo;
        }
    }
    while (drawn_lo > lo) {
        draw_column(--drawn_lo);
        if (drawn_hi - drawn_lo > 32) {
            --drawn_hi;
        }
    }
}

static void redraw(void)
{
    drawn_lo = drawn_hi = camera >> 4;
    stream();
}

/* --- the HUD: plane B's top two rows -------------------------------------------- */

static char digits[8];

static void hud_hearts(void)
{
    unsigned char i;
    unsigned entry;

    VADDR = VRAM_MAP_B + 1 * 2;
    for (i = 0; i < HEARTS; ++i) {
        entry = MAP_ENTRY(TILE_HUD + (i < hearts ? 0 : 1), PAL_HUD);
        VDATA = entry;
        VDATA = entry >> 8;
    }
}

static void hud_embers(void)
{
    utoa(embers, digits, 10);
    strcat(digits, "/");
    utoa(embers_total, digits + strlen(digits), 10);
    strcat(digits, " ");
    text_at(7, 0, PAL_TEXT, digits);
}

static void hud_clock(void)
{
    text_at(34, 0, PAL_TEXT, clock_text(seconds));
}

static void hud_name(unsigned char n)
{
    text_at(14, 0, PAL_AMBER, "                   "); /* as wide as PAUSED's */
    text_at(14, 0, PAL_AMBER, level_names[n]);
}

static void hud(unsigned char n)
{
    unsigned char i;
    unsigned entry;

    for (i = 0; i < SCREEN_COLUMNS; ++i) {
        entry = MAP_ENTRY(TILE_HUD + 4, PAL_HUD);
        VADDR = VRAM_MAP_B + i * 2;
        VDATA = entry;
        VDATA = entry >> 8;
        entry = MAP_ENTRY(TILE_HUD + 3, PAL_HUD);
        VADDR = VRAM_MAP_B + (MAP_COLUMNS + i) * 2;
        VDATA = entry;
        VDATA = entry >> 8;
    }
    VADDR = VRAM_MAP_B + 5 * 2;
    VDATA = TILE_HUD + 2;
    VDATA = (TILE_HUD + 2) >> 8 | PAL_HUD << 4;
    hud_hearts();
    hud_embers();
    hud_clock();
    hud_name(n);
}

/* Embers and lit lamps flicker: palette 0's colours 11-13 take turns. */
static const unsigned flames[] = {RGB(15, 7, 2), RGB(15, 11, 4), RGB(15, 15, 10)};

static void flicker(void)
{
    unsigned char i, t;

    if (frames & 7) {
        return;
    }
    t = (frames >> 3) % 3;
    VADDR = VRAM_PALETTES + PAL_LEVEL * 32 + EMBER_COLOR * 2;
    for (i = 0; i < 3; ++i) {
        VDATA = flames[t];
        VDATA = flames[t] >> 8;
        t = t == 2 ? 0 : t + 1;
    }
}

/* --- effects: sparks and puffs ------------------------------------------------- */

#define EFFECTS 4
static unsigned fx_x[EFFECTS];
static unsigned char fx_y[EFFECTS], fx_time[EFFECTS], fx_frame[EFFECTS];

static void effect(unsigned x, unsigned char y, unsigned char frame)
{
    unsigned char i;

    for (i = 0; i < EFFECTS; ++i) {
        if (!fx_time[i]) {
            fx_x[i] = x;
            fx_y[i] = y;
            fx_frame[i] = frame;
            fx_time[i] = 16;
            return;
        }
    }
}

static void effects(void)
{
    unsigned char i;

    for (i = 0; i < EFFECTS; ++i) {
        if (fx_time[i]) {
            --fx_time[i];
            sprite(fx_x[i] - camera, fx_y[i] + HUD_LINES, fx_frame[i] + (fx_time[i] < 8), 0);
        }
    }
}

/* --- enemies ---------------------------------------------------------------- */

#define ACTIVE 6
static unsigned char e_spawn[ACTIVE]; /* NONE: a free slot */
#define NONE 0xFF
static unsigned e_x[ACTIVE];
static unsigned char e_y[ACTIVE], e_left[ACTIVE], e_time[ACTIVE];
static unsigned char fresh; /* spawns in view come out: a new start */

static unsigned char ground_at(unsigned x, unsigned y)
{
    unsigned char m = cell(x, y);

    return SOLID(m) || m == PLANK;
}

/* Spawns coming into view come out; enemies far out of it go back to wait. */
static void spawn(void)
{
    unsigned char i, j;
    unsigned x;

    for (i = 0; i < ACTIVE; ++i) {
        j = e_spawn[i];
        if (j != NONE && (e_x[i] + 96 < camera || e_x[i] > camera + 320 + 96)) {
            spawn_state[j] = WAITING;
            e_spawn[i] = NONE;
        }
    }
    for (j = 0; j < spawns; ++j) {
        if (spawn_state[j] != WAITING) {
            continue;
        }
        x = spawn_x[j] * 16;
        if (x + 48 < camera || x > camera + 320 + 32) {
            continue;
        }
        if (!fresh && x + 16 > camera && x < camera + 320) {
            continue; /* not out of thin air, in view */
        }
        for (i = 0; i < ACTIVE && e_spawn[i] != NONE; ++i) {
        }
        if (i == ACTIVE) {
            return;
        }
        e_spawn[i] = j;
        e_x[i] = x;
        e_y[i] = spawn_y[j] * 16;
        e_left[i] = 1;
        e_time[i] = 0;
        spawn_state[j] = OUT;
    }
}

static void stomp(unsigned char i)
{
    spawn_state[e_spawn[i]] = GONE;
    e_spawn[i] = NONE;
    effect(e_x[i], e_y[i], FRAME_PUFF1);
    vy = held & JUMP_BUTTONS ? -JUMP : -BOUNCE;
    rise_pull = 0x20;
    fall_pull = 0x70;
    sfx_play(SFX_VOICE, &song_stomp);
}

static void hurt(void);

static void enemies(void)
{
    unsigned char i, j, t, frame, top, bottom;
    unsigned x, px, py, base;

    px = x16 >> 4;
    py = y16 >> 4;
    for (i = 0; i < ACTIVE; ++i) {
        j = e_spawn[i];
        if (j == NONE) {
            continue;
        }
        t = ++e_time[i];
        if (spawn_type[j] == BEETLE) {
            if (t & 1) {
                x = e_left[i] ? e_x[i] - 1 : e_x[i] + 1;
                base = e_left[i] ? x + 1 : x + 14;
                if (SOLID(cell(base, e_y[i] + 8)) || cell(base, e_y[i] + 8) == SPIKES
                    || !ground_at(base, e_y[i] + 16)) {
                    e_left[i] ^= 1;
                } else {
                    e_x[i] = x;
                }
            }
            frame = t & 16 ? FRAME_BEETLE1 : FRAME_BEETLE2;
            top = 4;
            bottom = 16;
        } else {
            /* To and fro across 64 pixels, bobbing. */
            base = spawn_x[j] * 16 - 32;
            x = t < 128 ? t : 255 - t;
            e_x[i] = base + (x >> 1);
            e_left[i] = t >= 128;
            x = t & 32 ? 31 - (t & 31) : t & 31;
            e_y[i] = spawn_y[j] * 16 + (x >> 2);
            frame = t & 4 ? FRAME_MOTH1 : FRAME_MOTH2;
            top = 4;
            bottom = 13;
        }
        sprite(e_x[i] - camera, e_y[i] + HUD_LINES, frame, e_left[i] ? SPRITE_FLIP_X : 0);

        if (dying || finished) {
            continue;
        }
        if (px + 13 > e_x[i] + 2 && px + 3 < e_x[i] + 14 && py + 16 > e_y[i] + top && py < e_y[i] + bottom + 12) {
            if (vy > 0 && py + 16 < e_y[i] + top + 8) {
                stomp(i);
            } else {
                hurt();
            }
        }
    }
    fresh = 0;
}

/* --- the lamplighter ------------------------------------------------------- */

static void respawn(void)
{
    unsigned char i;

    x16 = start_x * 256;
    y16 = start_y * 256;
    vx = vy = 0;
    hearts = HEARTS;
    invulnerable = 60;
    dying = 0;
    for (i = 0; i < ACTIVE; ++i) {
        if (e_spawn[i] != NONE) {
            spawn_state[e_spawn[i]] = WAITING;
            e_spawn[i] = NONE;
        }
    }
    fresh = 1;
    camera = start_x * 16 > CAMERA_LEFT ? start_x * 16 - CAMERA_LEFT : 0;
    if (camera > CAMERA_MAX) {
        camera = CAMERA_MAX;
    }
    redraw();
    hud_hearts();
}

/* Down: a moment showing it, then the last lamp. */
static void fall(void)
{
    dying = 50;
    ++falls;
    vx = 0;
    vy = -JUMP / 2;
    sfx_play(SFX_VOICE, &song_fall);
}

static void hurt(void)
{
    if (invulnerable || dying) {
        return;
    }
    --hearts;
    hud_hearts();
    if (!hearts) {
        fall();
        return;
    }
    invulnerable = 90;
    vy = -48;
    vx = facing ? 32 : -32;
    sfx_play(SFX_VOICE, &song_hurt);
}

/* vx and vx_sub as one number: `amount` 256ths of a 16th, to the right
 * or left. */
static void push(unsigned char amount, unsigned char right)
{
    if (right) {
        if ((unsigned)vx_sub + amount > 255) {
            ++vx;
        }
        vx_sub += amount;
    } else {
        if (vx_sub < amount) {
            --vx;
        }
        vx_sub -= amount;
    }
}

static void control(void)
{
    unsigned char run = (held & RUN_BUTTONS) != 0;
    unsigned char accel = run ? ACCEL_RUN : ACCEL_WALK;
    unsigned char top = run ? RUN : WALK;
    unsigned char speed = vx < 0 ? -vx : vx;
    unsigned char right;
    unsigned char pull;
    signed char input = 0;

    if (held & PAD_LEFT) {
        input = -1;
        facing = SPRITE_FLIP_X;
    } else if (held & PAD_RIGHT) {
        input = 1;
        facing = 0;
    }
    skidding = 0;
    if (invulnerable > 75) {
        /* Knocked back: no say for a moment. */
    } else if (input) {
        right = input > 0;
        if (vx && (vx > 0) != right) {
            push(accel, right); /* against the speed */
            if (on_ground) {
                push(accel, right);
                skidding = 1;
            }
        } else if (speed < top) {
            push(accel, right);
        } else if (speed > top && on_ground) {
            push(RELEASE, !right); /* a run let go of: down to a walk */
        }
    } else if (on_ground && (vx || vx_sub)) {
        right = vx < 0;
        push(RELEASE, right);
        if (right ? vx >= 0 : vx < 0) {
            vx = 0; /* slid to a stop */
            vx_sub = 0;
        }
    }

    if (pressed & JUMP_BUTTONS) {
        jump_buffer = JUMP_BUFFER;
    }
    if (on_ground) {
        coyote = COYOTE;
        rise_pull = 0x20;
        fall_pull = 0x70;
    }
    if (jump_buffer && coyote) {
        speed = vx < 0 ? -vx : vx;
        vy = speed >= FAST ? -JUMP_FAST : -JUMP;
        vy_sub = 0;
        rise_pull = speed >= FAST ? 0x28 : speed >= 16 ? 0x1E : 0x20;
        fall_pull = speed >= FAST ? 0x90 : speed >= 16 ? 0x60 : 0x70;
        coyote = 0;
        jump_buffer = 0;
        sfx_play(SFX_VOICE, &song_jump);
    }
    if (jump_buffer) {
        --jump_buffer;
    }
    if (coyote && !on_ground) {
        --coyote;
    }
    pull = vy < 0 && (held & JUMP_BUTTONS) ? rise_pull : fall_pull;
    vy += pull >> 4;
    vy_sub += pull & 15;
    if (vy_sub >= 16) {
        vy_sub -= 16;
        ++vy;
    }
    if (vy > FALL_MAX) {
        vy = FALL_MAX;
    }
}

/* A wall at x, from the head down to the feet? */
static unsigned char wall(unsigned x)
{
    unsigned py = y16 >> 4;

    /* Above the level's top (py < 12), the head's cell is nothing. */
    return SOLID(cell(x, py - 12)) || SOLID(cell(x, py + 2)) || SOLID(cell(x, py + 15));
}

static void move_x(void)
{
    unsigned edge;

    if (vx < 0 && x16 < (unsigned)-vx) {
        x16 = 0;
        vx = 0;
    }
    x16 += vx;
    if (vx > 0) {
        edge = (x16 >> 4) + 12;
        if (wall(edge)) {
            x16 = ((edge & ~15) - 13) << 4;
            vx = 0;
        }
    } else if (vx < 0) {
        edge = (x16 >> 4) + 3;
        if (wall(edge)) {
            x16 = ((edge | 15) + 1 - 3) << 4;
            vx = 0;
        }
    }
}

static void move_y(void)
{
    unsigned was = y16 >> 4;
    unsigned left = (x16 >> 4) + 3;
    unsigned right = (x16 >> 4) + 12;
    unsigned probe;
    unsigned char a, b;

    if (vy < 0 && y16 < (unsigned)-vy) {
        y16 = 0; /* the top of the level: a ceiling */
        vy = 0;
    }
    y16 += vy;
    on_ground = 0;
    if (vy >= 0) {
        probe = (y16 >> 4) + 16;
        a = cell(left, probe);
        b = cell(right, probe);
        if (SOLID(a) || SOLID(b) || ((a == PLANK || b == PLANK) && was + 16 <= (probe & ~15))) {
            y16 = ((probe & ~15) - 16) << 4;
            vy = 0;
            on_ground = 1;
        }
    } else {
        probe = (y16 >> 4) - 12; /* the head */
        if (SOLID(cell(left, probe)) || SOLID(cell(right, probe))) {
            y16 = ((probe | 15) + 1 + 12) << 4;
            vy = 0;
        }
    }
}

/* What the lamplighter touches: embers, lamps, spikes, the door. */
static void touch(void)
{
    unsigned px = x16 >> 4;
    unsigned py = y16 >> 4;
    unsigned cx = px + 8, cy = py + 8;
    unsigned char m;

    if (py >= LEVEL_H * 16) {
        fall();
        return;
    }
    m = cell(cx, cy);
    if (m != EMBER && m != LAMP && py >= 12 && (cell(cx, py - 4) == EMBER || cell(cx, py - 4) == LAMP)) {
        cy = py - 4; /* what the head touches */
        m = cell(cx, cy);
    }
    if (m == EMBER) {
        level[cy >> 4][cx >> 4] = AIR;
        draw_cell(cx >> 4, cy >> 4);
        ++embers;
        hud_embers();
        effect(cx & ~15, cy & ~15, FRAME_SPARK1);
        sfx_play(SFX_VOICE, &song_ember);
    } else if (m == LAMP) {
        level[cy >> 4][cx >> 4] = LAMP_LIT;
        draw_cell(cx >> 4, cy >> 4);
        start_x = cx >> 4;
        start_y = cy >> 4;
        effect(cx & ~15, cy & ~15, FRAME_SPARK1);
        sfx_play(SFX_VOICE, &song_lamp);
    } else if ((m == DOOR || m == DOOR_TOP) && on_ground) {
        finished = 1;
        music_play(&song_clear);
        return;
    }
    if ((((py + 14) & 15) >= 8 && (cell(px + 4, py + 14) == SPIKES || cell(px + 11, py + 14) == SPIKES))) {
        hurt();
        if (!dying) {
            vy = -JUMP * 3 / 4;
        }
    }
}

static void follow(void)
{
    unsigned px = x16 >> 4;

    if (px > camera + CAMERA_RIGHT) {
        camera = px - CAMERA_RIGHT;
    } else if (px < camera + CAMERA_LEFT) {
        camera = px > CAMERA_LEFT ? px - CAMERA_LEFT : 0;
    }
    if (camera > CAMERA_MAX) {
        camera = CAMERA_MAX;
    }
}

static const unsigned char run_frames[] = {HERO_RUN1, HERO_RUN2, HERO_RUN3, HERO_RUN2};

static void draw_lamplighter(void)
{
    unsigned char frame;
    unsigned speed;
    unsigned tile;
    int x, y;

    if (dying || invulnerable > 75) {
        frame = HERO_HURT;
    } else if (!on_ground) {
        frame = vy < 0 ? HERO_JUMP : HERO_FALL;
    } else if (skidding) {
        frame = HERO_SKID;
    } else if (vx) {
        speed = vx < 0 ? -vx : vx;
        run_anim += speed >> 1;
        frame = run_frames[run_anim >> 6];
    } else {
        frame = HERO_IDLE;
    }
    if (invulnerable && (invulnerable & 4) && !dying) {
        return; /* blinking */
    }
    tile = TILE_HERO + frame * 8;
    x = (x16 >> 4) - camera;
    y = (y16 >> 4) + HUD_LINES;
    sprite_tile(x, y - 16, tile, facing);
    sprite_tile(x, y, tile + 4, facing);
}

/* Start pauses; Start again goes on, Select goes back to the title. */
static unsigned char paused(unsigned char n)
{
    unsigned char voice;

    music_stop();
    for (voice = 0; voice < 4; ++voice) {
        SOUND_CTRL(voice) = 0;
    }
    text_at(14, 0, PAL_AMBER, "PAUSED  SELECT:QUIT");
    for (;;) {
        next_frame();
        if (pressed & PAD_START) {
            hud_name(n);
            music_play(&song_dusk);
            return 0;
        }
        if ((pressed & PAD_SELECT) && !(held & PAD_START)) {
            return 1; /* Select alone: with Start, it's the reset button */
        }
    }
}

unsigned char __fastcall__ play(unsigned char n, Result *result)
{
    unsigned char i;
    unsigned char after = 0;

    video_start();
    memset(level, AIR, sizeof level);
    load(n);
    for (i = 0; i < ACTIVE; ++i) {
        e_spawn[i] = NONE;
    }
    for (i = 0; i < EFFECTS; ++i) {
        fx_time[i] = 0;
    }
    embers = 0;
    falls = 0;
    seconds = 0;
    second_frames = 0;
    finished = 0;
    facing = 0;
    on_ground = 0;
    hearts = HEARTS;
    hud(n);
    respawn();
    music_play(&song_dusk);

    for (;;) {
        next_frame();
        if (finished) {
            /* Through the door; the tune, then the results. */
            if (++after == 100) {
                break;
            }
        } else {
            if (pressed & PAD_START) {
                if (paused(n)) {
                    video_stop();
                    music_stop();
                    return 0;
                }
            }
            if (++second_frames == 60) {
                second_frames = 0;
                ++seconds;
                hud_clock();
            }
        }

        if (dying) {
            vy += 0x70 >> 4; /* falling, heavily */
            y16 += vy;
            if (!--dying) {
                respawn();
            }
        } else if (!finished) {
            control();
            move_x();
            move_y();
            touch();
            if (invulnerable) {
                --invulnerable;
            }
        }

        effects();
        if (!finished) {
            draw_lamplighter();
        }
        spawn();
        enemies();
        follow();
        stream();
        flicker();
        scenery_at(camera);
        level_at(camera);
    }
    video_stop();
    result->embers = embers;
    result->total = embers_total;
    result->falls = falls;
    result->seconds = seconds;
    return 1;
}
