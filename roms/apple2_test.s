; The program for the Apple II test (src/machines/apple2/tests/machine.rs):
; a 12K ROM of the test's own at $D000, in place of the Apple's. It writes
; HELLO on the text screen's top row, waits for a key and puts it on row 1,
; marks row 8 and switches to graphics.

ROW0    = $0400                 ; the text rows: interleaved, so row 1 is
ROW1    = $0480                 ; 128 bytes on, and row 8 40 on from row 0
ROW8    = $0428
KBD     = $C000                 ; the key, bit 7 set when there is one
KBDSTRB = $C010                 ; taking it
TXTCLR  = $C050                 ; graphics

        .segment "CODE"

reset:  ldx     #0
hello:  lda     message,x
        beq     wait
        sta     ROW0,x
        inx
        bne     hello
wait:   lda     KBD
        bpl     wait
        sta     ROW1
        sta     KBDSTRB
        lda     #'*' | $80
        sta     ROW8
        lda     TXTCLR
done:   jmp     done

message:
        .byte   'H' | $80, 'E' | $80, 'L' | $80, 'L' | $80, 'O' | $80, 0

        .segment "VECTORS"

        .word   reset, reset, reset
