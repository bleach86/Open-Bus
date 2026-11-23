; Microsoft BASIC (mist64's msbasic) for this machine: its terminal, a key
; in at $FFF9 (0 when there is none; writing 0 takes it) and a character
; out at $FFF8; RAM below $C000, this ROM from $C000. Lines are typed and
; edited in the machine's line editor, as on the PET, and BASIC is given
; them finished: its own line editing ("@", "_") is off. Letters typed are
; upper case, as on the machines BASIC first ran on.

; configuration
CONFIG_2A := 1

CONFIG_NO_LINE_EDITING := 1
CONFIG_SCRTCH_ORDER := 2

; zero page
ZP_START1 = $00
ZP_START2 = $0D
ZP_START3 = $5B
ZP_START4 = $66

;extra ZP variables
USR             := $000A

; no tape or disk: LOAD and SAVE are ?FC ERROR
LOAD            := IQERR
SAVE            := IQERR

; constants
STACK_TOP       := $FC
SPACE_FOR_GOSUB := $3E
WIDTH           := 72
WIDTH2          := 56

; memory layout: page 2 is the line editor's, BASIC's program after it
RAMSTART2       := $0300

TERM_OUT        := $FFF8
TERM_IN         := $FFF9
POWER_OFF       := $FFF7        ; written: the machine turned off

; the line editor
ED_BUF          := $0200        ; the line, LINE_MAX at most
ED_LEN          := $0248
ED_POS          := $0249        ; the cursor: 0 to ED_LEN
ED_OUT          := $024A        ; the next of the line to hand to BASIC
ED_HANDING      := $024B        ; nonzero while the line is handed over
ED_X            := $024C
ED_Y            := $024D
ED_FIRST        := $024E        ; an escape sequence's first byte
ED_PROMPT       := $024F        ; nonzero when the line's for BASIC's prompt
LINE_MAX        = 71            ; as long as BASIC's input buffer holds

.segment "EXTRA"

; A key, waited for, in A; X and Y kept. For GETLN, which reads BASIC's
; lines (it is what calls from GETLN+2), the next key of a line from the
; line editor; for GET, a key on its own. A line is for BASIC's prompt,
; not a program's INPUT, when INLIN was called from the prompt's loop
; (L2351), as the stack has it: past the returns to GETLN and to INLIN.
MONRDKEY:
        stx     ED_X
        sty     ED_Y
        tsx
        lda     $0102,x
        cmp     #>(GETLN+2)
        bne     @key
        lda     $0101,x
        cmp     #<(GETLN+2)
        bne     @key
        ldy     #0
        lda     $0106,x
        cmp     #>(L2351+2)
        bne     @prompt
        lda     $0105,x
        cmp     #<(L2351+2)
        bne     @prompt
        iny
@prompt:
        sty     ED_PROMPT
        jsr     LINEKEY
        jmp     @done
@key:
        jsr     ONEKEY
@done:
        ldx     ED_X
        ldy     ED_Y
        rts

; The next key of the line for BASIC: a line edited first, unless one is
; being handed over; then its keys, then CR.
LINEKEY:
        lda     ED_HANDING
        bne     @hand
        jsr     EDITLINE
@hand:
        ldx     ED_OUT
        cpx     ED_LEN
        bcs     @end
        inc     ED_OUT
        lda     ED_BUF,x
        rts
@end:
        lda     #0
        sta     ED_HANDING
        lda     #$0D
        rts

; A key on its own, for GET: not echoed, as GET's keys aren't. Enter is
; CR, letters upper case; escape sequences are dropped.
ONEKEY:
        jsr     GETKEY
        cmp     #$1B            ; ESC
        bne     @notesc
        jsr     SEQUENCE
        jmp     ONEKEY
@notesc:
        cmp     #$0A            ; Enter
        bne     @upper
        lda     #$0D
@upper:
        jmp     UPPER

; A line typed into ED_BUF, the screen showing it as it is: keys go in at
; the cursor, letters upper case, Left and Right move it, Backspace and
; Delete take out, Home and End go to the ends; Enter ends it. Other control
; keys do nothing.
EDITLINE:
        lda     #0
        sta     ED_LEN
        sta     ED_POS
        sta     ED_OUT
@next:
        jsr     GETKEY
        cmp     #$0A            ; Enter
        beq     @enter
        cmp     #$0D
        beq     @enter
        cmp     #$7F            ; Backspace
        beq     @backspace
        cmp     #$08            ; Ctrl-H
        beq     @backspace
        cmp     #$1B            ; ESC
        beq     @escape
        cmp     #' '
        bcc     @next
        jsr     UPPER
        jsr     ED_INSERT
        jmp     @next
@backspace:
        lda     ED_POS
        beq     @next
        jsr     ED_LEFT
        jsr     ED_DELETE
        jmp     @next
@escape:
        jsr     SEQUENCE
        cmp     #'D'            ; Left
        beq     @left
        cmp     #'C'            ; Right
        beq     @right
        cmp     #'H'            ; Home
        beq     @home
        cmp     #'F'            ; End
        beq     @end
        cmp     #'~'            ; ESC [ n ~: 3 Delete, 1 or 7 Home, 4 or 8 End
        bne     @next
        lda     ED_FIRST
        cmp     #'3'
        beq     @delete
        cmp     #'1'
        beq     @home
        cmp     #'7'
        beq     @home
        cmp     #'4'
        beq     @end
        cmp     #'8'
        beq     @end
        jmp     @next
@left:
        jsr     ED_LEFT
        jmp     @next
@right:
        jsr     ED_RIGHT
        jmp     @next
@delete:
        jsr     ED_DELETE
        jmp     @next
@home:
        jsr     ED_HOME
        jmp     @next
@end:
        jsr     ED_END
        jmp     @next
@enter:
        jsr     ED_END          ; the cursor past the line, for BASIC's CR LF
        jsr     QUITLINE
        lda     #1
        sta     ED_HANDING
        rts

; Q or QUIT typed at BASIC's prompt, not to a program's INPUT: the machine
; turned off, as Ctrl-D turns it off. Either is ?SN ERROR to BASIC, so
; nothing is lost.
QUITLINE:
        lda     ED_PROMPT
        beq     @no
        ldx     ED_LEN
        cpx     #1
        beq     @q
        cpx     #4
        bne     @no
        dex
@quit:
        lda     ED_BUF,x
        cmp     QUITWORD,x
        bne     @no
        dex
        bpl     @quit
        bmi     @off
@q:
        lda     ED_BUF
        cmp     #'Q'
        bne     @no
@off:
        lda     #1
        sta     POWER_OFF
@no:
        rts
QUITWORD:
        .byte   "QUIT"

; After an ESC, the rest of the terminal's sequence for a key: ESC [ or
; ESC O, then any parameters, up to a byte from $40, which is returned,
; the first after the [ or O in ED_FIRST. After ESC on its own: 0, and the
; key after it dropped.
SEQUENCE:
        jsr     GETKEY
        cmp     #'['
        beq     @in
        cmp     #'O'
        beq     @in
        lda     #0
        rts
@in:
        jsr     GETKEY
        sta     ED_FIRST
@parameters:
        cmp     #$40
        bcs     @last
        jsr     GETKEY
        jmp     @parameters
@last:
        rts

; The key in A in at the cursor, the cursor after it; nothing if the line
; is full.
ED_INSERT:
        ldx     ED_LEN
        cpx     #LINE_MAX
        bcs     @full
        pha
@shift:
        cpx     ED_POS          ; the rest one along, from the end
        beq     @put
        lda     ED_BUF-1,x
        sta     ED_BUF,x
        dex
        jmp     @shift
@put:
        pla
        sta     ED_BUF,x
        inc     ED_LEN
        jsr     ED_SHOW
        inc     ED_POS
        jsr     ED_BACK
@full:
        rts

; The key at the cursor out, the rest one back.
ED_DELETE:
        ldx     ED_POS
        cpx     ED_LEN
        bcs     @none
@shift:
        inx
        cpx     ED_LEN
        bcs     @shifted
        lda     ED_BUF,x
        sta     ED_BUF-1,x
        jmp     @shift
@shifted:
        dec     ED_LEN
        jsr     ED_SHOW
        lda     #' '            ; over what was the last key
        jsr     MONCOUT
        lda     #$08
        jsr     MONCOUT
        jsr     ED_BACK
@none:
        rts

; The line from the cursor to its end, out: the screen's cursor at its end.
ED_SHOW:
        ldx     ED_POS
@loop:
        cpx     ED_LEN
        bcs     @done
        lda     ED_BUF,x
        jsr     MONCOUT
        inx
        jmp     @loop
@done:
        rts

; The screen's cursor from the line's end back to the cursor.
ED_BACK:
        lda     ED_LEN
        sec
        sbc     ED_POS
        tax
        beq     @done
        lda     #$08
@loop:
        jsr     MONCOUT
        dex
        bne     @loop
@done:
        rts

ED_LEFT:
        lda     ED_POS
        beq     @done
        dec     ED_POS
        lda     #$08
        jsr     MONCOUT
@done:
        rts

ED_RIGHT:
        ldx     ED_POS
        cpx     ED_LEN
        bcs     @done
        lda     ED_BUF,x
        jsr     MONCOUT
        inc     ED_POS
@done:
        rts

ED_HOME:
        lda     ED_POS
        beq     @done
        jsr     ED_LEFT
        jmp     ED_HOME
@done:
        rts

ED_END:
        ldx     ED_POS
        cpx     ED_LEN
        bcs     @done
        jsr     ED_RIGHT
        jmp     ED_END
@done:
        rts

; A letter in A to upper case.
UPPER:
        cmp     #'a'
        bcc     @done
        cmp     #'z'+1
        bcs     @done
        and     #$DF
@done:
        rts

; A key from the terminal, waited for and taken; X and Y kept.
GETKEY:
        lda     TERM_IN
        beq     GETKEY
        pha
        lda     #0
        sta     TERM_IN         ; taken: ready for the next
        pla
        rts

; A character out; A, X and Y kept.
MONCOUT:
        sta     TERM_OUT
        rts

; Reset: into BASIC, which sets up the stack. No NMI or IRQ comes on this
; machine; they go back at once.
RESET:
        cld
        jmp     COLD_START
NOINT:
        rti

.segment "RESETVEC"
        .word   NOINT, RESET, NOINT
