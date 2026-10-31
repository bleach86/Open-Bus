.segment "CODE"
; Ctrl-C waiting at the terminal? Taken if so, and STOP, as it runs into
; with A 3 and Z and C set; any other key is left for INPUT or GET.
ISCNTC:
        lda     TERM_IN
        cmp     #$03
        beq     @stop
        rts
@stop:
        lda     #0
        sta     TERM_IN
        lda     #$03
        cmp     #$03
;!!! runs into "STOP"
