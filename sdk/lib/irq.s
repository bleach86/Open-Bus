; Interrupts for C: cc65's interruptors (set_irq's handler, for one; see
; 6502.h) on the machine's IRQ. Linked only into a program that has one:
; cc65's callirq brings it in, and calls initirq at startup and doneirq at
; the end.

        .include "machine.inc"

        .export initirq, doneirq
        .import callirq

        .segment "BSS"

previous:
        .res    2                       ; the RAM vector as it was

        .segment "CODE"

; The BIOS's RAM vector to the handler below.
initirq:
        lda     IRQ_VECTOR
        sta     previous
        lda     IRQ_VECTOR+1
        sta     previous+1
        lda     #<irq
        sta     IRQ_VECTOR
        lda     #>irq
        sta     IRQ_VECTOR+1
        rts

doneirq:
        sei
        lda     previous
        sta     IRQ_VECTOR
        lda     previous+1
        sta     IRQ_VECTOR+1
        rts

; The registers kept around the interruptors.
irq:    pha
        txa
        pha
        tya
        pha
        jsr     callirq
        pla
        tay
        pla
        tax
        pla
        rti
