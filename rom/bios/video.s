; Video set-up at reset, and BIOS_VIDEORESET: the default palettes, the font
; as tiles $20-$FF, the cursor's tile, every sprite hidden and a clear screen
; in the default text colour. Software is free to change any of it after.
;
; BIOS_FONT: the font as tiles in any two colours, for software that wants
; text in more colours than one palette per colour.

        .include "machine.inc"

        .export video_init, font_tiles
        .import font, clear, setcolor

        .segment "BIOSZP": zeropage

src:    .res    2                       ; the next font byte
bits:   .res    1                       ; its pixels not yet drawn
pairs:  .res    1                       ; pixel pairs left in this byte

        .segment "BIOSRAM"

pair:   .res    4                       ; two pixels' colours, by their font bits
fg:     .res    1                       ; colour of the set pixels
bg:     .res    1                       ; and of the rest
glyphs: .res    1                       ; glyphs left

        .segment "CODE"

.macro set_vaddr address
        lda     #<(address)
        sta     VADDR
        lda     #>(address)
        sta     VADDR+1
.endmacro

video_init:
        lda     #0
        sta     VCTRL
        sta     BACKDROP
        sta     SCROLLX
        sta     SCROLLX+1

; Palette n: colour 0 transparent, 1 the nth standard colour, 2 black; the
; rest black too.
        set_vaddr VRAM_PALETTES
        ldx     #0
@palette:
        lda     #0
        sta     VDATA
        sta     VDATA
        lda     colors,x
        sta     VDATA
        lda     colors+1,x
        sta     VDATA
        lda     #0
        ldy     #13 * 2 + 2
@black: sta     VDATA
        dey
        bne     @black
        inx
        inx
        cpx     #16 * 2
        bne     @palette

; Tile 0 blank, tile 1 the cursor: an underline in colour 1.
        set_vaddr VRAM_TILES
        lda     #0
        ldy     #32 + 24
@blank: sta     VDATA
        dey
        bne     @blank
        lda     #$11
        ldy     #8
@line:  sta     VDATA
        dey
        bne     @line

; The font: set pixels colour 1, the rest colour 2.
        set_vaddr VRAM_TILES + $20 * 32
        lda     #$21
        ldx     #$20
        ldy     #256 - $20
        jsr     font_tiles

; Every sprite below the screen.
        set_vaddr VRAM_SPRITES
        ldx     #64
@sprite:
        lda     #240
        sta     VDATA
        lda     #0
        sta     VDATA
        sta     VDATA
        sta     VDATA
        dex
        bne     @sprite

; Every sprite's priority: in front.
        set_vaddr VRAM_PRIORITY
        lda     #0
        ldx     #64
@priority:
        sta     VDATA
        dex
        bne     @priority

        lda     #DEFAULT_COLOR
        jsr     setcolor
        jsr     clear
        lda     #VCTRL_DISPLAY | VCTRL_SPRITES
        sta     VCTRL
        rts

; BIOS_FONT: glyphs X to X+Y-1 ($20-$FF, Y at least 1) as tiles from VADDR
; on, the set pixels in colour A bits 0-3 and the rest in bits 4-7. Each
; 1-bit font row becomes four bytes of 4-bit pixels.
font_tiles:
        sty     glyphs
        tay
        and     #$0F
        sta     fg
        tya
        lsr     a
        lsr     a
        lsr     a
        lsr     a
        sta     bg
; Two pixels by their font bits, the left one in bit 1: bg bg, bg fg, fg bg,
; fg fg.
        asl     a
        asl     a
        asl     a
        asl     a
        tay                             ; bg on the left
        ora     bg
        sta     pair
        tya
        ora     fg
        sta     pair+1
        lda     fg
        asl     a
        asl     a
        asl     a
        asl     a
        tay                             ; fg on the left
        ora     bg
        sta     pair+2
        tya
        ora     fg
        sta     pair+3
; src = font + (X - $20) * 8
        txa
        sec
        sbc     #$20
        sta     src
        lda     #0
        sta     src+1
        asl     src
        rol     src+1
        asl     src
        rol     src+1
        asl     src
        rol     src+1
        clc
        lda     src
        adc     #<font
        sta     src
        lda     src+1
        adc     #>font
        sta     src+1
@glyph: ldy     #0
@row:   lda     (src),y
        sta     bits
        lda     #4
        sta     pairs
@pair:  lda     #0
        asl     bits
        rol     a
        asl     bits
        rol     a
        tax
        lda     pair,x
        sta     VDATA
        dec     pairs
        bne     @pair
        iny
        cpy     #8
        bne     @row
        clc
        lda     src
        adc     #8
        sta     src
        bcc     @next
        inc     src+1
@next:  dec     glyphs
        bne     @glyph
        rts

        .segment "RODATA"

; The standard colours, $0RGB.
colors: .word   $000, $C33, $3C3, $CC3, $46E, $C3C, $3CC, $CCC
        .word   $777, $F66, $6F6, $FF6, $79F, $F6F, $6FF, $FFF
