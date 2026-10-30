/*
 * Sprout on screen: its room, the pet itself wandering, eating, sleeping
 * and sulking, and the care panel along the bottom (feed, play, clean, the
 * light, medicine, how it's doing). Left and right pick, A does, B backs
 * out. Play is a game of catching falling stars.
 */
#include <stdlib.h>
#include <string.h>
#include "music.h"
#include "sprout.h"

/* --- art (banks 4 and 5) --------------------------------------------------- */

extern const unsigned char art_room[], art_room_palette[];
extern const unsigned char art_egg[];
extern const unsigned char art_baby[], art_baby_palette[];
extern const unsigned char art_adult[];
extern const unsigned char art_items[], art_items_palette[];
extern const unsigned char art_icons[], art_icons_palette[];
#define ART_BANK 4
#define ADULT_BANK 5

/* Tiles above the BIOS's font; sprites' below $200. */
#define TILE_ROOM 0x100
#define TILE_EGG 0x110   /* 16x16 frames, 4 tiles each */
#define TILE_BABY 0x118  /* 32x32 frames, 16 tiles each: four 16x16 sprites */
#define TILE_ADULT 0x178
#define TILE_ITEMS 0x1D8
#define TILE_ICONS 0x1F8

/* Room tiles. */
#define WALL 0
#define STRIPE 1
#define SKIRTING 2
#define BOARD 3
#define BOARD2 4
#define WINDOW_CORNER 5
#define WINDOW_TOP 6
#define WINDOW_SIDE 7
#define GLASS 8
#define GLINT 9
#define SILL 10
#define RUG 11

/* Pet frames, and items. */
#define IDLE 0
#define IDLE2 1
#define EAT 2
#define HAPPY 3
#define SLEEP 4
#define SAD 5
#define EGG 0 /* the egg's own two */
#define EGG2 1
#define APPLE 0
#define CANDY 1
#define POOP 2
#define STAR 3
#define HEART 4
#define ZZZ 5
#define CLOUD 6
#define PILL 7

/* Palettes. */
#define P_ROOM 0
#define P_TEXT 1 /* the font: white on the panel */
#define P_DIM 2
#define P_WARN 3
#define P_ICONS 4
#define P_ICON_ON 5
#define P_PET 6
#define P_ITEMS 7
#define P_GOOD 8
#define P_LOW 9

/* Sprites. */
#define S_PET 0 /* to 3 */
#define S_ITEM 4
#define S_EMOTE 5
#define S_POOPS 6 /* to 9 */
#define S_STARS 10
#define STARS 6

static unsigned char day_palette[32];
static unsigned char night_palette[32];

static void load_art(void)
{
    unsigned char bank = BANK1;
    unsigned char i;
    unsigned colour;

    BANK1 = ART_BANK;
    vram_write(VRAM_TILES + TILE_ROOM * 32, art_room, 12 * 32);
    vram_write(VRAM_TILES + TILE_EGG * 32, art_egg, 2 * 4 * 32);
    vram_write(VRAM_TILES + TILE_BABY * 32, art_baby, 6 * 16 * 32);
    vram_write(VRAM_TILES + TILE_ITEMS * 32, art_items, 8 * 4 * 32);
    vram_write(VRAM_TILES + TILE_ICONS * 32, art_icons, 6 * 4 * 32);
    vram_write(VRAM_PALETTES + P_PET * 32, art_baby_palette, 32);
    vram_write(VRAM_PALETTES + P_ITEMS * 32, art_items_palette, 32);
    vram_write(VRAM_PALETTES + P_ICONS * 32, art_icons_palette, 32);
    vram_write(VRAM_PALETTES + P_ICON_ON * 32, art_icons_palette, 32);
    memcpy(day_palette, art_room_palette, 32);
    BANK1 = ADULT_BANK;
    vram_write(VRAM_TILES + TILE_ADULT * 32, art_adult, 6 * 16 * 32);
    BANK1 = bank;

    /* The room with the light off: a quarter as bright, a little blue. */
    for (i = 0; i < 32; i += 2) {
        colour = day_palette[i] | day_palette[i + 1] << 8;
        colour = ((colour >> 8 & 15) >> 2) << 8 | ((colour >> 4 & 15) >> 2) << 4 | (colour & 15) >> 1;
        night_palette[i] = colour;
        night_palette[i + 1] = colour >> 8;
    }
    /* The selected care icon lights up its square. */
    VADDR = VRAM_PALETTES + P_ICON_ON * 32 + 2 * 2;
    VDATA = RGB(5, 7, 10) & 0xFF;
    VDATA = RGB(5, 7, 10) >> 8;
}

static void palette_colours(unsigned char palette, unsigned letters, unsigned behind)
{
    VADDR = VRAM_PALETTES + palette * 32 + 2;
    VDATA = letters;
    VDATA = letters >> 8;
    VDATA = behind;
    VDATA = behind >> 8;
}

#define PANEL RGB(3, 4, 6) /* the care icons' colour 2 */

static void set_palettes(void)
{
    vram_write(VRAM_PALETTES + P_ROOM * 32, pet.dark ? night_palette : day_palette, 32);
    palette_colours(P_TEXT, RGB(15, 15, 15), PANEL);
    palette_colours(P_DIM, RGB(10, 11, 12), PANEL);
    palette_colours(P_WARN, RGB(15, 13, 5), PANEL);
    palette_colours(P_GOOD, RGB(5, 12, 6), PANEL);
    palette_colours(P_LOW, RGB(14, 5, 5), PANEL);
}

/* --- the map ---------------------------------------------------------------- */

static void cell(unsigned char column, unsigned char row)
{
    VADDR = VRAM_MAP + (row * MAP_COLUMNS + column) * 2;
}

static void put(unsigned entry)
{
    VDATA = entry;
    VDATA = entry >> 8;
}

static void fill(unsigned char column, unsigned char row, unsigned char count, unsigned entry)
{
    cell(column, row);
    while (count--) {
        put(entry);
    }
}

/* Text in the BIOS's font: a character's code is its tile. */
static unsigned char text_at(unsigned char column, unsigned char row, unsigned char palette, const char *text)
{
    cell(column, row);
    while (*text) {
        put(MAP_ENTRY((unsigned char)*text++, palette));
        ++column;
    }
    return column;
}

#define ROOM(tile) MAP_ENTRY(TILE_ROOM + (tile), P_ROOM)

#define FLOOR_ROW 16
#define PANEL_ROW 22

static void draw_room(void)
{
    unsigned char row;
    unsigned char column;

    for (row = 0; row < FLOOR_ROW - 1; ++row) {
        cell(0, row);
        for (column = 0; column < SCREEN_COLUMNS; ++column) {
            put(ROOM(column % 5 == 2 ? STRIPE : WALL));
        }
    }
    fill(0, FLOOR_ROW - 1, SCREEN_COLUMNS, ROOM(SKIRTING));
    for (row = FLOOR_ROW; row < PANEL_ROW; ++row) {
        cell(0, row);
        for (column = 0; column < SCREEN_COLUMNS; ++column) {
            put(ROOM(row & 1 ? BOARD2 : BOARD));
        }
    }
    for (row = FLOOR_ROW + 2; row < FLOOR_ROW + 5; ++row) {
        fill(12, row, 16, ROOM(RUG));
    }
    /* The window. */
    cell(26, 3);
    put(ROOM(WINDOW_CORNER));
    fill(27, 3, 4, ROOM(WINDOW_TOP));
    put(ROOM(WINDOW_CORNER) | MAP_FLIP_X);
    for (row = 4; row < 9; ++row) {
        cell(26, row);
        put(ROOM(WINDOW_SIDE));
        for (column = 27; column < 31; ++column) {
            put(ROOM(row == 4 && column == 27 ? GLINT : GLASS));
        }
        put(ROOM(WINDOW_SIDE) | MAP_FLIP_X);
    }
    fill(26, 9, 6, ROOM(SILL));
}

/* --- sprites ---------------------------------------------------------------- */

static void sprite(unsigned char n, unsigned x, unsigned char y, unsigned tile, unsigned char attributes)
{
    VADDR = VRAM_SPRITES + n * 4;
    VDATA = y;
    VDATA = x;
    VDATA = tile;
    VDATA = attributes | (x >> 8 & 1) | (tile >> 8 & 1) << 1;
}

static void hide(unsigned char n)
{
    sprite(n, 0, SPRITE_HIDDEN, 0, 0);
}

static void item(unsigned char n, unsigned x, unsigned char y, unsigned char which)
{
    sprite(n, x, y, TILE_ITEMS + which * 4, SPRITE_PALETTE(P_ITEMS));
}

/* --- sound ------------------------------------------------------------------ */

/* Voice 0 the panel's tick and a caught star; 1 the room's music, with the
 * jingles played over it (music/, through the SDK's music driver); 2 a
 * crunch of noise; voice 3 runs unheard on noise, for random numbers
 * (SOUND_OSC3). */
extern const Song song_room, song_hatched, song_grew, song_cheer, song_alert, song_sulk, song_wandered;

#define JINGLE_VOICE 1

static void jingle(const Song *song)
{
    sfx_play(JINGLE_VOICE, song);
}

/* The room's music plays while the light is on. */
static void room_music(void)
{
    if (pet.dark) {
        music_stop();
    } else {
        music_play(&song_room);
    }
}

static void sound_setup(void)
{
    SOUND_FREQ(0) = SOUND_HZ(1568);
    SOUND_AD(0) = 0x02;
    SOUND_SR(0) = 0x00;
    SOUND_VOLUME(0) = 8;
    SOUND_FREQ(2) = 0x7000;
    SOUND_AD(2) = 0x03;
    SOUND_SR(2) = 0x00;
    SOUND_VOLUME(2) = 10;
    SOUND_FREQ(3) = 0xFFFF;
    SOUND_CTRL(3) = SOUND_NOISE;
    SOUND_CUTOFF = 1200;
    SOUND_FILTER = 0x40 | 0x02; /* the tunes, softened */
    SOUND_MODE = SOUND_LOWPASS | SOUND_VOICE3_OFF | 15;
}

static void play(unsigned char voice, unsigned char wave)
{
    SOUND_CTRL(voice) = wave;
    SOUND_CTRL(voice) = wave | SOUND_GATE;
}

/* --- the panel -------------------------------------------------------------- */

#define ICON_ROW (PANEL_ROW + 1)
#define LABEL_ROW (PANEL_ROW + 4)
#define LINE_ROW (PANEL_ROW + 6)

#define DO_FEED 0
#define DO_PLAY 1
#define DO_CLEAN 2
#define DO_LIGHT 3
#define DO_MEDICINE 4
#define DO_STATUS 5
#define ACTIONS 6

static const char *const action_names[ACTIONS] = {"Feed", "Play", "Clean", "Light", "Medicine", "How it's doing"};
static unsigned char choice;

static unsigned frames;
static unsigned message_frames;

static void center(unsigned char row, unsigned char palette, const char *text)
{
    fill(0, row, SCREEN_COLUMNS, MAP_ENTRY(' ', P_TEXT));
    text_at((SCREEN_COLUMNS - strlen(text)) / 2, row, palette, text);
}

static void draw_icon(unsigned char i)
{
    unsigned tile = TILE_ICONS + i * 4;
    unsigned char palette = i == choice ? P_ICON_ON : P_ICONS;
    unsigned char column = 3 + i * 6;

    cell(column, ICON_ROW);
    put(MAP_ENTRY(tile, palette));
    put(MAP_ENTRY(tile + 1, palette));
    cell(column, ICON_ROW + 1);
    put(MAP_ENTRY(tile + 2, palette));
    put(MAP_ENTRY(tile + 3, palette));
}

static void draw_line(void);

static void draw_panel(void)
{
    unsigned char row;
    unsigned char i;

    fill(0, PANEL_ROW, SCREEN_COLUMNS, MAP_ENTRY(CH_DOUBLE_HLINE, P_DIM));
    for (row = PANEL_ROW + 1; row < MAP_ROWS; ++row) {
        fill(0, row, SCREEN_COLUMNS, MAP_ENTRY(' ', P_TEXT));
    }
    for (i = 0; i < ACTIONS; ++i) {
        draw_icon(i);
    }
    center(LABEL_ROW, P_TEXT, action_names[choice]);
    draw_line();
}

/* The bottom line: a message for a few seconds, or how the pet is. */
static void say(const char *text)
{
    center(LINE_ROW, P_WARN, text);
    message_frames = 180;
}

static void draw_line(void)
{
    static char text[40];
    static char digits[8];
    const char *mood;

    if (message_frames) {
        return;
    }
    if (pet.stage == STAGE_EGG) {
        strcpy(text, "An egg. Something moves inside.");
    } else {
        mood = pet.asleep ? "asleep" : pet.sick ? "ill" : life_needy() ? "unhappy" : pet.happiness > FULL / 3 * 2 ? "happy" : "fine";
        strcpy(text, pet.stage == STAGE_BABY ? "Baby, " : "Grown, ");
        strcat(text, utoa((unsigned)(pet.age / (24 * 60)), digits, 10));
        strcat(text, pet.age / (24 * 60) == 1 ? " day old, " : " days old, ");
        strcat(text, mood);
    }
    center(LINE_ROW, life_needy() ? P_WARN : P_DIM, text);
}

/* --- the pet on screen ------------------------------------------------------ */

#define DOING_NOTHING 0
#define EATING 1
#define CHEERING 2
#define REFUSING 3

static unsigned pet_x = 144;
static unsigned target_x = 144;
static unsigned char facing_left;
static unsigned char doing;
static unsigned char doing_frames;
static unsigned char eating;  /* the item */
static unsigned char wait;    /* frames before it wanders again */
static unsigned char was_needy;

static unsigned char pet_width(void)
{
    return pet.stage == STAGE_EGG ? 16 : 32;
}

static unsigned char pet_y(void)
{
    return pet.stage == STAGE_EGG ? 152 : 136;
}

static void show_pet(unsigned x, unsigned char frame)
{
    unsigned char y = pet_y();
    unsigned char flip = facing_left ? SPRITE_FLIP_X : 0;
    unsigned tile;
    unsigned char attributes = SPRITE_PALETTE(P_PET) | flip;

    if (pet.stage == STAGE_EGG) {
        sprite(S_PET, x, y, TILE_EGG + frame * 4, attributes);
        hide(S_PET + 1);
        hide(S_PET + 2);
        hide(S_PET + 3);
        return;
    }
    tile = (pet.stage == STAGE_ADULT ? TILE_ADULT : TILE_BABY) + frame * 16;
    /* Flipped, the right-hand quarters go on the left. */
    sprite(S_PET, x + (flip ? 16 : 0), y, tile, attributes);
    sprite(S_PET + 1, x + (flip ? 0 : 16), y, tile + 4, attributes);
    sprite(S_PET + 2, x + (flip ? 16 : 0), y + 16, tile + 8, attributes);
    sprite(S_PET + 3, x + (flip ? 0 : 16), y + 16, tile + 12, attributes);
}

static const unsigned poop_x[4] = {40, 84, 236, 272};

static void show_poops(void)
{
    unsigned char i;

    for (i = 0; i < 4; ++i) {
        if (i < pet.poops) {
            item(S_POOPS + i, poop_x[i], 154, POOP);
        } else {
            hide(S_POOPS + i);
        }
    }
}

static void start(unsigned char what, unsigned char frames_long)
{
    doing = what;
    doing_frames = frames_long;
}

static void step_pet(void)
{
    unsigned char frame;
    unsigned x = pet_x;
    unsigned char y = pet_y();
    unsigned char width = pet_width();
    unsigned char bob = frames >> 5 & 1;

    if (pet.stage == STAGE_EGG) {
        show_pet(152, frames >> 4 & 1 ? EGG2 : EGG);
        hide(S_EMOTE);
        hide(S_ITEM);
        return;
    }
    if (doing && --doing_frames == 0) {
        doing = DOING_NOTHING;
    }
    hide(S_ITEM);
    if (pet.asleep) {
        frame = SLEEP;
        item(S_EMOTE, x + width, y - 16 - (frames >> 3 & 7), ZZZ);
    } else if (doing == EATING) {
        frame = doing_frames >> 3 & 1 ? EAT : IDLE;
        item(S_ITEM, facing_left ? x - 14 : x + width - 2, y + width - 16, eating);
        if ((doing_frames & 15) == 0) {
            play(2, SOUND_NOISE);
        }
        hide(S_EMOTE);
    } else if (doing == CHEERING) {
        frame = HAPPY;
        item(S_EMOTE, x + width / 2 - 8, y - 16 - (60 - doing_frames) / 3, HEART);
    } else if (doing == REFUSING) {
        frame = SAD;
        x += doing_frames & 4 ? 2 : 0;
        hide(S_EMOTE);
    } else {
        /* Wander the floor. */
        if (pet_x != target_x) {
            if (frames & 1) {
                facing_left = target_x < pet_x;
                pet_x += facing_left ? -1 : 1;
            }
        } else if (wait) {
            --wait;
        } else {
            target_x = 24 + rand() % (320 - 48 - width);
            wait = 60 + rand() % 180;
        }
        x = pet_x;
        frame = life_needy() ? SAD : bob ? IDLE2 : IDLE;
        if (pet.sick) {
            item(S_EMOTE, x + width / 2 - 8, y - 18, CLOUD);
        } else {
            hide(S_EMOTE);
        }
    }
    show_pet(x, frame);
}

/* --- caring ----------------------------------------------------------------- */

static unsigned char feeding;   /* the meal-or-snack question is up */
static unsigned char snack;     /* and its answer so far */

static void ask_food(void)
{
    center(LABEL_ROW, P_TEXT, snack ? "  Meal  [Snack] " : " [Meal]  Snack  ");
}

static void give(unsigned *stat, unsigned by)
{
    *stat = FULL - *stat < by ? FULL : *stat + by;
}

static void take(unsigned *stat, unsigned by)
{
    *stat = *stat > by ? *stat - by : 0;
}

static void refuse(const char *why)
{
    start(REFUSING, 30);
    jingle(&song_sulk);
    say(why);
}

static void eat(void)
{
    feeding = 0;
    center(LABEL_ROW, P_TEXT, action_names[choice]);
    if (!snack && pet.fullness > FULL / 10 * 9) {
        refuse("It's full.");
        return;
    }
    if (snack) {
        give(&pet.happiness, 1500);
        give(&pet.fullness, 500);
        eating = CANDY;
    } else {
        give(&pet.fullness, 3000);
        give(&pet.happiness, 200);
        eating = APPLE;
    }
    start(EATING, 96);
    life_keep();
}

static void play_game(void);
static void show_status(void);

static void act(void)
{
    if (choice == DO_STATUS) {
        show_status();
        return;
    }
    if (pet.stage == STAGE_EGG && choice != DO_LIGHT) {
        say("It's still an egg.");
        return;
    }
    if (pet.asleep && choice != DO_LIGHT && choice != DO_CLEAN) {
        say("Shh, it's asleep.");
        return;
    }
    switch (choice) {
    case DO_FEED:
        feeding = 1;
        ask_food();
        return;
    case DO_PLAY:
        if (pet.energy < FULL / 6) {
            refuse("Too tired to play.");
        } else {
            play_game();
        }
        return;
    case DO_CLEAN:
        if (pet.poops == 0) {
            say("Nothing to clean.");
            return;
        }
        pet.poops = 0;
        give(&pet.happiness, 300);
        play(2, SOUND_NOISE);
        show_poops();
        if (!pet.asleep) {
            start(CHEERING, 60);
            jingle(&song_cheer);
        }
        say("All clean.");
        break;
    case DO_LIGHT:
        pet.dark = !pet.dark;
        if (pet.stage != STAGE_EGG) {
            if (pet.dark) {
                pet.asleep = 1;
            } else if (pet.asleep) {
                pet.asleep = 0;
                if (pet.energy < FULL / 2) {
                    take(&pet.happiness, 1000);
                    refuse("Woken up too soon!");
                }
            }
        }
        set_palettes();
        room_music();
        break;
    case DO_MEDICINE:
        if (!pet.sick) {
            refuse("It isn't ill.");
            return;
        }
        pet.sick = 0;
        give(&pet.health, 3000);
        eating = PILL;
        start(EATING, 48);
        say("That's better.");
        break;
    }
    life_keep();
    draw_line();
}

/* --- how it's doing --------------------------------------------------------- */

static void bar_at(unsigned char row, const char *name, unsigned value)
{
    unsigned char i;
    unsigned char filled = (value + FULL / 20) / (FULL / 10);

    text_at(8, row, P_TEXT, name);
    cell(18, row);
    for (i = 0; i < 10; ++i) {
        put(i < filled ? MAP_ENTRY(CH_BLOCK, value < FULL / 4 ? P_LOW : P_GOOD) : MAP_ENTRY(CH_SHADE_LIGHT, P_DIM));
    }
}

static void show_status(void)
{
    static char digits[8];
    unsigned char row;
    unsigned char i;

    for (i = 0; i < 10; ++i) {
        hide(i);
    }
    for (row = 2; row < 18; ++row) {
        fill(6, row, 28, MAP_ENTRY(' ', P_TEXT));
    }
    text_at(17, 3, P_WARN, "SPROUT");
    if (pet.stage == STAGE_EGG) {
        text_at(8, 7, P_TEXT, "Still an egg.");
    } else {
        text_at(8, 5, P_TEXT, "Age");
        text_at(18, 5, P_TEXT, utoa((unsigned)(pet.age / 60), digits, 10));
        text_at(18 + strlen(digits), 5, P_TEXT, pet.age / 60 == 1 ? " hour" : " hours");
        bar_at(7, "Full", pet.fullness);
        bar_at(9, "Happy", pet.happiness);
        bar_at(11, "Energy", pet.energy);
        bar_at(13, "Health", pet.health);
    }
    text_at(14, 16, P_DIM, "B to go back");
    while (!(PAD & PAD_B) && bios_getkey() != KEY_ESCAPE) {
        bios_waitframe();
        music_tick();
    }
    while (PAD & PAD_B) {
        bios_waitframe();
    }
    draw_room();
    show_poops();
}

/* --- catch the stars -------------------------------------------------------- */

#define GAME_SECONDS 30

static void play_game(void)
{
    static char text[40];
    static char digits[8];
    unsigned star_x[STARS];
    unsigned char star_y[STARS];
    unsigned char star_speed[STARS];
    unsigned char i;
    unsigned char caught = 0;
    unsigned frames_left = GAME_SECONDS * 60;
    unsigned char y = pet_y();
    unsigned char width = pet_width();
    unsigned pad;

    for (i = 0; i < STARS; ++i) {
        star_y[i] = SPRITE_HIDDEN;
    }
    hide(S_EMOTE);
    hide(S_ITEM);
    for (i = 0; i < 4; ++i) {
        hide(S_POOPS + i);
    }
    center(LABEL_ROW, P_TEXT, "Catch the stars! B stops");
    while (frames_left--) {
        bios_waitframe();
        ++frames;
        music_tick();
        pad = PAD;
        if (pad & PAD_B) {
            break;
        }
        if ((pad & PAD_LEFT) && pet_x > 4) {
            pet_x -= 3;
            facing_left = 1;
        }
        if ((pad & PAD_RIGHT) && pet_x < 320 - 4 - width) {
            pet_x += 3;
            facing_left = 0;
        }
        show_pet(pet_x, frames >> 3 & 1 ? HAPPY : IDLE);
        for (i = 0; i < STARS; ++i) {
            if (star_y[i] == SPRITE_HIDDEN) {
                if (rand() % 40 == 0) {
                    star_x[i] = 8 + rand() % (320 - 32);
                    star_y[i] = 0;
                    star_speed[i] = 1 + rand() % 2;
                }
                continue;
            }
            star_y[i] += star_speed[i];
            if (star_y[i] + 12 >= y && star_y[i] < y + width && star_x[i] + 12 > pet_x && star_x[i] < pet_x + width - 4) {
                ++caught;
                play(0, SOUND_TRIANGLE);
                star_y[i] = SPRITE_HIDDEN;
            } else if (star_y[i] > 200) {
                star_y[i] = SPRITE_HIDDEN;
            }
            if (star_y[i] == SPRITE_HIDDEN) {
                hide(S_STARS + i);
            } else {
                item(S_STARS + i, star_x[i], star_y[i], STAR);
            }
        }
        if (frames_left % 60 == 0) {
            strcpy(text, "Stars ");
            strcat(text, utoa(caught, digits, 10));
            strcat(text, "    Time ");
            strcat(text, utoa(frames_left / 60, digits, 10));
            center(LINE_ROW, P_TEXT, text);
            life_second();
        }
    }
    for (i = 0; i < STARS; ++i) {
        hide(S_STARS + i);
    }
    give(&pet.happiness, caught * 400u);
    take(&pet.energy, 1000);
    life_keep();
    center(LABEL_ROW, P_TEXT, action_names[choice]);
    strcpy(text, "Caught ");
    strcat(text, utoa(caught, digits, 10));
    strcat(text, caught == 1 ? " star!" : " stars!");
    say(text);
    start(CHEERING, 60);
    jingle(&song_cheer);
    show_poops();
}

/* --- the loop --------------------------------------------------------------- */

static void draw(void)
{
    VCTRL = 0;
    SCROLLX = 0;
    SCROLLY = 0;
    set_palettes();
    draw_room();
    draw_panel();
    show_poops();
    VCTRL = VCTRL_DISPLAY | VCTRL_SPRITES | VCTRL_BIG;
}

/* What the minutes lived since last time brought, told once. */
static void news(unsigned char stage_before)
{
    if (life_left) {
        life_left = 0;
        jingle(&song_wandered);
        say("Sprout wandered off. A new egg is here.");
    } else if (life_grew) {
        life_grew = 0;
        jingle(pet.stage == STAGE_BABY ? &song_hatched : &song_grew);
        say(pet.stage == STAGE_BABY ? "It hatched!" : "Sprout has grown up!");
        start(CHEERING, 60);
    }
    if (pet.stage != stage_before) {
        show_poops();
    }
    if (life_needy() && !was_needy && !message_frames) {
        jingle(&song_alert);
        say("Sprout needs you.");
    }
    was_needy = life_needy();
}

/* Seeding again from the moment of the first press: how long the player
 * took is the one thing no clock or noise register can know ahead. */
static void seed_from_player(void)
{
    unsigned char low = SOUND_OSC3;
    unsigned char high = SOUND_OSC3;

    srand(rand() ^ frames ^ (high << 8 | low));
}

int main(void)
{
    unsigned char seeded = 0;
    unsigned held = PAD;
    unsigned pad;
    unsigned pressed;
    unsigned char key;
    unsigned char stage;
    unsigned char i;

    for (i = 0; i < 64; ++i) {
        hide(i);
    }
    load_art();
    sound_setup();
    life_start();
    draw();
    room_music();
    if (life_away >= 60) {
        say("You were away a while.");
    }
    news(pet.stage);
    for (;;) {
        bios_waitframe();
        ++frames;
        music_tick();
        step_pet();
        if (message_frames && --message_frames == 0) {
            draw_line();
        }
        if (frames % 60 == 0) {
            stage = pet.stage;
            life_second();
            news(stage);
            show_poops();
            draw_line();
        }

        pad = PAD;
        pressed = pad & ~held;
        held = pad;
        key = bios_getkey();
        if (key == KEY_LEFT) {
            pressed |= PAD_LEFT;
        } else if (key == KEY_RIGHT) {
            pressed |= PAD_RIGHT;
        } else if (key == KEY_RETURN) {
            pressed |= PAD_A;
        } else if (key == KEY_ESCAPE) {
            pressed |= PAD_B;
        }
        if (pressed && !seeded) {
            seed_from_player();
            seeded = 1;
        }

        if (feeding) {
            if (pressed & (PAD_LEFT | PAD_RIGHT)) {
                snack = !snack;
                ask_food();
                play(0, SOUND_TRIANGLE);
            } else if (pressed & PAD_A) {
                eat();
            } else if (pressed & PAD_B) {
                feeding = 0;
                center(LABEL_ROW, P_TEXT, action_names[choice]);
            }
            continue;
        }
        if (pressed & (PAD_LEFT | PAD_RIGHT)) {
            i = choice;
            choice = pressed & PAD_LEFT ? (choice + ACTIONS - 1) % ACTIONS : (choice + 1) % ACTIONS;
            draw_icon(i);
            draw_icon(choice);
            center(LABEL_ROW, P_TEXT, action_names[choice]);
            play(0, SOUND_TRIANGLE);
        } else if (pressed & PAD_A) {
            act();
        }
    }
}
