; Save memory from C. SAVE_DATA gives a byte a read, and cc65's optimizer
; can't be trusted to read a port each time C says to: it may use the byte
; it read for the line before instead. So these loop in assembly.
;
; void __fastcall__ save_read(unsigned address, void *data, unsigned length);
; void __fastcall__ save_write(unsigned address, const void *data, unsigned length);

        .include "machine.inc"

        .import popax, popptr1
        .importzp ptr1, ptr2

        .export _save_read, _save_write

; The arguments: length to ptr2, data to ptr1, the address to SAVE_ADDR.
.proc   arguments
        sta     ptr2
        stx     ptr2+1
        jsr     popptr1
        jsr     popax
        sta     SAVE_ADDR
        stx     SAVE_ADDR+1
        ldy     #0
        ldx     ptr2+1                  ; whole pages first
        rts
.endproc

.proc   _save_read
        jsr     arguments
        beq     rest
page:   lda     SAVE_DATA
        sta     (ptr1),y
        iny
        bne     page
        inc     ptr1+1
        dex
        bne     page
rest:   ldx     ptr2                    ; then what is left
        beq     done
byte:   lda     SAVE_DATA
        sta     (ptr1),y
        iny
        dex
        bne     byte
done:   rts
.endproc

.proc   _save_write
        jsr     arguments
        beq     rest
page:   lda     (ptr1),y
        sta     SAVE_DATA
        iny
        bne     page
        inc     ptr1+1
        dex
        bne     page
rest:   ldx     ptr2
        beq     done
byte:   lda     (ptr1),y
        sta     SAVE_DATA
        iny
        dex
        bne     byte
done:   rts
.endproc
