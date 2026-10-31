; The program for the bus test (src/cpu/tests/bus.rs), on a test machine with a
; register at $D000 that counts its reads and writes. Each line's comment
; is what the 6502 does to the register.

PORT    = $D000

        .segment "CODE"

start:  lda PORT                ; read once
        ldx #1
        lda PORT-1,x            ; a page crossed: $CF00 read first, then
                                ; the register once
        inc PORT                ; read once, written twice: the old value
                                ; back, then the new
        ldx #$FF
        sta PORT+1,x            ; a page crossed: the register read before
                                ; the carry, then $D100 written
done:   jmp done

        .segment "VECTORS"

        .word start, start, start
