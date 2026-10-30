/*
 * Sprout: a pet that lives on the machine's real-time clock. It hatches,
 * grows from a baby into an adult over a day, and gets hungry, bored,
 * tired and messy whether the machine is on or not; the save memory keeps
 * it, and the time it was last seen, between runs.
 */
#ifndef SPROUT_H
#define SPROUT_H

#include "machine.h"

/* A stat at its best. */
#define FULL 10000u

#define STAGE_EGG 0
#define STAGE_BABY 1
#define STAGE_ADULT 2

/* Everything the save memory keeps. */
typedef struct {
    char magic[4];          /* "SPRT" */
    unsigned char version;  /* SAVE_VERSION */
    unsigned char stage;
    unsigned char poops;    /* 0-4 */
    unsigned char sick;
    unsigned char asleep;
    unsigned char dark;     /* the light is off */
    unsigned fullness;      /* 0-FULL, as are the next three */
    unsigned happiness;
    unsigned energy;
    unsigned health;
    unsigned poop_timer;    /* minutes awake to the next */
    unsigned egg_left;      /* seconds on screen until it hatches */
    unsigned long age;      /* minutes since it hatched */
    unsigned long last;     /* the clock (rtc_seconds) the pet has lived to */
} Pet;

#define SAVE_VERSION 1

extern Pet pet;

/* life.c: the pet's needs, minute by minute. */
void life_start(void);          /* load the pet, or a new egg; catch up */
void life_second(void);         /* once a real second */
void life_keep(void);           /* into save memory */
unsigned char life_needy(void); /* it wants something */
/* What happened in the last minutes lived, for the screen to tell. */
extern unsigned char life_grew;
extern unsigned char life_left;
extern unsigned long life_away; /* minutes caught up at the start */

#endif
