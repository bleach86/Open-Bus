; Cart startup: the cart file's header, the program header at the start of
; bank 0, and the code the BIOS jumps to at reset when the cart is enabled.
;
; The cart has the machine to itself: it sets up the C runtime (stack, data,
; bss, constructors) and runs main(). When main() returns, or exit() is
; called, the cart is done and the machine waits for the reset button.
;
; A program that runs from RAM (the cfg/cartN-ram.cfg configs) is first
; copied there: CODE, RODATA and DATA, one after another, from banks 1 on,
; a bank at a time through the $A000 slot.

        .export _exit
        .export __STARTUP__ : absolute = 1
        .import copydata, zerobss, initlib, donelib, callmain
        .import __RAM_START__, __RAM_SIZE__
        .import __CODE_RUN__, __DATA_RUN__, __DATA_SIZE__
        .import __CART_BANKS__, __CART_SAVE__ ; from the linker config
        .import __PROGRAM_IN_RAM__

        .include "zeropage.inc"
        .include "machine.inc"

; The cart file's header, ahead of the banks (docs/machine.md).
        .segment "CARTHDR"

        .byte   "6CRT", CART_VERSION, <__CART_BANKS__, 0, <__CART_SAVE__
        .res    CART_HEADER_SIZE - 8, 0

        .segment "HEADER"

        .byte   PROGRAM_MAGIC, PROGRAM_VERSION
        .addr   start

        .segment "STARTUP"

start:  ldx     #$FF
        txs
        lda     #<(__RAM_START__ + __RAM_SIZE__)
        sta     c_sp
        lda     #>(__RAM_START__ + __RAM_SIZE__)
        sta     c_sp+1
        lda     #<__PROGRAM_IN_RAM__
        beq     @cart
        jsr     load_program            ; DATA comes with it
        jsr     zerobss
        jmp     @init
@cart:  jsr     zerobss
        jsr     copydata
@init:  jsr     initlib
        jsr     callmain
_exit:  jsr     donelib
@done:  sta     FRAME_WAIT
        jmp     @done

; The program into RAM, whole pages: up to 255 bytes too many go into BSS,
; which zerobss clears after.
PROGRAM_PAGES = >(__DATA_RUN__ + __DATA_SIZE__ - __CODE_RUN__ + 255)

load_program:
        lda     #<__CODE_RUN__
        sta     ptr2
        lda     #>__CODE_RUN__
        sta     ptr2+1
        lda     #PROGRAM_PAGES
        sta     tmp1                    ; pages left
        lda     #1
        sta     tmp2                    ; the bank
@bank:  lda     tmp2
        sta     BANK1
        lda     #<$A000
        sta     ptr1
        lda     #>$A000
        sta     ptr1+1
        ldx     #$2000 / $100           ; pages in a bank
@page:  ldy     #0
@byte:  lda     (ptr1),y
        sta     (ptr2),y
        iny
        bne     @byte
        inc     ptr1+1
        inc     ptr2+1
        dec     tmp1
        beq     @done
        dex
        bne     @page
        inc     tmp2
        bne     @bank
@done:  lda     #1                      ; as at reset
        sta     BANK1
        rts
