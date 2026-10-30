/*
 * The pet's life, a minute at a time, by the real-time clock: whatever
 * minutes passed since it was last seen are lived when the cart starts
 * (up to two days of them), and the rest as they come.
 *
 * Over a minute awake it gets a little hungrier, more bored and more tired;
 * every two or three hours it makes a mess. Asleep (the light off) it
 * gets its energy back. Left hungry, bored, worn out or in a mess, its
 * health goes; low health makes it ill, and ill it goes on getting worse
 * until it has its medicine. At no health left, it leaves.
 */
#include <stdlib.h>
#include <string.h>
#include "sprout.h"

Pet pet;
unsigned char life_grew;
unsigned char life_left;
unsigned long life_away;

static const char magic[4] = {'S', 'P', 'R', 'T'};

#define MAX_CATCH_UP (2 * 24 * 60) /* minutes */
#define EGG_SECONDS 30
#define GROWN_UP (24 * 60)         /* minutes old */
#define ILL_BELOW 4000

static void lose(unsigned *stat, unsigned by)
{
    *stat = *stat > by ? *stat - by : 0;
}

static void gain(unsigned *stat, unsigned by)
{
    *stat = FULL - *stat < by ? FULL : *stat + by;
}

/* A byte at a time each way: see docs/machine.md on reading ports. */
void life_keep(void)
{
    const unsigned char *byte = (const unsigned char *)&pet;
    unsigned char i;

    SAVE_ADDR = 0;
    for (i = 0; i < sizeof pet; ++i) {
        SAVE_DATA = byte[i];
    }
}

static void hatch_new(void)
{
    memset(&pet, 0, sizeof pet);
    memcpy(pet.magic, magic, 4);
    pet.version = SAVE_VERSION;
    pet.stage = STAGE_EGG;
    pet.egg_left = EGG_SECONDS;
    pet.fullness = pet.happiness = pet.energy = FULL / 10 * 8;
    pet.health = FULL;
    pet.poop_timer = 120;
    pet.last = rtc_seconds();
}

static void live_minute(void)
{
    unsigned char bad;

    if (pet.stage == STAGE_EGG) {
        return; /* an egg wants nothing */
    }
    ++pet.age;
    if (pet.stage == STAGE_BABY && pet.age >= GROWN_UP) {
        pet.stage = STAGE_ADULT;
        life_grew = 1;
    }
    lose(&pet.fullness, pet.asleep ? 7 : 14);
    lose(&pet.happiness, pet.asleep ? 4 : 10);
    if (pet.asleep) {
        gain(&pet.energy, 40);
        if (pet.energy == FULL && !pet.dark) {
            pet.asleep = 0;
        }
    } else {
        lose(&pet.energy, 8);
        if (pet.energy == 0) {
            pet.asleep = 1; /* it drops off where it stands */
        }
        if (--pet.poop_timer == 0) {
            if (pet.poops < 4) {
                ++pet.poops;
            }
            pet.poop_timer = 150 + rand() % 60;
        }
    }
    bad = pet.fullness == 0 || pet.happiness == 0 || pet.energy == 0 || pet.poops >= 3;
    if (bad) {
        lose(&pet.health, 20);
    } else if (!pet.sick) {
        gain(&pet.health, 5);
    }
    if (pet.health < ILL_BELOW) {
        pet.sick = 1;
    }
    if (pet.sick) {
        lose(&pet.health, 4);
    }
    if (pet.health == 0) {
        life_left = 1;
        hatch_new();
    }
}

void life_start(void)
{
    unsigned char *byte = (unsigned char *)&pet;
    unsigned char i;
    unsigned long now = rtc_seconds();
    unsigned long minutes;
    unsigned char low;
    unsigned char high;

    /* The clock, and the sound chip's noise (voice 3, set up running). Each
     * byte read on its own: see docs/machine.md on reading ports. */
    low = SOUND_OSC3;
    high = SOUND_OSC3;
    srand((unsigned)now ^ (high << 8 | low));
    SAVE_ADDR = 0;
    for (i = 0; i < sizeof pet; ++i) {
        byte[i] = SAVE_DATA;
    }
    if (memcmp(pet.magic, magic, 4) != 0 || pet.version != SAVE_VERSION || now < pet.last) {
        hatch_new();
    }
    minutes = (now - pet.last) / 60;
    life_away = minutes;
    if (minutes > MAX_CATCH_UP) {
        pet.last = now - MAX_CATCH_UP * 60UL;
        minutes = MAX_CATCH_UP;
    }
    while (minutes--) {
        live_minute();
        pet.last += 60;
    }
    life_keep();
}

void life_second(void)
{
    unsigned long now = rtc_seconds();
    unsigned char lived = 0;

    if (pet.stage == STAGE_EGG) {
        pet.last = now; /* the egg's time is on screen only */
        if (--pet.egg_left == 0) {
            pet.stage = STAGE_BABY;
            life_grew = 1;
        }
        lived = 1;
    }
    while (now - pet.last >= 60) {
        live_minute();
        pet.last += 60;
        lived = 1;
    }
    if (lived) {
        life_keep();
    }
}

unsigned char life_needy(void)
{
    return pet.stage != STAGE_EGG &&
           (pet.fullness < FULL / 4 || pet.happiness < FULL / 4 || pet.energy < FULL / 5 || pet.sick ||
            pet.poops >= 2);
}
