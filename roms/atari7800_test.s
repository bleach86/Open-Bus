; The program for the Atari 7800 test (src/machines/atari7800/tests/
; machine.rs): a 16K cartridge at $C000. It runs the program the test has
; put at $2600 before reset, each a count at $2400 (16 bits), counted as
; fast as it goes, or a line at a time, or reading the RIOT or RAM each
; time; and the NMI's own count, of MARIA's DLIs, at $2410.

COUNT   = $2400
NMIS    = $2410
WHICH   = $2600
WSYNC   = $24                   ; MARIA's: the CPU stopped to the line's end
SWCHA   = $0280                 ; the RIOT's, a slow cycle to read

        .segment "CODE"

reset:  lda     WHICH
        beq     count
        cmp     #1
        beq     lines
        cmp     #2
        beq     riot
        jmp     ram

; 0: as fast as it goes, 9 cycles a count.
count:  inc     COUNT
        bne     count
        inc     COUNT+1
        jmp     count

; 1: a count a line.
lines:  sta     WSYNC
        inc     COUNT
        bne     lines
        inc     COUNT+1
        jmp     lines

; 2: the RIOT read each count, 13 cycles, one of them slow.
riot:   lda     SWCHA
        inc     COUNT
        bne     riot
        inc     COUNT+1
        jmp     riot

; 3: RAM read each count, the same but none slow.
ram:    lda     COUNT+2
        inc     COUNT
        bne     ram
        inc     COUNT+1
        jmp     ram

nmi:    inc     NMIS
        rti

        .segment "VECTORS"

        .word   nmi, reset, reset
