//! The sound chip, `$FF80`, after the C64's SID: four voices, each an
//! oscillator (triangle, saw, pulse of any width, noise; ring modulation and
//! hard sync from the voice before it) shaped by an ADSR envelope that a
//! gate bit starts and releases; then a resonant multimode filter the voices
//! can be routed through, and the master volume. Voice 3's oscillator and
//! envelope can be read back (OSC3, ENV3): on noise, a source of random
//! bytes, as on the SID. docs/machine.md has the registers.
//!
//! Each frame it makes that frame's samples from the registers as they
//! stand. The saw and pulse are band-limited (polyBLEP), so their edges
//! don't alias into noise, and the mix goes through the output stage a real
//! chip sits behind: a gentle low-pass and the coupling capacitor's
//! high-pass. The speaker is the host's `aplay`, run only while there is
//! sound: silence costs nothing.

use std::f64::consts::{PI, TAU};
use std::io::Write;
use std::process::{Child, Command, Stdio};

pub const SOUND: u16 = 0xFF80;
pub const SOUND_END: u16 = 0xFFA3; /* the registers written; OSC3 and ENV3 follow, read only */

const SAMPLE_RATE: u32 = 48_000;
const CHIP_CLOCK: f64 = 1_000_000.0;
const VOICES: usize = 4;
const VOICE_SIZE: usize = 8;
/// Registers after the voices.
const CUTOFF: usize = 0x20;
const FILTER: usize = 0x22;
const MODE: usize = 0x23;
const OSC3: usize = 0x24;
const ENV3: usize = 0x25;
/* MODE bit 7: voice 3 out of the direct output (still readable, still
 * heard through the filter if routed there). */
const VOICE3_OFF: u8 = 0x80;

const GATE: u8 = 0x01;
const SYNC: u8 = 0x02;
const RING: u8 = 0x04;
const TEST: u8 = 0x08;
const TRIANGLE: u8 = 0x10;
const SAW: u8 = 0x20;
const PULSE: u8 = 0x40;
const NOISE: u8 = 0x80;
const LOW_PASS: u8 = 0x10;
const BAND_PASS: u8 = 0x20;
const HIGH_PASS: u8 = 0x40;

/// The SID's envelope times, in ms: attack from silence to full; decay and
/// release three times as long, from full to silence.
const ATTACK_MS: [f64; 16] = [
    2.0, 8.0, 16.0, 24.0, 38.0, 56.0, 68.0, 80.0, 100.0, 250.0, 500.0, 800.0, 1000.0, 3000.0, 5000.0, 8000.0,
];

/// One voice at full volume; four of them, resonance and all, fit in an i16.
const VOICE_PEAK: f64 = 6_000.0;
/// The output stage.
const LOW_PASS_HZ: f64 = 14_000.0;
const HIGH_PASS_HZ: f64 = 20.0;
/// Silence the stream starts with, so that a late frame finds sound still
/// queued instead of an empty speaker (an underrun is a crack).
const LEAD_MS: u32 = 80;
/// Silence after which the speaker stops.
const SPEAKER_IDLE_FRAMES: u32 = 60;

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    Attack,
    Decay,
    Release,
}

#[derive(Clone, Copy)]
struct Voice {
    /// Where the oscillator is in its cycle, 0-1.
    phase: f64,
    /// The cycle wrapped this sample (for the next voice's sync).
    wrapped: bool,
    /// Noise: a 23-bit shift register, stepped 16 times a cycle.
    lfsr: u32,
    noise_phase: f64,
    envelope: f64,
    stage: Stage,
}

impl Voice {
    /// The oscillator `dt` of a cycle on: its phase, and the noise register
    /// stepped 16 times a cycle.
    fn advance(&mut self, dt: f64) {
        self.phase += dt;
        self.wrapped = self.phase >= 1.0;
        self.phase -= self.phase.floor();
        self.noise_phase += 16.0 * dt;
        for _ in 0..self.noise_phase.floor() as u32 {
            self.step_noise();
        }
        self.noise_phase -= self.noise_phase.floor();
    }

    /// The envelope `samples` on, in the stage the gate set (see `write`).
    fn step_envelope(&mut self, set: &VoiceSettings, samples: i32) {
        match self.stage {
            Stage::Attack => {
                self.envelope += set.attack_step * samples as f64;
                if self.envelope >= 1.0 {
                    self.envelope = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.envelope = set.sustain + (self.envelope - set.sustain) * set.decay_fall.powi(samples)
            }
            Stage::Release => self.envelope *= set.release_fall.powi(samples),
        }
    }

    fn step_noise(&mut self) {
        let bit = (self.lfsr >> 22 ^ self.lfsr >> 17) & 1;
        self.lfsr = (self.lfsr << 1 | bit) & 0x7F_FFFF;
    }

    /// Eight of the noise register's bits, as the SID takes them.
    fn noise_byte(&self) -> u8 {
        [22, 20, 16, 13, 11, 7, 4, 2].iter().fold(0, |byte, &bit| byte << 1 | (self.lfsr >> bit & 1) as u8)
    }

    /// The oscillator's output as a byte, 0-255, as OSC3 reads it: the
    /// waves `control` picks, ANDed if more than one; 0 for none.
    fn wave_byte(&self, control: u8, width: f64) -> u8 {
        let t = self.phase;
        let mut byte = 0xFF;
        let mut any = false;
        let mut take = |bit: u8, value: u8| {
            if control & bit != 0 {
                byte &= value;
                any = true;
            }
        };
        take(TRIANGLE, ((1.0 - (2.0 * t - 1.0).abs()) * 255.0) as u8);
        take(SAW, (t * 256.0) as u8);
        take(PULSE, if t < width { 0xFF } else { 0 });
        take(NOISE, self.noise_byte());
        if any { byte } else { 0 }
    }
}

impl Default for Voice {
    fn default() -> Self {
        Voice {
            phase: 0.0,
            wrapped: false,
            lfsr: 0x7F_FFF8,
            noise_phase: 0.0,
            envelope: 0.0,
            stage: Stage::Release,
        }
    }
}

pub struct Sound {
    regs: [u8; SOUND_END as usize - SOUND as usize + 1],
    voices: [Voice; VOICES],
    /// The filter's two integrators.
    band: f64,
    low: f64,
    /// The output stage: the low-pass's last output, and the high-pass's
    /// last input and output.
    out_low: f64,
    high_in: f64,
    high_out: f64,
    /// Samples made but not yet a whole sample.
    owed: f64,
    speaker: Option<Child>,
    /// A speaker playing out what it was given, to be reaped.
    stopping: Option<Child>,
    /// Frames of silence in a row.
    silent_frames: u32,
}

impl Default for Sound {
    fn default() -> Self {
        Sound {
            regs: [0; SOUND_END as usize - SOUND as usize + 1],
            voices: [Voice::default(); VOICES],
            band: 0.0,
            low: 0.0,
            out_low: 0.0,
            high_in: 0.0,
            high_out: 0.0,
            owed: 0.0,
            speaker: None,
            stopping: None,
            silent_frames: 0,
        }
    }
}

/// The correction that turns a naive step at phase 0 into a band-limited
/// one; `dt` is the phase a sample moves.
fn poly_blep(t: f64, dt: f64) -> f64 {
    if t < dt {
        let t = t / dt;
        2.0 * t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + 2.0 * t + 1.0
    } else {
        0.0
    }
}

impl Sound {
    /// The reset line: every register 0, so every voice silent.
    pub fn reset(&mut self) {
        self.regs = [0; SOUND_END as usize - SOUND as usize + 1];
        self.voices = [Voice::default(); VOICES];
    }

    /// The registers read back as written, and voice 3's oscillator and
    /// envelope. Each read of OSC3 on noise steps the noise on, so that
    /// reads in a row give different bytes.
    pub fn read(&mut self, addr: u16) -> u8 {
        let value = self.peek(addr);
        if (addr - SOUND) as usize == OSC3 && self.regs[3 * VOICE_SIZE + 4] & NOISE != 0 {
            self.voices[3].step_noise();
        }
        value
    }

    /// What a read gives, without the read's effect (OSC3's noise moving on).
    pub fn peek(&self, addr: u16) -> u8 {
        let at = (addr - SOUND) as usize;
        let voice = &self.voices[3];
        match at {
            OSC3 => {
                let control = self.regs[3 * VOICE_SIZE + 4];
                let width = (u16::from_le_bytes([self.regs[3 * VOICE_SIZE + 2], self.regs[3 * VOICE_SIZE + 3]]) & 0xFFF) as f64;
                voice.wave_byte(control, width / 4096.0)
            }
            ENV3 => (voice.envelope * 255.0) as u8,
            _ => self.regs.get(at).copied().unwrap_or(0),
        }
    }

    /// A write to a voice's control register acts on its gate at once, so
    /// that the gate going off and on again inside one frame still starts a
    /// new attack.
    pub fn write(&mut self, addr: u16, value: u8) {
        let at = (addr - SOUND) as usize;
        if at < VOICES * VOICE_SIZE && at % VOICE_SIZE == 4 {
            let (was, now) = (self.regs[at] & GATE != 0, value & GATE != 0);
            let voice = &mut self.voices[at / VOICE_SIZE];
            if now && !was {
                voice.stage = Stage::Attack;
            } else if was && !now {
                voice.stage = Stage::Release;
            }
        }
        if let Some(reg) = self.regs.get_mut(at) {
            *reg = value;
        }
    }

    /// One frame's samples, sent to the speaker. The speaker starts with a
    /// sound and stops after a second of silence.
    pub fn end_frame(&mut self) {
        self.owed += SAMPLE_RATE as f64 / 60.0;
        let count = self.owed as usize;
        self.owed -= count as f64;
        if let Some(stopping) = &mut self.stopping {
            if !matches!(stopping.try_wait(), Ok(None)) {
                self.stopping = None;
            }
        }
        if self.quiet() {
            // Nothing to hear, but the oscillators and envelopes run on
            // (OSC3 and ENV3 read them).
            let settings = Settings::new(&self.regs);
            for (voice, set) in self.voices.iter_mut().zip(&settings.voices) {
                voice.step_envelope(set, count as i32);
                if set.control & TEST == 0 {
                    voice.advance(set.dt * count as f64);
                }
            }
            self.silent_frames = self.silent_frames.saturating_add(1);
            if self.silent_frames < SPEAKER_IDLE_FRAMES {
                self.play(&vec![0; count]);
            } else if let Some(mut speaker) = self.speaker.take() {
                drop(speaker.stdin.take()); // it plays out what it has, then exits
                self.stopping = Some(speaker);
            }
            return;
        }
        self.silent_frames = 0;
        let samples = self.samples(count);
        if self.speaker.is_none() {
            self.speaker = Command::new("aplay")
                .args(["-q", "-t", "raw", "-f", "S16_LE", "-c", "1", "-r"])
                .arg(SAMPLE_RATE.to_string())
                .arg("-B")
                .arg((4 * LEAD_MS * 1000).to_string())
                .stdin(Stdio::piped())
                .spawn()
                .map_err(|error| eprintln!("aplay: {error}"))
                .ok();
            self.play(&vec![0; (SAMPLE_RATE * LEAD_MS / 1000) as usize]);
        }
        self.play(&samples);
    }

    fn play(&mut self, samples: &[i16]) {
        let bytes: Vec<u8> = samples.iter().flat_map(|sample| sample.to_le_bytes()).collect();
        if let Some(stdin) = self.speaker.as_mut().and_then(|speaker| speaker.stdin.as_mut()) {
            if stdin.write_all(&bytes).is_err() {
                self.speaker = None;
            }
        }
    }

    fn samples(&mut self, count: usize) -> Vec<i16> {
        let settings = Settings::new(&self.regs);
        (0..count).map(|_| self.sample(&settings)).collect()
    }

    /// Every voice has died away (released, or decayed to no sustain) and
    /// the filters have settled: silence, at no cost. Settles what's left to
    /// exact zero.
    fn quiet(&mut self) -> bool {
        const SETTLED: f64 = 1e-5;
        let unheard = |n: usize| {
            n == 3 && self.regs[MODE] & VOICE3_OFF != 0 && self.regs[FILTER] & 1 << 3 == 0
        };
        let voices = (0..VOICES)
            .all(|n| unheard(n) || self.voices[n].stage != Stage::Attack && self.voices[n].envelope < SETTLED);
        let filters = [self.band, self.low, self.out_low, self.high_in, self.high_out];
        if !voices || filters.iter().any(|x| x.abs() >= SETTLED) {
            return false;
        }
        for (n, voice) in self.voices.iter_mut().enumerate() {
            if !unheard(n) {
                voice.envelope = 0.0;
            }
        }
        (self.band, self.low, self.out_low, self.high_in, self.high_out) = (0.0, 0.0, 0.0, 0.0, 0.0);
        true
    }

    fn sample(&mut self, settings: &Settings) -> i16 {
        let (mut direct, mut filtered) = (0.0, 0.0);
        for n in 0..VOICES {
            let level = self.voice(n, &settings.voices[n]) * self.voices[n].envelope;
            if settings.route & 1 << n != 0 {
                filtered += level;
            } else if !(n == 3 && settings.mode & VOICE3_OFF != 0) {
                direct += level;
            }
        }

        // The filter: a state-variable filter (trapezoidal, so it stays
        // stable up to its top cutoff).
        let (g, k) = (settings.g, settings.k);
        let high = (filtered - (k + g) * self.band - self.low) / (1.0 + g * (g + k));
        let band = g * high + self.band;
        self.band = g * high + band;
        let low = g * band + self.low;
        self.low = g * band + low;
        let mode = settings.mode;
        let mut out = direct;
        out += if mode & LOW_PASS != 0 { low } else { 0.0 };
        out += if mode & BAND_PASS != 0 { band } else { 0.0 };
        out += if mode & HIGH_PASS != 0 { high } else { 0.0 };
        out *= settings.master;

        // The output stage: one-pole low-pass, then the coupling high-pass.
        self.out_low += (out - self.out_low) * settings.out_low_k;
        self.high_out = settings.high_k * (self.high_out + self.out_low - self.high_in);
        self.high_in = self.out_low;
        (self.high_out * VOICE_PEAK).clamp(i16::MIN as f64, i16::MAX as f64) as i16
    }

    /// Voice `n`'s next sample, -1 to 1, before its envelope; steps its
    /// oscillator and envelope.
    fn voice(&mut self, n: usize, set: &VoiceSettings) -> f64 {
        let source = self.voices[(n + VOICES - 1) % VOICES];
        let voice = &mut self.voices[n];
        let (control, width, dt) = (set.control, set.width, set.dt);

        voice.step_envelope(set, 1);

        // The oscillator.
        if control & TEST != 0 {
            voice.phase = 0.0;
            voice.lfsr = 0x7F_FFF8;
            voice.wrapped = false;
            return 0.0;
        }
        voice.advance(dt);
        if control & SYNC != 0 && source.wrapped {
            voice.phase = 0.0;
        }

        let t = voice.phase;
        let triangle = || {
            let wave = 1.0 - 4.0 * (t - 0.5).abs();
            if control & RING != 0 && source.phase >= 0.5 { -wave } else { wave }
        };
        let saw = || 2.0 * t - 1.0;
        let pulse = || if t < width { 1.0 } else { -1.0 };
        let noise = || voice.noise_byte() as f64 / 127.5 - 1.0;
        let waves = control & (TRIANGLE | SAW | PULSE | NOISE);
        let level = match waves {
            0 => 0.0,
            TRIANGLE => triangle(),
            SAW => saw() - poly_blep(t, dt),
            PULSE => pulse() + poly_blep(t, dt) - poly_blep((t - width).rem_euclid(1.0), dt),
            NOISE => noise(),
            _ => {
                // More than one: the SID ANDs them, as 8-bit values.
                let byte = |level: f64| ((level + 1.0) * 127.5) as u8;
                let mut and = 0xFF;
                for (bit, level) in [(TRIANGLE, triangle()), (SAW, saw()), (PULSE, pulse()), (NOISE, noise())] {
                    if waves & bit != 0 {
                        and &= byte(level);
                    }
                }
                and as f64 / 127.5 - 1.0
            }
        };
        level * set.volume
    }
}

/// What the registers ask for, worked out once a frame rather than once a
/// sample: they only change between samples a frame apart.
struct Settings {
    voices: [VoiceSettings; VOICES],
    route: u8,
    mode: u8,
    master: f64,
    /// The filter: cutoff 30 Hz-12 kHz, resonance from none to near
    /// self-oscillation.
    g: f64,
    k: f64,
    out_low_k: f64,
    high_k: f64,
}

struct VoiceSettings {
    /// Oscillator phase a sample.
    dt: f64,
    width: f64,
    control: u8,
    /// Attack is a straight rise; decay and release fall exponentially,
    /// 60 dB in their time.
    attack_step: f64,
    decay_fall: f64,
    release_fall: f64,
    sustain: f64,
    volume: f64,
}

impl Settings {
    fn new(regs: &[u8]) -> Settings {
        let rate = SAMPLE_RATE as f64;
        let fall = |ms: f64| (-(1000.0f64).ln() / (ms / 1000.0 * rate)).exp();
        let voices = std::array::from_fn(|n| {
            let regs = &regs[n * VOICE_SIZE..(n + 1) * VOICE_SIZE];
            let frequency = u16::from_le_bytes([regs[0], regs[1]]) as f64 * CHIP_CLOCK / 16_777_216.0;
            VoiceSettings {
                dt: frequency / rate,
                width: (u16::from_le_bytes([regs[2], regs[3]]) & 0xFFF) as f64 / 4096.0,
                control: regs[4],
                attack_step: 1000.0 / (ATTACK_MS[(regs[5] >> 4) as usize] * rate),
                decay_fall: fall(3.0 * ATTACK_MS[(regs[5] & 0x0F) as usize]),
                release_fall: fall(3.0 * ATTACK_MS[(regs[6] & 0x0F) as usize]),
                sustain: (regs[6] >> 4) as f64 / 15.0,
                volume: (regs[7] & 0x0F) as f64 / 15.0,
            }
        });
        let cutoff = (u16::from_le_bytes([regs[CUTOFF], regs[CUTOFF + 1]]) & 0x7FF) as f64;
        let hz = 30.0 + cutoff * (12_000.0 - 30.0) / 2047.0;
        Settings {
            voices,
            route: regs[FILTER] & 0x0F,
            mode: regs[MODE],
            master: (regs[MODE] & 0x0F) as f64 / 15.0,
            g: (PI * hz / rate).tan(),
            k: 2.0 - 1.9 * (regs[FILTER] >> 4) as f64 / 15.0,
            out_low_k: 1.0 - (-TAU * LOW_PASS_HZ / rate).exp(),
            high_k: 1.0 / (1.0 + TAU * HIGH_PASS_HZ / rate),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Voice 0 at `hz` with `waves`, full volume, instant attack, full
    /// sustain; the master volume up.
    fn note(sound: &mut Sound, hz: f64, waves: u8) {
        let freq = (hz * 16_777_216.0 / CHIP_CLOCK).round() as u16;
        let [lo, hi] = freq.to_le_bytes();
        for (reg, value) in [(0, lo), (1, hi), (2, 0x00), (3, 0x08), (5, 0x00), (6, 0xF0), (7, 15)] {
            sound.write(SOUND + reg, value);
        }
        sound.write(SOUND + MODE as u16, 0x0F);
        sound.write(SOUND + 4, waves | GATE);
    }

    fn rises(samples: &[i16]) -> usize {
        samples.windows(2).filter(|pair| pair[0] < 0 && pair[1] >= 0).count()
    }

    #[test]
    fn silent_at_reset() {
        let mut sound = Sound::default();
        assert!(sound.samples(1000).iter().all(|&sample| sample == 0));
    }

    #[test]
    fn each_wave_makes_its_frequency() {
        for waves in [TRIANGLE, SAW, PULSE] {
            let mut sound = Sound::default();
            note(&mut sound, 440.0, waves);
            let samples = sound.samples(SAMPLE_RATE as usize);
            let rises = rises(&samples[4800..]) as f64 / 0.9;
            assert!((437.0..=443.0).contains(&rises), "wave {waves:#x}: {rises} Hz");
            let peak = *samples.iter().max().unwrap() as f64 / VOICE_PEAK;
            assert!((0.8..1.2).contains(&peak), "wave {waves:#x}: peak {peak}");
        }
    }

    #[test]
    fn the_gate_starts_and_releases_the_envelope() {
        let mut sound = Sound::default();
        note(&mut sound, 440.0, PULSE);
        sound.write(SOUND + 6, 0xF0); // sustain full, release 6 ms
        sound.samples(4800);
        sound.write(SOUND + 4, PULSE); // gate off
        let tail = sound.samples(4800);
        let loud = |samples: &[i16]| samples.iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!(loud(&tail[..100]) > 3000, "still sounding as the release starts");
        assert!(loud(&tail[2400..]) < 30, "silent after it");
    }

    #[test]
    fn the_gate_off_and_on_between_samples_starts_a_new_note() {
        let mut sound = Sound::default();
        note(&mut sound, 440.0, PULSE);
        sound.write(SOUND + 5, 0x03); // decays away, no sustain
        sound.write(SOUND + 6, 0x00);
        sound.samples(48_000);
        let loud = |samples: &[i16]| samples.iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!(loud(&sound.samples(480)) < 30, "died away");
        sound.write(SOUND + 4, PULSE);
        sound.write(SOUND + 4, PULSE | GATE);
        assert!(loud(&sound.samples(480)) > 3000, "sounding again");
    }

    #[test]
    fn a_note_that_decays_away_is_silence_even_with_its_gate_on() {
        let mut sound = Sound::default();
        note(&mut sound, 440.0, PULSE);
        sound.write(SOUND + 5, 0x02); // decays away, no sustain
        sound.write(SOUND + 6, 0x00);
        assert!(!sound.quiet(), "sounding");
        sound.samples(48_000);
        assert!(sound.quiet(), "gone quiet with the gate still on");
    }

    #[test]
    fn voice_3_reads_back_as_random_bytes_without_being_heard() {
        let mut sound = Sound::default();
        sound.write(SOUND + 3 * 8, 0xFF); // the top frequency
        sound.write(SOUND + 3 * 8 + 1, 0xFF);
        sound.write(SOUND + 3 * 8 + 4, NOISE); // no gate: silent anyway
        sound.write(SOUND + MODE as u16, VOICE3_OFF | 15);
        let bytes: Vec<u8> = (0..256).map(|_| sound.read(SOUND + OSC3 as u16)).collect();
        let mut seen = [false; 256];
        bytes.iter().for_each(|&b| seen[b as usize] = true);
        assert!(seen.iter().filter(|&&s| s).count() > 100, "spread over the bytes");
        // Silent frames run the oscillator on: the next read differs.
        let before = sound.voices[3].lfsr;
        sound.end_frame();
        assert_ne!(sound.voices[3].lfsr, before);
        assert_eq!(sound.read(SOUND + ENV3 as u16), 0);
        // Gated and off the output, it is still not heard.
        sound.write(SOUND + 3 * 8 + 6, 0xF0);
        sound.write(SOUND + 3 * 8 + 7, 15);
        sound.write(SOUND + 3 * 8 + 4, NOISE | GATE);
        assert!(sound.samples(4800).iter().all(|&s| s == 0));
        assert!(sound.read(SOUND + ENV3 as u16) > 200, "its envelope is up all the same");
    }

    #[test]
    fn the_low_pass_takes_the_top_off() {
        let energy = |cutoff: u16| {
            let mut sound = Sound::default();
            note(&mut sound, 2000.0, SAW);
            sound.write(SOUND + CUTOFF as u16, cutoff as u8);
            sound.write(SOUND + CUTOFF as u16 + 1, (cutoff >> 8) as u8);
            sound.write(SOUND + FILTER as u16, 0x01); // voice 0 filtered
            sound.write(SOUND + MODE as u16, LOW_PASS | 0x0F);
            sound.samples(4800).iter().map(|&s| (s as f64).powi(2)).sum::<f64>()
        };
        assert!(energy(20) < energy(2047) / 10.0);
    }
}
