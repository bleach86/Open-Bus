/*
 * The screen. Plane A holds the scenery in three bands, each scrolling at
 * its own speed (parallax): the sky with its clouds and sun, the far hills,
 * and the near hills with their pines. Plane B holds the HUD along the top,
 * unscrolled, and under it the level. The interrupt that makes the bands
 * comes at vblank and then at three lines down the screen, each setting the
 * line after: the video chip arms the raster line again whenever it is set.
 *
 * Sprites go through a list in RAM, to VRAM in vblank all together, so that
 * they move with the scroll and never halfway through a frame.
 */
#include <6502.h>
#include <string.h>
#include "lamplight.h"

/* In bank BANK_ART (ART_SEGMENT). */
extern const unsigned char art_level[], art_level_palette[];
extern const unsigned char art_sprites[], art_sprites_palette[];
extern const unsigned char art_hero[];
extern const unsigned char art_hud[], art_hud_palette[];
/* Tile maps (art/map), in banks BANK_SKY and BANK_NEAR. */
extern const unsigned char art_sky[], art_sky_palette[], art_far[], art_near[];
extern const unsigned art_sky_count, art_sky_map[], art_far_count, art_far_map[];
extern const unsigned art_near_count, art_near_map[];

/* Text: the font's colour 1, on colour 2 (the HUD's dark). */
static const unsigned char text_palettes[] = {PAL_TEXT, PAL_AMBER, PAL_DIM};
static const unsigned text_colors[] = {RGB(15, 14, 15), RGB(15, 11, 4), RGB(7, 6, 9)};
#define TEXT_BACKGROUND RGB(1, 1, 3)

/* Map `count` rows of a tile map `map` (64 cells a row) onto plane A from
 * `row`, its tiles at `tile`. */
static void band(const unsigned *map, unsigned char row, unsigned char count, unsigned tile)
{
    unsigned i;
    unsigned entry;

    VADDR = VRAM_MAP + row * MAP_COLUMNS * 2;
    for (i = 0; i < count * MAP_COLUMNS; ++i) {
        entry = map[i] + tile + MAP_ENTRY(0, PAL_SCENERY);
        VDATA = entry;
        VDATA = entry >> 8;
    }
}

static unsigned char irq_stack[96];
static unsigned char irq_stage;
static unsigned split_b, split_sky, split_far, split_near;
/* As the interrupt reads them: set together, with interrupts off. */
static unsigned next_b, next_sky, next_far, next_near;

static unsigned char split(void)
{
    unsigned char status = VSTATUS;

    if (status & VIRQ_VBLANK) {
        SCROLLX2 = 0; /* the HUD */
        SCROLLX = split_sky;
        irq_stage = 0;
        RASTER = HUD_LINES;
        VSTATUS = VIRQ_VBLANK;
    }
    if (status & VIRQ_RASTER) {
        if (irq_stage == 0) {
            SCROLLX2 = split_b;
            RASTER = FAR_LINE;
        } else if (irq_stage == 1) {
            SCROLLX = split_far;
            RASTER = NEAR_LINE;
        } else {
            SCROLLX = split_near;
        }
        ++irq_stage;
        VSTATUS = VIRQ_RASTER;
    }
    return IRQ_HANDLED;
}

void video_start(void)
{
    unsigned char bank = BANK1;
    unsigned char i;

    VIRQ = 0;
    bios_videoreset();
    VCTRL = 0;
    vram_fill(VRAM_TILES, 0, 32); /* tile 0: nothing, for empty cells */

    BANK1 = BANK_ART;
    vram_write(VRAM_TILES + TILE_HERO * 32, art_hero, 8 * 8 * 32);
    vram_write(VRAM_TILES + TILE_SPRITES * 32, art_sprites, 9 * 4 * 32);
    vram_write(VRAM_TILES + TILE_LEVEL * 32, art_level, 17 * 4 * 32);
    vram_write(VRAM_TILES + TILE_HUD * 32, art_hud, 5 * 32);
    vram_write(VRAM_PALETTES + PAL_LEVEL * 32, art_level_palette, 32);
    vram_write(VRAM_PALETTES + PAL_SPRITES * 32, art_sprites_palette, 32);
    vram_write(VRAM_PALETTES + PAL_HUD * 32, art_hud_palette, 32);
    BANK1 = BANK_SKY;
    vram_write(VRAM_TILES + TILE_SKY * 32, art_sky, art_sky_count * 32);
    vram_write(VRAM_TILES + TILE_FAR * 32, art_far, art_far_count * 32);
    vram_write(VRAM_PALETTES + PAL_SCENERY * 32, art_sky_palette, 32);
    vram_fill(VRAM_MAP, 0, 2 * MAP_COLUMNS * 2); /* behind the HUD */
    band(art_sky_map, 2, 12, TILE_SKY);
    band(art_far_map, 14, 6, TILE_FAR);
    BANK1 = BANK_NEAR;
    vram_write(VRAM_TILES + TILE_NEAR * 32, art_near, art_near_count * 32);
    band(art_near_map, 20, 10, TILE_NEAR);
    BANK1 = bank;
    vram_fill(VRAM_MAP + 30 * MAP_COLUMNS * 2, 0, 2 * MAP_COLUMNS * 2);

    for (i = 0; i < sizeof text_palettes; ++i) {
        VADDR = VRAM_PALETTES + text_palettes[i] * 32 + 2;
        VDATA = text_colors[i];
        VDATA = text_colors[i] >> 8;
        VDATA = TEXT_BACKGROUND & 0xFF;
        VDATA = TEXT_BACKGROUND >> 8;
    }
    BACKDROP = SKY_COLOR;
    clear_b();
    /* Sprites walk behind plane B: behind the tufts, the lamps, the door. */
    vram_fill(VRAM_PRIORITY, SPRITE_BEHIND_B, 64);

    scenery_at(0);
    level_at(0);
    present();
    set_irq(split, irq_stack, sizeof irq_stack);
    VSTATUS = VIRQ_VBLANK | VIRQ_RASTER;
    VIRQ = VIRQ_VBLANK | VIRQ_RASTER;
    VCTRL = VCTRL_DISPLAY | VCTRL_SPRITES | VCTRL_BIG | VCTRL_PLANE_B;
}

void video_stop(void)
{
    VIRQ = 0;
    reset_irq();
}

void scenery_at(unsigned x)
{
    next_sky = x >> 3;
    next_far = x >> 2;
    next_near = x >> 1;
}

void level_at(unsigned x)
{
    next_b = x & 511;
}

void clear_b(void)
{
    vram_fill(VRAM_MAP_B, 0, MAP_COLUMNS * MAP_ROWS * 2);
}

void panel(unsigned char column, unsigned char row, unsigned char width, unsigned char height)
{
    static char line[41];
    unsigned char y;

    memset(line, ' ', width);
    line[width] = '\0';
    for (y = 0; y < height; ++y) {
        line[0] = y == 0 ? CH_TOP_LEFT : y == height - 1 ? CH_BOTTOM_LEFT : CH_VLINE;
        line[width - 1] = y == 0 ? CH_TOP_RIGHT : y == height - 1 ? CH_BOTTOM_RIGHT : CH_VLINE;
        memset(line + 1, y == 0 || y == height - 1 ? CH_HLINE : ' ', width - 2);
        text_at(column, row + y, PAL_AMBER, line);
    }
}

/* --- sprites ------------------------------------------------------------ */

static unsigned char list[64 * 4];
static unsigned char listed;
static unsigned char shown;

void sprite_tile(int x, int y, unsigned tile, unsigned char attributes)
{
    unsigned char *s;

    if (x <= -16 || x >= 320 || y <= -16 || y >= 240 || listed == 64) {
        return;
    }
    if (y < 0) {
        return; /* Y doesn't wrap: off the top is hidden (the HUD is there) */
    }
    x &= 511; /* left of the edge: 496-511 */
    s = list + listed * 4;
    s[0] = y;
    s[1] = x;
    s[2] = tile;
    s[3] = attributes | SPRITE_PALETTE(PAL_SPRITES) | (x >> 8) | (tile >> 8 & 1) << 1;
    ++listed;
}

void present(void)
{
    unsigned char i;

    SEI();
    split_b = next_b;
    split_sky = next_sky;
    split_far = next_far;
    split_near = next_near;
    CLI();
    vram_write(VRAM_SPRITES, list, listed * 4);
    for (i = listed; i < shown; ++i) {
        VADDR = VRAM_SPRITES + i * 4;
        VDATA = SPRITE_HIDDEN;
    }
    shown = listed;
    listed = 0;
}
