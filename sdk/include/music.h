/*
 * music.h: the SDK's music driver (sdk/lib/music.c). Songs are written as
 * text (music/NAME.song in a cart; docs/machine.md, "Music"), which cart.mk
 * turns into song_NAME:
 *
 *     extern const Song song_theme;
 *     music_play(&song_theme);
 *     for (;;) { bios_waitframe(); music_tick(); ... }
 *
 * A song plays on the voices it has parts for and leaves the others alone.
 * A sound effect is a song too, its voice 0 part played on any one voice
 * over the music: the music's part there goes on silently, and is heard
 * again when the effect ends.
 */
#ifndef MUSIC_H
#define MUSIC_H

typedef struct Song Song;

/* Start `song` from its beginning, in place of any song playing. */
void __fastcall__ music_play(const Song *song);
void music_stop(void);
/* Once a frame: the songs and effects move on. */
void music_tick(void);
/* `sfx`'s voice 0 part on voice `voice`, over the music. */
void __fastcall__ sfx_play(unsigned char voice, const Song *sfx);
/* The note voice `voice` is sounding, as its SOUND_FREQ; 0 if none. */
unsigned __fastcall__ music_note(unsigned char voice);

#endif
