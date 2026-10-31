; The program for the WDC 65C02's WAI and STP test (src/cpu/tests/
; wai_stp.rs). The test starts the CPU, a WDC 65C02, at one of these
; labels, raises IRQ or NMI on its pins, and runs it, to see it wait,
; wake, and take the interrupt or not; or stop, until it's reset.

irqs    = $00                   ; IRQs taken
nmis    = $01                   ; NMIs taken

        .setcpu "W65C02"
        .segment "CODE"

; I clear: WAI waits, until an IRQ, taken.
wai_irq:
        cli
        wai
after_irq:
        nop
        jmp     loop

; I set: WAI waits, until an IRQ, not taken: on, after the WAI.
wai_masked:
        sei
        wai
after_masked:
        nop
        jmp     loop

; STP: stopped, until reset.
stop:   stp
after_stop:
        nop

loop:   nop
        jmp     loop

irq:    inc     irqs
        rti

nmi:    inc     nmis
        rti

        .segment "VECTORS"

        .word   nmi, loop, irq
