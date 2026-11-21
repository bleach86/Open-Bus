//! The 16550 UART: its divisor and baud rate; a byte sent, as long as its
//! rate says, and its FIFO; bytes received, at its rate, the FIFO's
//! trigger and its timeout, an overrun; RTS dropped by auto flow control,
//! the other end waiting; CTS holding what's sent; the modem's lines and
//! their changes; loopback; the interrupts, the most pressing first.

#[cfg(test)]
mod tests {
    use crate::chips::uart16550::Uart16550;

    /// The X16's CPU clock; a byte's cycles at 115,200 baud, 10 bits.
    const CLOCK: u32 = 8_000_000;
    const BYTE: u64 = (CLOCK as u64 * 10).div_ceil(115_200);

    /// Set up as x16net sets the X16 card's: 115,200 baud (divisor 8), 8
    /// bits, no parity, a stop bit; the FIFOs on, the trigger 8; DTR, RTS,
    /// OUT1 and auto flow control.
    fn uart() -> Uart16550 {
        let mut uart = Uart16550::new(CLOCK);
        uart.write(3, 0x80);
        uart.write(0, 8);
        uart.write(1, 0);
        uart.write(3, 0x03);
        uart.write(1, 0);
        uart.write(2, 0x87);
        uart.write(4, 0x27);
        uart.set_lines(true, true, false, false);
        uart.read(6);
        uart
    }

    #[test]
    fn its_divisor_is_behind_dlab_and_sets_its_rate() {
        let mut uart = uart();
        assert_eq!(uart.baud(), 115_200);
        uart.write(3, 0x83);
        assert_eq!((uart.read(0), uart.read(1)), (8, 0));
        uart.write(0, 96);
        uart.write(3, 0x03);
        assert_eq!(uart.baud(), 9_600);
        uart.write(7, 0x55);
        assert_eq!(uart.read(7), 0x55, "its scratch register, for finding it");
    }

    #[test]
    fn a_byte_is_sent_in_its_time_its_fifo_after() {
        let mut uart = uart();
        uart.write(0, b'A');
        uart.write(0, b'T');
        assert_eq!(uart.read(5) & 0x60, 0x00, "T waiting, A going");
        uart.run(BYTE - 1);
        assert!(uart.outgoing.is_empty());
        uart.run(1);
        assert_eq!(uart.outgoing, b"A");
        assert_eq!(uart.read(5) & 0x60, 0x20, "T going: the FIFO empty");
        uart.run(BYTE);
        assert_eq!(uart.outgoing, b"AT");
        assert_eq!(uart.read(5) & 0x60, 0x60, "all sent");
        for byte in 0..20 {
            uart.write(0, byte);
        }
        uart.run(BYTE * 20);
        assert_eq!(
            uart.outgoing.len(),
            2 + 17,
            "16 in the FIFO, 1 going, the rest lost"
        );
    }

    #[test]
    fn bytes_come_in_at_its_rate_and_rts_drops_at_the_trigger() {
        let mut uart = uart();
        uart.flow = true;
        uart.incoming.extend(0..20u8);
        assert!(uart.rts());
        uart.run(BYTE * 3);
        assert_eq!(uart.read(5) & 1, 1);
        assert_eq!(uart.read(0), 0);
        uart.run(BYTE * 20);
        assert!(!uart.rts(), "8 waiting, the trigger: RTS dropped");
        assert_eq!(
            uart.incoming.len(),
            20 - 9,
            "the rest held at the other end"
        );
        for byte in 1..9 {
            assert_eq!(uart.read(0), byte);
        }
        assert!(uart.rts());
        uart.run(BYTE * 20);
        assert_eq!(uart.read(0), 9, "and on from there, none lost");
        assert_eq!(uart.read(5) & 2, 0, "no overrun");
    }

    #[test]
    fn with_no_flow_control_what_comes_past_its_fifo_is_lost() {
        let mut uart = uart();
        uart.incoming.extend(0..20u8);
        uart.run(BYTE * 20);
        assert_eq!(uart.read(5) & 3, 3, "received, and an overrun");
        assert_eq!(uart.read(5) & 2, 0, "cleared, as it's read");
        let received: Vec<u8> = (0..16).map(|_| uart.read(0)).collect();
        assert_eq!(received, (0..16).collect::<Vec<u8>>());
        assert_eq!(uart.read(5) & 1, 0);
    }

    #[test]
    fn cts_down_holds_what_it_sends_with_auto_flow_control() {
        let mut uart = uart();
        uart.set_lines(false, true, false, false);
        uart.write(0, b'X');
        uart.run(BYTE * 4);
        assert!(uart.outgoing.is_empty());
        uart.set_lines(true, true, false, false);
        uart.run(BYTE);
        assert_eq!(uart.outgoing, b"X");
    }

    #[test]
    fn its_interrupts_come_most_pressing_first() {
        let mut uart = uart();
        assert_eq!(uart.read(2), 0xC1, "the FIFOs on, none");
        uart.write(1, 0x0F);
        assert_eq!(uart.read(2) & 0x0F, 0x02, "the transmitter empty, at once");
        assert_eq!(uart.read(2) & 0x0F, 0x01, "read: gone");
        uart.incoming.extend(b"hello");
        uart.run(BYTE * 5);
        assert!(!uart.irq(), "5 bytes, the trigger 8");
        uart.run(BYTE * 4);
        assert_eq!(
            uart.read(2) & 0x0F,
            0x0C,
            "none for 4 bytes' time: the timeout"
        );
        uart.read(0);
        assert!(!uart.irq(), "a byte taken: the wait starts again");
        uart.incoming.extend(b"0123456789ABCDEF");
        uart.run(BYTE * 6);
        assert_eq!(uart.read(2) & 0x0F, 0x04, "at the trigger");
        uart.run(BYTE * 20);
        assert_eq!(uart.read(2) & 0x0F, 0x06, "an overrun, first");
        uart.read(5);
        assert_eq!(uart.read(2) & 0x0F, 0x04);
        while uart.read(5) & 1 != 0 {
            uart.read(0);
        }
        uart.set_lines(true, true, false, true);
        assert_eq!(uart.read(2) & 0x0F, 0x00, "DCD up");
        assert_eq!(uart.read(6), 0xB8, "CTS, DSR, DCD; DCD changed");
        assert!(!uart.irq());
    }

    #[test]
    fn in_loopback_what_it_sends_comes_back_and_its_lines_are_its_own() {
        let mut uart = uart();
        uart.write(4, 0x10 | 0x08 | 0x01);
        assert_eq!(
            uart.read(6) & 0xF0,
            0xA0,
            "DTR's DSR, OUT2's DCD; no RTS, no CTS"
        );
        uart.write(0, 0x5A);
        uart.run(BYTE);
        assert!(uart.outgoing.is_empty());
        assert_eq!(uart.read(0), 0x5A);
        uart.write(4, 0x27);
        assert!(uart.rts() && uart.dtr());
    }

    #[test]
    fn due_says_when_the_next_byte_is() {
        let mut uart = uart();
        assert_eq!(uart.due(), u64::MAX, "nothing to do");
        uart.write(0, 1);
        assert_eq!(uart.due(), BYTE);
        uart.run(BYTE - 10);
        assert_eq!(uart.due(), 10);
        uart.run(10);
        uart.flow = true;
        uart.incoming.push_back(2);
        assert_eq!(uart.due(), BYTE);
        uart.write(4, 0x25);
        assert_eq!(uart.due(), u64::MAX, "RTS down: the other end waits");
    }
}
