//! TexElec's Serial & ESP32 Network Card, as the X16's: a TL16C2550, two
//! 16550 UARTs (`chips::uart16550`), at `$9FE0` and `$9FE8`, as its
//! jumpers have them first; the first's on the card's ESP32, a WiFi modem
//! with Zimodem's commands (`host::modem`, `Modem::zimodem`), which dials
//! places on the internet as terminal programs (ROMTERM, say) ask it to;
//! the second's its RS-232 port, nothing on it. Their interrupts are the
//! CPU's IRQ, as its jumper can have them.
//!
//! The modem's lines, as the UART has them: CTS and DSR up, as S48 and its
//! ESP32 have them; DCD while there's a carrier (as S46 says), and after,
//! till what came before it's gone is all taken, as a modem's buffer
//! empties; RI never, as it doesn't listen. What it has for the machine
//! waits for RTS, with its RTS and CTS flow control (`F0`, as it starts),
//! and DTR dropped hangs up, with S63=2. Its bytes, both ways, at the
//! rate the UART's set to, which it follows, as if it had been told to
//! (`ATB`).

use crate::chips::uart16550::Uart16550;
use crate::host::modem::Modem;

/// The X16's I/O addresses it's at, as its jumpers have them first.
pub const BASE: u16 = 0x9FE0;
pub const END: u16 = BASE + 0x0F;

pub struct NetworkCard {
    pub uarts: [Uart16550; 2],
    pub modem: Modem,
    /// DCD, as it's held up after the carrier's gone, till what came is
    /// all taken.
    dcd: bool,
}

impl NetworkCard {
    pub fn new(clock_hz: u32) -> NetworkCard {
        let mut card = NetworkCard {
            uarts: [Uart16550::new(clock_hz), Uart16550::new(clock_hz)],
            modem: Modem::zimodem(),
            dcd: false,
        };
        // The modem's there from the start: CTS and DSR up.
        card.uarts[0].set_lines(true, true, false, false);
        card.uarts[0].read(6);
        card
    }

    /// Reset, with the X16: its UARTs; the modem, on the ESP32, isn't.
    pub fn reset(&mut self) {
        for uart in &mut self.uarts {
            uart.reset();
        }
    }

    /// Its register at `addr`, `BASE` to `END`, read.
    pub fn read(&mut self, addr: u16) -> u8 {
        let uart = (addr >> 3 & 1) as usize;
        self.uarts[uart].read(addr as u8 & 7)
    }

    /// Its register at `addr` written: DTR, as it's set, to the modem.
    pub fn write(&mut self, addr: u16, value: u8) {
        let uart = (addr >> 3 & 1) as usize;
        self.uarts[uart].write(addr as u8 & 7, value);
        if uart == 0 {
            self.modem.dtr(self.uarts[0].dtr());
        }
    }

    /// `cycles` gone by, the UARTs'.
    pub fn run(&mut self, cycles: u32) {
        for uart in &mut self.uarts {
            uart.run(cycles as u64);
        }
    }

    /// The cycles until a UART's next to do something.
    pub fn due(&self) -> u32 {
        let due = self
            .uarts
            .iter()
            .map(Uart16550::due)
            .min()
            .unwrap_or(u64::MAX);
        due.min(u32::MAX as u64) as u32
    }

    pub fn irq(&self) -> bool {
        self.uarts.iter().any(Uart16550::irq)
    }

    /// A frame gone by: what the machine's sent, to the modem; what the
    /// modem has, toward the machine, two frames' worth at the UART's rate
    /// at most, the rest left with the modem; the modem's lines.
    pub fn frame(&mut self) {
        let [uart, rs232] = &mut self.uarts;
        rs232.outgoing.clear();
        let sent = std::mem::take(&mut uart.outgoing);
        self.modem.rate = uart.baud();
        self.modem.dtr(uart.dtr());
        self.modem.send(&sent);
        self.modem.frame();
        let room = (uart.baud() as usize / 10 / 60 * 2 + 16).saturating_sub(uart.incoming.len());
        uart.incoming.extend(self.modem.for_machine(room));
        uart.flow = self.modem.hardware_flow();
        let carrier = self.modem.carrier();
        let waiting = !uart.incoming.is_empty() || !self.modem.to_machine.is_empty();
        self.dcd = carrier || self.dcd && waiting;
        uart.set_lines(self.modem.cts(), true, false, self.modem.dcd(self.dcd));
    }
}

impl Default for NetworkCard {
    fn default() -> NetworkCard {
        NetworkCard::new(super::CLOCK_HZ)
    }
}
