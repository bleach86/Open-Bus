/*
 * Bank 5: the input and sound page. Every button shows lit while held. A, B,
 * X and Y each play a voice for as long as they are held, the sound chip's
 * envelope shaping it: a pulse lead (up and down change its note), a saw
 * bass through the low-pass filter (L2 and R2 sweep the cutoff), a triangle,
 * and a noise drum. Start plays a song on all four voices (music/demo.song,
 * through the SDK's music driver), showing the note each voice sounds.
 */
#include "music.h"
#include "test.h"

extern const Song song_demo;

#pragma code-name (push, "BANK5")
#pragma rodata-name (push, "BANK5")

static const char *const names[16] = {
    "A", "B", "X", "Y", "L1", "R1", "L2", "R2",
    "SELECT", "START", "L3", "R3", "UP", "DOWN", "LEFT", "RIGHT",
};
static const unsigned char name_column[16] = {
    2, 6, 10, 14, 18, 22, 26, 30,  /* A to R2 */
    2, 9, 15, 18, 21, 24, 29, 34,  /* SELECT to RIGHT, longer */
};
#define BUTTON_ROW 3

/* Each voice: the button that plays it, its waves and what it is. */
static const unsigned voice_button[4] = {PAD_A, PAD_B, PAD_X, PAD_Y};
static const unsigned char voice_wave[4] = {SOUND_PULSE_WAVE, SOUND_SAW, SOUND_TRIANGLE, SOUND_NOISE};
static const char *const voice_name[4] = {
    "A  pulse lead",
    "B  saw bass, filtered",
    "X  triangle",
    "Y  noise drum",
};
#define VOICE_ROW 9

/* C3 to B3 as SOUND_FREQ values; each octave up doubles them. */
static const unsigned octave3[12] = {
    2195, 2325, 2463, 2610, 2765, 2929, 3104, 3288, 3484, 3691, 3910, 4143,
};
static const char note_names[] = "C C#D D#E F F#G G#A A#B ";
#define LOWEST_OCTAVE 3
#define NOTES 36 /* C3 to B5 */

static unsigned shown;          /* the buttons drawn lit */
static unsigned char playing;   /* the song */
static unsigned char note = 12; /* C4 */
static unsigned cutoff = 600;

static void draw_button(unsigned char i)
{
    unsigned bit = 1u << i;

    text_at(name_column[i], BUTTON_ROW + (i >> 3) * 2, held & bit ? COLOR_WHITE : COLOR_DARKGREY, names[i]);
}

static void draw_lead(void)
{
    static char name[4];

    name[0] = note_names[note % 12 * 2];
    name[1] = note_names[note % 12 * 2 + 1];
    name[2] = '0' + LOWEST_OCTAVE + note / 12;
    name[3] = '\0';
    text_at(30, VOICE_ROW, COLOR_CYAN, name);
    SOUND_FREQ(0) = octave3[note % 12] << (note / 12);
}

/* A SOUND_FREQ value as a note's name, "C#5", or blanks for none. */
static const char *note_name(unsigned freq)
{
    static char name[5]; /* "C#5 ": the space clears a longer name before */
    char *at = name;
    unsigned char octave = LOWEST_OCTAVE;
    unsigned char i;
    unsigned char best = 0;
    unsigned off;
    unsigned best_off = 0xFFFF;

    if (freq == 0) {
        return "   ";
    }
    while (freq > octave3[11] + octave3[11] / 32) {
        freq >>= 1;
        ++octave;
    }
    while (freq < octave3[0] - octave3[0] / 32 && octave > 0) {
        freq <<= 1;
        --octave;
    }
    for (i = 0; i < 12; ++i) {
        off = freq > octave3[i] ? freq - octave3[i] : octave3[i] - freq;
        if (off < best_off) {
            best_off = off;
            best = i;
        }
    }
    *at++ = note_names[best * 2];
    if (note_names[best * 2 + 1] != ' ') {
        *at++ = note_names[best * 2 + 1];
    }
    *at++ = '0' + octave;
    *at++ = ' ';
    *at = '\0';
    return name;
}

static void voices_setup(void);

void sound_show(void)
{
    unsigned char i;

    text_at(1, BUTTON_ROW - 1, COLOR_GREY, "BUTTONS");
    for (i = 0; i < 16; ++i) {
        draw_button(i);
    }
    shown = held;
    text_at(1, VOICE_ROW - 2, COLOR_GREY, "SOUND");
    for (i = 0; i < 4; ++i) {
        text_at(2, VOICE_ROW + i, COLOR_DARKGREY, voice_name[i]);
    }
    text_at(2, VOICE_ROW + 5, COLOR_DARKGREY, "UP DOWN  lead note");
    text_at(2, VOICE_ROW + 6, COLOR_DARKGREY, "L2 R2    filter cutoff");
    text_at(2, VOICE_ROW + 7, COLOR_DARKGREY, "START    play a song");
    playing = 0;
    voices_setup();
}

static void voices_setup(void)
{
    /* The lead: a thin pulse that holds. */
    SOUND_PULSE(0) = 1024;
    SOUND_AD(0) = 0x16;
    SOUND_SR(0) = 0xA6;
    SOUND_VOLUME(0) = 11;
    /* The bass: a low C, a little pluck, through the filter. */
    SOUND_FREQ(1) = SOUND_HZ(65);
    SOUND_AD(1) = 0x05;
    SOUND_SR(1) = 0x84;
    SOUND_VOLUME(1) = 14;
    /* The triangle: a soft C5 that rings on. */
    SOUND_FREQ(2) = SOUND_HZ(523);
    SOUND_AD(2) = 0x29;
    SOUND_SR(2) = 0x89;
    SOUND_VOLUME(2) = 12;
    /* The drum: a burst of noise that dies away by itself. */
    SOUND_FREQ(3) = 0x5000;
    SOUND_AD(3) = 0x05;
    SOUND_SR(3) = 0x00;
    SOUND_VOLUME(3) = 12;
    SOUND_FILTER = 0x80 | 0x02; /* some resonance; the bass */
    SOUND_MODE = SOUND_LOWPASS | 15;
    SOUND_CUTOFF = cutoff;
    draw_lead();
    number_at(30, VOICE_ROW + 6, COLOR_CYAN, cutoff, 4);
    text_at(30, VOICE_ROW + 1, COLOR_CYAN, "   ");
    text_at(30, VOICE_ROW + 2, COLOR_CYAN, "   ");
    text_at(30, VOICE_ROW + 3, COLOR_CYAN, "   ");
}

void sound_step(void)
{
    unsigned char i;
    unsigned bit;
    unsigned changed = held ^ shown;

    for (i = 0; i < 16; ++i) {
        if (changed & 1u << i) {
            draw_button(i);
        }
    }
    shown = held;

    for (i = 0; i < 4; ++i) {
        bit = voice_button[i];
        if (changed & bit) {
            SOUND_CTRL(i) = voice_wave[i] | (held & bit ? SOUND_GATE : 0);
            text_at(2, VOICE_ROW + i, held & bit ? COLOR_WHITE : COLOR_DARKGREY, voice_name[i]);
        }
    }

    if (pressed & PAD_START) {
        playing = !playing;
        if (playing) {
            music_play(&song_demo);
        } else {
            music_stop();
            voices_setup();
        }
    }
    if (playing) {
        for (i = 0; i < 4; ++i) {
            text_at(30, VOICE_ROW + i, COLOR_CYAN, note_name(music_note(i)));
        }
    }

    if ((pressed & PAD_UP) && note < NOTES - 1) {
        ++note;
        draw_lead();
    }
    if ((pressed & PAD_DOWN) && note > 0) {
        --note;
        draw_lead();
    }
    if ((held & PAD_R2) && cutoff < 2047 - 16) {
        cutoff += 16;
    } else if ((held & PAD_L2) && cutoff >= 16) {
        cutoff -= 16;
    } else {
        return;
    }
    SOUND_CUTOFF = cutoff;
    number_at(30, VOICE_ROW + 6, COLOR_CYAN, cutoff, 4);
}

/* For the system page, in bank 6: a call across banks. */
unsigned char bank5_says(void)
{
    return 5;
}

#pragma rodata-name (pop)
#pragma code-name (pop)
