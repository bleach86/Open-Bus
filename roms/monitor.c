/*
 * The monitor: a command line on the terminal. Keys come in at $FFF9 (0 when
 * there is none; writing 0 takes the key), characters go out at $FFF8. The
 * terminal is a VT100: the arrows come as escape sequences, and the cursor
 * is moved and the screen cleared with them.
 */
#include <string.h>

#define TERM_OUT (*(volatile unsigned char *)0xFFF8)
#define TERM_IN (*(volatile unsigned char *)0xFFF9)

#define KEY_RETURN '\n'
#define KEY_ESCAPE 0x1B
#define KEY_BACKSPACE 0x7F
#define KEY_CTRL_H 0x08
#define KEY_CTRL_U 0x15

#define PROMPT "> "
#define LINE_SIZE 72 /* with the prompt, one line of an 80-column terminal */
#define HISTORY_SIZE 8

typedef struct {
    const char *name;
    void (*run)(char *args);
    const char *help;
} Builtin;

static char line[LINE_SIZE + 1];
static unsigned char length;
static unsigned char position; /* where typing goes: 0 to length */

/* The last HISTORY_SIZE lines, as typed; history[newest] is the latest. */
static char history[HISTORY_SIZE][LINE_SIZE + 1];
static unsigned char newest;
static unsigned char history_count;

/* --- the terminal --------------------------------------------------------- */

static unsigned char getkey(void)
{
    unsigned char key;

    while ((key = TERM_IN) == 0) {
    }
    TERM_IN = 0;
    return key;
}

static void putch(char c)
{
    TERM_OUT = c;
}

static void print(const char *text)
{
    while (*text != 0) {
        putch(*text++);
    }
}

static void print_hex_byte(unsigned char value)
{
    static const char digits[] = "0123456789ABCDEF";
    putch(digits[value >> 4]);
    putch(digits[value & 0x0F]);
}

static void print_hex_word(unsigned value)
{
    print_hex_byte(value >> 8);
    print_hex_byte(value);
}

static void usage(const char *text)
{
    print("Usage: ");
    print(text);
    putch('\n');
}

/* Move the cursor `count` columns left. */
static void cursor_left(unsigned char count)
{
    if (count == 0) {
        return;
    }
    print("\x1B[");
    if (count >= 10) {
        putch('0' + count / 10);
    }
    putch('0' + count % 10);
    putch('D');
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

/*
 * Print the line from line[from], where the cursor is, to its end, then
 * `blanks` spaces over what a deletion left behind; the cursor goes back to
 * line[position].
 */
static void redraw(unsigned char from, unsigned char blanks)
{
    unsigned char i;

    for (i = from; i < length; ++i) {
        putch(line[i]);
    }
    for (i = 0; i < blanks; ++i) {
        putch(' ');
    }
    cursor_left(length + blanks - position);
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
    cursor_left(1);
    redraw(position, 1);
}

/* Replace the line being edited, on screen too; the cursor goes to its end. */
static void replace_line(const char *text)
{
    cursor_left(position);
    strcpy(line, text);
    length = position = strlen(line);
    print(line);
    print("\x1B[K"); /* clear what was left of the old line */
}

/*
 * Read a line into `line`. Left and Right move the cursor, typing inserts at
 * it, Backspace deletes before it, Ctrl-U clears the line, and Up and Down
 * step through the history.
 */
static void read_line(void)
{
    unsigned char back = 0; /* 0: the new line, n: n lines back */
    unsigned char key;

    length = 0;
    position = 0;
    for (;;) {
        key = getkey();
        if (key == KEY_ESCAPE) {
            /* An arrow: ESC [ A to D (or ESC O, in the keypad's mode). */
            key = getkey();
            if (key != '[' && key != 'O') {
                continue;
            }
            key = getkey();
            switch (key) {
            case 'A':
                if (back < history_count) {
                    replace_line(history_line(++back));
                }
                break;
            case 'B':
                if (back != 0) {
                    --back;
                    replace_line(back == 0 ? "" : history_line(back));
                }
                break;
            case 'C':
                if (position != length) {
                    putch(line[position++]);
                }
                break;
            case 'D':
                if (position != 0) {
                    --position;
                    cursor_left(1);
                }
                break;
            }
            continue;
        }
        switch (key) {
        case KEY_RETURN:
        case '\r':
            line[length] = 0;
            putch('\n');
            return;
        case KEY_BACKSPACE:
        case KEY_CTRL_H:
            delete_back();
            break;
        case KEY_CTRL_U:
            back = 0;
            replace_line("");
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
    print(skip_spaces(args));
    putch('\n');
}

static void cls(char *args)
{
    (void)args;
    print("\x1B[H\x1B[2J\x1B[3J"); /* home, the screen, the scrollback */
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
        putch(':');
        do {
            putch(' ');
            print_hex_byte(*(volatile unsigned char *)address);
            if (address == end) {
                putch('\n');
                return;
            }
            ++address;
        } while ((address & 7) != 0);
        putch('\n');
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
            print("Bytes must be hex, 00 to FF.\n");
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

static const Builtin builtins[] = {
    {"HELP", help, "HELP               this list"},
    {"CLS", cls, "CLS                clear the screen"},
    {"ECHO", echo, "ECHO text          print the text"},
    {"MEM", mem, "MEM addr [end]     show memory"},
    {"POKE", poke, "POKE addr byte...  write to memory"},
    {"CALL", call, "CALL addr          call machine code (RTS returns)"},
    {"RESET", reset, "RESET              restart the machine"},
};

#define BUILTIN_COUNT (sizeof builtins / sizeof builtins[0])

static void help(char *args)
{
    unsigned char i;

    (void)args;
    for (i = 0; i < BUILTIN_COUNT; ++i) {
        print(builtins[i].help);
        putch('\n');
    }
    print("Up, Down: earlier lines. Ctrl-U: clear.\n");
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
    print("Unknown command. Type HELP for a list.\n");
}

int main(void)
{
    print("\n6502 MONITOR\nType HELP for a list of commands.\n\n");
    for (;;) {
        print(PROMPT);
        read_line();
        if (*skip_spaces(line) != 0) {
            history_add();
            execute();
        }
    }
}
