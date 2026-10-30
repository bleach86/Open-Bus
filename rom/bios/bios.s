; BIOS: reset, the jump table and the small routines that need no driver.
; The jump table and its calling convention are documented in machine.inc.
;
; Reset sets up the CPU and the video chip, then boots the cart if it is
; enabled and has a program header; otherwise it starts the system software
; (the menu). The BIOS never takes the machine back from a cart.

        .include "machine.inc"

        .import video_init, font_tiles
        .import chrout, print, waitkey, setcursor, getcursor, setcolor, clear
        .import getkey
        .import system_start

BIOS_REVISION = 4

; --- jump table -------------------------------------------------------------

.macro entry address, routine
        .assert * = address, error, "BIOS jump table entry out of place"
        jmp     routine
.endmacro

        .segment "JUMPTABLE"

        entry   BIOS_VERSION, version
        entry   BIOS_CHROUT, chrout
        entry   BIOS_PRINT, print
        entry   BIOS_GETKEY, getkey
        entry   BIOS_WAITKEY, waitkey
        entry   BIOS_SETCURSOR, setcursor
        entry   BIOS_GETCURSOR, getcursor
        entry   BIOS_SETCOLOR, setcolor
        entry   BIOS_CLEAR, clear
        entry   BIOS_WAITFRAME, waitframe
        entry   BIOS_VIDEORESET, video_init
        entry   BIOS_FONT, font_tiles

        .segment "CODE"

; --- reset ------------------------------------------------------------------

reset:  sei
        cld
        ldx     #$FF
        txs
        ldx     #3                      ; NMI and IRQ do nothing until hooked
@vector:
        lda     default_vectors,x
        sta     NMI_VECTOR,x
        dex
        bpl     @vector
        jsr     video_init
        lda     #15                     ; the sound chip resets silent
        sta     SOUND_MODE
        lda     CART_CTL                ; an enabled cart takes over
        and     #CART_ENABLED
        beq     @system
        ldx     #3
@check: lda     PROGRAM_HEADER,x
        cmp     program_id,x
        bne     @system
        dex
        bpl     @check
        jmp     (PROGRAM_ENTRY)
@system:
        jmp     system_start

nmi:    jmp     (NMI_VECTOR)
irq:    jmp     (IRQ_VECTOR)
ignore: rti

; --- jump table routines ----------------------------------------------------

version:
        lda     #BIOS_REVISION
        ldx     #0
        rts

; FRAME_WAIT halts until vblank or an interrupt, whichever comes first: wait
; on until it was vblank, the frame count moving on.
waitframe:
        lda     FRAME_COUNT
@wait:  sta     FRAME_WAIT
        cmp     FRAME_COUNT
        beq     @wait
        rts

        .segment "RODATA"

default_vectors:
        .addr   ignore                  ; NMI
        .addr   ignore                  ; IRQ

program_id:
        .byte   PROGRAM_MAGIC, PROGRAM_VERSION

        .segment "VECTORS"

        .addr   nmi
        .addr   reset
        .addr   irq
