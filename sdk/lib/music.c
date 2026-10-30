/*
 * The music driver (music.h). A song is a header (the four voices' streams,
 * the filter) and for each voice a stream of events, made by the SDK's
 * `song` from text: see sdk/song/src/lib.rs for the bytes. Each voice has
 * a music track and an effect track; the effect, while it lasts, is the
 * one heard.
 */
#include "machine.h"
#include "music.h"

struct Song {
    const unsigned char *voices[4];
    unsigned cutoff; /* $FFFF: leave the filter alone */
    unsigned char resonance;
    unsigned char kind;
};

#define END 0
#define JUMP 1
#define INSTRUMENT 2
#define REST 3
#define NOTE 4
#define HOLD 5

typedef struct {
    const unsigned char *at;         /* the next event; 0 when done */
    const unsigned char *instrument; /* the one in force */
    unsigned char frames;            /* left of the event playing */
    unsigned char waves;
    unsigned freq;                   /* the note sounding; 0 for none */
} Track;

static Track music[4];
static Track effects[4];

/* An instrument event's settings onto voice `voice`. */
static void set_instrument(unsigned char voice, const unsigned char *at)
{
    unsigned char bit = 1 << voice;

    SOUND_PULSE(voice) = at[2] | at[3] << 8;
    SOUND_AD(voice) = at[4];
    SOUND_SR(voice) = at[5];
    SOUND_VOLUME(voice) = at[6];
    SOUND_FILTER = at[7] ? SOUND_FILTER | bit : SOUND_FILTER & ~bit;
}

/* A frame of `track` on voice `voice`; `heard`, it plays on the chip. */
static void step(Track *track, unsigned char voice, unsigned char heard)
{
    const unsigned char *at = track->at;

    if (at == 0 || (track->frames && --track->frames)) {
        return;
    }
    for (;;) {
        switch (*at) {
        case JUMP:
            at = *(const unsigned char *const *)(at + 1);
            continue;
        case INSTRUMENT:
            track->instrument = at;
            track->waves = at[1];
            if (heard) {
                set_instrument(voice, at);
            }
            at += 8;
            continue;
        case REST:
            track->freq = 0;
            if (heard) {
                SOUND_CTRL(voice) = track->waves;
            }
            track->frames = at[1];
            track->at = at + 2;
            return;
        case NOTE:
            track->freq = at[1] | at[2] << 8;
            if (heard) {
                SOUND_FREQ(voice) = track->freq;
                SOUND_CTRL(voice) = track->waves;
                SOUND_CTRL(voice) = track->waves | SOUND_GATE;
            }
            track->frames = at[3];
            track->at = at + 4;
            return;
        case HOLD:
            track->frames = at[1];
            track->at = at + 2;
            return;
        default: /* END */
            track->freq = 0;
            if (heard) {
                SOUND_CTRL(voice) = track->waves;
            }
            track->at = 0;
            return;
        }
    }
}

void __fastcall__ music_play(const Song *song)
{
    unsigned char voice;

    music_stop();
    for (voice = 0; voice < 4; ++voice) {
        music[voice].at = song->voices[voice];
        music[voice].frames = 0;
    }
    if (song->cutoff != 0xFFFF) {
        SOUND_CUTOFF = song->cutoff;
        SOUND_FILTER = song->resonance | (SOUND_FILTER & 0x0F);
        SOUND_MODE = song->kind | (SOUND_MODE & (SOUND_VOICE3_OFF | 0x0F));
    }
}

void music_stop(void)
{
    unsigned char voice;

    for (voice = 0; voice < 4; ++voice) {
        if (music[voice].at && !effects[voice].at) {
            SOUND_CTRL(voice) = music[voice].waves;
        }
        music[voice].at = 0;
        music[voice].freq = 0;
    }
}

void music_tick(void)
{
    unsigned char voice;
    Track *effect;

    for (voice = 0; voice < 4; ++voice) {
        effect = &effects[voice];
        if (effect->at) {
            step(effect, voice, 1);
            if (effect->at == 0 && music[voice].instrument) {
                /* Over: the music's own instrument back, for its next note. */
                set_instrument(voice, music[voice].instrument);
            }
        }
        step(&music[voice], voice, effect->at == 0);
    }
}

void __fastcall__ sfx_play(unsigned char voice, const Song *sfx)
{
    effects[voice].at = sfx->voices[0];
    effects[voice].frames = 0;
    step(&effects[voice], voice, 1);
}

unsigned __fastcall__ music_note(unsigned char voice)
{
    return effects[voice].at ? effects[voice].freq : music[voice].freq;
}
