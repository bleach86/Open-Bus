; System software startup: the BIOS jumps to system_start when there is no
; cart to boot. The menu (main) runs from here; exit() starts it again.

        .export system_start, _exit
        .export __STARTUP__ : absolute = 1
        .import copydata, zerobss, initlib, _main
        .import __RAM_START__, __RAM_SIZE__

        .include "zeropage.inc"

        .segment "STARTUP"

system_start:
        ldx     #$FF
        txs
        lda     #<(__RAM_START__ + __RAM_SIZE__)
        sta     c_sp
        lda     #>(__RAM_START__ + __RAM_SIZE__)
        sta     c_sp+1
        jsr     zerobss
        jsr     copydata
        jsr     initlib
        jsr     _main
_exit:  jmp     system_start
