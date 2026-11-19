//! A modem, Hayes's: what a machine's serial port has on its other end, to
//! go online with. As a WiFi modem does now, it dials a place on the
//! internet, not a phone number: `ATDT bbs.example.org:23`, a host and its
//! port (23, telnet's, if there's none), over TCP, as the BBSes still up
//! are reached. On the web, where a page can't open a connection of its
//! own, it never connects.
//!
//! Offline, it's in its command mode: a line typed, `AT` and its commands,
//! ended by a return, each answered: `OK`, `ERROR`, `CONNECT 9600` (the
//! port's rate, as the machine has it), `NO CARRIER`, as words or, `V0`,
//! numbers. Its commands: `D` (`DT`, `DP`) and the place, dialling; `H`,
//! hanging up; `O`, back online; `Z` and `&F`, as it was; `E`, its echo;
//! `V`, words or numbers; `Q`, quiet; `NET`, telnet's or plain TCP's; `I`,
//! what it is; and the rest it's given, as a modem takes them, `M`, `L`,
//! `X`, `&C`, `&D`, `S` registers, done as nothing. `A/` again, the last.
//! A key pressed as it dials gives up.
//!
//! Online, what the machine sends goes to the place, and what comes back
//! to the machine, the carrier up while it lasts. Telnet's: its commands
//! answered (no to all it's asked, but its binary, its echo and its
//! suppressing of go-ahead), $FF doubled, a return's NUL left out. `+++`,
//! with a second's quiet before it and after, back to the command mode,
//! still connected (the `+`s sent on, as a Hayes modem sends them). The
//! place gone, `NO CARRIER`, the carrier dropped.

use std::collections::VecDeque;
#[cfg(not(target_arch = "wasm32"))]
use std::io::{ErrorKind, Read, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::net::{TcpStream, ToSocketAddrs};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::{Receiver, TryRecvError, channel};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

/// The port dialled, a place given none: telnet's.
const TELNET_PORT: u16 = 23;
/// The guard time either side of `+++`, in frames: a second.
const GUARD_FRAMES: u32 = 60;
/// Longer than this dialling, and there's no answer.
#[cfg(not(target_arch = "wasm32"))]
const DIAL_TIME: Duration = Duration::from_secs(20);

const IAC: u8 = 255;
const DONT: u8 = 254;
const DO: u8 = 253;
const WONT: u8 = 252;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;
/// The options it does: binary, echo (the other end's), and suppressing
/// go-ahead.
const BINARY: u8 = 0;
const ECHO: u8 = 1;
const SUPPRESS_GO_AHEAD: u8 = 3;

/// Its results, as words and as numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Result {
    Ok,
    Connect,
    NoCarrier,
    Error,
    NoAnswer,
}

impl Result {
    fn words(self) -> &'static str {
        match self {
            Result::Ok => "OK",
            Result::Connect => "CONNECT",
            Result::NoCarrier => "NO CARRIER",
            Result::Error => "ERROR",
            Result::NoAnswer => "NO ANSWER",
        }
    }

    fn number(self) -> u8 {
        match self {
            Result::Ok => 0,
            Result::Connect => 1,
            Result::NoCarrier => 3,
            Result::Error => 4,
            Result::NoAnswer => 8,
        }
    }
}

/// Where telnet's commands, coming in, are taken apart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Telnet {
    #[default]
    Data,
    /// A return come: its NUL, if one's next, left out.
    Return,
    Command,
    /// An option's asked for or offered: WILL, WONT, DO or DONT.
    Option(u8),
    /// A subnegotiation, to its end; and an IAC in it.
    Sub,
    SubCommand,
}

/// The connection, once it's made; or being made.
#[cfg(not(target_arch = "wasm32"))]
enum Line {
    Dialling(Receiver<std::io::Result<TcpStream>>),
    Up(TcpStream),
}

#[cfg(target_arch = "wasm32")]
enum Line {}

pub struct Modem {
    line: Option<Line>,
    /// Online (not in the command mode): what's sent goes to the place.
    online: bool,
    /// The command line as it's typed, and the last one, for `A/`.
    typed: Vec<u8>,
    last: Vec<u8>,
    echo: bool,
    words: bool,
    quiet: bool,
    telnet: bool,
    taking: Telnet,
    /// For the machine: its results, and what's come from the place.
    pub to_machine: VecDeque<u8>,
    /// What's to go to the place, not yet taken by it.
    to_place: Vec<u8>,
    /// The `+++` escape: the frames since anything was sent, and the `+`s
    /// sent since; and the frames since the third.
    quiet_frames: u32,
    pluses: u8,
    escaped_frames: Option<u32>,
    /// The rate it connects at: the machine's port's.
    pub rate: u32,
}

impl Default for Modem {
    fn default() -> Modem {
        Modem::new()
    }
}

impl Modem {
    pub fn new() -> Modem {
        Modem {
            line: None,
            online: false,
            typed: Vec::new(),
            last: Vec::new(),
            echo: true,
            words: true,
            quiet: false,
            telnet: true,
            taking: Telnet::Data,
            to_machine: VecDeque::new(),
            to_place: Vec::new(),
            quiet_frames: GUARD_FRAMES,
            pluses: 0,
            escaped_frames: None,
            rate: 9600,
        }
    }

    /// Whether it has a carrier: connected.
    pub fn carrier(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        return matches!(self.line, Some(Line::Up(_)));
        #[cfg(target_arch = "wasm32")]
        false
    }

    fn dialling(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        return matches!(self.line, Some(Line::Dialling(_)));
        #[cfg(target_arch = "wasm32")]
        false
    }

    /// Bytes from the machine's serial port.
    pub fn send(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if self.online {
                self.send_online(byte);
            } else {
                self.command_byte(byte);
            }
        }
    }

    /// A byte to the place, as `+++` is watched for: three of them, a
    /// second's quiet before.
    fn send_online(&mut self, byte: u8) {
        if byte == b'+' && (self.pluses > 0 || self.quiet_frames >= GUARD_FRAMES) {
            self.pluses += 1;
            if self.pluses == 3 {
                self.escaped_frames = Some(0);
            }
        } else {
            self.pluses = 0;
            self.escaped_frames = None;
        }
        self.quiet_frames = 0;
        if self.telnet && byte == IAC {
            self.to_place.push(IAC);
        }
        self.to_place.push(byte);
    }

    /// A byte typed in the command mode: echoed, if it's to be; a line, at
    /// its return; `A/`, at once. Dialling, any of them gives up.
    fn command_byte(&mut self, byte: u8) {
        if self.dialling() {
            self.line = None;
            self.result(Result::NoCarrier);
            return;
        }
        let byte = byte & 0x7F;
        if self.echo {
            self.to_machine.push_back(byte);
        }
        match byte {
            b'\r' => {
                let line = std::mem::take(&mut self.typed);
                self.command(&line);
            }
            8 | 127 => {
                self.typed.pop();
            }
            b'\n' => {}
            _ => {
                self.typed.push(byte);
                if self.typed.eq_ignore_ascii_case(b"A/") {
                    self.typed.clear();
                    let last = self.last.clone();
                    self.command(&last);
                }
            }
        }
    }

    /// A command line: `AT` and its commands, each done in turn; a line
    /// that's not, nothing.
    fn command(&mut self, line: &[u8]) {
        let line: Vec<u8> = line
            .iter()
            .filter(|byte| !byte.is_ascii_whitespace())
            .map(u8::to_ascii_uppercase)
            .collect();
        if line.is_empty() {
            return;
        }
        if !line.starts_with(b"AT") {
            self.result(Result::Error);
            return;
        }
        self.last = line.clone();
        let mut at = 2;
        // The number after a command letter, if there's one.
        let number = |at: &mut usize| -> Option<u32> {
            let start = *at;
            while *at < line.len() && line[*at].is_ascii_digit() {
                *at += 1;
            }
            std::str::from_utf8(&line[start..*at]).ok()?.parse().ok()
        };
        while at < line.len() {
            let letter = line[at];
            at += 1;
            match letter {
                b'D' => {
                    if matches!(line.get(at), Some(b'T' | b'P')) {
                        at += 1;
                    }
                    let place = String::from_utf8_lossy(&line[at..]).to_ascii_lowercase();
                    self.dial(&place);
                    return;
                }
                b'H' => {
                    number(&mut at);
                    self.hang_up();
                }
                b'O' => {
                    number(&mut at);
                    if self.carrier() {
                        self.online = true;
                        self.result(Result::Connect);
                    } else {
                        self.result(Result::NoCarrier);
                    }
                    return;
                }
                b'Z' => {
                    number(&mut at);
                    self.hang_up();
                    self.settings_as_they_were();
                }
                b'E' => self.echo = number(&mut at).unwrap_or(0) != 0,
                b'V' => self.words = number(&mut at).unwrap_or(0) != 0,
                b'Q' => self.quiet = number(&mut at).unwrap_or(0) != 0,
                b'I' => {
                    number(&mut at);
                    self.say("OPEN BUS MODEM");
                }
                b'N' if line[at..].starts_with(b"ET") => {
                    at += 2;
                    self.telnet = number(&mut at).unwrap_or(0) != 0;
                }
                b'&' => {
                    let what = line.get(at).copied();
                    at += 1;
                    number(&mut at);
                    if what == Some(b'F') {
                        self.settings_as_they_were();
                    }
                }
                b'S' => {
                    number(&mut at);
                    match line.get(at) {
                        Some(b'=') => {
                            at += 1;
                            number(&mut at);
                        }
                        Some(b'?') => {
                            at += 1;
                            self.say("000");
                        }
                        _ => {}
                    }
                }
                b'M' | b'L' | b'X' | b'B' | b'C' | b'N' | b'W' | b'Y' => {
                    number(&mut at);
                }
                _ => {
                    self.result(Result::Error);
                    return;
                }
            }
        }
        self.result(Result::Ok);
    }

    fn settings_as_they_were(&mut self) {
        (self.echo, self.words, self.quiet, self.telnet) = (true, true, false, true);
    }

    /// Dialling `place`, `host` or `host:port`, the connection made on a
    /// thread of its own, as it may take a while.
    fn dial(&mut self, place: &str) {
        self.hang_up();
        let place = place.trim_matches(|c: char| c == '"' || c == ',');
        if place.is_empty() {
            self.result(Result::Error);
            return;
        }
        let place = if place.contains(':') {
            place.to_string()
        } else {
            format!("{place}:{TELNET_PORT}")
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (sent, answered) = channel();
            std::thread::spawn(move || {
                let connected = place.to_socket_addrs().and_then(|mut addresses| {
                    let address = addresses
                        .next()
                        .ok_or_else(|| std::io::Error::from(ErrorKind::NotFound))?;
                    TcpStream::connect_timeout(&address, DIAL_TIME)
                });
                let _ = sent.send(connected);
            });
            self.line = Some(Line::Dialling(answered));
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = place;
            self.result(Result::NoCarrier);
        }
    }

    fn hang_up(&mut self) {
        self.line = None;
        self.online = false;
        self.to_place.clear();
        self.taking = Telnet::Data;
    }

    /// A result, unless it's quiet: as words, on lines of their own, or
    /// as a number and a return. CONNECT, with the rate.
    fn result(&mut self, result: Result) {
        if self.quiet {
            return;
        }
        if self.words {
            let words = match result {
                Result::Connect => format!("CONNECT {}", self.rate),
                _ => result.words().to_string(),
            };
            self.say(&words);
        } else {
            self.to_machine
                .extend(format!("{}\r", result.number()).bytes());
        }
    }

    /// `words` on a line of their own.
    fn say(&mut self, words: &str) {
        self.to_machine.extend(b"\r\n");
        self.to_machine.extend(words.bytes());
        self.to_machine.extend(b"\r\n");
    }

    /// A frame gone by: the dialling answered, if it has been; what's to
    /// go to the place sent, and what's come from it taken; the escape's
    /// quiet after it waited out.
    pub fn frame(&mut self) {
        self.quiet_frames = self.quiet_frames.saturating_add(1);
        if let Some(frames) = &mut self.escaped_frames {
            *frames += 1;
            if *frames >= GUARD_FRAMES {
                self.escaped_frames = None;
                self.pluses = 0;
                self.online = false;
                self.result(Result::Ok);
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.connection();
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn connection(&mut self) {
        if let Some(Line::Dialling(answered)) = &self.line {
            match answered.try_recv() {
                Ok(Ok(stream)) => {
                    let _ = stream.set_nonblocking(true);
                    let _ = stream.set_nodelay(true);
                    self.line = Some(Line::Up(stream));
                    self.online = true;
                    self.quiet_frames = 0;
                    self.result(Result::Connect);
                }
                Ok(Err(error)) => {
                    self.line = None;
                    let timed_out = error.kind() == ErrorKind::TimedOut;
                    self.result(if timed_out {
                        Result::NoAnswer
                    } else {
                        Result::NoCarrier
                    });
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.line = None;
                    self.result(Result::NoCarrier);
                }
            }
            return;
        }
        let Some(Line::Up(stream)) = &mut self.line else {
            return;
        };
        let mut gone = false;
        if !self.to_place.is_empty() {
            match stream.write(&self.to_place) {
                Ok(written) => {
                    self.to_place.drain(..written);
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                Err(_) => gone = true,
            }
        }
        let mut buffer = [0; 4096];
        let mut came = Vec::new();
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => {
                    gone = true;
                    break;
                }
                Ok(read) => came.extend_from_slice(&buffer[..read]),
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(_) => {
                    gone = true;
                    break;
                }
            }
        }
        for byte in came {
            self.came(byte);
        }
        if gone {
            self.hang_up();
            self.result(Result::NoCarrier);
        }
    }

    /// A byte from the place: the machine's, if it's data; telnet's, if it
    /// isn't, its options answered.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    fn came(&mut self, byte: u8) {
        if !self.telnet {
            self.to_machine.push_back(byte);
            return;
        }
        self.taking = match (self.taking, byte) {
            (Telnet::Data | Telnet::Return, IAC) => Telnet::Command,
            (Telnet::Return, 0) => Telnet::Data,
            (Telnet::Data | Telnet::Return, byte) => {
                self.to_machine.push_back(byte);
                if byte == b'\r' {
                    Telnet::Return
                } else {
                    Telnet::Data
                }
            }
            (Telnet::Command, IAC) => {
                self.to_machine.push_back(IAC);
                Telnet::Data
            }
            (Telnet::Command, WILL | WONT | DO | DONT) => Telnet::Option(byte),
            (Telnet::Command, SB) => Telnet::Sub,
            (Telnet::Command, _) => Telnet::Data,
            (Telnet::Option(asked), option) => {
                self.answer(asked, option);
                Telnet::Data
            }
            (Telnet::Sub, IAC) => Telnet::SubCommand,
            (Telnet::Sub, _) => Telnet::Sub,
            (Telnet::SubCommand, SE) => Telnet::Data,
            (Telnet::SubCommand, _) => Telnet::Sub,
        };
    }

    /// Telnet's option `option` asked for (`DO`) or offered (`WILL`): yes
    /// to those it does, no to the rest. Told no (`DONT`, `WONT`), nothing
    /// said back, as that could go back and forth for ever.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    fn answer(&mut self, asked: u8, option: u8) {
        let does = matches!(option, BINARY | SUPPRESS_GO_AHEAD);
        let reply = match asked {
            DO if does => WILL,
            DO => WONT,
            WILL if does || option == ECHO => DO,
            WILL => DONT,
            _ => return,
        };
        self.to_place.extend([IAC, reply, option]);
    }
}
