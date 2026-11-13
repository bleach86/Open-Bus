; Skink's test boot image: its title on the screen's first row; the
; frame's interrupt and the VIA's timer 1, free running every 1002 cycles,
; counted; each key typed put on the second row; each byte the ACIA
; receives sent back.
;
; Counted at:
;   $0300-$0301  frames
;   $0302-$0303  the VIA's timer 1
;   $0304        keys typed

INTERRUPTS    = $9000
INTERRUPTS_ON = $9001
VIA_T1CL      = $9014
VIA_T1CH      = $9015
VIA_ACR       = $901B
VIA_IFR       = $901D
VIA_IER       = $901E
KEY           = $9020
KEY_WAITING   = $9021
ACIA_DATA     = $9030
ACIA_STATUS   = $9031
ACIA_COMMAND  = $9032
ACIA_CONTROL  = $9033
SCREEN        = $8000

FRAMES = $0300
TIMERS = $0302
KEYS   = $0304

.segment "CODE"

reset:  sei
        cld
        ldx #$FF
        txs
        lda #0
        ldx #4
clear:  sta FRAMES,x
        dex
        bpl clear
        ldx #0
title:  lda text,x
        beq titled
        sta SCREEN,x
        inx
        bne title
titled:
        ; The ACIA: 9600 baud, 8 bits, 1 stop bit; on, its receiver's
        ; interrupt on.
        lda #$1E
        sta ACIA_CONTROL
        lda #$01
        sta ACIA_COMMAND
        ; The VIA's timer 1, free running, its interrupt on.
        lda #$40
        sta VIA_ACR
        lda #$C0
        sta VIA_IER
        lda #<1000
        sta VIA_T1CL
        lda #>1000
        sta VIA_T1CH
        ; The frame's interrupt.
        lda #$80
        sta INTERRUPTS_ON
        cli
main:   bit KEY_WAITING
        bpl main
        lda KEY
        ldx KEYS
        sta SCREEN+80,x
        inc KEYS
        jmp main

irq:    pha
        bit INTERRUPTS
        bpl not_frame
        sta INTERRUPTS
        inc FRAMES
        bne not_frame
        inc FRAMES+1
not_frame:
        bit VIA_IFR
        bvc not_timer
        lda VIA_T1CL
        inc TIMERS
        bne not_timer
        inc TIMERS+1
not_timer:
        lda ACIA_STATUS
        and #$08
        beq not_serial
        lda ACIA_DATA
        sta ACIA_DATA
not_serial:
        pla
nmi:    rti

text:   .byte "Skink test", 0

.segment "VECTORS"
        .word nmi, reset, irq
