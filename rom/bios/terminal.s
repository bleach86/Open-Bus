; Terminal: text on the video chip's background map, 40x30 characters, and
; the cursor (a sprite) while waiting for a key.
;
; A character is a map entry: its code is the tile, the text colour the
; palette. The map is 32 rows deep and the screen shows 30, starting at map
; row `top`; scrolling clears the row coming into view and moves SCROLLY
; down a row, as hardware scrolling was done.

        .include "machine.inc"

        .export chrout, print, waitkey, setcursor, getcursor, setcolor, clear
        .import getkey

        .segment "BIOSZP": zeropage

column: .res    1
row:    .res    1                       ; on screen, 0-29
color:  .res    1                       ; palette for new text, 0-15
top:    .res    1                       ; the map row at the top of the screen
text:   .res    2                       ; print: the next character
key:    .res    1                       ; waitkey: the key to return
page:   .res    1                       ; set_map: scratch
low:    .res    1

        .segment "CODE"

; VADDR: the map entry of screen cell (X, Y).
set_cell:
        tya
        clc
        adc     top
        and     #MAP_ROWS - 1
        ; fall through

; VADDR: map row A, column X: VRAM_MAP + row * 128 + column * 2.
set_map:
        lsr     a                       ; row / 2: the page; row & 1 in carry
        sta     page
        lda     #0
        ror     a                       ; (row & 1) << 7
        sta     low
        txa
        asl     a                       ; column * 2, under 128
        ora     low
        sta     VADDR
        lda     page
        clc
        adc     #>VRAM_MAP
        sta     VADDR+1
        rts

; Fill map row A with spaces in the current colour.
clear_row:
        ldx     #0
        jsr     set_map
        ldx     #SCREEN_COLUMNS
@cell:  lda     #' '
        sta     VDATA
        lda     color
        asl     a
        asl     a
        asl     a
        asl     a
        sta     VDATA
        dex
        bne     @cell
        rts

; Clear the whole map to the current colour; cursor home, no scroll.
clear:  lda     #MAP_ROWS - 1
@row:   pha
        jsr     clear_row
        pla
        sec
        sbc     #1
        bpl     @row
        lda     #0
        sta     top
        sta     SCROLLY
        sta     SCROLLY+1
        sta     row
        sta     column
        rts

; A: a glyph ($20-$FF) or a control byte (see machine.inc).
chrout: cmp     #' '
        bcc     control
        pha
        ldx     column
        ldy     row
        jsr     set_cell
        pla
        sta     VDATA
        lda     color
        asl     a
        asl     a
        asl     a
        asl     a
        sta     VDATA
        inc     column
        lda     column
        cmp     #SCREEN_COLUMNS
        bcs     newline                 ; wrap at the right edge
        rts

control:
        cmp     #CH_NEWLINE
        beq     newline
        cmp     #CH_RETURN
        beq     return
        cmp     #CH_BACKSPACE
        beq     backspace
        cmp     #CH_CLEAR
        beq     clear
        rts                             ; other control bytes do nothing

return: lda     #0
        sta     column
        rts

backspace:
        lda     column
        beq     @done
        dec     column
@done:  rts

newline:
        lda     #0
        sta     column
        lda     row
        cmp     #SCREEN_ROWS - 1
        bcs     scroll
        inc     row
        rts

; Bring a cleared row into view at the bottom; the cursor stays on it.
scroll: inc     top
        lda     top
        and     #MAP_ROWS - 1
        sta     top
        clc
        adc     #SCREEN_ROWS - 1
        and     #MAP_ROWS - 1
        jsr     clear_row
        lda     top
        asl     a
        asl     a
        asl     a
        sta     SCROLLY                 ; top * 8: under 256
        lda     #0
        sta     SCROLLY+1
        rts

; A/X: NUL-terminated text.
print:  sta     text
        stx     text+1
@next:  ldy     #0
        lda     (text),y
        beq     @done
        jsr     chrout
        inc     text
        bne     @next
        inc     text+1
        bne     @next
@done:  rts

; X: column, Y: row. Carry set, and the cursor unmoved, if off screen.
setcursor:
        cpx     #SCREEN_COLUMNS
        bcs     @off
        cpy     #SCREEN_ROWS
        bcs     @off
        stx     column
        sty     row
        clc
@off:   rts

getcursor:
        ldx     column
        ldy     row
        rts

; A: the colour for new text, 0-15. -> A: the previous one.
setcolor:
        and     #$0F
        ldx     color
        sta     color
        txa
        ldx     #0
        rts

; Wait for a key, with the cursor showing: an underline in the text colour
; under the cursor's cell.
waitkey:
        lda     #<(VRAM_SPRITES + CURSOR_SPRITE * 4)
        sta     VADDR
        lda     #>(VRAM_SPRITES + CURSOR_SPRITE * 4)
        sta     VADDR+1
        lda     row                     ; Y
        asl     a
        asl     a
        asl     a
        sta     VDATA
        lda     column                  ; X, 9 bits: column * 8
        asl     a
        asl     a
        asl     a
        sta     VDATA
        lda     #CURSOR_TILE
        sta     VDATA
        lda     column                  ; X bit 8 is column bit 5
        lsr     a
        lsr     a
        lsr     a
        lsr     a
        lsr     a
        and     #1
        sta     key
        lda     color
        asl     a
        asl     a
        asl     a
        asl     a
        ora     key
        sta     VDATA
@wait:  jsr     getkey
        cmp     #0
        bne     @key
        sta     FRAME_WAIT
        beq     @wait
@key:   sta     key
        lda     #<(VRAM_SPRITES + CURSOR_SPRITE * 4)
        sta     VADDR
        lda     #>(VRAM_SPRITES + CURSOR_SPRITE * 4)
        sta     VADDR+1
        lda     #SPRITE_HIDDEN
        sta     VDATA
        lda     key
        ldx     #0
        rts
