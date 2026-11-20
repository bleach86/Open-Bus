//! The serial card: its firmware's signature, as terminal programs look
//! for it, and its switches; its BASIC entries, the hook pointed at its
//! own and a character out and in; its Pascal entries; and, as a terminal
//! program drives its 6551, the modem dialling a server of the test's,
//! the carrier up, and what's typed echoed back.

#[cfg(test)]
mod tests {
    use crate::cpu::CPU;
    use crate::machines::apple2::serial_card::{FIRMWARE, SerialCard};
    use crate::machines::apple2::{Apple2, CLOCK_HZ};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::{Duration, Instant};

    const DATA: u16 = 0xC0A8;
    const STATUS: u16 = 0xC0A9;
    const COMMAND: u16 = 0xC0AA;
    const CONTROL: u16 = 0xC0AB;
    /// A frame's cycles, about.
    const FRAME: u32 = 17_030;

    #[test]
    fn its_signature_and_its_switches() {
        assert_eq!(
            [FIRMWARE[5], FIRMWARE[7], FIRMWARE[0x0B], FIRMWARE[0x0C]],
            [0x38, 0x18, 0x01, 0x31],
            "the SSC's signature"
        );
        let mut card = SerialCard::new(CLOCK_HZ);
        assert_eq!(
            card.io(0xC0A1, None),
            Some(0xE0),
            "SW1: 9600, communications"
        );
        assert_eq!(card.io(0xC0A2, None), Some(0x02), "SW2: 8N1, interrupts on");
        card.io(CONTROL, Some(0x1E));
        assert_eq!(card.io(CONTROL, None), Some(0x1E), "the 6551's");
    }

    /// An Apple with the card in slot 2, and an RTS at $FF58, as the
    /// monitor has, for the firmware to find its slot by.
    fn apple() -> CPU<Apple2> {
        let mut rom = vec![0xEA; 0x3000];
        rom[0xFF58 - 0xD000] = 0x60;
        let mut apple = Apple2::new(rom);
        apple.serial = Some(SerialCard::new(CLOCK_HZ));
        let mut cpu = CPU::with_bus(apple);
        cpu.sp = 0xFF;
        cpu
    }

    /// The firmware called at `entry`, with A, X and Y, back to $1000, a
    /// frame at a time as long as it waits: its A, X and Y then.
    fn call(cpu: &mut CPU<Apple2>, entry: u16, (a, x, y): (u8, u8, u8)) -> (u8, u8, u8) {
        cpu.push_word(0x0FFF);
        (cpu.a, cpu.x, cpu.y, cpu.pc) = (a, x, y, entry);
        let start = cpu.clock;
        while cpu.pc != 0x1000 {
            assert!(cpu.clock - start < 200_000, "not back from ${entry:04X}");
            cpu.execute(1);
        }
        (cpu.a, cpu.x, cpu.y)
    }

    fn sent(cpu: &mut CPU<Apple2>) -> Vec<u8> {
        std::mem::take(&mut cpu.memory.serial.as_mut().unwrap().acia.outgoing)
    }

    #[test]
    fn basics_pr_and_in_send_and_take_through_its_own_entries() {
        let mut cpu = apple();
        // PR#2: CSW at $C200, a character printed.
        cpu.memory.ram[0x36..0x38].copy_from_slice(&[0x00, 0xC2]);
        let after = call(&mut cpu, 0xC200, (0xC8, 0x11, 0x22));
        assert_eq!(after, (0xC8, 0x11, 0x22), "A, X and Y as they were");
        assert_eq!(
            &cpu.memory.ram[0x36..0x38],
            &[0x07, 0xC2],
            "CSW, its output entry"
        );
        assert_eq!(cpu.memory.ram[0x07F8], 0xC2, "MSLOT");
        call(&mut cpu, 0xC207, (0xC9, 0, 0));
        cpu.execute(3_000);
        assert_eq!(sent(&mut cpu), b"HI", "bit 7 clear");
        // IN#2: KSW at $C200, the next character taken.
        cpu.memory.ram[0x38..0x3A].copy_from_slice(&[0x00, 0xC2]);
        cpu.memory
            .serial
            .as_mut()
            .unwrap()
            .acia
            .incoming
            .extend(b"OK");
        let (a, ..) = call(&mut cpu, 0xC200, (0xA0, 0, 0));
        assert_eq!(a, b'O' | 0x80, "bit 7 set");
        assert_eq!(
            &cpu.memory.ram[0x38..0x3A],
            &[0x05, 0xC2],
            "KSW, its input entry"
        );
        let (a, ..) = call(&mut cpu, 0xC205, (0xA0, 0, 0));
        assert_eq!(a, b'K' | 0x80);
    }

    #[test]
    fn pascals_entries_set_it_going_and_write_read_and_tell_its_status() {
        let mut cpu = apple();
        let entry = |offset: usize| 0xC200 | FIRMWARE[offset] as u16;
        let (_, x, _) = call(&mut cpu, entry(0x0D), (0, 0xC2, 0x20));
        assert_eq!(x, 0, "INIT, no error");
        let acia = &mut cpu.memory.serial.as_mut().unwrap().acia;
        assert_eq!(acia.rate(), 9600);
        call(&mut cpu, entry(0x0F), (b'A', 0xC2, 0x20));
        cpu.execute(3_000);
        assert_eq!(sent(&mut cpu), b"A", "WRITE");
        let status = |cpu: &mut CPU<Apple2>, request: u8| {
            call(cpu, entry(0x10), (request, 0xC2, 0x20));
            cpu.status.carry
        };
        assert!(status(&mut cpu, 0), "ready for a byte out");
        assert!(!status(&mut cpu, 1), "nothing in");
        cpu.memory
            .serial
            .as_mut()
            .unwrap()
            .acia
            .incoming
            .push_back(b'Z');
        cpu.execute(2_000);
        assert!(status(&mut cpu, 1), "a byte in");
        let (a, x, _) = call(&mut cpu, entry(0x0E), (0, 0xC2, 0x20));
        assert_eq!((a, x), (b'Z', 0), "READ");
    }

    /// The card, as a terminal program has it: a byte sent as its 6551's
    /// free; what's come, taken as it comes; a frame, to the modem.
    struct Terminal {
        card: SerialCard,
        cycles: u32,
        came: Vec<u8>,
    }

    impl Terminal {
        fn run(&mut self, cycles: u32) {
            for _ in 0..cycles {
                self.card.tick();
                self.cycles += 1;
                if self.cycles.is_multiple_of(FRAME) {
                    self.card.frame();
                    thread::sleep(Duration::from_millis(1));
                }
                if self.card.io(STATUS, None).unwrap() & 0x08 != 0 {
                    let byte = self.card.io(DATA, None).unwrap();
                    self.came.push(byte);
                }
            }
        }

        fn send(&mut self, text: &[u8]) {
            for &byte in text {
                while self.card.io(STATUS, None).unwrap() & 0x10 == 0 {
                    self.run(1);
                }
                self.card.io(DATA, Some(byte));
            }
        }

        fn wait_for(&mut self, text: &str) -> String {
            let start = Instant::now();
            loop {
                let came = String::from_utf8_lossy(&self.came).into_owned();
                if came.contains(text) {
                    self.came.clear();
                    return came;
                }
                assert!(
                    start.elapsed() < Duration::from_secs(5),
                    "no {text:?}: {came:?}"
                );
                self.run(FRAME);
            }
        }
    }

    #[test]
    fn a_terminal_dials_through_it_and_talks_to_the_place() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let place = thread::spawn(move || {
            let (mut caller, _) = server.accept().unwrap();
            caller.write_all(b"WELCOME TO THE BOARD\r\n").unwrap();
            let mut typed = [0; 5];
            caller.read_exact(&mut typed).unwrap();
            caller.write_all(&typed).unwrap();
            caller
        });
        let mut terminal = Terminal {
            card: SerialCard::new(CLOCK_HZ),
            cycles: 0,
            came: Vec::new(),
        };
        terminal.card.io(CONTROL, Some(0x1E));
        terminal.card.io(COMMAND, Some(0x0B));
        terminal.run(FRAME);
        let status = terminal.card.io(STATUS, None).unwrap();
        assert_eq!(status & 0x60, 0x20, "DSR, the modem on; no carrier");
        terminal.send(b"AT\r");
        assert_eq!(terminal.wait_for("OK\r\n"), "AT\r\r\nOK\r\n");
        terminal.send(format!("ATDT127.0.0.1:{port}\r").as_bytes());
        let came = terminal.wait_for("BOARD\r\n");
        assert!(
            came.ends_with("CONNECT 9600\r\nWELCOME TO THE BOARD\r\n"),
            "{came:?}"
        );
        let status = terminal.card.io(STATUS, None).unwrap();
        assert_eq!(status & 0x60, 0, "DSR and the carrier");
        terminal.send(b"HELLO");
        assert_eq!(terminal.wait_for("HELLO"), "HELLO");
        drop(place.join().unwrap());
        terminal.wait_for("NO CARRIER");
        let status = terminal.card.io(STATUS, None).unwrap();
        assert_eq!(status & 0x60, 0x20, "the carrier gone");
    }
}
