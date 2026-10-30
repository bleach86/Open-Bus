; Keyboard: the hardware holds one key until it is acknowledged.

        .include "machine.inc"

        .export getkey

        .segment "CODE"

; -> A: the waiting key, or 0 if none; X = 0.
getkey: lda     KEY_STATUS
        and     #KEY_WAITING
        beq     @none
        lda     KEY_CODE
        sta     KEY_STATUS              ; acknowledge: the next key can latch
@none:  ldx     #0
        rts
