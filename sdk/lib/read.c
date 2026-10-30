/*
 * Standard input is the keyboard, a line at a time: read() collects a line,
 * echoing it and taking Backspace, and hands it out ending in '\n'.
 */
#include <unistd.h>
#include "machine.h"

#define LINE_SIZE 80

static char line[LINE_SIZE];
static unsigned char length;
static unsigned char next;

static void read_line(void)
{
    unsigned char key;

    length = 0;
    next = 0;
    for (;;) {
        key = bios_waitkey();
        if (key == KEY_RETURN) {
            line[length++] = '\n';
            bios_chrout('\n');
            return;
        }
        if (key == KEY_BACKSPACE) {
            if (length != 0) {
                --length;
                bios_print("\b \b");
            }
        } else if (key >= ' ' && key < 0x7F && length < LINE_SIZE - 1) {
            line[length++] = key;
            bios_chrout(key);
        }
    }
}

int __fastcall__ read(int fd, void *buf, unsigned count)
{
    unsigned n = 0;

    (void)fd;
    if (next == length) {
        read_line();
    }
    while (n < count && next < length) {
        ((char *)buf)[n++] = line[next++];
    }
    return n;
}
