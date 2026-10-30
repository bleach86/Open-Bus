; bank_call: the trampoline for calls into another bank, for cc65's
; `#pragma wrapped-call (push, bank_call, bank)` (see machine.h). cc65 puts
; the function's address in ptr4 and its bank, from the linker config, in
; tmp4. The trampoline lives in bank 0, which stays at $8000; the callee's
; bank goes into slot 1 ($A000) for the call, and the caller's comes back.

        .include "machine.inc"

        .export _bank_call
        .import callptr4
        .importzp tmp4

        .segment "CODE"

; A, X and Y are the callee's arguments (fastcall and variadic) on the way in
; and its result on the way out, so they pass through untouched.
_bank_call:
        sta     a_save
        lda     BANK1                   ; the caller's bank, for after
        pha
        lda     tmp4
        sta     BANK1
        lda     a_save
        jsr     callptr4
        sta     a_save
        pla
        sta     BANK1
        lda     a_save
        rts

        .segment "BSS"

a_save: .res    1
