//! The X16's network card, through its bus: found by its scratch register,
//! set up as x16net sets it; its modem's commands and results through the
//! UART; a place dialled, DCD up as it's connected, what's sent and what
//! comes back, at its rate; DTR dropped hanging up, DCD down after. An
//! answer's turned round in milliseconds, between frames, as Zimodem's.

#[cfg(test)]
mod tests {
    use crate::cpu::bus::Bus;
    use crate::host::machine::{Input, Machine};
    use crate::machines::x16::X16;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::{Duration, Instant};

    const UART: u16 = 0x9FE0;

    fn x16() -> X16 {
        X16::new(&vec![0xEA; 0x8000]).unwrap()
    }

    /// `cycles` of the CPU's, as an instruction's are.
    fn cycles(x16: &mut X16, cycles: u32) {
        for _ in 0..cycles {
            x16.tick();
        }
    }

    /// A frame's end: the modem's I/O.
    fn frame(x16: &mut X16) {
        x16.input(Input::default());
    }

    /// As x16net's `init` has it: 115,200 baud, 8N1, the FIFOs on, DTR,
    /// RTS, OUT1 and auto flow control.
    fn set_up(x16: &mut X16) {
        for (reg, value) in [
            (3, 0x80),
            (0, 8),
            (1, 0),
            (3, 3),
            (1, 0),
            (2, 0x87),
            (4, 0x27),
        ] {
            x16.write(UART + reg, value);
        }
    }

    fn send(x16: &mut X16, bytes: &[u8]) {
        for &byte in bytes {
            while x16.read(UART + 5) & 0x20 == 0 {
                cycles(x16, 50);
            }
            x16.write(UART, byte);
        }
    }

    /// What it's received, till `done`, or 5 seconds; frames a millisecond
    /// apart, its UART read as a program polls it.
    fn received(x16: &mut X16, mut done: impl FnMut(&[u8], &mut X16) -> bool) -> Vec<u8> {
        let start = Instant::now();
        let mut got = Vec::new();
        while !done(&got, x16) {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "{:?}",
                String::from_utf8_lossy(&got)
            );
            for _ in 0..200 {
                cycles(x16, 100);
                while x16.read(UART + 5) & 1 != 0 {
                    got.push(x16.read(UART));
                }
            }
            frame(x16);
            thread::sleep(Duration::from_millis(1));
        }
        got
    }

    #[test]
    fn its_found_by_its_scratch_register_and_its_modem_answers() {
        let mut x16 = x16();
        x16.write(UART + 7, 0x55);
        assert_eq!(x16.read(UART + 7), 0x55);
        x16.write(UART + 0x0F, 0xAA);
        assert_eq!(x16.read(UART + 0x0F), 0xAA, "the RS-232 port's own");
        assert_eq!(x16.read(UART + 7), 0x55);
        set_up(&mut x16);
        frame(&mut x16);
        assert_eq!(x16.read(UART + 6) & 0xB0, 0x30, "CTS, DSR; no carrier");
        send(
            &mut x16,
            b"ATE0V1Q0X0F0R0&P0S62=0S63=2S46=0S48=0S50=0S54=0N0H\r",
        );
        let got = received(&mut x16, |got, _| got.ends_with(b"OK\r"));
        assert_eq!(
            got,
            b"ATE0V1Q0X0F0R0&P0S62=0S63=2S46=0S48=0S50=0S54=0N0H\r\nOK\r"
        );
    }

    #[test]
    fn a_place_is_dialled_its_bytes_carried_and_dtr_dropped_hangs_up() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let place = thread::spawn(move || {
            let (mut caller, _) = server.accept().unwrap();
            let all: Vec<u8> = (0..=255).cycle().take(2000).collect();
            caller.write_all(&all).unwrap();
            let mut got = [0; 4];
            caller.read_exact(&mut got).unwrap();
            let mut rest = Vec::new();
            let _ = caller.read_to_end(&mut rest);
            got
        });
        let mut x16 = x16();
        set_up(&mut x16);
        send(&mut x16, b"ATE0Q0S63=2\r");
        received(&mut x16, |got, _| got.ends_with(b"OK\r\n"));
        // Quiet, as x16net dials: DCD says it's connected.
        send(&mut x16, format!("ATQ1D\"127.0.0.1:{port}\"\r").as_bytes());
        let got = received(&mut x16, |got, _| got.len() >= 2000);
        assert_eq!(
            got,
            (0..=255).cycle().take(2000).collect::<Vec<u8>>(),
            "every byte, none lost"
        );
        assert_eq!(x16.read(UART + 6) & 0x88, 0x88, "DCD up, and it changed");
        assert_eq!(x16.read(UART + 5) & 2, 0, "no overrun: RTS held it");
        send(&mut x16, &[0, 0xFF, b'+', 13]);
        // A few frames for them to go; then DTR dropped.
        let mut frames = 0;
        received(&mut x16, |_, _| {
            frames += 1;
            frames > 5
        });
        x16.write(UART + 4, 0x26);
        received(&mut x16, |_, x16| x16.read(UART + 6) & 0x80 == 0);
        assert_eq!(
            place.join().unwrap(),
            [0, 0xFF, b'+', 13],
            "as they were sent"
        );
    }

    #[test]
    fn a_byte_and_its_answer_go_between_frames_as_zimodem_s_do() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let place = thread::spawn(move || {
            let (mut caller, _) = server.accept().unwrap();
            caller.set_nodelay(true).unwrap();
            // An XMODEM sender's turn: an ACK, and the next block's start.
            let mut ack = [0; 1];
            caller.read_exact(&mut ack).unwrap();
            caller.write_all(&[0x01]).unwrap();
            let mut rest = Vec::new();
            let _ = caller.read_to_end(&mut rest);
            ack[0]
        });
        let mut x16 = x16();
        set_up(&mut x16);
        send(&mut x16, b"ATE0Q0S63=2\r");
        received(&mut x16, |got, _| got.ends_with(b"OK\r\n"));
        send(&mut x16, format!("ATQ1D\"127.0.0.1:{port}\"\r").as_bytes());
        received(&mut x16, |_, x16| x16.read(UART + 6) & 0x80 != 0);
        // No frame from here: only the CPU's time, a millisecond at a time.
        send(&mut x16, &[0x06]);
        let start = Instant::now();
        loop {
            assert!(start.elapsed() < Duration::from_secs(5), "never came");
            cycles(&mut x16, 8000);
            if x16.read(UART + 5) & 1 != 0 {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(x16.read(UART), 0x01, "the answer, with no frame gone by");
        x16.write(UART + 4, 0x26);
        received(&mut x16, |_, x16| x16.read(UART + 6) & 0x80 == 0);
        assert_eq!(place.join().unwrap(), 0x06);
    }
}
