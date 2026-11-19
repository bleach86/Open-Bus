; The Super Serial Card's firmware (src/machines/apple2/serial_card.rs), our
; own: its 256 bytes, at $Cn00 in whichever slot n it's in, so nothing in it
; depends on n but what it finds. It has what programs look for in an SSC:
; its signature, $38 at $Cn05, $18 at $Cn07, $01 at $Cn0B and $31 at $Cn0C;
; BASIC's entries, PR#n and IN#n, a character out to its 6551 or in from
; it; and Pascal 1.1's, at the offsets at $Cn0D to $Cn10, called with X its
; $Cn and Y its n0. Terminal programs drive the 6551 themselves.
;
; As BASIC has it: PR#n, each character printed sent, bit 7 clear (not on
; the screen); IN#n, each one asked for the next received, bit 7 set. The
; first call, at $Cn00, sets the 6551 going, 9600 baud, 8 bits, no parity,
; and points CSW or KSW at the entry for it, $Cn07 or $Cn05.
;
; The card's registers, at $C080 + n0 (Y = n0):
;   +8  the 6551's data
;   +9  its status: bit 4, the transmitter's free; 3, a byte received
;   +A  its command
;   +B  its control

IORTS   = $FF58                 ; an RTS in the monitor's ROM, $60: for BIT, V
CSWL    = $36                   ; the output hook
CSWH    = $37
KSWL    = $38                   ; the input hook
KSWH    = $39
MSLOT   = $07F8                 ; the $Cn of the card in use, as cards keep it
DATA    = $C088
STATUS  = $C089
COMMAND = $C08A
CONTROL = $C08B

        .segment "CODE"

; $Cn00: the first call, from PR#n or IN#n. IORTS's byte, $60, sets V.
        bit     IORTS
        bvs     init
; $Cn05: the input entry, SEC; the $90 a BCC, not taken, over the CLC.
input:  sec
        .byte   $90
; $Cn07: the output entry, CLC.
output: clc
        clv
        bvc     common
; $Cn0B and $Cn0C: Pascal 1.1's generic signature, and the SSC's ID.
        .byte   $01, $31
; $Cn0D to $Cn10: the Pascal entries' offsets.
        .byte   <pinit, <pread, <pwrite, <pstatus

; The first call: the 6551 set going; CSW or KSW, whichever's at $Cn00,
; pointed at its entry; then on to it, V set for output.
init:   pha
        txa
        pha
        tya
        pha
        jsr     slot
        lda     #$1E            ; 9600 baud, 8 bits, a stop bit
        sta     CONTROL,y
        lda     #$0B            ; DTR, no parity, no interrupts
        sta     COMMAND,y
        lda     KSWL
        bne     :+
        lda     KSWH
        cmp     MSLOT
        bne     :+
        lda     #<input
        sta     KSWL
:       clv                     ; input, unless CSW was this
        lda     CSWL
        bne     :+
        lda     CSWH
        cmp     MSLOT
        bne     :+
        lda     #<output
        sta     CSWL
        bit     IORTS
:       pla                     ; PLA, TAX and TAY leave V
        tay
        pla
        tax
        pla
        bvs     output
        bvc     input

; MSLOT the card's $Cn, as the stack has its return address after a JSR
; to an RTS of the monitor's; Y its n0.
slot:   jsr     IORTS
        tsx
        lda     $0100,x
        sta     MSLOT
        and     #$0F
        asl     a
        asl     a
        asl     a
        asl     a
        tay
        rts

; BASIC's: C set, the next byte in, into A, bit 7 set; C clear, A's out,
; bit 7 clear. X and Y as they were.
common: php
        pha
        txa
        pha
        tya
        pha
        jsr     slot
        tsx                     ; $0101,X Y, $0102 X, $0103 A, $0104 P
        lda     $0104,x
        lsr     a               ; its C
        bcs     in
out:    lda     STATUS,y
        and     #$10
        beq     out
        lda     $0103,x
        and     #$7F
        sta     DATA,y
        bcc     done            ; C clear still
in:     lda     STATUS,y
        and     #$08
        beq     in
        lda     DATA,y
        ora     #$80
        sta     $0103,x
done:   pla
        tay
        pla
        tax
        pla
        plp
        rts

; Pascal's, with X $Cn and Y n0; each X 0, no error.
pinit:  lda     #$1E
        sta     CONTROL,y
        lda     #$0B
        sta     COMMAND,y
        ldx     #0
        rts

pread:  lda     STATUS,y
        and     #$08
        beq     pread
        lda     DATA,y
        ldx     #0
        rts

pwrite: pha
:       lda     STATUS,y
        and     #$10
        beq     :-
        pla
        sta     DATA,y
        ldx     #0
        rts

; A 0: C set, ready for a byte out; A 1: C set, a byte in.
pstatus:
        cmp     #1
        lda     STATUS,y
        bcs     :+
        and     #$10
        bne     yes
        beq     no
:       and     #$08
        bne     yes
no:     ldx     #0
        clc
        rts
yes:    ldx     #0
        sec
        rts
