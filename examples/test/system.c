/*
 * Bank 6: the system page. Save memory counts the times the cart has
 * started, and keeps the count with the power off; the banks report in, one
 * of them through a call from this bank into bank 5; the real-time clock and
 * the status chip show what they read.
 */
#include <stdlib.h>
#include <string.h>
#include "test.h"

#pragma code-name (push, "BANK6")
#pragma rodata-name (push, "BANK6")

/* --- save memory: "TEST", then the count, 16 bits ------------------------ */

static const char magic[4] = {'T', 'E', 'S', 'T'};
static unsigned starts;

static void keep_starts(void)
{
    unsigned char i;

    SAVE_ADDR = 0;
    for (i = 0; i < 4; ++i) {
        SAVE_DATA = magic[i];
    }
    SAVE_DATA = starts;
    SAVE_DATA = starts >> 8;
}

void system_boot(void)
{
    unsigned char i;
    unsigned char saved = 1;
    unsigned char low;
    unsigned char high;

    SAVE_ADDR = 0;
    for (i = 0; i < 4; ++i) {
        if (SAVE_DATA != magic[i]) {
            saved = 0;
        }
    }
    starts = 0;
    if (saved) {
        /* A byte at a time into variables of their own: cc65's optimizer
         * can read a port again where the code reads a variable just
         * stored from it ("starts |= SAVE_DATA << 8" did). */
        low = SAVE_DATA;
        high = SAVE_DATA;
        starts = high << 8 | low;
    }
    ++starts;
    keep_starts();
}

static void draw_starts(void)
{
    text_at(2, 3, COLOR_WHITE, "Started        times");
    number_at(10, 3, COLOR_WHITE, starts, 5);
}

/* --- the clock: ms since 1970, UTC ---------------------------------------- */

static char *two(char *out, unsigned char n)
{
    out[0] = '0' + n / 10;
    out[1] = '0' + n % 10;
    return out + 2;
}

/* YYYY-MM-DD HH:MM:SS. The date from the day count after Howard Hinnant's
 * civil_from_days. */
static void draw_clock(void)
{
    static char text[20];
    unsigned long seconds = rtc_seconds();
    unsigned long z = seconds / 86400UL + 719468UL;
    unsigned long era = z / 146097UL;
    unsigned long doe = z - era * 146097UL;
    unsigned yoe = (doe - doe / 1460 + doe / 36524UL - doe / 146096UL) / 365;
    unsigned year = yoe + era * 400;
    unsigned doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    unsigned mp = (5 * doy + 2) / 153;
    unsigned char day = doy - (153 * mp + 2) / 5 + 1;
    unsigned char month = mp < 10 ? mp + 3 : mp - 9;
    unsigned long time = seconds % 86400UL;
    char *out = text;

    if (month <= 2) {
        ++year;
    }
    out = two(out, year / 100);
    out = two(out, year % 100);
    *out++ = '-';
    out = two(out, month);
    *out++ = '-';
    out = two(out, day);
    *out++ = ' ';
    out = two(out, time / 3600);
    *out++ = ':';
    out = two(out, time / 60 % 60);
    *out++ = ':';
    out = two(out, time % 60);
    *out = '\0';
    text_at(2, 13, COLOR_WHITE, text);
}

/* --- the status chip ------------------------------------------------------ */

static void draw_status(void)
{
    static char text[24];
    static char digits[6];
    unsigned char i;

    if (BATTERY == NO_BATTERY) {
        strcpy(text, "Battery: none");
    } else {
        strcpy(text, "Battery ");
        strcat(text, utoa(BATTERY, digits, 10));
        strcat(text, POWER_STATUS & POWER_CHARGING ? "%, charging   " : "%            ");
    }
    text_at(2, 16, COLOR_WHITE, text);
    text_at(2, 17, COLOR_WHITE, "CPU      C");
    number_at(6, 17, COLOR_WHITE, TEMPERATURE, 3);
    if (IP[0] == 0) {
        strcpy(text, "offline        ");
    } else {
        text[0] = '\0';
        for (i = 0; i < 4; ++i) {
            strcat(text, utoa(IP[i], digits, 10));
            strcat(text, i < 3 ? "." : "   ");
        }
    }
    text_at(2, 18, COLOR_WHITE, "IP");
    text_at(5, 18, COLOR_WHITE, text);
}

/* --- the page ------------------------------------------------------------- */

void system_show(void)
{
    text_at(1, 2, COLOR_GREY, "SAVE MEMORY");
    draw_starts();
    text_at(2, 4, COLOR_DARKGREY, "A adds one, B sets it to 0");

    text_at(1, 6, COLOR_GREY, "BANKS");
    text_at(2, 7, COLOR_WHITE, "RAM runs the program");
    text_at(2, 8, COLOR_WHITE, "Bank 4 holds the art");
    text_at(2, 9, COLOR_WHITE, "Bank 5 says");
    number_at(14, 9, COLOR_WHITE, bank5_says(), 1);
    text_at(2, 10, COLOR_WHITE, "Bank 6 runs this page");

    text_at(1, 12, COLOR_GREY, "CLOCK, UTC");
    draw_clock();
    text_at(1, 15, COLOR_GREY, "STATUS");
    draw_status();
    text_at(1, 20, COLOR_GREY, "FRAMES");
}

void system_step(void)
{
    if (pressed & (PAD_A | PAD_B)) {
        starts = pressed & PAD_A ? starts + 1 : 0;
        keep_starts();
        draw_starts();
    }
    if (frames % 30 == 0) {
        draw_clock();
        draw_status();
    }
    number_at(2, 21, COLOR_WHITE, frames, 5);
}

#pragma rodata-name (pop)
#pragma code-name (pop)
