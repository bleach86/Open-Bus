; The clock card's firmware (src/machines/apple2/clock_card.rs): its 256
; bytes, at $Cn00 in whichever slot n it's in, so nothing in it depends on
; n but what it's told. It answers as a Thunderclock does, as ProDOS knows
; one: ProDOS finds it by its signature, then for the time sets its form
; (A, '#', numbers) at $Cn0B and reads it at $Cn08, with X its $Cn, as
; cards' firmware is called (ProDOS's own RAM over the monitor's ROM
; then, so it can't look for itself on the stack), into the input
; buffer: "MO,DW,DT,HR,MN,SC" and a return, in ASCII with bit 7 set: the
; month, the day of the week (0 Sunday), the date, the hour (24), the
; minute and the second.
;
; The card's registers, at $C080 + n0 (X = n0):
;   +0  written: the time now taken, its text from the start
;   +1  read: its text's next character, to its return

IORTS   = $FF58                 ; an RTS in the monitor's ROM, $60: for BIT, V
INBUF   = $0200                 ; the input buffer
CARD    = $C080                 ; the card's registers, with X its slot's n0

        .segment "CODE"

; The signature ProDOS looks for: $08, $28, $58, $70 at $Cn00, $Cn02,
; $Cn04 and $Cn06. IORTS's byte is an RTS, $60: BIT sets V, BVS goes.
        php
        sei
        plp
        bit     IORTS
        bvs     init

; The entries, at $Cn08 and $Cn0B.
read:   clc
        bcc     time
form:   clc
        bcc     set

init:                           ; PR#n or IN#n: nothing
set:    rts                     ; '#': the only form it has

; The time's text into the input buffer.
time:   txa                     ; $Cn, as the caller has it
        asl     a
        asl     a
        asl     a
        asl     a
        tax                     ; X: the slot's n0
        sta     CARD+0,x        ; the time, now
        ldy     #0
next:   lda     CARD+1,x
        sta     INBUF,y
        iny
        cmp     #$8D            ; to its return
        bne     next
        rts
