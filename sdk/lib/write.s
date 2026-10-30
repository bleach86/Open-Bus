; int __fastcall__ write (int fd, const void* buf, unsigned count);
;
; Every descriptor writes to the terminal: stdout and stderr both go to the
; screen, and '\n' starts a new line.

        .include "machine.inc"

        .import popax, popptr1
        .importzp ptr1, ptr2, ptr3

        .export _write

.proc   _write
        sta     ptr3                    ; count: the result
        stx     ptr3+1
        sta     ptr2                    ; bytes left
        stx     ptr2+1
        jsr     popptr1                 ; buf
        jsr     popax                   ; fd: unused

next:   lda     ptr2
        ora     ptr2+1
        beq     done
        ldy     #0
        lda     (ptr1),y
        jsr     BIOS_CHROUT
        inc     ptr1
        bne     :+
        inc     ptr1+1
:       lda     ptr2
        bne     :+
        dec     ptr2+1
:       dec     ptr2
        jmp     next

done:   lda     ptr3
        ldx     ptr3+1
        rts
.endproc
