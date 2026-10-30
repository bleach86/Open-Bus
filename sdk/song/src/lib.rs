//! Songs for the machine's sound chip: text, the way a cart's music is
//! written, to the data the SDK's music driver (sdk/lib/music.c) plays.
//! docs/machine.md ("Music") describes the text; in short:
//!
//! ```text
//! # comments, at the start of a line or after a space
//! tempo 132                                   quarter notes a minute
//! filter 900 resonance 8 lowpass              the chip's filter, if used
//! instrument lead pulse 1024 attack 1 decay 6 sustain 10 release 6 volume 11
//! instrument bass saw decay 5 sustain 8 release 4 volume 13 filter
//! voice 0 lead: o4 l8 L e g a b > c4 < b a | g4 e4 d4 e4
//! voice 1 bass: o2 l4 L [c g]2 [a e]2
//! ```
//!
//! A voice line names its instrument and goes on in MML: notes `a`-`g` (`#`
//! or `+` sharp, `-` flat) with an optional length (1, 2, 4, 8, 16, 32, 64;
//! dots add half), `r` a rest, `^` and a length to hold the last note
//! longer, `o` an octave (4 has middle C), `<` and `>` an octave down and
//! up, `l` the length for notes without one, `@name` another instrument,
//! `[...]n` a part n times, `L` where the song loops back to at its end, `|`
//! nothing (for bar lines). More `voice` lines for the same voice go on
//! from where the last stopped.
//!
//! The driver's data, for each voice a stream of events:
//!
//! | event | bytes |
//! |---|---|
//! | end | `00` |
//! | jump | `01` address: go on from there (the loop) |
//! | instrument | `02` waves, pulse width (2), attack/decay, sustain/release, volume, filtered |
//! | rest | `03` frames |
//! | note | `04` frequency (2), frames: the gate on afresh |
//! | hold | `05` frames: the note goes on |
//!
//! and a header: the four streams' addresses (0 for a voice not used), the
//! filter cutoff ($FFFF: leave the filter alone), resonance << 4, and the
//! filter type bits.

use std::collections::HashMap;

/// Frames a second.
const FRAME_RATE: u64 = 60;
/// Ticks in a whole note: every length, dotted or not, is a whole number.
const TICKS: u64 = 192;

#[derive(Clone, Debug, PartialEq)]
pub struct Instrument {
    pub waves: u8,
    pub pulse: u16,
    pub attack: u8,
    pub decay: u8,
    pub sustain: u8,
    pub release: u8,
    pub volume: u8,
    pub filtered: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Instrument(Instrument),
    Rest(u8),
    Note(u16, u8),
    Hold(u8),
    LoopHere,
}

#[derive(Debug)]
pub struct Song {
    pub filter: Option<(u16, u8, u8)>, // cutoff, resonance, type bits
    pub voices: [Option<Vec<Event>>; 4],
    pub looped: [bool; 4],
}

const TRIANGLE: u8 = 0x10;
const SAW: u8 = 0x20;
const PULSE: u8 = 0x40;
const NOISE: u8 = 0x80;
const SYNC: u8 = 0x02;
const RING: u8 = 0x04;

fn number<T: std::str::FromStr>(word: Option<&str>, what: &str) -> Result<T, String> {
    word.and_then(|word| word.parse().ok()).ok_or_else(|| format!("{what}: a number expected"))
}

fn instrument(words: &[&str]) -> Result<Instrument, String> {
    let mut instrument =
        Instrument { waves: 0, pulse: 2048, attack: 0, decay: 0, sustain: 15, release: 0, volume: 15, filtered: false };
    let mut words = words.iter().copied();
    while let Some(word) = words.next() {
        match word {
            "triangle" => instrument.waves |= TRIANGLE,
            "saw" => instrument.waves |= SAW,
            "noise" => instrument.waves |= NOISE,
            "pulse" => {
                instrument.waves |= PULSE;
                instrument.pulse = number(words.next(), "pulse width")?;
            }
            "sync" => instrument.waves |= SYNC,
            "ring" => instrument.waves |= RING,
            "filter" => instrument.filtered = true,
            "attack" => instrument.attack = number(words.next(), word)?,
            "decay" => instrument.decay = number(words.next(), word)?,
            "sustain" => instrument.sustain = number(words.next(), word)?,
            "release" => instrument.release = number(words.next(), word)?,
            "volume" => instrument.volume = number(words.next(), word)?,
            _ => return Err(format!("{word}: not part of an instrument")),
        }
    }
    let nibbles = [instrument.attack, instrument.decay, instrument.sustain, instrument.release, instrument.volume];
    if nibbles.iter().any(|&n| n > 15) || instrument.pulse > 4095 {
        return Err("envelope and volume are 0-15, the pulse width 0-4095".into());
    }
    if instrument.waves & (TRIANGLE | SAW | PULSE | NOISE) == 0 {
        return Err("an instrument needs a wave: triangle, saw, pulse or noise".into());
    }
    Ok(instrument)
}

/// A note's frequency as SOUND_FREQ takes it: Hz x 2^24 / 1 MHz.
fn frequency(octave: i32, semitone: i32) -> u16 {
    let midi = (octave + 1) * 12 + semitone;
    let hz = 440.0 * 2f64.powf((midi - 69) as f64 / 12.0);
    (hz * 16_777_216.0 / 1_000_000.0).round().min(65535.0) as u16
}

/// One voice's MML, as it is written: the time is still in ticks.
#[derive(Debug, PartialEq)]
enum Timed {
    Instrument(String),
    Rest(u64),
    Note(u16, u64),
    Hold(u64),
    LoopHere,
}

struct Mml<'a> {
    text: &'a [u8],
    at: usize,
    octave: i32,
    length: u64,
}

impl Mml<'_> {
    fn peek(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }

    fn digits(&mut self) -> Option<u64> {
        let start = self.at;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.at += 1;
        }
        std::str::from_utf8(&self.text[start..self.at]).ok()?.parse().ok()
    }

    /// A length, if one is written (a note value and dots), in ticks.
    fn length(&mut self) -> Result<Option<u64>, String> {
        let Some(value) = self.digits() else {
            return Ok(None);
        };
        if value == 0 || TICKS % value != 0 {
            return Err(format!("{value}: not a note length"));
        }
        let mut ticks = TICKS / value;
        let mut dot = ticks / 2;
        while self.peek() == Some(b'.') {
            self.at += 1;
            ticks += dot;
            dot /= 2;
        }
        Ok(Some(ticks))
    }

    fn parse(&mut self, until_bracket: bool) -> Result<Vec<Timed>, String> {
        let mut events = Vec::new();
        while let Some(c) = self.peek() {
            self.at += 1;
            match c {
                b' ' | b'\t' | b'|' => {}
                b'a'..=b'g' => {
                    let mut semitone = [9, 11, 0, 2, 4, 5, 7][(c - b'a') as usize];
                    loop {
                        match self.peek() {
                            Some(b'#' | b'+') => semitone += 1,
                            Some(b'-') => semitone -= 1,
                            _ => break,
                        }
                        self.at += 1;
                    }
                    let length = self.length()?.unwrap_or(self.length);
                    events.push(Timed::Note(frequency(self.octave, semitone), length));
                }
                b'r' => {
                    let length = self.length()?.unwrap_or(self.length);
                    events.push(Timed::Rest(length));
                }
                b'^' => {
                    let length = self.length()?.ok_or("^: a length expected")?;
                    events.push(Timed::Hold(length));
                }
                b'o' => self.octave = self.digits().ok_or("o: an octave expected")? as i32,
                b'<' => self.octave -= 1,
                b'>' => self.octave += 1,
                b'l' => self.length = self.length()?.ok_or("l: a length expected")?,
                b'L' => events.push(Timed::LoopHere),
                b'@' => {
                    let start = self.at;
                    while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_') {
                        self.at += 1;
                    }
                    let name = String::from_utf8_lossy(&self.text[start..self.at]).into_owned();
                    events.push(Timed::Instrument(name));
                }
                b'[' => {
                    let part = self.parse(true)?;
                    let times = self.digits().unwrap_or(2);
                    if part.contains(&Timed::LoopHere) {
                        return Err("L inside a repeat".into());
                    }
                    for _ in 0..times {
                        events.extend(part.iter().map(|e| match e {
                            Timed::Instrument(name) => Timed::Instrument(name.clone()),
                            Timed::Rest(t) => Timed::Rest(*t),
                            Timed::Note(f, t) => Timed::Note(*f, *t),
                            Timed::Hold(t) => Timed::Hold(*t),
                            Timed::LoopHere => Timed::LoopHere,
                        }));
                    }
                }
                b']' if until_bracket => return Ok(events),
                _ => return Err(format!("'{}' at {}: not MML", c as char, self.at)),
            }
        }
        if until_bracket {
            return Err("[ without its ]".into());
        }
        Ok(events)
    }
}

/// A line up to its comment: a `#` at the start or after a space. Straight
/// after a note, `#` is a sharp (`g#`).
fn without_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let comment = (0..bytes.len()).find(|&i| bytes[i] == b'#' && (i == 0 || bytes[i - 1].is_ascii_whitespace()));
    &line[..comment.unwrap_or(line.len())]
}

/// A song from its text.
pub fn compile(text: &str) -> Result<Song, String> {
    let mut tempo = 120u64;
    let mut filter = None;
    let mut instruments: HashMap<String, Instrument> = HashMap::new();
    let mut parts: [Option<(String, String)>; 4] = Default::default(); // instrument, MML
    for (index, line) in text.lines().enumerate() {
        let fail = |error: String| format!("line {}: {error}", index + 1);
        let line = without_comment(line).trim();
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.first().copied() {
            None => {}
            Some("tempo") => {
                tempo = number_or(words.get(1), "tempo").map_err(fail)?;
                if tempo == 0 {
                    return Err(fail("tempo 0".into()));
                }
            }
            Some("filter") => {
                let cutoff: u16 = number_or(words.get(1), "filter cutoff").map_err(fail)?;
                let mut resonance = 0u8;
                let mut kind = 0u8;
                let mut rest = words[2..].iter().copied();
                while let Some(word) = rest.next() {
                    match word {
                        "resonance" => resonance = number(rest.next(), word).map_err(fail)?,
                        "lowpass" => kind |= 0x10,
                        "bandpass" => kind |= 0x20,
                        "highpass" => kind |= 0x40,
                        _ => return Err(fail(format!("{word}: not part of the filter"))),
                    }
                }
                if cutoff > 2047 || resonance > 15 {
                    return Err(fail("the cutoff is 0-2047, the resonance 0-15".into()));
                }
                filter = Some((cutoff, resonance, kind));
            }
            Some("instrument") => {
                let name = words.get(1).ok_or_else(|| fail("instrument: a name expected".into()))?;
                instruments.insert(name.to_string(), instrument(&words[2..]).map_err(fail)?);
            }
            Some("voice") => {
                let (head, mml) = line.split_once(':').ok_or_else(|| fail("voice N instrument: MML".into()))?;
                let head: Vec<&str> = head.split_whitespace().collect();
                let voice: usize = number_or(head.get(1), "voice").map_err(fail)?;
                if voice > 3 {
                    return Err(fail("voices are 0-3".into()));
                }
                let part = parts[voice].get_or_insert_with(|| (String::new(), String::new()));
                if let Some(name) = head.get(2) {
                    if part.0.is_empty() {
                        part.0 = name.to_string();
                    }
                }
                part.1.push(' ');
                part.1.push_str(mml);
            }
            Some(word) => return Err(fail(format!("{word}: not tempo, filter, instrument or voice"))),
        }
    }

    let mut voices: [Option<Vec<Event>>; 4] = Default::default();
    let mut looped = [false; 4];
    for (n, part) in parts.iter().enumerate() {
        let Some((first, mml)) = part else { continue };
        let fail = |error: String| format!("voice {n}: {error}");
        let timed = Mml { text: mml.as_bytes(), at: 0, octave: 4, length: TICKS / 4 }.parse(false).map_err(fail)?;
        let find = |name: &str| instruments.get(name).cloned().ok_or_else(|| fail(format!("{name}: no such instrument")));
        let mut events = vec![Event::Instrument(find(first)?)];
        let mut current = first.clone();
        // Ticks to frames: a quarter note is 60 / tempo seconds.
        let frame = |tick: u64| (tick * FRAME_RATE * 60 * 4 + TICKS * tempo / 2) / (TICKS * tempo);
        let mut tick = 0u64;
        for event in timed {
            let (ticks, sound) = match event {
                Timed::Instrument(name) => {
                    events.push(Event::Instrument(find(&name)?));
                    current = name;
                    continue;
                }
                Timed::LoopHere => {
                    looped[n] = true;
                    events.push(Event::LoopHere);
                    events.push(Event::Instrument(find(&current)?));
                    continue;
                }
                Timed::Rest(t) => (t, Sound::Rest),
                Timed::Note(frequency, t) => (t, Sound::Note(frequency)),
                Timed::Hold(t) => (t, Sound::Hold),
            };
            let mut frames = frame(tick + ticks) - frame(tick);
            tick += ticks;
            // In pieces of at most 255 frames; a note's pieces after the
            // first hold it.
            let mut sound = sound;
            while frames > 0 {
                let piece = frames.min(255) as u8;
                events.push(match sound {
                    Sound::Rest => Event::Rest(piece),
                    Sound::Note(frequency) => Event::Note(frequency, piece),
                    Sound::Hold => Event::Hold(piece),
                });
                if let Sound::Note(_) = sound {
                    sound = Sound::Hold;
                }
                frames -= piece as u64;
            }
        }
        voices[n] = Some(events);
    }
    Ok(Song { filter, voices, looped })
}

#[derive(Clone, Copy)]
enum Sound {
    Rest,
    Note(u16),
    Hold,
}

fn number_or<T: std::str::FromStr>(word: Option<&&str>, what: &str) -> Result<T, String> {
    number(word.copied(), what)
}

/// The song as ca65 source exporting `_song_NAME`, in `segment`; C declares
/// it `extern const Song song_NAME;` (music.h).
pub fn asm(name: &str, song: &Song, segment: &str) -> String {
    let name: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let label = |voice: usize| format!("song_{name}_{voice}");
    let mut out = format!(
        "; The song {name}. Made by `song`; edit the .song file instead.\n\n        .export _song_{name}\n\n        .segment \"{segment}\"\n\n_song_{name}:\n"
    );
    let streams: Vec<String> =
        (0..4).map(|v| if song.voices[v].is_some() { label(v) } else { "0".into() }).collect();
    out += &format!("        .addr   {}\n", streams.join(", "));
    let (cutoff, resonance, kind) = song.filter.unwrap_or((0xFFFF, 0, 0));
    out += &format!("        .word   ${cutoff:04X}                   ; filter cutoff\n");
    out += &format!("        .byte   ${:02X}, ${kind:02X}               ; resonance, filter type\n", resonance << 4);
    for (voice, events) in song.voices.iter().enumerate() {
        let Some(events) = events else { continue };
        out += &format!("\n{}:\n", label(voice));
        for event in events {
            out += &match event {
                Event::Instrument(i) => format!(
                    "        .byte   $02, ${:02X}, ${:02X}, ${:02X}, ${:02X}, ${:02X}, ${:02X}, {}\n",
                    i.waves,
                    i.pulse & 0xFF,
                    i.pulse >> 8,
                    i.attack << 4 | i.decay,
                    i.sustain << 4 | i.release,
                    i.volume,
                    i.filtered as u8
                ),
                Event::Rest(f) => format!("        .byte   $03, {f}\n"),
                Event::Note(freq, f) => format!("        .byte   $04, ${:02X}, ${:02X}, {f}\n", freq & 0xFF, freq >> 8),
                Event::Hold(f) => format!("        .byte   $05, {f}\n"),
                Event::LoopHere => format!("{}_loop:\n", label(voice)),
            };
        }
        out += &if song.looped[voice] {
            format!("        .byte   $01\n        .addr   {}_loop\n", label(voice))
        } else {
            "        .byte   $00\n".to_string()
        };
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SONG: &str = "
        # A test.
        tempo 120
        filter 900 resonance 8 lowpass
        instrument lead pulse 1024 attack 1 decay 6 sustain 10 release 6 volume 11
        instrument bass saw volume 13 filter
        voice 0 lead: o4 l8 c d e4. r16 | >c2^4
        voice 1 bass: o2 L [c g]2
    ";

    #[test]
    fn notes_take_their_frames_at_the_tempo() {
        let song = compile(SONG).unwrap();
        let lead = song.voices[0].as_ref().unwrap();
        // 120 a minute: a quarter note is half a second, 30 frames.
        let c4 = frequency(4, 0);
        assert_eq!(c4, (261.6256f64 * 16.777216).round() as u16);
        assert_eq!(lead[1], Event::Note(c4, 15), "an eighth");
        assert_eq!(lead[3], Event::Note(frequency(4, 4), 45), "a dotted quarter");
        assert_eq!(lead[4], Event::Rest(8), "a sixteenth: 7.5 frames, rounded where it ends");
        assert_eq!(lead[5], Event::Note(frequency(5, 0), 60), "an octave up, a half");
        assert_eq!(lead[6], Event::Hold(30), "held a quarter longer");
        assert!(!song.looped[0]);
    }

    #[test]
    fn repeats_unfold_and_the_loop_point_carries_the_instrument() {
        let song = compile(SONG).unwrap();
        let bass = song.voices[1].as_ref().unwrap();
        assert!(matches!(bass[0], Event::Instrument(Instrument { waves: SAW, filtered: true, .. })));
        assert_eq!(bass[1], Event::LoopHere);
        assert!(matches!(bass[2], Event::Instrument(_)));
        let notes: Vec<u16> = bass.iter().filter_map(|e| if let Event::Note(f, _) = e { Some(*f) } else { None }).collect();
        assert_eq!(notes, [frequency(2, 0), frequency(2, 7), frequency(2, 0), frequency(2, 7)]);
        assert!(song.looped[1]);
        let source = asm("test", &song, "RODATA");
        assert!(source.contains("song_test_1_loop:") && source.contains(".addr   song_test_1_loop"));
        assert!(source.contains(".addr   song_test_0, song_test_1, 0, 0"));
    }

    /// `#` after a note is a sharp; at the start of a line or after a
    /// space, a comment.
    #[test]
    fn a_hash_is_a_sharp_after_a_note_and_otherwise_a_comment() {
        let song = compile("# a song\ninstrument a triangle  # the only one\nvoice 0 a: g# a #c d").unwrap();
        let notes: Vec<u16> = song.voices[0].as_ref().unwrap().iter().filter_map(|e| if let Event::Note(f, _) = e { Some(*f) } else { None }).collect();
        assert_eq!(notes, [frequency(4, 8), frequency(4, 9)], "g#, a; the rest a comment");
    }

    #[test]
    fn long_notes_come_in_pieces_and_mistakes_are_named() {
        let song = compile("tempo 30\ninstrument a triangle\nvoice 0 a: c1").unwrap();
        // 30 a minute: a whole note is 8 seconds, 480 frames.
        assert_eq!(&song.voices[0].as_ref().unwrap()[1..], [Event::Note(frequency(4, 0), 255), Event::Hold(225)]);
        assert!(compile("instrument a triangle\nvoice 0 b: c").unwrap_err().contains("b: no such instrument"));
        assert!(compile("instrument a triangle\nvoice 0 a: c5").unwrap_err().contains("not a note length"));
        assert!(compile("instrument a volume 3").unwrap_err().contains("needs a wave"));
    }
}
