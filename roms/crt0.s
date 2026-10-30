; The monitor's startup, from the reset vector: the stack, the C runtime,
; then main. exit() starts it again.
;
; NMI and IRQ (BRK too) go through the RAM vectors at $D000 and $D002, so a
; program can set its own handler: "POKE D002 lo hi". They start out at an
; RTI.

        .export _exit
        .export __STARTUP__ : absolute = 1
        .import copydata, zerobss, initlib, _main
        .import __RAM_START__, __RAM_SIZE__

        .include "zeropage.inc"

NMIVEC  = $D000
IRQVEC  = $D002

        .segment "STARTUP"

reset:  cld
        ldx     #$FF
        txs
        lda     #<ignore
        sta     NMIVEC
        sta     IRQVEC
        lda     #>ignore
        sta     NMIVEC+1
        sta     IRQVEC+1
        lda     #<(__RAM_START__ + __RAM_SIZE__)
        sta     c_sp
        lda     #>(__RAM_START__ + __RAM_SIZE__)
        sta     c_sp+1
        jsr     zerobss
        jsr     copydata
        jsr     initlib
        jsr     _main
_exit:  jmp     reset

nmi:    jmp     (NMIVEC)
irq:    jmp     (IRQVEC)
ignore: rti

        .segment "VECTORS"

        .word   nmi, reset, irq
