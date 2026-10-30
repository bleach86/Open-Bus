/*
 * The machine test cart, shared between its banks.
 *
 * The program (main.c: the page switching, the text helpers and the video
 * page) runs from RAM: the SDK's startup copies it there from banks 1-3
 * (RAM = yes in the Makefile). Bank 4 holds the art (the PNGs in art/, put
 * there by ART_SEGMENT); bank 5 the input and sound page (sound.c); bank 6
 * the system page (system.c). The banked pages are called through the SDK's
 * bank_call, and call back into the program freely: RAM is always there.
 */
#ifndef TEST_H
#define TEST_H

#include "machine.h"

/* The pad this frame: buttons held, and those newly pressed. */
extern unsigned held;
extern unsigned pressed;
/* Frames since the cart started. */
extern unsigned frames;

/* Text on the map at (column, row), in a text colour's palette (the BIOS's
 * default palettes: colour n on black). */
void __fastcall__ text_at(unsigned char column, unsigned char row, unsigned char color, const char *text);
/* `n` right-aligned in `width` cells. */
void number_at(unsigned char column, unsigned char row, unsigned char color, unsigned n, unsigned char width);
/* The BIOS's screen back (font, palettes, sprites off, cleared, unscrolled)
 * and the page's title row. */
void plain_screen(const char *title);

#pragma wrapped-call (push, bank_call, bank)
/* sound.c, bank 5 */
void sound_show(void);
void sound_step(void);
unsigned char bank5_says(void);
/* system.c, bank 6 */
void system_boot(void);
void system_show(void);
void system_step(void);
#pragma wrapped-call (pop)

#endif
