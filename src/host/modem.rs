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
//! what it is; `&K`, flow control, `&K4` the machine's XON and XOFF (as it
//! starts), `&K0` none; `F`, online echo, `F0` always, `F1` never, else as
//! telnet says (what's typed echoed till the place says it echoes, as a
//! telnet client shows it); and the rest it's given, as a modem takes them,
//! `M`, `L`, `X`, `&C`, `&D`, `S` registers, done as nothing. `A/` again,
//! the last.
//! A key pressed as it dials gives up.
//!
//! Online, what the machine sends goes to the place, and what comes back
//! to the machine, the carrier up while it lasts. Telnet's: its commands
//! answered (no to all it's asked, but its binary, its echo and its
//! suppressing of go-ahead), $FF doubled, a return's NUL left out. `+++`,
//! with a second's quiet before it and after, back to the command mode,
//! still connected (the `+`s sent on, as a Hayes modem sends them). The
//! place gone, `NO CARRIER`, the carrier dropped.
//!
//! Or, `Modem::zimodem`, Zimodem's (Bo Zimmerman's firmware for WiFi
//! modems, as the X16's network card has on its ESP32), as its README and
//! source have it: `ATD"host:port"`, the place in quotes, over plain TCP,
//! telnet's only with `T` (`ATDT"host:port"`) and S62 not 0; with `E`,
//! what's typed echoed online, `X`, XON and XOFF. `CONNECT 1`, the
//! connection's number (`CONNECT` alone with `X0`); a place that isn't
//! there, `NO ANSWER`. Its flow control `F`: `F0` the machine's RTS (as it
//! starts, as the X16 card's build has it), `F1` to `F3` XON and XOFF,
//! `F4` none; `&K` as Zimodem has it too. `R`, the end of its lines: `R0`
//! a return, `R1` a return and a line feed (as it starts), `R2` the other
//! way round, `R3` a line feed. S registers kept and read back (`S62?`);
//! S2 the escape's character; S46 and S48 its DCD and CTS, as they're up
//! (0) or the other way round (1), always up (2) or never (3); S63=2, DTR
//! dropped hanging up. `+++` hangs up, as it does a place dialled. Its
//! WiFi is the host's network, so `ATW`, `ATB` and `AT+CONFIG` change
//! nothing; its phone book, listening (`ATA`), packets (`ATC`, `ATT`),
//! downloads (`AT&G`), SSH and PETSCII aren't here.

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

/// Software flow control's pause and go on: Ctrl-S and Ctrl-Q.
const XOFF: u8 = 0x13;
const XON: u8 = 0x11;

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
    /// Not on a web page, which never dials.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
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
    /// Local XON/XOFF flow control (`&K4`, as it starts; `&K0` or `&K3`
    /// off): the machine's XOFF holding what's come for it here, its XON
    /// letting it go, neither sent on. Terminal programs such as ProTERM
    /// ask for a pause as they go online, a place's banner pouring in.
    xon_xoff: bool,
    held: bool,
    /// Whether the place echoes what's sent, as telnet's `WILL ECHO` says.
    /// Until it does, what's typed online is echoed here, as a telnet client
    /// shows what it types: a MUD's name prompt, say. `ATF0` always, `ATF1`
    /// never, as a Hayes modem's online echo; else as telnet has it.
    far_echoes: bool,
    online_echo: Option<bool>,
    /// Zimodem's commands, not Hayes's.
    zimodem: bool,
    /// Zimodem's flow control by the machine's RTS (`F0`); the end of its
    /// lines (`R`); `CONNECT` with the connection's number (`X1`); its S
    /// registers; DTR, as the machine has it; the connections it's made,
    /// for their numbers.
    hardware_flow: bool,
    eol: &'static [u8],
    long_results: bool,
    registers: [u8; 64],
    dtr: bool,
    connections: u32,
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
            xon_xoff: true,
            held: false,
            far_echoes: false,
            online_echo: None,
            zimodem: false,
            hardware_flow: false,
            eol: b"\r\n",
            long_results: true,
            registers: [0; 64],
            dtr: false,
            connections: 0,
        }
    }

    /// One with Zimodem's commands, as the X16's network card has it.
    pub fn zimodem() -> Modem {
        let mut modem = Modem::new();
        modem.zimodem = true;
        modem.settings_as_they_were();
        modem
    }

    /// Whether what's come for the machine waits for its RTS: Zimodem's
    /// `F0`.
    pub fn hardware_flow(&self) -> bool {
        self.zimodem && self.hardware_flow
    }

    /// Its DCD, up while there's a carrier (`up`), as S46 has it.
    pub fn dcd(&self, up: bool) -> bool {
        match (self.zimodem, self.registers[46]) {
            (true, 1) => !up,
            (true, 2) => true,
            (true, 3) => false,
            _ => up,
        }
    }

    /// Its CTS, always ready, as S48 has it.
    pub fn cts(&self) -> bool {
        !(self.zimodem && matches!(self.registers[48], 1 | 3))
    }

    /// The machine's DTR, up or down: dropped, with S63=2, a hang-up.
    pub fn dtr(&mut self, up: bool) {
        let dropped = self.dtr && !up;
        self.dtr = up;
        if dropped && self.zimodem && self.registers[63] == 2 && self.carrier() {
            self.hang_up();
            self.result(Result::NoCarrier);
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
            if self.xon_xoff && matches!(byte, XON | XOFF) {
                self.held = byte == XOFF;
                continue;
            }
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
        let escape = if self.zimodem {
            self.registers[2]
        } else {
            b'+'
        };
        if byte == escape && escape < 0x80 && (self.pluses > 0 || self.quiet_frames >= GUARD_FRAMES)
        {
            self.pluses += 1;
            if self.pluses == 3 {
                self.escaped_frames = Some(0);
            }
        } else {
            self.pluses = 0;
            self.escaped_frames = None;
        }
        self.quiet_frames = 0;
        let echo = self.online_echo.unwrap_or(self.telnet && !self.far_echoes);
        if echo {
            self.to_machine.push_back(byte);
            if byte == b'\r' {
                self.to_machine.push_back(b'\n');
            }
        }
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
        // Zimodem's: a line ended by a return or a line feed, echoed as
        // the end of its lines.
        let ended = byte == b'\r' || self.zimodem && byte == b'\n';
        if self.echo {
            if self.zimodem && ended {
                self.to_machine.extend(self.eol);
            } else {
                self.to_machine.push_back(byte);
            }
        }
        match byte {
            _ if ended => {
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
        if self.zimodem {
            self.zimodem_command(line);
            return;
        }
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
                    let value = number(&mut at);
                    match what {
                        Some(b'F') => self.settings_as_they_were(),
                        Some(b'K') => {
                            self.xon_xoff = value == Some(4);
                            self.held &= self.xon_xoff;
                        }
                        _ => {}
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
                b'F' => {
                    self.online_echo = Some(number(&mut at).unwrap_or(0) == 0);
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

    /// A command line as Zimodem has it: what's in quotes as it's typed,
    /// the rest in capitals, its spaces left out.
    fn zimodem_command(&mut self, typed: &[u8]) {
        let mut line = Vec::new();
        let mut quoted = false;
        for &byte in typed {
            if byte == b'"' {
                quoted = !quoted;
                line.push(byte);
            } else if quoted {
                line.push(byte);
            } else if !byte.is_ascii_whitespace() {
                line.push(byte.to_ascii_uppercase());
            }
        }
        if line.is_empty() {
            return;
        }
        if !line.starts_with(b"AT") {
            self.result(Result::Error);
            return;
        }
        self.last = line.clone();
        let mut at = 2;
        let number = |at: &mut usize| -> Option<u32> {
            let start = *at;
            while *at < line.len() && line[*at].is_ascii_digit() {
                *at += 1;
            }
            std::str::from_utf8(&line[start..*at]).ok()?.parse().ok()
        };
        // What's in quotes next, if there is: its text, as it's typed.
        let quoted = |at: &mut usize| -> Option<String> {
            if line.get(*at) != Some(&b'"') {
                return None;
            }
            let start = *at + 1;
            let end = line[start..]
                .iter()
                .position(|&byte| byte == b'"')
                .map_or(line.len(), |n| start + n);
            *at = (end + 1).min(line.len());
            Some(String::from_utf8_lossy(&line[start..end]).into_owned())
        };
        while at < line.len() {
            let letter = line[at];
            at += 1;
            match letter {
                b'D' => {
                    let mut modifiers = Vec::new();
                    while let Some(&modifier @ (b'T' | b'P' | b'E' | b'X' | b'S')) = line.get(at) {
                        modifiers.push(modifier);
                        at += 1;
                    }
                    let place = quoted(&mut at)
                        .unwrap_or_else(|| String::from_utf8_lossy(&line[at..]).into_owned());
                    if place.is_empty() {
                        // The connection there is, online again.
                        if self.carrier() {
                            self.online = true;
                            self.result(Result::Connect);
                        } else {
                            self.result(Result::Error);
                        }
                    } else if modifiers.contains(&b'S') || place.bytes().all(|c| c.is_ascii_digit())
                    {
                        // SSH, or a number in its phone book: not here.
                        self.result(Result::Error);
                    } else {
                        self.telnet = modifiers.contains(&b'T') && self.registers[62] != 0;
                        self.online_echo = Some(modifiers.contains(&b'E'));
                        if modifiers.contains(&b'X') {
                            self.xon_xoff = true;
                        }
                        self.dial(&place.to_ascii_lowercase());
                    }
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
                        self.result(Result::Error);
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
                b'X' => self.long_results = number(&mut at).unwrap_or(0) != 0,
                b'R' => {
                    self.eol = match number(&mut at).unwrap_or(0) {
                        0 => b"\r",
                        2 => b"\n\r",
                        3 => b"\n",
                        _ => b"\r\n",
                    }
                }
                b'F' => self.flow_control(number(&mut at).unwrap_or(0), [0], [1, 2, 3]),
                b'I' => match number(&mut at).unwrap_or(0) {
                    2 => self.say("127.0.0.1"),
                    3 => self.say("THE HOST'S NETWORK"),
                    4 => self.say("4.0"),
                    _ => {
                        self.say("OPEN BUS MODEM, ZIMODEM'S COMMANDS");
                        self.say("ONLINE BY THE HOST'S NETWORK");
                    }
                },
                // Its WiFi, its baud rate, its phone book: the host's, the
                // port's, none.
                b'W' | b'B' | b'P' => {
                    while matches!(line.get(at), Some(b'P' | b'T' | b'E' | b'X')) {
                        at += 1;
                    }
                    if quoted(&mut at).is_none() {
                        number(&mut at);
                    }
                }
                b'C' if line.get(at) != Some(&b'"') => {
                    number(&mut at);
                }
                b'N' | b'L' | b'M' | b'Y' => {
                    number(&mut at);
                }
                b'S' => {
                    let register = number(&mut at).unwrap_or(0) as usize;
                    match line.get(at) {
                        Some(b'=') => {
                            at += 1;
                            let value = number(&mut at).unwrap_or(0).min(255) as u8;
                            if let Some(kept) = self.registers.get_mut(register) {
                                *kept = value;
                            }
                        }
                        Some(b'?') => {
                            at += 1;
                            let value = self.registers.get(register).copied().unwrap_or(0);
                            self.say(&value.to_string());
                        }
                        _ => {}
                    }
                }
                b'&' => {
                    let what = line.get(at).copied();
                    at += 1;
                    if what == Some(b'G') {
                        self.result(Result::Error);
                        return;
                    }
                    if quoted(&mut at).is_some() {
                        continue;
                    }
                    let value = number(&mut at);
                    match what {
                        Some(b'F') => self.settings_as_they_were(),
                        Some(b'K') => self.flow_control(value.unwrap_or(0), [3, 6], [4, 5]),
                        _ => {}
                    }
                }
                b'+' => {
                    let word: Vec<u8> = line[at..]
                        .iter()
                        .copied()
                        .take_while(u8::is_ascii_alphanumeric)
                        .collect();
                    match word.as_slice() {
                        b"CONFIG" => self.say("ITS WIFI IS THE HOST'S NETWORK: NOTHING TO SET"),
                        b"PING" => {}
                        _ => {
                            self.result(Result::Error);
                            return;
                        }
                    }
                    at = line.len();
                }
                _ => {
                    self.result(Result::Error);
                    return;
                }
            }
        }
        self.result(Result::Ok);
    }

    /// Zimodem's flow control `value`: by RTS if it's one of `rts`, XON and
    /// XOFF if it's one of `xon`, else none.
    fn flow_control<const R: usize, const X: usize>(
        &mut self,
        value: u32,
        rts: [u32; R],
        xon: [u32; X],
    ) {
        self.hardware_flow = rts.contains(&value);
        self.xon_xoff = xon.contains(&value);
        self.held &= self.xon_xoff;
    }

    fn settings_as_they_were(&mut self) {
        (self.echo, self.words, self.quiet, self.telnet) = (true, true, false, true);
        self.xon_xoff = true;
        self.held = false;
        self.online_echo = None;
        if self.zimodem {
            self.telnet = false;
            self.xon_xoff = false;
            self.hardware_flow = true;
            self.eol = b"\r\n";
            self.long_results = true;
            self.online_echo = Some(false);
            self.registers = [0; 64];
            (self.registers[2], self.registers[3], self.registers[4]) = (b'+', b'\r', b'\n');
            (self.registers[5], self.registers[40], self.registers[62]) = (8, 127, 1);
        }
    }

    /// What's come for the machine, `most` of it at most, as its port has
    /// room: none while it's held by an XOFF.
    pub fn for_machine(&mut self, most: usize) -> Vec<u8> {
        if self.held {
            return Vec::new();
        }
        let take = most.min(self.to_machine.len());
        self.to_machine.drain(..take).collect()
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
        self.held = false;
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
                Result::Connect if self.zimodem && self.long_results => {
                    format!("CONNECT {}", self.connections)
                }
                Result::Connect if self.zimodem => "CONNECT".to_string(),
                Result::Connect => format!("CONNECT {}", self.rate),
                _ => result.words().to_string(),
            };
            self.say(&words);
        } else if self.zimodem {
            self.to_machine.extend(result.number().to_string().bytes());
            self.to_machine.extend(self.eol);
        } else {
            self.to_machine
                .extend(format!("{}\r", result.number()).bytes());
        }
    }

    /// `words` on a line of their own: Hayes's with a line ended before
    /// them, Zimodem's after only.
    fn say(&mut self, words: &str) {
        if !self.zimodem {
            self.to_machine.extend(self.eol);
        }
        self.to_machine.extend(words.bytes());
        self.to_machine.extend(self.eol);
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
                if self.zimodem {
                    // A place dialled, hung up, as Zimodem does.
                    self.hang_up();
                    self.result(Result::NoCarrier);
                } else {
                    self.result(Result::Ok);
                }
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
                    self.far_echoes = false;
                    self.quiet_frames = 0;
                    self.connections += 1;
                    self.result(Result::Connect);
                }
                Ok(Err(error)) => {
                    self.line = None;
                    let timed_out = error.kind() == ErrorKind::TimedOut || self.zimodem;
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
        if option == ECHO && matches!(asked, WILL | WONT) {
            self.far_echoes = asked == WILL;
        }
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
