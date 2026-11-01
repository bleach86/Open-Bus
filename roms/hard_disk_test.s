; The program for the hard disk card's test (src/machines/apple2/tests/
; hard_disk.rs): a 12K ROM of the test's own at $D000, on a II+ with the
; card in slot 7. It calls the card's driver as ProDOS does: STATUS; a
; block written, then read back elsewhere; and drive 2, with nothing in
; it. What each gives is kept in zero page, for the test.

COMMAND = $42
UNIT    = $43
BUFFER  = $44
BLOCK   = $46
DRIVER  = $08                   ; where the driver is: $C700 + $C7FF
SLOT7   = $70                   ; unit: drive 1 of slot 7

; What the calls give, kept.
blocks  = $00                   ; STATUS: the blocks, low and high
written = $02                   ; WRITE's error and carry
read    = $04                   ; READ's
none    = $06                   ; drive 2's STATUS

FROM    = $2000                 ; the block written: 0, 1, 2, ... (its low bytes)
TO      = $3000                 ; and read back here

        .segment "CODE"

reset:  ldx     #$FF
        txs
        lda     $C7FF
        sta     DRIVER
        lda     #$C7
        sta     DRIVER+1
        ldx     #0
fill:   txa
        sta     FROM,x
        eor     #$FF
        sta     FROM+$100,x
        inx
        bne     fill

        lda     #0              ; STATUS
        jsr     call
        stx     blocks
        sty     blocks+1

        lda     #<FROM
        sta     BUFFER
        lda     #>FROM
        sta     BUFFER+1
        lda     #5
        sta     BLOCK
        lda     #0
        sta     BLOCK+1
        lda     #2              ; WRITE block 5
        jsr     call
        sta     written
        rol     written+1       ; the carry

        lda     #>TO
        sta     BUFFER+1
        lda     #1              ; READ block 5, to $3000
        jsr     call
        sta     read
        rol     read+1

        lda     #SLOT7 | $80    ; drive 2: nothing in it
        sta     UNIT
        lda     #0
        jsr     call
        sta     none
        rol     none+1
done:   jmp     done

; The driver called, with command A, on drive 1 unless drive 2 is set.
call:   sta     COMMAND
        lda     UNIT
        and     #$80
        ora     #SLOT7
        sta     UNIT
        jmp     (DRIVER)

        .segment "VECTORS"

        .word   reset, reset, reset
