//! The 6551 ACIA: a byte sent, as long as its baud rate says, and its
//! interrupt; a byte received, its interrupt, and an overrun; the status
//! register's interrupt cleared by reading it; a programmed reset.

#[cfg(test)]
mod tests {
    use crate::chips::acia6551::Acia6551;

    const CLOCK: u32 = 2_000_000;
    /// 9600 baud, 8 bits, 1 stop bit; and on, the receiver's interrupt on,
    /// the transmitter's.
    const CONTROL_9600: u8 = 0x1E;
    const ON: u8 = 0x01;
    const TRANSMIT_IRQ: u8 = 0x04;
    /// A byte's cycles at 9600 baud, 10 bits, on a 2 MHz clock.
    const BYTE: u32 = CLOCK * 10 / 9600;

    fn acia(command: u8) -> Acia6551 {
        let mut acia = Acia6551::new(CLOCK);
        acia.write(3, CONTROL_9600);
        acia.write(2, command);
        acia
    }

    fn run(acia: &mut Acia6551, cycles: u32) {
        for _ in 0..cycles {
            acia.tick();
        }
    }

    #[test]
    fn a_byte_sent_takes_its_baud_rates_time() {
        let mut acia = acia(ON);
        acia.write(0, b'A');
        assert_eq!(
            acia.read(1) & 0x10,
            0x10,
            "to the transmitter at once: empty"
        );
        acia.write(0, b'B');
        assert_eq!(acia.read(1) & 0x10, 0, "B held: full");
        run(&mut acia, BYTE - 1);
        assert!(acia.outgoing.is_empty(), "A not gone yet");
        run(&mut acia, 1);
        assert_eq!(acia.outgoing, b"A");
        assert_eq!(acia.read(1) & 0x10, 0x10, "B to the transmitter");
        run(&mut acia, BYTE);
        assert_eq!(acia.outgoing, b"AB");
    }

    #[test]
    fn the_transmitter_interrupts_as_its_register_empties_if_its_on() {
        let mut off = acia(ON);
        off.write(0, b'A');
        assert!(!off.irq(), "its interrupt off");
        let mut on = acia(ON | TRANSMIT_IRQ);
        on.write(0, b'A');
        assert!(on.irq());
        assert_eq!(on.read(1) & 0x80, 0x80, "the status says so");
        assert!(!on.irq(), "and reading it clears it");
    }

    #[test]
    fn a_byte_received_interrupts_and_a_second_unread_overruns() {
        let mut acia = acia(ON);
        acia.incoming.extend(b"xy");
        run(&mut acia, BYTE);
        assert!(acia.irq(), "the receiver's interrupt on");
        let status = acia.read(1);
        assert_eq!(status & 0x88, 0x88, "an interrupt, the register full");
        run(&mut acia, BYTE);
        assert_eq!(acia.read(1) & 0x04, 0x04, "y came in before x was read");
        assert_eq!(acia.read(0), b'x', "and was lost");
        assert_eq!(acia.read(1) & 0x0C, 0, "read: empty, its errors cleared");
    }

    #[test]
    fn its_off_till_dtr_and_its_receiver_interrupt_can_be_off() {
        let mut acia = acia(0);
        acia.incoming.push_back(b'x');
        run(&mut acia, BYTE * 2);
        assert_eq!(acia.read(1) & 0x08, 0, "off: nothing received");
        acia.write(2, ON | 0x02);
        run(&mut acia, BYTE);
        assert_eq!(acia.read(1) & 0x08, 0x08);
        assert!(!acia.irq(), "its interrupt off");
    }

    #[test]
    fn dsr_and_dcd_are_low_while_the_other_ends_there() {
        let mut acia = acia(ON);
        assert_eq!(acia.read(1) & 0x60, 0x60);
        acia.connected = true;
        assert_eq!(acia.read(1) & 0x60, 0);
    }

    #[test]
    fn a_programmed_reset_clears_the_commands_low_bits() {
        let mut acia = acia(0xFF);
        acia.write(1, 0);
        assert_eq!(acia.read(2), 0xE0);
    }
}
