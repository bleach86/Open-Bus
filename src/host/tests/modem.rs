//! The modem: its command mode's lines and results, words or numbers,
//! echoed or not; dialling a place (a server of the test's own, here),
//! connecting, telnet's commands answered and taken out, what's sent and
//! what comes back; `+++` to the command mode and `ATO` back; hanging up;
//! the place gone, or never there. What comes held to a few frames' worth,
//! the rest left in the network. Zimodem's: x16net's setting up taken,
//! a place dialled in quotes over plain TCP or telnet, `+++` and DTR
//! hanging up, its S registers, flow control and the ends of its lines;
//! its phone book, places kept, listed and dialled by number.

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
        assert_eq!(
            said(&mut modem),
            "+++",
            "echoed, the place not echoing; a second, not yet the command mode"
        );
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
        assert_eq!(said(&mut modem), "x+++", "echoed, and online still");
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
    fn xoff_holds_what_comes_and_xon_lets_it_go_neither_sent_on() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let place = thread::spawn(move || {
            let (mut caller, _) = server.accept().unwrap();
            caller.write_all(b"banner").unwrap();
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
        until(&mut modem, |modem| modem.carrier());
        // A pause asked for as it goes online, as ProTERM asks, the place's
        // banner pouring in: held here.
        modem.send(&[0x13]);
        until(&mut modem, |modem| {
            modem
                .to_machine
                .iter()
                .rev()
                .take(6)
                .eq(b"banner".iter().rev())
        });
        assert!(modem.for_machine(1000).is_empty(), "held by the XOFF");
        modem.send(&[0x11]);
        let came = String::from_utf8_lossy(&modem.for_machine(1000)).into_owned();
        assert!(
            came.ends_with("CONNECT 9600\r\nbanner"),
            "let go by the XON: {came:?}"
        );
        // Neither sent on to the place.
        modem.send(b"a\x13b\x11c");
        frames(&mut modem, 10);
        drop(modem);
        assert_eq!(place.join().unwrap(), b"abc");
    }

    #[test]
    fn what_s_typed_online_is_echoed_till_the_place_says_it_echoes() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let (go_on, step) = std::sync::mpsc::channel::<()>();
        let place = thread::spawn(move || {
            let (mut caller, _) = server.accept().unwrap();
            step.recv().unwrap();
            // A password's coming: the place echoes (nothing), not the
            // client.
            caller.write_all(&[255, 251, 1]).unwrap();
            caller.write_all(b"password:").unwrap();
            step.recv().unwrap();
            caller.write_all(&[255, 252, 1]).unwrap();
            caller.write_all(b"name:").unwrap();
            step.recv().unwrap();
        });
        let mut modem = Modem::new();
        modem.send(format!("ATE0D127.0.0.1:{port}\r").as_bytes());
        wait_for(&mut modem, "CONNECT");
        modem.send(b"ab\r");
        assert_eq!(said(&mut modem), "ab\r\n", "a name, shown as it's typed");
        go_on.send(()).unwrap();
        wait_for(&mut modem, "password:");
        modem.send(b"secret");
        assert_eq!(said(&mut modem), "", "the place's to echo, and it doesn't");
        go_on.send(()).unwrap();
        wait_for(&mut modem, "name:");
        modem.send(b"c");
        assert_eq!(said(&mut modem), "c", "WONT ECHO, the client's again");
        // ATF1: never, whatever telnet says.
        frames(&mut modem, 60);
        modem.send(b"+++");
        frames(&mut modem, 60);
        modem.send(b"ATF1O\r");
        wait_for(&mut modem, "CONNECT");
        modem.send(b"d");
        assert_eq!(said(&mut modem), "", "ATF1");
        go_on.send(()).unwrap();
        place.join().unwrap();
    }

    #[test]
    fn xon_and_xoff_hold_results_too_and_at_k0_passes_them_as_they_are() {
        let mut modem = Modem::new();
        modem.send(b"\x13AT\r");
        assert!(modem.for_machine(100).is_empty(), "held");
        modem.send(b"\x11");
        assert_eq!(
            String::from_utf8_lossy(&modem.for_machine(100)),
            "AT\r\r\nOK\r\n",
            "neither echoed"
        );
        modem.send(b"AT&K0\r");
        said(&mut modem);
        modem.send(b"\x13");
        assert_eq!(
            modem.for_machine(100),
            [0x13],
            "echoed as typed, nothing held"
        );
        modem.send(b"\rAT&K4\r");
        said(&mut modem);
        modem.send(b"\x13AT\r");
        assert!(modem.for_machine(100).is_empty(), "&K4, held again");
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

    #[test]
    fn zimodem_takes_x16nets_setting_up_and_keeps_its_registers() {
        let mut modem = Modem::zimodem();
        assert!(
            modem.hardware_flow(),
            "RTS and CTS, as the X16 card's has it"
        );
        modem.send(b"ATE0V1Q0X0F0R0&P0S62=0S63=2S46=0S48=0S50=0S54=0N0H\r");
        assert_eq!(
            said(&mut modem),
            "ATE0V1Q0X0F0R0&P0S62=0S63=2S46=0S48=0S50=0S54=0N0H\r\nOK\r",
            "its return echoed as a line's end, before R0; its results, after"
        );
        modem.send(b"ATS63?\r");
        assert_eq!(said(&mut modem), "2\rOK\r");
        modem.send(b"ATR1F4\r");
        assert_eq!(said(&mut modem), "OK\r\n");
        assert!(!modem.hardware_flow());
        modem.send(b"AT&K3\r");
        assert!(modem.hardware_flow(), "&K3, RTS and CTS too");
        said(&mut modem);
        modem.send(b"ATW\"home,secret\"B115200\r");
        assert_eq!(said(&mut modem), "OK\r\n", "its WiFi the host's");
        modem.send(b"AT&G\"example.org/x\"\r");
        assert_eq!(said(&mut modem), "ERROR\r\n", "no downloads");
        modem.send(b"ATZ\r");
        said(&mut modem);
        modem.send(b"ATS62?\r");
        assert_eq!(
            said(&mut modem),
            "ATS62?\r\n1\r\nOK\r\n",
            "as it was, its echo on"
        );
        assert!(modem.dcd(true) && !modem.dcd(false) && modem.cts());
        modem.send(b"ATS46=1S48=3\r");
        assert!(!modem.dcd(true) && modem.dcd(false) && !modem.cts());
    }

    #[test]
    fn zimodem_dials_in_quotes_over_plain_tcp_and_plus_plus_plus_hangs_up() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let place = thread::spawn(move || {
            let (mut caller, _) = server.accept().unwrap();
            // Not telnet's: as it is, $FF and all.
            caller.write_all(&[255, 253, 24, b'H', b'I']).unwrap();
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
        let mut modem = Modem::zimodem();
        modem.send(format!("ATE0D\"127.0.0.1:{port}\"\r").as_bytes());
        let came = wait_for(&mut modem, "HI");
        assert!(
            came.ends_with("CONNECT 1\r\n\u{FFFD}\u{FFFD}\u{18}HI"),
            "{came:?}"
        );
        modem.send(&[b'A', 255]);
        frames(&mut modem, 60);
        modem.send(b"+++");
        frames(&mut modem, 60);
        assert_eq!(said(&mut modem), "NO CARRIER\r\n", "no echo; hung up");
        assert!(!modem.carrier());
        assert_eq!(place.join().unwrap(), [b'A', 255, b'+', b'+', b'+']);
    }

    #[test]
    fn zimodem_s_telnet_says_no_but_to_its_terminal_type_and_leaves_data_be() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let place = thread::spawn(move || {
            let (mut caller, _) = server.accept().unwrap();
            // Options offered and asked for, binary among them, one twice;
            // told no; its terminal type asked; then data, a return's NUL
            // and $FF, as XMODEM's blocks have them.
            caller
                .write_all(&[
                    255, 251, 0, 255, 253, 0, 255, 251, 1, 255, 251, 3, 255, 253, 31, 255, 254, 3,
                    255, 253, 24, 255, 253, 24, 255, 252, 1, 255, 250, 24, 1, 255, 240, b'A',
                    b'\r', 0, b'B', 255, 255, 0, b'\r', b'\n',
                ])
                .unwrap();
            let mut answers = vec![0; 8 * 3 + 13];
            caller.read_exact(&mut answers).unwrap();
            answers
        });
        let mut modem = Modem::zimodem();
        modem.send(format!("ATE0DT\"127.0.0.1:{port}\"\r").as_bytes());
        wait_for(&mut modem, "CONNECT 1\r\n");
        let mut came = Vec::new();
        until(&mut modem, |modem| {
            came.extend(modem.to_machine.drain(..));
            came.ends_with(b"\r\n")
        });
        assert_eq!(came, b"A\r\0B\xFF\0\r\n", "its NULs and one $FF");
        until(&mut modem, |_| place.is_finished());
        let answers = place.join().unwrap();
        #[rustfmt::skip]
        let zimodem = [
            255, 254, 0, // WILL BINARY: DONT
            255, 252, 0, // DO BINARY: WONT
            255, 254, 1, // WILL ECHO: DONT
            255, 254, 3, // WILL SGA: DONT
            255, 252, 31, // DO NAWS: WONT
            255, 252, 3, // DONT SGA: WONT
            255, 251, 24, // DO TTYPE: WILL, twice
            255, 251, 24,
            // WONT ECHO: nothing; SB TTYPE SEND: IS "Zimodem"
            255, 250, 24, 0, b'Z', b'i', b'm', b'o', b'd', b'e', b'm', 255, 240,
        ];
        assert_eq!(answers, zimodem);
    }

    #[test]
    fn a_return_s_nul_is_data_when_the_place_sends_in_binary() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let place = thread::spawn(move || {
            let (mut caller, _) = server.accept().unwrap();
            caller.write_all(&[b'A', b'\r', 0, 255, 251, 0]).unwrap();
            let mut answer = [0; 3];
            caller.read_exact(&mut answer).unwrap();
            caller.write_all(&[b'B', b'\r', 0, b'!']).unwrap();
            answer
        });
        let mut modem = Modem::new();
        modem.send(format!("ATE0DT 127.0.0.1:{port}\r").as_bytes());
        let came = wait_for(&mut modem, "!");
        assert!(
            came.contains("CONNECT 9600\r\nA\rB\r\0!"),
            "its NUL telnet's, then binary's data: {came:?}"
        );
        until(&mut modem, |_| place.is_finished());
        assert_eq!(place.join().unwrap(), [255, 253, 0], "DO BINARY");
    }

    #[test]
    fn zimodem_dials_telnet_with_t_and_a_dropped_dtr_hangs_up_with_s63_2() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let place = thread::spawn(move || {
            let (mut caller, _) = server.accept().unwrap();
            caller.write_all(&[255, 251, 1, b'O', b'K']).unwrap();
            let mut answer = [0; 3];
            caller.read_exact(&mut answer).unwrap();
            let mut rest = Vec::new();
            let _ = caller.read_to_end(&mut rest);
            answer
        });
        let mut modem = Modem::zimodem();
        modem.dtr(true);
        modem.send(format!("ATE0X0S63=2DT\"127.0.0.1:{port}\"\r").as_bytes());
        let came = wait_for(&mut modem, "OK");
        assert!(
            came.ends_with("\"\r\nCONNECT\r\nOK"),
            "X0: CONNECT alone: {came:?}"
        );
        // Its telnet answer sent; then DTR dropped.
        frames(&mut modem, 1);
        modem.dtr(false);
        assert_eq!(said(&mut modem), "NO CARRIER\r\n");
        assert!(!modem.carrier());
        assert_eq!(place.join().unwrap(), [255, 254, 1], "Zimodem's, DONT ECHO");
        // Nobody there: no answer.
        let closed = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = closed.local_addr().unwrap().port();
        drop(closed);
        modem.send(format!("ATD\"127.0.0.1:{port}\"\r").as_bytes());
        assert_eq!(wait_for(&mut modem, "ANSWER\r\n"), "NO ANSWER\r\n");
    }

    #[test]
    fn what_comes_is_held_back_in_the_network_past_a_few_frames_worth() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let place = thread::spawn(move || {
            let (mut caller, _) = server.accept().unwrap();
            //  far more than the modem holds: the rest waits in the network
            let _ = caller.write_all(&vec![b'x'; 100_000]);
            caller
        });
        let mut modem = Modem::new();
        modem.send(format!("ATE0NET0DT127.0.0.1:{port}\r").as_bytes());
        until(&mut modem, |modem| modem.carrier());
        modem.to_machine.clear();
        for _ in 0..200 {
            modem.frame();
            thread::sleep(Duration::from_millis(1));
        }
        //  9600 baud: four frames' worth is 64 bytes, so the least, 512
        let held = modem.to_machine.len();
        assert!((256..=512).contains(&held), "{held} held");
        modem.to_machine.drain(..);
        until(&mut modem, |modem| !modem.to_machine.is_empty());
        drop(place);
    }

    #[test]
    fn zimodem_keeps_places_in_its_phone_book_lists_them_and_dials_them_by_number() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let place = thread::spawn(move || {
            let (mut caller, _) = server.accept().unwrap();
            caller.write_all(b"HI").unwrap();
            caller
        });
        let mut modem = Modem::zimodem();
        let command = |modem: &mut Modem, line: &str| {
            modem.send(format!("{line}\r").as_bytes());
            said(modem)
        };
        assert_eq!(
            command(&mut modem, "ATE0"),
            "ATE0\r\nOK\r\n",
            "echoed, then not"
        );
        let keep = format!("ATP\"5=127.0.0.1:{port},the test's place\"");
        assert_eq!(command(&mut modem, &keep), "OK\r\n");
        assert_eq!(
            command(&mut modem, "ATPT\"64=bbs.example.org:6400\""),
            "OK\r\n"
        );
        assert_eq!(
            command(&mut modem, "ATP"),
            format!(
                "5               127.0.0.1:{port}\r\n64         t    bbs.example.org:6400\r\nOK\r\n"
            ),
            "by number; their modifiers; no notes"
        );
        assert_eq!(
            command(&mut modem, "ATP?"),
            format!(
                "5               127.0.0.1:{port} (the test's place)\r\n\
                 64         t    bbs.example.org:6400 ()\r\nOK\r\n"
            ),
            "with their notes"
        );
        assert_eq!(
            command(&mut modem, "ATP64"),
            "64         t    bbs.example.org:6400\r\nOK\r\n"
        );
        for wrong in [
            "ATP\"x=host:23\"",
            "ATP\"1234567890=host:23\"",
            "ATP\"6=host\"",
            "ATP\"6=host:telnet\"",
            "ATP\"6=DELETE\"",
            "ATD6",
        ] {
            assert_eq!(command(&mut modem, wrong), "ERROR\r\n", "{wrong}");
        }
        assert_eq!(command(&mut modem, "ATP\"64=DELETE\""), "OK\r\n");
        assert_eq!(command(&mut modem, "AT&F"), "OK\r\n");
        modem.send(b"ATE0D5\r");
        let came = wait_for(&mut modem, "HI");
        assert!(
            came.ends_with("CONNECT 1\r\nHI"),
            "kept past AT&F: {came:?}"
        );
        drop(place);
    }
}
