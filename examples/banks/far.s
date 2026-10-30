; Banks 1 to 14 of the banks cart. Each has the same layout (see banks.c): a
; routine at $A000 that returns the bank's number, a line of text naming the
; bank at $A040, and from $A100 to the end, a table of the bank's number.
;
; The routine works the number out from the table, so it reads the whole
; bank: TABLE_SIZE copies of any byte sum to 0 (mod 256), as TABLE_SIZE is
; a multiple of 256, and the routine adds its own number to that. A wrong
; bank or a bad byte gives a different number.

        .include "machine.inc"
        .importzp ptr1

BASE = $A000
TEXT = $40                              ; offsets into the bank
TABLE = $100
TABLE_SIZE = $2000 - TABLE

.macro far_bank number
        .segment .sprintf("BANK%d", number)

.local start
start:  lda     #<(BASE + TABLE)
        sta     ptr1
        lda     #>(BASE + TABLE)
        sta     ptr1+1
        lda     #0
        ldx     #>TABLE_SIZE            ; pages
        ldy     #0
:       clc
        adc     (ptr1),y
        iny
        bne     :-
        inc     ptr1+1
        dex
        bne     :-
        clc
        adc     #number
        ldx     #0
        rts

        .res    TEXT - (* - start), $EA
        .byte   .sprintf("This is bank %d.", number), 0

        .res    TABLE - (* - start), $EA
        .res    TABLE_SIZE, number
.endmacro

.repeat 14, i
        far_bank i + 1
.endrepeat
