; VRAM from C: the video chip's data port takes a byte at a time, so these
; loop in assembly.
;
; void __fastcall__ vram_write(unsigned address, const void *data, unsigned length);
; void __fastcall__ vram_fill(unsigned address, unsigned char value, unsigned length);

        .include "machine.inc"

        .import popa, popax, popptr1
        .importzp ptr1, ptr2

        .export _vram_write, _vram_fill

.proc   _vram_write
        sta     ptr2                    ; length
        stx     ptr2+1
        jsr     popptr1                 ; data
        jsr     popax                   ; address
        sta     VADDR
        stx     VADDR+1
        ldy     #0
        ldx     ptr2+1                  ; whole pages first
        beq     rest
page:   lda     (ptr1),y
        sta     VDATA
        iny
        bne     page
        inc     ptr1+1
        dex
        bne     page
rest:   ldx     ptr2                    ; then what is left
        beq     done
byte:   lda     (ptr1),y
        sta     VDATA
        iny
        dex
        bne     byte
done:   rts
.endproc

.proc   _vram_fill
        sta     ptr2                    ; length
        stx     ptr2+1
        jsr     popa                    ; value
        pha
        jsr     popax                   ; address
        sta     VADDR
        stx     VADDR+1
        pla
        ldy     ptr2                    ; the low byte's worth, then pages
        ldx     ptr2+1
        inx
        cpy     #0
        beq     next
loop:   sta     VDATA
        dey
        bne     loop
next:   dex
        bne     loop
        rts
.endproc
