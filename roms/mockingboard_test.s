; The program for the Mockingboard test (src/machines/apple2/tests/
; mockingboard.rs): a 12K ROM of the test's own at $D000, on an Apple with
; a Mockingboard in slot 4. It finds the card as a music player does, by
; its first 6522's timer 1 counting down between two reads; writes a tone
; into its first AY, through the 6522's ports; then has timer 1 interrupt
; every PERIOD + 2 cycles, and counts the interrupts.

VIA     = $C400                 ; the first 6522
PORTB   = VIA + $0              ; the AY's control: BC1, BDIR, /RESET
PORTA   = VIA + $1              ; the AY's bus
DDRB    = VIA + $2
DDRA    = VIA + $3
T1CL    = VIA + $4
T1CH    = VIA + $5
T1LL    = VIA + $6
ACR     = VIA + $B
IER     = VIA + $E

LATCH   = $07                   ; BC1, BDIR, /RESET high: a register's number
WRITE   = $06                   ; BDIR, /RESET high: the register written
IDLE    = $04                   ; /RESET high alone: the bus let be

PERIOD  = 1000

count   = $00                   ; interrupts taken, 16 bits
counted = $02                   ; timer 1's count between its two reads

        .segment "CODE"

reset:  ldx     #$FF
        txs
        lda     #0
        sta     count
        sta     count+1

; Timer 1 started, and read twice: a 6522 there counts down between them.
        lda     #$FF
        sta     T1LL
        sta     T1CH
        lda     T1CL            ; read on its 4th cycle
        sec
        sbc     T1CL            ; 6 cycles on
        sta     counted

; Tone A, period $123, at 15, alone: each register latched then written.
        lda     #$07
        sta     DDRB            ; BC1, BDIR and /RESET out
        lda     #$FF
        sta     DDRA            ; the bus out
        ldx     #0
tone:   lda     registers,x
        sta     PORTA
        lda     #LATCH
        sta     PORTB
        lda     #IDLE
        sta     PORTB
        lda     values,x
        sta     PORTA
        lda     #WRITE
        sta     PORTB
        lda     #IDLE
        sta     PORTB
        inx
        cpx     #4
        bne     tone

; Timer 1 free running, interrupting: every PERIOD + 2 cycles.
        lda     #$40
        sta     ACR
        lda     #<PERIOD
        sta     T1CL
        lda     #>PERIOD
        sta     T1CH
        lda     #$C0            ; timer 1's interrupt enabled
        sta     IER
        cli
loop:   jmp     loop

irq:    pha
        lda     T1CL            ; the flag cleared
        inc     count
        bne     :+
        inc     count+1
:       pla
        rti

registers:
        .byte   0, 1, 7, 8
values: .byte   $23, $01, $3E, 15

        .segment "VECTORS"

        .word   reset, reset, irq
