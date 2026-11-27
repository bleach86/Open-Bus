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
//! suppressing of go-ahead), $FF doubled, a return's NUL left out unless
//! the place sends in binary. `+++`,
//! with a second's quiet before it and after, back to the command mode,
//! still connected (the `+`s sent on, as a Hayes modem sends them). The
//! place gone, `NO CARRIER`, the carrier dropped.
//!
//! Or, `Modem::zimodem`, Zimodem's (Bo Zimmerman's firmware for WiFi
//! modems, as the X16's network card has on its ESP32), as its README and
//! source have it: `ATD"host:port"`, the place in quotes, over plain TCP,
//! telnet's only with `T` (`ATDT"host:port"`) and S62 not 0, its telnet
//! Zimodem's own: no to every option but the terminal type (`WILL` and
//! `DO` answered `DONT` and `WONT` each time they come, `DONT` with
//! `WONT`), the type "Zimodem" whenever it's asked, nothing asked itself,
//! and nothing taken out but its commands, $FF doubled each way (a
//! return's NUL comes through, so XMODEM's blocks do); with `E`,
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
//! phone book (`host::phone_book`): `ATP"5=host:port,note"` keeps a place
//! as 5, with the dial modifiers before the quote (`ATPT"..."`, telnet's);
//! `ATP` lists them, `ATP?` with their notes; `ATP"5=DELETE"`; `ATD5`
//! dials it. `AT&F` leaves it be, which Zimodem's doesn't, so a program
//! setting the modem up loses nobody's places. Its WiFi is the host's
//! network, so `ATW`, `ATB` and `AT+CONFIG` change nothing.
//!
//! Zimodem's connections in its command mode, too, as programs that keep
//! several open at once use them (Veranda's): `ATC"host:port"` connects,
//! `CONNECT 2`, the connection's number, and stays in the command mode,
//! that one the current one; `ATCS"host:port"` the same over TLS, made by
//! the modem, the server's certificate not checked, as Zimodem's
//! `WiFiClientSecure` doesn't (`setInsecure`). `ATC2` makes 2 the current one; `ATT12`
//! sends it the next 12 bytes, as they are, `OK` after them (`ATT"text"`,
//! the text and a return and line feed); `ATH2` hangs it up, `ATH` and
//! `ATH0` all of them. What comes from them waits in the modem: with `F3`,
//! each XON the machine sends gets a packet, `[ 2 12 34 ]` (the
//! connection, the bytes, their Dallas CRC-8) on a line and then the bytes
//! (S40 of them at most), or `[ 0 0 0 ]` with nothing come, or, once a
//! place that's gone has had all it sent taken, `NO CARRIER 2`. Listening
//! (`ATA`), downloads (`AT&G`), SSH and PETSCII aren't here.

use crate::host::phone_book::{self, Entry, PhoneBook};
#[cfg(not(target_arch = "wasm32"))]
use socket2::{Domain, Protocol, Socket, Type};
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

/// What a modem's buffer holds of what's come for the machine: frames'
/// worth at the port's rate, or this many bytes, if that's more.
#[cfg(not(target_arch = "wasm32"))]
const RECEIVE_BUFFER_FRAMES: usize = 4;
#[cfg(not(target_arch = "wasm32"))]
const RECEIVE_BUFFER_LEAST: usize = 512;
/// The network's receive buffer for the modem's socket, in bytes: small,
/// enough for 80K a second over a tenth of a second's round trip.
#[cfg(not(target_arch = "wasm32"))]
const SOCKET_RECEIVE_BUFFER: usize = 8192;

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
/// The terminal type: Zimodem's only yes, and what it says it is.
const TERMINAL_TYPE: u8 = 24;
const ZIMODEM_TERMINAL: &[u8] = b"Zimodem";

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
    /// A subnegotiation: its option next; then the rest, to its end; and
    /// an IAC in it.
    SubOption,
    Sub(u8),
    SubCommand(u8),
}

/// The connection, once it's made; or being made.
#[cfg(not(target_arch = "wasm32"))]
enum Line {
    Dialling(Receiver<std::io::Result<TcpStream>>),
    Up(TcpStream),
}

#[cfg(target_arch = "wasm32")]
enum Line {}

/// A connection of Zimodem's command mode (`ATC`): its number; how it is;
/// what's come from it, for the machine's packets, and what's to go to it;
/// whether its place has gone, and whether that's been said.
struct Channel {
    id: u32,
    /// Not on a web page, which never connects.
    #[cfg(not(target_arch = "wasm32"))]
    link: Link,
    came: VecDeque<u8>,
    to_place: Vec<u8>,
    gone: bool,
    told: bool,
}

/// Being made, its connection to come; or made.
#[cfg(not(target_arch = "wasm32"))]
enum Link {
    Dialling(Receiver<std::io::Result<Stream>>),
    Up(Stream),
}

/// What a channel's held back in the modem of what's come from its place,
/// at most: the rest's left in the network, as the machine takes packets.
#[cfg(not(target_arch = "wasm32"))]
const CHANNEL_HOLDS: usize = 16 * 1024;

/// A connection's stream: as it comes, or through the modem's TLS.
#[cfg(not(target_arch = "wasm32"))]
enum Stream {
    Plain(TcpStream),
    Secure(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

#[cfg(not(target_arch = "wasm32"))]
impl Stream {
    fn socket(&self) -> &TcpStream {
        match self {
            Stream::Plain(socket) => socket,
            Stream::Secure(stream) => &stream.sock,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Read for Stream {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Stream::Plain(socket) => socket.read(buffer),
            Stream::Secure(stream) => stream.read(buffer),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Write for Stream {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        match self {
            Stream::Plain(socket) => socket.write(bytes),
            Stream::Secure(stream) => stream.write(bytes),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Stream::Plain(socket) => socket.flush(),
            Stream::Secure(stream) => stream.flush(),
        }
    }
}

/// A place's certificate taken as it is, as Zimodem's TLS takes it
/// (`WiFiClientSecure`'s `setInsecure`, its mbedTLS checking none): its
/// handshake's signatures checked, so the connection is the one it says,
/// but not who signed it. mbedTLS reads an X.509 certificate of any
/// version, the first too, as many of Gemini's servers have, which webpki
/// won't: its key's taken from it here, by its DER.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
struct Unchecked(std::sync::Arc<rustls::crypto::CryptoProvider>);

#[cfg(not(target_arch = "wasm32"))]
impl Unchecked {
    /// The handshake's `signed` `message` checked against the key in
    /// `certificate`: by any of the scheme's algorithms for the key's kind
    /// in TLS 1.2, by its first in TLS 1.3, as rustls checks it.
    fn verify(
        &self,
        message: &[u8],
        certificate: &rustls::pki_types::CertificateDer<'_>,
        signed: &rustls::DigitallySignedStruct,
        tls13: bool,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        let bad = |error| rustls::Error::InvalidCertificate(error);
        // TLS 1.3's own (RFC 8446): no PKCS #1, nor SHA-1.
        use rustls::SignatureScheme::*;
        let in_tls13 = matches!(
            signed.scheme,
            ECDSA_NISTP256_SHA256
                | ECDSA_NISTP384_SHA384
                | ECDSA_NISTP521_SHA512
                | RSA_PSS_SHA256
                | RSA_PSS_SHA384
                | RSA_PSS_SHA512
                | ED25519
                | ED448
        );
        if tls13 && !in_tls13 {
            return Err(bad(rustls::CertificateError::BadSignature));
        }
        let (kind, key) =
            public_key(certificate).ok_or(bad(rustls::CertificateError::BadEncoding))?;
        let algorithms = self
            .0
            .signature_verification_algorithms
            .mapping
            .iter()
            .find(|(scheme, _)| *scheme == signed.scheme)
            .map_or(&[][..], |(_, algorithms)| *algorithms);
        let tried = if tls13 {
            &algorithms[..algorithms.len().min(1)]
        } else {
            algorithms
        };
        tried
            .iter()
            .filter(|algorithm| algorithm.public_key_alg_id().as_ref() == kind)
            .any(|algorithm| {
                algorithm
                    .verify_signature(key, message, signed.signature())
                    .is_ok()
            })
            .then(rustls::client::danger::HandshakeSignatureValid::assertion)
            .ok_or(bad(rustls::CertificateError::BadSignature))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl rustls::client::danger::ServerCertVerifier for Unchecked {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &rustls::pki_types::CertificateDer<'_>,
        signed: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.verify(message, certificate, signed, false)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &rustls::pki_types::CertificateDer<'_>,
        signed: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.verify(message, certificate, signed, true)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// A DER element at the start of `bytes`: its tag, what's in it, and
/// what's after it.
fn der(bytes: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let (&tag, rest) = bytes.split_first()?;
    let (&first, rest) = rest.split_first()?;
    let (length, rest) = if first < 0x80 {
        (usize::from(first), rest)
    } else {
        let count = usize::from(first & 0x7F);
        if count == 0 || count > 4 || rest.len() < count {
            return None;
        }
        let length = rest[..count]
            .iter()
            .fold(0, |length, &byte| length << 8 | usize::from(byte));
        (length, &rest[count..])
    };
    (rest.len() >= length).then(|| (tag, &rest[..length], &rest[length..]))
}

/// An X.509 certificate's public key, any version's: its algorithm
/// (inside its AlgorithmIdentifier) and its key's bits.
pub fn public_key(certificate: &[u8]) -> Option<(&[u8], &[u8])> {
    let (_, whole, _) = der(certificate)?;
    let (_, signed, _) = der(whole)?;
    // Its version, [0], there only after the first.
    let mut rest = signed;
    let (tag, _, after) = der(rest)?;
    if tag == 0xA0 {
        rest = after;
    }
    // Its serial number, signature's algorithm, issuer, validity, subject.
    for _ in 0..5 {
        rest = der(rest)?.2;
    }
    let (_, key_info, _) = der(rest)?;
    let (_, algorithm, rest) = der(key_info)?;
    let (tag, bits, _) = der(rest)?;
    let (&unused, key) = bits.split_first()?;
    (tag == 0x03 && unused == 0).then_some((algorithm, key))
}

/// The modem's TLS, made once: ring's, the certificates unchecked.
#[cfg(not(target_arch = "wasm32"))]
fn tls() -> std::io::Result<std::sync::Arc<rustls::ClientConfig>> {
    static CONFIG: std::sync::OnceLock<std::sync::Arc<rustls::ClientConfig>> =
        std::sync::OnceLock::new();
    if let Some(config) = CONFIG.get() {
        return Ok(config.clone());
    }
    let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(std::io::Error::other)?
        .dangerous()
        .with_custom_certificate_verifier(std::sync::Arc::new(Unchecked(provider)))
        .with_no_client_auth();
    Ok(CONFIG.get_or_init(|| std::sync::Arc::new(config)).clone())
}

/// A TCP connection to `place`, `host:port`, its receive buffer small,
/// set before it connects (the window it offers is set then): what the
/// modem hasn't room for waits at the other end, not in this computer's
/// network buffers, a hundred K or more.
#[cfg(not(target_arch = "wasm32"))]
fn connect(place: &str) -> std::io::Result<TcpStream> {
    let address = place
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::from(ErrorKind::NotFound))?;
    let socket = Socket::new(
        Domain::for_address(address),
        Type::STREAM,
        Some(Protocol::TCP),
    )?;
    let _ = socket.set_recv_buffer_size(SOCKET_RECEIVE_BUFFER);
    socket.connect_timeout(&address.into(), DIAL_TIME)?;
    Ok(TcpStream::from(socket))
}

/// `place`, `host:port`, connected to, over the modem's TLS if it's
/// `secure`, its handshake done here (as long as dialling may take) and
/// the stream then left not to block.
#[cfg(not(target_arch = "wasm32"))]
fn open(place: &str, secure: bool) -> std::io::Result<Stream> {
    let socket = connect(place)?;
    let stream = if secure {
        let host = place.rsplit_once(':').map_or(place, |(host, _)| host);
        let name = rustls::pki_types::ServerName::try_from(host.to_string())
            .map_err(|_| std::io::Error::from(ErrorKind::InvalidInput))?;
        let mut tls = rustls::ClientConnection::new(tls()?, name).map_err(std::io::Error::other)?;
        let mut socket = socket;
        socket.set_read_timeout(Some(DIAL_TIME))?;
        socket.set_write_timeout(Some(DIAL_TIME))?;
        while tls.is_handshaking() {
            tls.complete_io(&mut socket)?;
        }
        socket.set_read_timeout(None)?;
        socket.set_write_timeout(None)?;
        Stream::Secure(Box::new(rustls::StreamOwned::new(tls, socket)))
    } else {
        Stream::Plain(socket)
    };
    stream.socket().set_nonblocking(true)?;
    let _ = stream.socket().set_nodelay(true);
    Ok(stream)
}

/// The Dallas (Maxim) CRC-8 of `bytes`, as Zimodem's packets carry it: its
/// polynomial $31 reflected, $8C, from 0.
fn crc8(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0, |crc, &byte| {
        (0..8).fold(crc ^ byte, |crc, _| {
            if crc & 1 != 0 {
                crc >> 1 ^ 0x8C
            } else {
                crc >> 1
            }
        })
    })
}

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
    /// Whether the place sends in binary (`WILL BINARY`, said yes to):
    /// then a return's NUL is its data, not telnet's.
    far_binary: bool,
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
    /// Not read on a web page, which never connects.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    connections: u32,
    /// Zimodem's phone book (`ATP`), kept where the host says.
    pub phone_book: PhoneBook,
    /// Zimodem's connections in its command mode (`ATC`), and the current
    /// one, that `ATT` sends to; the bytes `ATT` has still to take, and
    /// those it has; a packet for each XON (`F3`); the channel last
    /// packeted, the next one's after it.
    channels: Vec<Channel>,
    current: Option<u32>,
    payload: Option<(usize, Vec<u8>)>,
    packet_xon: bool,
    packeted: usize,
    /// The number `CONNECT` says: the connection last made's.
    connected: u32,
}

/// The number in the phone book that's dialled, if what's dialled is a
/// number: digits, and any of `-@,;` (Zimodem's, between them); `None`
/// in it if it's too long to be one kept.
fn dialled_number(place: &str) -> Option<Option<u32>> {
    let a_number = place.bytes().any(|c| c.is_ascii_digit())
        && place
            .bytes()
            .all(|c| c.is_ascii_digit() || b"-@,;".contains(&c));
    if !a_number {
        return None;
    }
    let digits: String = place.chars().filter(char::is_ascii_digit).collect();
    Some(
        digits
            .parse()
            .ok()
            .filter(|_| digits.len() <= phone_book::DIGITS),
    )
}

/// Whether two sets of dial modifiers are the same, in any order.
fn same_modifiers(these: &str, those: &str) -> bool {
    let sorted = |modifiers: &str| {
        let mut letters: Vec<char> = modifiers.to_ascii_lowercase().chars().collect();
        letters.sort_unstable();
        letters.dedup();
        letters
    };
    sorted(these) == sorted(those)
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
            far_binary: false,
            zimodem: false,
            hardware_flow: false,
            eol: b"\r\n",
            long_results: true,
            registers: [0; 64],
            dtr: false,
            connections: 0,
            phone_book: PhoneBook::default(),
            channels: Vec::new(),
            current: None,
            payload: None,
            packet_xon: false,
            packeted: 0,
            connected: 0,
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
            // `ATT`'s bytes, as they are: an XON among them its data.
            if let Some((left, taken)) = &mut self.payload {
                taken.push(byte);
                *left -= 1;
                if *left == 0 {
                    self.payload_taken();
                }
                continue;
            }
            // Zimodem's command mode, `F3`: an XON, a packet.
            if self.zimodem && !self.online && self.packet_xon && byte == XON {
                self.packet();
                continue;
            }
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
                    } else if let Some(number) = dialled_number(&place) {
                        // A number: its place in the phone book, dialled
                        // with the modifiers kept with it, not these.
                        match number.and_then(|number| self.phone_book.find(number)) {
                            Some(entry) => {
                                let modifiers = entry.modifiers.to_ascii_uppercase().into_bytes();
                                let place = entry.place.clone();
                                self.zimodem_dial(&place, &modifiers);
                            }
                            None => self.result(Result::Error),
                        }
                    } else {
                        self.zimodem_dial(&place, &modifiers);
                    }
                    return;
                }
                // A connection of the command mode's hung up, by its
                // number; or, with none (or 0), every one, and a place
                // dialled.
                b'H' => match number(&mut at).filter(|&id| id > 0) {
                    Some(id) if self.channels.iter().any(|channel| channel.id == id) => {
                        self.channels.retain(|channel| channel.id != id);
                        if self.current == Some(id) {
                            self.current = None;
                        }
                    }
                    _ => {
                        self.hang_up();
                        self.hang_up_channels();
                    }
                },
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
                    self.hang_up_channels();
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
                b'F' => {
                    let value = number(&mut at).unwrap_or(0);
                    self.flow_control(value, [0], [1, 2, 3]);
                    // Its own: a packet for each XON, in the command mode.
                    self.packet_xon = value == 3;
                }
                b'I' => match number(&mut at).unwrap_or(0) {
                    2 => self.say("127.0.0.1"),
                    3 => self.say("THE HOST'S NETWORK"),
                    4 => self.say("4.0"),
                    _ => {
                        self.say("OPEN BUS MODEM, ZIMODEM'S COMMANDS");
                        self.say("ONLINE BY THE HOST'S NETWORK");
                    }
                },
                b'P' => {
                    let mut modifiers = String::new();
                    while let Some(
                        &modifier @ (b'E' | b'X' | b'P' | b'R' | b'T' | b'S' | b'+' | b','),
                    ) = line.get(at)
                    {
                        modifiers.push(modifier.to_ascii_lowercase() as char);
                        at += 1;
                    }
                    let value = quoted(&mut at).unwrap_or_else(|| {
                        let rest = String::from_utf8_lossy(&line[at..]).into_owned();
                        at = line.len();
                        rest
                    });
                    if !self.phone_book_command(&value, &modifiers) {
                        self.result(Result::Error);
                        return;
                    }
                }
                // Its WiFi, its baud rate: the host's, the port's.
                b'W' | b'B' => {
                    while matches!(line.get(at), Some(b'P' | b'T' | b'E' | b'X')) {
                        at += 1;
                    }
                    if quoted(&mut at).is_none() {
                        number(&mut at);
                    }
                }
                // A connection in the command mode: made (`S`, over TLS),
                // or made the current one.
                b'C' => {
                    let mut modifiers = Vec::new();
                    while let Some(&modifier @ (b'S' | b'T' | b'E' | b'X' | b'P')) = line.get(at) {
                        modifiers.push(modifier);
                        at += 1;
                    }
                    if let Some(place) = quoted(&mut at) {
                        self.open_channel(&place, modifiers.contains(&b'S'));
                        return;
                    }
                    if let Some(id) = number(&mut at).filter(|&id| id > 0) {
                        if !self.channels.iter().any(|channel| channel.id == id) {
                            self.result(Result::Error);
                            return;
                        }
                        self.current = Some(id);
                    }
                }
                // Bytes to the current connection: so many, as they come
                // next, `OK` after them; or what's quoted, a line.
                b'T' => {
                    if let Some(text) = quoted(&mut at) {
                        let mut bytes = text.into_bytes();
                        bytes.extend(b"\r\n");
                        if !self.send_to_channel(bytes) {
                            self.result(Result::Error);
                            return;
                        }
                        continue;
                    }
                    let count = number(&mut at).unwrap_or(0) as usize;
                    if count > 0 {
                        self.payload = Some((count, Vec::with_capacity(count)));
                        return;
                    }
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

    /// A place dialled, Zimodem's way, with its dial `modifiers`: `T`,
    /// telnet's, if S62 isn't 0; `E`, what's typed echoed online; `X`, XON
    /// and XOFF; `S`, SSH's, which isn't here.
    fn zimodem_dial(&mut self, place: &str, modifiers: &[u8]) {
        if modifiers.contains(&b'S') {
            self.result(Result::Error);
            return;
        }
        self.telnet = modifiers.contains(&b'T') && self.registers[62] != 0;
        self.online_echo = Some(modifiers.contains(&b'E'));
        if modifiers.contains(&b'X') {
            self.xon_xoff = true;
        }
        self.dial(&place.to_ascii_lowercase());
    }

    /// Zimodem's `ATP`, its phone book, with `value` and the dial
    /// `modifiers` before it: whether it's done, as it's not with an
    /// ERROR.
    /// - No value, a number or a character (`ATP?`): the entries listed, a
    ///   number's alone, those with these modifiers if there are any; their
    ///   notes too, but for a number or no value.
    /// - `5=host:port,note`: kept as 5, with these modifiers, over any 5
    ///   there was; the port digits, if it's there.
    /// - `5=DELETE`: 5 gone.
    fn phone_book_command(&mut self, value: &str, modifiers: &str) -> bool {
        let a_number = value.bytes().all(|c| c.is_ascii_digit() || c == b'-');
        if a_number || value.len() == 1 {
            let only: Option<u32> = if a_number && !value.is_empty() {
                let digits: String = value.chars().filter(char::is_ascii_digit).collect();
                digits.parse().ok().filter(|&number| number != 0)
            } else {
                None
            };
            let lines: Vec<String> = self
                .phone_book
                .entries()
                .iter()
                .filter(|entry| only.is_none_or(|number| entry.number == number))
                .filter(|entry| modifiers.is_empty() || same_modifiers(modifiers, &entry.modifiers))
                .map(|entry| {
                    let mut line = format!(
                        "{:<10} {:<4} {}",
                        entry.number, entry.modifiers, entry.place
                    );
                    if !a_number {
                        line += &format!(" ({})", entry.note);
                    }
                    line
                })
                .collect();
            for line in lines {
                self.to_machine.extend(line.bytes());
                self.to_machine.extend(self.eol);
            }
            return true;
        }
        let Some((number, rest)) = value.split_once('=') else {
            return false;
        };
        if number.len() > phone_book::DIGITS || !number.bytes().all(|c| c.is_ascii_digit()) {
            return false;
        }
        let number = number.parse().unwrap_or(0);
        if rest == "DELETE" || rest == "delete" {
            return self.phone_book.delete(number);
        }
        let Some(colon) = rest.find(':') else {
            return false;
        };
        let (place, note) = match rest[colon..].find(',') {
            Some(comma) => (&rest[..colon + comma], &rest[colon + comma + 1..]),
            None => (rest, ""),
        };
        let port = &place[colon + 1..];
        if port.len() > phone_book::DIGITS || !port.bytes().all(|c| c.is_ascii_digit()) {
            return false;
        }
        self.phone_book.add(Entry {
            number,
            place: place.to_string(),
            modifiers: modifiers.to_string(),
            note: note.to_string(),
        });
        true
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
        self.packet_xon = false;
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

    /// A connection of the command mode's to `place`, `host:port`, over
    /// TLS if it's `secure`: made on a thread of its own, `CONNECT` and its
    /// number when it's made, the modem in its command mode still.
    fn open_channel(&mut self, place: &str, secure: bool) {
        let place = place.trim().to_ascii_lowercase();
        if place.is_empty() {
            self.result(Result::Error);
            return;
        }
        let place = if place.contains(':') {
            place
        } else {
            format!("{place}:{TELNET_PORT}")
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (sent, answered) = channel();
            std::thread::spawn(move || {
                let _ = sent.send(open(&place, secure));
            });
            self.channels.push(Channel {
                id: 0,
                link: Link::Dialling(answered),
                came: VecDeque::new(),
                to_place: Vec::new(),
                gone: false,
                told: false,
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (place, secure);
            self.result(Result::NoAnswer);
        }
    }

    /// Every connection of the command mode's hung up.
    fn hang_up_channels(&mut self) {
        self.channels.clear();
        self.current = None;
    }

    /// `bytes` to the current connection: whether there's one to take them.
    fn send_to_channel(&mut self, bytes: Vec<u8>) -> bool {
        let current = self.current;
        match self
            .channels
            .iter_mut()
            .find(|channel| Some(channel.id) == current && channel.id > 0 && !channel.gone)
        {
            Some(channel) => {
                channel.to_place.extend(bytes);
                true
            }
            None => false,
        }
    }

    /// `ATT`'s bytes all taken: to the current connection, `OK`.
    fn payload_taken(&mut self) {
        let Some((_, bytes)) = self.payload.take() else {
            return;
        };
        let result = if self.send_to_channel(bytes) {
            Result::Ok
        } else {
            Result::Error
        };
        self.result(result);
    }

    /// A packet, for the machine's XON: what's come from the next
    /// connection with any, after the one last packeted, S40 bytes of it
    /// at most, after its header, `[ 2 12 34 ]`, the connection, how many
    /// and their CRC; with nothing come, a place gone's `NO CARRIER 2`, once;
    /// else `[ 0 0 0 ]`.
    fn packet(&mut self) {
        let count = self.channels.len();
        let next = (1..=count)
            .map(|step| (self.packeted + step) % count.max(1))
            .find(|&at| !self.channels[at].came.is_empty());
        if let Some(at) = next {
            self.packeted = at;
            let most = usize::from(self.registers[40]).max(1);
            let channel = &mut self.channels[at];
            let take = most.min(channel.came.len());
            let bytes: Vec<u8> = channel.came.drain(..take).collect();
            let header = format!("[ {} {} {} ]", channel.id, bytes.len(), crc8(&bytes));
            self.to_machine.extend(header.bytes());
            self.to_machine.extend(self.eol);
            self.to_machine.extend(bytes);
            return;
        }
        if let Some(channel) = self
            .channels
            .iter_mut()
            .find(|channel| channel.gone && !channel.told)
        {
            channel.told = true;
            let id = channel.id;
            self.say(&format!("NO CARRIER {id}"));
            return;
        }
        self.say("[ 0 0 0 ]");
    }

    /// The command mode's connections: those being made, made, or not,
    /// said; what's to go to each sent, and what's come from each kept, as
    /// much as the modem holds; a place that's gone, kept till what it
    /// sent's taken and it's hung up.
    #[cfg(not(target_arch = "wasm32"))]
    fn channels(&mut self) {
        let mut said = Vec::new();
        let mut failed = Vec::new();
        for (at, channel) in self.channels.iter_mut().enumerate() {
            match &mut channel.link {
                Link::Dialling(answered) => match answered.try_recv() {
                    Ok(Ok(stream)) => {
                        self.connections += 1;
                        channel.id = self.connections;
                        channel.link = Link::Up(stream);
                        said.push(channel.id);
                    }
                    Ok(Err(_)) | Err(TryRecvError::Disconnected) => failed.push(at),
                    Err(TryRecvError::Empty) => {}
                },
                Link::Up(stream) => {
                    if channel.gone {
                        continue;
                    }
                    if !channel.to_place.is_empty() {
                        match stream.write(&channel.to_place) {
                            Ok(written) => {
                                channel.to_place.drain(..written);
                            }
                            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                            Err(_) => channel.gone = true,
                        }
                    }
                    // TLS's records still to go, sent.
                    match stream.flush() {
                        Err(error) if error.kind() != ErrorKind::WouldBlock => channel.gone = true,
                        _ => {}
                    }
                    let mut buffer = [0; 4096];
                    while channel.came.len() < CHANNEL_HOLDS && !channel.gone {
                        let want = (CHANNEL_HOLDS - channel.came.len()).min(buffer.len());
                        match stream.read(&mut buffer[..want]) {
                            Ok(0) => channel.gone = true,
                            Ok(read) => channel.came.extend(&buffer[..read]),
                            Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                            Err(error) if error.kind() == ErrorKind::Interrupted => {}
                            // A TLS place gone without saying so, as many do.
                            Err(_) => channel.gone = true,
                        }
                    }
                }
            }
        }
        for at in failed.into_iter().rev() {
            self.channels.remove(at);
            self.result(Result::NoAnswer);
        }
        for id in said {
            self.current = Some(id);
            self.connected = id;
            self.result(Result::Connect);
        }
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
                let _ = sent.send(connect(&place));
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
                    format!("CONNECT {}", self.connected)
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

    /// Between frames: the network's sending and taking only, as a modem
    /// does all the time; its timing (the escape's guard) left to the
    /// frames.
    pub fn poll(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.connection();
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn connection(&mut self) {
        self.channels();
        if let Some(Line::Dialling(answered)) = &self.line {
            match answered.try_recv() {
                Ok(Ok(stream)) => {
                    let _ = stream.set_nonblocking(true);
                    let _ = stream.set_nodelay(true);
                    self.line = Some(Line::Up(stream));
                    self.online = true;
                    self.far_echoes = false;
                    self.far_binary = false;
                    self.quiet_frames = 0;
                    self.connections += 1;
                    self.connected = self.connections;
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
        // What's come, as its buffer has room: a few frames of it at the
        // port's rate, the rest left in the network, for its flow control to
        // hold back at the other end. A machine slower than its port (a
        // terminal program drawing its screen) takes it slower still; with
        // more held, a place's output could be minutes ahead of the
        // machine, a break or a key answered only once it was all through.
        let most = (self.rate as usize / 10 / 60 * RECEIVE_BUFFER_FRAMES).max(RECEIVE_BUFFER_LEAST);
        let room = most.saturating_sub(self.to_machine.len());
        let mut buffer = [0; 4096];
        let mut came = Vec::new();
        loop {
            let want = (room - came.len()).min(buffer.len());
            if want == 0 {
                break;
            }
            match stream.read(&mut buffer[..want]) {
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
        // A return's NUL, telnet's (RFC 854), left out; not in binary
        // (RFC 856), nor ever by Zimodem, which leaves all data be.
        let drops_nul = !self.zimodem && !self.far_binary;
        self.taking = match (self.taking, byte) {
            (Telnet::Data | Telnet::Return, IAC) => Telnet::Command,
            (Telnet::Return, 0) if drops_nul => Telnet::Data,
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
            (Telnet::Command, SB) => Telnet::SubOption,
            (Telnet::Command, _) => Telnet::Data,
            (Telnet::Option(asked), option) => {
                self.answer(asked, option);
                Telnet::Data
            }
            (Telnet::SubOption, IAC) => Telnet::SubCommand(IAC),
            (Telnet::SubOption, option) => Telnet::Sub(option),
            (Telnet::Sub(option), IAC) => Telnet::SubCommand(option),
            (Telnet::Sub(option), _) => Telnet::Sub(option),
            (Telnet::SubCommand(option), SE) => {
                self.subnegotiated(option);
                Telnet::Data
            }
            (Telnet::SubCommand(option), _) => Telnet::Sub(option),
        };
    }

    /// A subnegotiation ended: Zimodem's terminal type sent, for one about
    /// the terminal type (its `SEND` not looked at, as Zimodem doesn't).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    fn subnegotiated(&mut self, option: u8) {
        if self.zimodem && option == TERMINAL_TYPE {
            self.to_place.extend([IAC, SB, TERMINAL_TYPE, 0]);
            self.to_place.extend_from_slice(ZIMODEM_TERMINAL);
            self.to_place.extend([IAC, SE]);
        }
    }

    /// Telnet's option `option` asked for (`DO`) or offered (`WILL`): yes
    /// to those it does, no to the rest. Told no (`DONT`, `WONT`), nothing
    /// said back, as that could go back and forth for ever. Zimodem's
    /// answers are its own: no to all but the terminal type, and `DONT`
    /// answered with `WONT`, each time, as its firmware does.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    fn answer(&mut self, asked: u8, option: u8) {
        if self.zimodem {
            // Zimodem's: no to all but the terminal type, every time.
            let reply = match (asked, option == TERMINAL_TYPE) {
                (WILL, true) => DO,
                (WILL, false) => DONT,
                (DO, true) => WILL,
                (DO | DONT, _) => WONT,
                _ => return,
            };
            self.to_place.extend([IAC, reply, option]);
            return;
        }
        if option == BINARY && matches!(asked, WILL | WONT) {
            self.far_binary = asked == WILL;
        }
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
