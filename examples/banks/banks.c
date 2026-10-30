/*
 * A 128K cart, to try bank switching: sixteen 8K banks. Bank 0, with this
 * program and the C runtime, stays at $8000. Banks 1 to 14 (far.s) each hold
 * a routine and a line of text at the same addresses, which the program
 * maps in at $A000 itself through BANK1; bank 15 (bank15.c) holds C code,
 * called through the SDK's bank_call.
 */
#include <stdio.h>
#include "machine.h"

#define C_BANK 15

/* The same in banks 1 to 14. */
#define bank_number ((unsigned (*)(void))0xA000) /* works it out from the bank */
#define BANK_TEXT ((const char *)0xA040)

/* In bank 15: bank_call maps it in for the call. */
#pragma wrapped-call (push, bank_call, bank)
void describe_square(char *out, unsigned n);
#pragma wrapped-call (pop)

int main(void)
{
    static char text[40];
    unsigned char bank;
    unsigned number;
    unsigned char failures = 0;

    printf("Banks 1 to %u, at $A000:\n", C_BANK);
    for (bank = 1; bank < C_BANK; ++bank) {
        BANK1 = bank;
        number = bank_number();
        printf("Bank %u says %u: %s\n", bank, number, BANK_TEXT);
        if (number != bank) {
            ++failures;
        }
    }
    describe_square(text, 12);
    printf("Bank %u runs C: %s\n", C_BANK, text);
    if (failures) {
        printf("%u banks failed.\n", failures);
    } else {
        printf("All banks good.\n");
    }
    return failures;
}
