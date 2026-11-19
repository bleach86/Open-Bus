//! The modem: its command mode's lines and results, words or numbers,
//! echoed or not; dialling a place (a server of the test's own, here),
//! connecting, telnet's commands answered and taken out, what's sent and
//! what comes back; `+++` to the command mode and `ATO` back; hanging up;
//! the place gone, or never there.

#[cfg(test)]
mod tests {
    use crate::host::modem::Modem;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::{Duration, Instant};

    /// What it's said to the machine, since last asked.
    fn said(modem: &mut Modem) -> String {
        let bytes: Vec<u8> = modem.to_machine.drain(..).collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// Frames, a millisecond apart, until `done`, or 5 seconds.
    fn until(modem: &mut Modem, mut done: impl FnMut(&mut Modem) -> bool) {
        let start = Instant::now();
        while !done(modem) {
            assert!(start.elapsed() < Duration::from_secs(5), "never happened");
            modem.frame();
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn frames(modem: &mut Modem, frames: u32) {
        for _ in 0..frames {
            modem.frame();
        }
    }

    /// What's come to the machine, waited for till it holds `text`.
    fn wait_for(modem: &mut Modem, text: &str) -> String {
        let mut all = String::new();
        until(modem, |modem| {
            all += &said(modem);
            all.contains(text)
        });
        all
    }

    #[test]
    fn a_command_line_is_echoed_and_answered_in_words_or_numbers() {
        let mut modem = Modem::new();
        modem.send(b"AT\r");
        assert_eq!(said(&mut modem), "AT\r\r\nOK\r\n");
        modem.send(b"ate0\r");
        assert_eq!(said(&mut modem), "ate0\r\r\nOK\r\n", "echoed as it's typed");
        modem.send(b"AT&C1 S0=0 M1L2X4\r");
        assert_eq!(
            said(&mut modem),
            "\r\nOK\r\n",
            "taken, as a modem takes them"
        );
        modem.send(b"ATG\r");
        assert_eq!(said(&mut modem), "\r\nERROR\r\n");
        modem.send(b"HELLO\r");
        assert_eq!(said(&mut modem), "\r\nERROR\r\n", "not AT");
        modem.send(b"ATV0\r");
        assert_eq!(said(&mut modem), "0\r");
        modem.send(b"ATG\r");
        assert_eq!(said(&mut modem), "4\r", "ERROR, as a number");
        modem.send(b"A/");
        assert_eq!(said(&mut modem), "4\r", "the last again");
        modem.send(b"ATZ\r");
        assert_eq!(said(&mut modem), "\r\nOK\r\n", "as it was: words");
        modem.send(b"AT\r");
        assert_eq!(said(&mut modem), "AT\r\r\nOK\r\n", "and its echo");
        modem.send(b"ATDT\r");
        assert!(said(&mut modem).ends_with("ERROR\r\n"), "nowhere to dial");
    }

    #[test]
    fn it_dials_a_place_answers_telnet_and_carries_what_both_send() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let place = thread::spawn(move || {
            let (mut caller, _) = server.accept().unwrap();
            // Telnet's: asked for its terminal type (24), offered its echo;
            // a greeting, its return's NUL telnet's, and $FF doubled.
            caller
                .write_all(&[
                    255, 253, 24, 255, 251, 1, b'H', b'I', b'\r', 0, 255, 255, b'\n',
                ])
                .unwrap();
            let mut answers = [0; 6];
            caller.read_exact(&mut answers).unwrap();
            let mut typed = [0; 4];
            caller.read_exact(&mut typed).unwrap();
            caller.write_all(b"ECHO ").unwrap();
            caller.write_all(&typed).unwrap();
            let mut more = [0; 3];
            caller.read_exact(&mut more).unwrap();
            (answers, typed, more, caller)
        });
        let mut modem = Modem::new();
        modem.send(format!("ATDT 127.0.0.1:{port}\r").as_bytes());
        let came = wait_for(&mut modem, "HI\r\u{FFFD}\n");
        assert!(modem.carrier());
        assert!(
            came.ends_with("\r\nCONNECT 9600\r\nHI\r\u{FFFD}\n"),
            "{came:?}"
        );
        modem.send(b"A\xFFB");
        let came = wait_for(&mut modem, "B");
        assert_eq!(came, "ECHO A\u{FFFD}B", "its $FF, one, the place's two");
        modem.send(b"BYE");
        until(&mut modem, |_| place.is_finished());
        let (answers, typed, more, caller) = place.join().unwrap();
        assert_eq!(answers, [255, 252, 24, 255, 253, 1], "WONT TTYPE, DO ECHO");
        assert_eq!(typed, [b'A', 255, 255, b'B'], "its $FF doubled");
        assert_eq!(&more, b"BYE");
        drop(caller);
        let came = wait_for(&mut modem, "NO CARRIER");
        assert_eq!(came, "\r\nNO CARRIER\r\n", "the place gone");
        assert!(!modem.carrier());
    }

    #[test]
    fn plus_plus_plus_is_the_command_mode_still_connected_and_ato_back() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let place = thread::spawn(move || {
            let (mut caller, _) = server.accept().unwrap();
            let mut got = Vec::new();
            let mut buffer = [0; 64];
            while let Ok(read) = caller.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                got.extend_from_slice(&buffer[..read]);
            }
            got
        });
        let mut modem = Modem::new();
        modem.send(format!("ATE0D127.0.0.1:{port}\r").as_bytes());
        wait_for(&mut modem, "CONNECT");
        frames(&mut modem, 60);
        modem.send(b"+++");
        frames(&mut modem, 59);
        assert_eq!(said(&mut modem), "", "a second, not yet");
        frames(&mut modem, 1);
        assert_eq!(said(&mut modem), "\r\nOK\r\n");
        assert!(modem.carrier(), "still connected");
        modem.send(b"AT\r");
        assert_eq!(said(&mut modem), "\r\nOK\r\n", "commands, not the place's");
        modem.send(b"ATO\r");
        assert_eq!(said(&mut modem), "\r\nCONNECT 9600\r\n");
        // Not quiet a second before: the place's.
        modem.send(b"x+++");
        frames(&mut modem, 120);
        assert_eq!(said(&mut modem), "");
        frames(&mut modem, 60);
        modem.send(b"+++");
        frames(&mut modem, 60);
        modem.send(b"ATH\r");
        let came = said(&mut modem);
        assert!(came.ends_with("OK\r\n"), "{came:?}");
        assert!(!modem.carrier(), "hung up");
        let got = place.join().unwrap();
        assert_eq!(got, b"+++x++++++", "the +s sent on, as a modem's are");
    }

    #[test]
    fn nobody_there_is_no_carrier_and_a_key_while_dialling_gives_up() {
        let closed = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = closed.local_addr().unwrap().port();
        drop(closed);
        let mut modem = Modem::new();
        modem.send(format!("ATE0DT127.0.0.1:{port}\r").as_bytes());
        let came = wait_for(&mut modem, "NO CARRIER");
        assert!(
            came.ends_with("\r\r\nNO CARRIER\r\n"),
            "its line echoed, then E0"
        );
        assert!(!modem.carrier());
        // A place that's slow to answer: given up on by a key.
        modem.send(b"ATDT10.255.255.1\r");
        modem.send(b" ");
        assert_eq!(said(&mut modem), "\r\nNO CARRIER\r\n");
        frames(&mut modem, 10);
        modem.send(b"AT\r");
        assert_eq!(said(&mut modem), "\r\nOK\r\n");
    }
}
