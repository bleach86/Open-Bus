/*
 * The shell: the command line, on the menu's system page. MENU goes back.
 */
#include <string.h>
#include "machine.h"
#include "system.h"

#define PROMPT "> "
#define LINE_SIZE 127
#define HISTORY_SIZE 16

typedef struct {
    const char *name;
    void (*run)(char *args);
    const char *help;
} Builtin;

static char line[LINE_SIZE + 1];
static unsigned char length;
static unsigned char position; /* where typing goes: 0 to length */
/* Where line[0] is on screen; the line wraps onto the rows below. */
static unsigned char start_column;
static unsigned char start_row;

/* The last HISTORY_SIZE lines, as typed; history[newest] is the latest. */
static char history[HISTORY_SIZE][LINE_SIZE + 1];
static unsigned char newest;
static unsigned char history_count;

/* --- output --------------------------------------------------------------- */

static void print_hex_byte(unsigned char value)
{
    static const char digits[] = "0123456789ABCDEF";
    bios_chrout(digits[value >> 4]);
    bios_chrout(digits[value & 0x0F]);
}

static void print_hex_word(unsigned value)
{
    print_hex_byte(value >> 8);
    print_hex_byte(value);
}

static void usage(const char *text)
{
    bios_print("Usage: ");
    bios_print(text);
    bios_chrout('\n');
}

/* --- line editing --------------------------------------------------------- */

static void history_add(void)
{
    if (history_count != 0 && strcmp(history[newest], line) == 0) {
        return;
    }
    newest = (newest + 1) % HISTORY_SIZE;
    strcpy(history[newest], line);
    if (history_count < HISTORY_SIZE) {
        ++history_count;
    }
}

/* The line `back` steps into the history: 1 is the latest. */
static const char *history_line(unsigned char back)
{
    return history[(newest + HISTORY_SIZE + 1 - back) % HISTORY_SIZE];
}

/* Put the cursor on line[index]. */
static void place_cursor(unsigned char index)
{
    unsigned column = start_column + index;
    bios_setcursor(column % SCREEN_COLUMNS, start_row + column / SCREEN_COLUMNS);
}

/*
 * Print the line from line[from], where the cursor is, to its end, then
 * `blanks` spaces over what a deletion left behind; the cursor goes back to
 * line[position]. Printing past the bottom row scrolls the screen, which
 * moves the start of the line up.
 */
static void redraw(unsigned char from, unsigned char blanks)
{
    unsigned char i;

    for (i = from; i < length; ++i) {
        bios_chrout(line[i]);
    }
    for (i = 0; i < blanks; ++i) {
        bios_chrout(' ');
    }
    start_row = bios_row() - (start_column + length + blanks) / SCREEN_COLUMNS;
    place_cursor(position);
}

static void insert(char c)
{
    if (length == LINE_SIZE) {
        return;
    }
    memmove(line + position + 1, line + position, length - position);
    line[position] = c;
    ++length;
    redraw(position++, 0);
}

/* Delete the character before the cursor. */
static void delete_back(void)
{
    if (position == 0) {
        return;
    }
    --position;
    memmove(line + position, line + position + 1, length - position - 1);
    --length;
    place_cursor(position);
    redraw(position, 1);
}

/* Replace the line being edited, on screen too; the cursor goes to its end. */
static void replace_line(const char *text)
{
    unsigned char old_length = length;

    strcpy(line, text);
    length = position = strlen(line);
    place_cursor(0);
    redraw(0, old_length > length ? old_length - length : 0);
}

/*
 * Read a line into `line`, wrapping onto the rows below as it grows, like
 * the C64's screen editor. Left and Right move the cursor, typing inserts
 * at it, Backspace deletes before it, Escape clears the line, and Up and
 * Down step through the history.
 */
static void read_line(void)
{
    unsigned char back = 0; /* 0: the new line, n: n lines back */
    unsigned char key;

    length = 0;
    position = 0;
    start_column = bios_column();
    start_row = bios_row();
    for (;;) {
        key = bios_waitkey();
        switch (key) {
        case KEY_RETURN:
            line[length] = 0;
            place_cursor(length);
            bios_chrout('\n');
            return;
        case KEY_BACKSPACE:
            delete_back();
            break;
        case KEY_LEFT:
            if (position != 0) {
                place_cursor(--position);
            }
            break;
        case KEY_RIGHT:
            if (position != length) {
                place_cursor(++position);
            }
            break;
        case KEY_ESCAPE:
            back = 0;
            replace_line("");
            break;
        case KEY_UP:
            if (back < history_count) {
                replace_line(history_line(++back));
            }
            break;
        case KEY_DOWN:
            if (back != 0) {
                --back;
                replace_line(back == 0 ? "" : history_line(back));
            }
            break;
        default:
            if (key >= ' ' && key < 0x7F) {
                insert(key);
            }
            break;
        }
    }
}

/* --- parsing -------------------------------------------------------------- */

static char *skip_spaces(char *text)
{
    while (*text == ' ') {
        ++text;
    }
    return text;
}

/* The next word of *text, NUL-terminated in place; *text moves past it. */
static char *next_word(char **text)
{
    char *word = skip_spaces(*text);
    char *end = word;

    while (*end != 0 && *end != ' ') {
        ++end;
    }
    if (*end != 0) {
        *end++ = 0;
    }
    *text = end;
    return word;
}

/* One to four hex digits; returns 0 if `word` is not that. */
static unsigned char parse_hex(const char *word, unsigned *value)
{
    unsigned char digits = 0;
    char c;

    *value = 0;
    while ((c = *word++) != 0) {
        if (c >= '0' && c <= '9') {
            c -= '0';
        } else if (c >= 'A' && c <= 'F') {
            c -= 'A' - 10;
        } else if (c >= 'a' && c <= 'f') {
            c -= 'a' - 10;
        } else {
            return 0;
        }
        if (++digits > 4) {
            return 0;
        }
        *value = *value << 4 | c;
    }
    return digits != 0;
}

/* --- built-in commands ---------------------------------------------------- */

static void help(char *args);

static void echo(char *args)
{
    bios_print(skip_spaces(args));
    bios_chrout('\n');
}

static void cls(char *args)
{
    (void)args;
    bios_clear();
}

static void mem(char *args)
{
    unsigned address;
    unsigned end;

    if (!parse_hex(next_word(&args), &address)) {
        usage("MEM addr [end]");
        return;
    }
    if (!parse_hex(next_word(&args), &end) || end < address) {
        end = address;
    }
    for (;;) {
        print_hex_word(address);
        bios_chrout(':');
        do {
            bios_chrout(' ');
            print_hex_byte(*(volatile unsigned char *)address);
            if (address == end) {
                bios_chrout('\n');
                return;
            }
            ++address;
        } while ((address & 7) != 0);
        bios_chrout('\n');
    }
}

static void poke(char *args)
{
    unsigned address;
    unsigned value;
    char *word;

    if (!parse_hex(next_word(&args), &address)) {
        usage("POKE addr byte...");
        return;
    }
    while (*(word = next_word(&args)) != 0) {
        if (!parse_hex(word, &value) || value > 0xFF) {
            bios_print("Bytes must be hex, 00 to FF.\n");
            return;
        }
        *(volatile unsigned char *)address++ = value;
    }
}

static void call(char *args)
{
    unsigned address;

    if (!parse_hex(next_word(&args), &address)) {
        usage("CALL addr");
        return;
    }
    ((void (*)(void))address)();
}

static void reset(char *args)
{
    (void)args;
    __asm__("jmp ($FFFC)");
}

static unsigned char leaving;

static void menu(char *args)
{
    (void)args;
    leaving = 1;
}

static const Builtin builtins[] = {
    {"HELP", help, "HELP               this list"},
    {"CLS", cls, "CLS                clear the screen"},
    {"ECHO", echo, "ECHO text          print the text"},
    {"MEM", mem, "MEM addr [end]     show memory"},
    {"POKE", poke, "POKE addr byte...  write to memory"},
    {"CALL", call, "CALL addr          call machine code"},
    {"MENU", menu, "MENU               back to the menu"},
    {"RESET", reset, "RESET              restart the machine"},
};

#define BUILTIN_COUNT (sizeof builtins / sizeof builtins[0])

static void help(char *args)
{
    unsigned char i;

    (void)args;
    for (i = 0; i < BUILTIN_COUNT; ++i) {
        bios_print(builtins[i].help);
        bios_chrout('\n');
    }
    bios_print("Up, Down: earlier lines. Escape: clear.\n");
}

/* --- main ----------------------------------------------------------------- */

static void execute(void)
{
    char *args = line;
    char *name = next_word(&args);
    unsigned char i;

    if (*name == 0) {
        return;
    }
    for (i = 0; i < BUILTIN_COUNT; ++i) {
        if (stricmp(name, builtins[i].name) == 0) {
            builtins[i].run(args);
            return;
        }
    }
    bios_print("Unknown command. Type HELP for a list.\n");
}

void shell(void)
{
    leaving = 0;
    bios_setcolor(DEFAULT_COLOR);
    bios_clear();
    bios_print("6502 SYSTEM  BIOS ");
    print_hex_byte(bios_version());
    bios_print("\nType HELP for a list of commands.\n\n");
    while (!leaving) {
        bios_print(PROMPT);
        read_line();
        if (*skip_spaces(line) != 0) {
            history_add();
            execute();
        }
    }
}
