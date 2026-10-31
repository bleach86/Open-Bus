; The program for the CPU's interrupt test (src/cpu/tests/interrupts.rs). The
; test starts the CPU at one of these labels, raises IRQ or NMI on its pins
; and runs it an instruction at a time, to see when the interrupt is taken,
; what was pushed and where RTI goes back to.

irqs    = $00                   ; IRQs taken
nmis    = $01                   ; NMIs taken

        .segment "CODE"

; I is set by reset: CLI lets an IRQ in, after the instruction after it.
cli_then:
        cli
        nop                     ; the IRQ is taken after this
        nop

; I clear, then SEI: an IRQ raised as SEI runs is still taken after it.
sei_then:
        cli
        nop
        sei                     ; the IRQ is taken after this
        nop

; I clear, then a branch taken to the same page: it looks for IRQ before
; its operand, so an IRQ raised before it is taken after it.
branch_then:
        cli
        nop
        clc
        bcc taken               ; the IRQ is taken after this
        nop
taken:  nop

; Somewhere to run while an interrupt does or doesn't come.
loop:   nop
        jmp loop

; The handlers: count, and go back.
irq:    inc irqs
        rti

nmi:    inc nmis
        rti

        .segment "VECTORS"

        .word nmi, cli_then, irq
