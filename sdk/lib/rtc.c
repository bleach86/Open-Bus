/*
 * The real-time clock in seconds since 1970, UTC. RTC holds milliseconds in
 * 64 bits; dividing by 1000 a byte at a time from the top keeps the sums in
 * a long. Seconds fit 32 bits until 2106.
 */
#include "machine.h"

unsigned long rtc_seconds(void)
{
    static unsigned char quotient[8];
    unsigned long remainder = 0;
    unsigned char i = 8;

    RTC_LATCH = 0;
    while (i--) {
        remainder = remainder * 256 + RTC[i];
        quotient[i] = remainder / 1000;
        remainder %= 1000;
    }
    return quotient[0] | (unsigned)quotient[1] << 8 | (unsigned long)quotient[2] << 16 |
           (unsigned long)quotient[3] << 24;
}
