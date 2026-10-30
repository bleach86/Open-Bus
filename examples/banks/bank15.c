/*
 * Bank 15 of the banks cart: C code in a switched bank. It calls the C
 * runtime (sprintf, and the multiply helper) in bank 0, which stays mapped.
 */
#include <stdio.h>

#pragma code-name (push, "BANK15")
#pragma rodata-name (push, "BANK15")

void describe_square(char *out, unsigned n)
{
    sprintf(out, "%u squared is %u.", n, n * n);
}

#pragma rodata-name (pop)
#pragma code-name (pop)
