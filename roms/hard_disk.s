; The hard disk card's firmware (src/machines/apple2/hard_disk.rs): its
; 256 bytes, at $Cn00 in whichever slot n it's in, so nothing in it
; depends on n but what it works out. It boots a ProDOS volume, and is
; the volume's ProDOS block device driver.
;
; The card's registers, at $C080 + n0 (X = n0 throughout):
;   +0  the command: 0 STATUS, 1 READ, 2 WRITE
;   +1  the unit: drive 1 or 2 (bit 7), as ProDOS has it
;   +2  the block, low byte; +3 high
;   +4  written: the command done; read: its error, 0 if none
;   +5  read: the unit's blocks, low byte; +6 high
;   +8  the data: a block's 512 bytes in turn, read or to be written
;
; ProDOS calls the driver (at $Cn00 + $CnFF) with its command at $42, the
; unit at $43, the buffer at $44 and the block at $46; it returns A, the
; error, and the carry set if there was one; STATUS, the blocks in X
; (low) and Y (high).

IORTS   = $FF58                 ; an RTS in the monitor: where we are, on the stack
SLOOP   = $FABA                 ; the boot scan, on to the next slot down
COMMAND = $42
UNIT    = $43
BUFFER  = $44
BLOCK   = $46
CARD    = $C080                 ; the card's registers, with X its slot's n0

        .segment "CODE"

; The signature the boot scan and ProDOS look for: $20, $00, $03 at
; $Cn01, $Cn03 and $Cn05; $3C at $Cn07, a block device, not SmartPort.
        ldx     #$20
        ldy     #$00
        ldx     #$03
        ldy     #$3C

; Booted: block 0 read to $0800, from drive 1, then run at $0801 with X
; the slot's n0, as ProDOS's boot block wants; no volume, the slot below.
boot:   jsr     IORTS
        tsx
        lda     $0100,x         ; $Cn, this page
        pha                     ; the driver's return, its high byte
        asl     a
        asl     a
        asl     a
        asl     a
        sta     UNIT            ; drive 1, slot n
        lda     #<(booted - 1)
        pha                     ; and its low
        lda     #1
        sta     COMMAND         ; READ
        lda     #0
        sta     BUFFER
        sta     BLOCK
        sta     BLOCK+1
        lda     #$08
        sta     BUFFER+1        ; block 0, to $0800
        clc
        bcc     driver          ; called, as it were
booted: bcs     none
        ldx     UNIT
        jmp     $0801
none:   jmp     SLOOP

; The driver.
driver: lda     UNIT
        and     #$70
        tax                     ; X: the slot's n0
        lda     COMMAND
        cmp     #3
        bcs     format          ; FORMAT, or no command at all
        sta     CARD+0,x
        lda     UNIT
        sta     CARD+1,x
        lda     BLOCK
        sta     CARD+2,x
        lda     BLOCK+1
        sta     CARD+3,x
        lda     COMMAND
        cmp     #2
        bne     do              ; WRITE: the buffer's 512 bytes first
        ldy     #0
put:    lda     (BUFFER),y
        sta     CARD+8,x
        iny
        bne     put
        inc     BUFFER+1
put2:   lda     (BUFFER),y
        sta     CARD+8,x
        iny
        bne     put2
        dec     BUFFER+1
do:     sta     CARD+4,x        ; done
        lda     CARD+4,x
        bne     error
        lda     COMMAND
        beq     status
        cmp     #1
        bne     ok              ; READ: the block's 512 bytes, after
        ldy     #0
get:    lda     CARD+8,x
        sta     (BUFFER),y
        iny
        bne     get
        inc     BUFFER+1
get2:   lda     CARD+8,x
        sta     (BUFFER),y
        iny
        bne     get2
        dec     BUFFER+1
ok:     lda     #0
        clc
        rts
status: ldy     CARD+6,x
        lda     CARD+5,x
        tax
        lda     #0
        clc
        rts
format: bne     bad             ; FORMAT: nothing to do, ProDOS does it
        lda     #0
        clc
        rts
bad:    lda     #$01            ; no such command
error:  sec
        rts

; Its last four bytes: the blocks (0: ask STATUS); what it can do (two
; volumes; format, write, read, status); and where the driver is.
        .segment "TAIL"

        .word   0
        .byte   $1F
        .byte   <driver
