; C bindings for the BIOS (declared in machine.h).
;
; Most BIOS routines take and return values the way cc65 passes them, so
; their C names are simply the jump table addresses. The rest need a few
; instructions to move arguments between the C stack and X/Y.

        .include "machine.inc"

        .import popa, popax
        .importzp tmp1, tmp2

        .export _bios_version, _bios_chrout, _bios_print
        .export _bios_getkey, _bios_waitkey, _bios_setcolor
        .export _bios_clear, _bios_waitframe, _bios_videoreset
        .export _bios_setcursor, _bios_column, _bios_row, _bios_font

_bios_version    = BIOS_VERSION
_bios_chrout     = BIOS_CHROUT
_bios_print      = BIOS_PRINT
_bios_getkey     = BIOS_GETKEY
_bios_waitkey    = BIOS_WAITKEY
_bios_setcolor   = BIOS_SETCOLOR
_bios_clear      = BIOS_CLEAR
_bios_waitframe  = BIOS_WAITFRAME
_bios_videoreset = BIOS_VIDEORESET

        .segment "CODE"

; void __fastcall__ bios_setcursor(unsigned char column, unsigned char row);
_bios_setcursor:
        pha                             ; row
        jsr     popa                    ; column
        tax
        pla
        tay
        jmp     BIOS_SETCURSOR

; void __fastcall__ bios_font(unsigned address, unsigned char first,
;                             unsigned char count, unsigned char colors);
_bios_font:
        sta     tmp1                    ; colors
        jsr     popa
        sta     tmp2                    ; count
        jsr     popa
        pha                             ; first
        jsr     popax                   ; address
        sta     VADDR
        stx     VADDR+1
        pla
        tax
        ldy     tmp2
        lda     tmp1
        jmp     BIOS_FONT

; unsigned char bios_column(void);
_bios_column:
        jsr     BIOS_GETCURSOR
        txa
        ldx     #0
        rts

; unsigned char bios_row(void);
_bios_row:
        jsr     BIOS_GETCURSOR
        tya
        ldx     #0
        rts
