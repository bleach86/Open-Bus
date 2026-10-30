/*
 * Lamplight's title, its results and its save. The title lists the levels
 * with the best run of each; a level opens once the one before is done.
 * Save memory keeps, for each level, whether it is done, the most embers
 * gathered and the best time: `save`, as it is in RAM.
 */
#include <stdlib.h>
#include <string.h>
#include "music.h"
#include "lamplight.h"

extern const Song song_title;

unsigned held;
unsigned pressed;
unsigned char frames;

const char *const level_names[LEVELS] = {"Dusk Fields", "Old Quarry", "Lantern Hill"};

void next_frame(void)
{
    unsigned pad;

    bios_waitframe();
    present();
    ++frames;
    music_tick();
    pad = PAD | KEYPAD;
    pressed = pad & ~held;
    held = pad;
    bios_getkey(); /* keys as typed: not wanted here */
}

void __fastcall__ text_at(unsigned char column, unsigned char row, unsigned char palette, const char *text)
{
    VADDR = VRAM_MAP_B + (row * MAP_COLUMNS + column) * 2;
    while (*text) {
        VDATA = *text++;
        VDATA = palette << 4;
    }
}

const char *__fastcall__ clock_text(unsigned seconds)
{
    static char text[8];
    unsigned char length;

    utoa(seconds / 60, text, 10);
    length = strlen(text);
    text[length++] = ':';
    text[length++] = '0' + seconds % 60 / 10;
    text[length++] = '0' + seconds % 10;
    text[length] = '\0';
    return text;
}

/* --- the save ------------------------------------------------------------- */

static struct {
    char magic[4];
    unsigned char done[LEVELS];
    unsigned char best_embers[LEVELS];
    unsigned best_time[LEVELS]; /* seconds */
} save;
#define done save.done
#define best_embers save.best_embers
#define best_time save.best_time
static const char magic[4] = {'L', 'M', 'P', '1'};

static void load_save(void)
{
    save_read(0, &save, sizeof save);
    if (memcmp(save.magic, magic, sizeof magic) != 0) {
        memset(&save, 0, sizeof save); /* nothing kept yet */
        memcpy(save.magic, magic, sizeof magic);
    }
}

static void store_save(void)
{
    save_write(0, &save, sizeof save);
}

static unsigned char open(unsigned char n)
{
    return n == 0 || done[n - 1];
}

/* --- the title ------------------------------------------------------------ */

#define LIST_ROW 11
static unsigned drift;

static void lamp_at(unsigned char column, unsigned char row)
{
    unsigned tile = TILE_LEVEL + LAMP_LIT * 4;

    VADDR = VRAM_MAP_B + (row * MAP_COLUMNS + column) * 2;
    VDATA = tile;
    VDATA = tile >> 8;
    VDATA = tile + 1;
    VDATA = (tile + 1) >> 8;
    VADDR = VRAM_MAP_B + ((row + 1) * MAP_COLUMNS + column) * 2;
    VDATA = tile + 2;
    VDATA = (tile + 2) >> 8;
    VDATA = tile + 3;
    VDATA = (tile + 3) >> 8;
}

static void list_entry(unsigned char n, unsigned char chosen)
{
    static char text[12];
    unsigned char row = LIST_ROW + n * 2;
    unsigned char palette = open(n) ? PAL_TEXT : PAL_DIM;

    text_at(5, row, PAL_AMBER, chosen ? ">" : " ");
    text[0] = '1' + n;
    text[1] = '\0';
    text_at(7, row, palette, text);
    text_at(9, row, palette, level_names[n]);
    if (!open(n)) {
        text_at(23, row, PAL_DIM, "locked");
    } else if (done[n]) {
        /* The best: the HUD's ember and how many, and the time. */
        VADDR = VRAM_MAP_B + (row * MAP_COLUMNS + 23) * 2;
        VDATA = TILE_HUD + 2;
        VDATA = (TILE_HUD + 2) >> 8 | PAL_HUD << 4;
        utoa(best_embers[n], text, 10);
        text_at(24, row, PAL_AMBER, text);
        text_at(29, row, palette, clock_text(best_time[n]));
    } else {
        text_at(23, row, PAL_DIM, "not yet lit");
    }
}

/* Scenery drifting by behind the screen, a frame. */
static void drift_frame(void)
{
    next_frame();
    drift += 2;
    scenery_at(drift);
}

static unsigned char title(void)
{
    unsigned char choice = 0;
    unsigned char i;

    video_start();
    while (choice + 1 < LEVELS && done[choice] && open(choice + 1)) {
        ++choice; /* the first level not yet done */
    }
    panel(9, 3, 22, 5);
    text_at(11, 5, PAL_AMBER, "L A M P L I G H T");
    lamp_at(6, 4);
    lamp_at(32, 4);
    panel(3, 9, 34, 9);
    for (i = 0; i < LEVELS; ++i) {
        list_entry(i, i == choice);
    }
    panel(3, 19, 34, 4);
    text_at(5, 20, PAL_TEXT, "B/A jump  Y/X run  Start pause");
    text_at(5, 21, PAL_DIM, "Hold jump to go higher");
    text_at(12, 25, PAL_AMBER, " Start: light it ");
    music_play(&song_title);

    for (;;) {
        drift_frame();
        if (pressed & (PAD_UP | PAD_DOWN)) {
            i = choice;
            if (pressed & PAD_UP) {
                i = choice ? choice - 1 : choice;
            } else if (choice + 1 < LEVELS && open(choice + 1)) {
                i = choice + 1;
            }
            if (i != choice) {
                list_entry(choice, 0);
                choice = i;
                list_entry(choice, 1);
            }
        }
        if (pressed & (PAD_A | PAD_B | PAD_START)) {
            music_stop();
            video_stop();
            return choice;
        }
    }
}

/* --- the results ---------------------------------------------------------- */

static void results(unsigned char n, const Result *result)
{
    static char text[16];
    unsigned char best = 0;

    if (!done[n] || result->seconds < best_time[n]) {
        best_time[n] = result->seconds;
        best = 1;
    }
    if (result->embers > best_embers[n]) {
        best_embers[n] = result->embers;
        best = 1;
    }
    done[n] = 1;
    store_save();

    video_start();
    panel(8, 7, 24, 14);
    text_at(10, 9, PAL_AMBER, level_names[n]);
    text_at(10, 10, PAL_TEXT, "The lamps are lit.");
    utoa(result->embers, text, 10);
    strcat(text, " of ");
    utoa(result->total, text + strlen(text), 10);
    text_at(10, 12, PAL_DIM, "Embers");
    text_at(18, 12, PAL_TEXT, text);
    text_at(10, 13, PAL_DIM, "Time");
    text_at(18, 13, PAL_TEXT, clock_text(result->seconds));
    utoa(result->falls, text, 10);
    text_at(10, 14, PAL_DIM, "Falls");
    text_at(18, 14, PAL_TEXT, text);
    if (best) {
        text_at(10, 16, PAL_AMBER, "A best for this one!");
    }
    if (n + 1 < LEVELS) {
        text_at(10, 18, PAL_DIM, "Next:");
        text_at(16, 18, PAL_TEXT, level_names[n + 1]);
    } else {
        text_at(10, 18, PAL_TEXT, "Every lamp is lit.");
    }
    do {
        drift_frame();
    } while (!(pressed & (PAD_A | PAD_B | PAD_START)));
    music_stop();
    video_stop();
}

int main(void)
{
    unsigned char n;
    Result result;

    load_save();
    held = PAD | KEYPAD;
    for (;;) {
        n = title();
        if (play(n, &result)) {
            results(n, &result);
        }
    }
}
