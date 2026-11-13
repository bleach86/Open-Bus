//! The TIA, the 2600's picture, sound and fire buttons, a color clock at a
//! time, three each CPU cycle, as Stella has it.
//!
//! A line is 228 color clocks: 68 of horizontal blank, then 160 of picture.
//! The picture's made as the beam draws it, of the objects (objects.rs) and
//! the background; nothing's kept but their registers, so a game changes
//! them as the lines go by.
//!
//! A write's taken at once, or some color clocks later, as each register
//! has it on the chip: the playfield's 2 (PF0, PF1, PF2), the players'
//! graphics 1 (and the other player's, or the ball's, old ones swapped in a
//! clock later), the HMxx registers and HMCLR 2, HMOVE 6; REFPx, ENAMx,
//! ENABL and VBLANK 1. The rest at once: colours, NUSIZx, CTRLPF, VDELxx,
//! RESxx, RESMPx, CXCLR, VSYNC, RSYNC, and the sound's.
//!
//! RESxx sets an object's counter to where the beam is (157, or in the
//! blank 159, or 158 in its last clocks). HMOVE starts its movement clocks,
//! and lengthens the blank by 8 clocks, the left of the line black, the
//! "comb" some games show. WSYNC holds the CPU's RDY till the next line;
//! the machine sees to that.
//!
//! Its reads: the collision latches (bits 7 and 6), cleared by CXCLR, not
//! latched during VBLANK; INPT0 to INPT3, the paddles', 0 with none; INPT4
//! and INPT5, the fire buttons, 0 while pressed, or with VBLANK's bit 6,
//! latched low once they've been. Its other bits aren't driven: the data
//! bus's, as it last was.
//!
//! VBLANK's bit 1 blanks the picture, black; its bit 7 grounds the paddles'
//! capacitors, and once it's cleared they charge through each paddle's pot,
//! 1.8 kΩ to 1 MΩ, INPT0 to INPT3 set as one's charged past the comparator's
//! trip point: so how long it takes, a game counting lines, says how far
//! the paddle's turned. As Stella has it, a capacitor of 68 nF from 5 V,
//! the trip point where a pot all the way round takes 379 lines. A keypad's
//! column on one charges it through the keypad's own resistor, or straight
//! from 5 V, or grounds it, through the input's own 1.8 kΩ.

use super::frame::Frame;
use super::objects::{
    BALL, Ball, MISSILE_0, MISSILE_1, Missile, PLAYER_0, PLAYER_1, PLAYFIELD, Player, Playfield,
    resx_counter,
};
use super::sound::Sound;
use crate::machines::region::Region;
use serde::{Deserialize, Serialize};

/// The line's color clocks; the blank's; the picture's.
pub const LINE: u8 = 228;
const BLANK: u8 = 68;
pub const WIDTH: usize = 160;

const VSYNC: u8 = 0x00;
const VBLANK: u8 = 0x01;
const WSYNC: u8 = 0x02;
const RSYNC: u8 = 0x03;
const NUSIZ0: u8 = 0x04;
const NUSIZ1: u8 = 0x05;
const COLUP0: u8 = 0x06;
const COLUP1: u8 = 0x07;
const COLUPF: u8 = 0x08;
const COLUBK: u8 = 0x09;
const CTRLPF: u8 = 0x0A;
const REFP0: u8 = 0x0B;
const REFP1: u8 = 0x0C;
const PF0: u8 = 0x0D;
const PF1: u8 = 0x0E;
const PF2: u8 = 0x0F;
const RESP0: u8 = 0x10;
const RESP1: u8 = 0x11;
const RESM0: u8 = 0x12;
const RESM1: u8 = 0x13;
const RESBL: u8 = 0x14;
const AUDC0: u8 = 0x15;
const AUDC1: u8 = 0x16;
const AUDF0: u8 = 0x17;
const AUDF1: u8 = 0x18;
const AUDV0: u8 = 0x19;
const AUDV1: u8 = 0x1A;
const GRP0: u8 = 0x1B;
const GRP1: u8 = 0x1C;
const ENAM0: u8 = 0x1D;
const ENAM1: u8 = 0x1E;
const ENABL: u8 = 0x1F;
const HMP0: u8 = 0x20;
const HMP1: u8 = 0x21;
const HMM0: u8 = 0x22;
const HMM1: u8 = 0x23;
const HMBL: u8 = 0x24;
const VDELP0: u8 = 0x25;
const VDELP1: u8 = 0x26;
const VDELBL: u8 = 0x27;
const RESMP0: u8 = 0x28;
const RESMP1: u8 = 0x29;
const HMOVE: u8 = 0x2A;
const HMCLR: u8 = 0x2B;
const CXCLR: u8 = 0x2C;
/// Not registers: a player's or the ball's old graphics swapped in, a
/// clock after the other player's GRP is written.
const SHUFFLE_P0: u8 = 0xF0;
const SHUFFLE_P1: u8 = 0xF1;
const SHUFFLE_BL: u8 = 0xF2;

/// What's drawn over what: the playfield and ball over the players; the
/// score mode's; or the players over them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
enum Priority {
    Playfield,
    Score,
    #[default]
    Normal,
}

/// Writes to come, each in the slot of the clock it's taken on.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Queue {
    slots: [Vec<(u8, u8)>; 8],
    index: usize,
}

impl Queue {
    fn push(&mut self, register: u8, value: u8, delay: usize) {
        self.slots[(self.index + delay) % 8].push((register, value));
    }

    /// This clock's writes, if there are any, the queue moved on a clock:
    /// most clocks have none, and are quickly done.
    #[inline(always)]
    fn take(&mut self) -> Option<Vec<(u8, u8)>> {
        let slot = &mut self.slots[self.index];
        self.index = (self.index + 1) % 8;
        (!slot.is_empty()).then(|| std::mem::take(slot))
    }
}

/// A paddle's input: its capacitor's charge, in µV; whether VBLANK's
/// grounding it; the color clock it was last worked out at; and the pot
/// it charges through, if there's a paddle; or whether a keypad's key is
/// grounding it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Pot {
    charge: u32,
    grounded: bool,
    at: u64,
    resistance: Option<u32>,
    #[serde(skip)]
    shorted: bool,
}

const SUPPLY: f64 = 5.0;
const SERIES: f64 = 1_800.0;
const CAPACITOR: f64 = 68e-9;
const GROUNDING: f64 = 50.0;
const TRIP_LINES: f64 = 379.0;

impl Pot {
    /// Its charge as it's come to by color clock `now`, of `per_second`.
    fn update(&mut self, now: u64, per_second: f64) {
        let seconds = now.saturating_sub(self.at) as f64 / per_second;
        let charge = self.charge as f64 / 1e6;
        let charge = if self.grounded {
            charge * (-seconds / GROUNDING / CAPACITOR).exp()
        } else if self.shorted {
            charge * (-seconds / SERIES / CAPACITOR).exp()
        } else if let Some(resistance) = self.resistance {
            let decay = (-seconds / (resistance as f64 + SERIES) / CAPACITOR).exp();
            SUPPLY * (1.0 - (1.0 - charge / SUPPLY) * decay)
        } else {
            charge
        };
        self.charge = (charge * 1e6) as u32;
        self.at = now;
    }
}

/// A fire button's input: with VBLANK's bit 6, latched low.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Latch {
    latching: bool,
    value: u8,
}

impl Latch {
    fn vblank(&mut self, value: u8) {
        self.latching = value & 0x40 != 0;
        if !self.latching {
            self.value = 0x80;
        }
    }

    fn read(&mut self, pressed: bool) -> u8 {
        let value = if pressed { 0 } else { 0x80 };
        if self.latching {
            self.value &= value;
            return self.value;
        }
        value
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tia {
    /// The line's color clock; RSYNC's shift of the picture against it.
    pub hctr: u8,
    hctr_delta: i32,
    blank: bool,
    extended_blank: bool,
    moving: bool,
    movement_clock: u8,
    priority: Priority,
    collisions: u16,
    queue: Queue,
    background: u8,
    playfield: Playfield,
    players: [Player; 2],
    missiles: [Missile; 2],
    ball: Ball,
    pub frame: Frame,
    pub sound: Sound,
    latches: [Latch; 2],
    /// The color clocks since power-on, and a second's of them, for the
    /// paddles' capacitors; and those.
    clocks: u64,
    per_second: u32,
    pots: [Pot; 4],
    /// The fire buttons, pressed or not, as the host last gave them.
    #[serde(skip)]
    pub fire: [bool; 2],
    /// The frame being drawn, and the last one done, a colour register's
    /// value each dot; and whether that's new.
    #[serde(skip)]
    back: Vec<u8>,
    #[serde(skip)]
    pub picture: Vec<u8>,
    #[serde(skip)]
    pub frame_ready: bool,
}

impl Tia {
    pub fn new(region: Region) -> Tia {
        let frame = Frame::new(region);
        let size = WIDTH * frame.height as usize;
        Tia {
            hctr: 0,
            hctr_delta: 0,
            blank: true,
            extended_blank: false,
            moving: false,
            movement_clock: 0,
            priority: Priority::Normal,
            collisions: 0,
            queue: Queue::default(),
            background: 0,
            playfield: Playfield::new(),
            players: [Player::new(PLAYER_0), Player::new(PLAYER_1)],
            missiles: [Missile::new(MISSILE_0), Missile::new(MISSILE_1)],
            ball: Ball::new(BALL),
            frame,
            sound: Sound::default(),
            latches: [Latch::default(), Latch::default()],
            clocks: 0,
            per_second: match region {
                Region::Ntsc => 60 * 228 * 262,
                Region::Pal => 50 * 228 * 312,
            },
            pots: Default::default(),
            fire: [false; 2],
            back: vec![0; size],
            picture: vec![0; size],
            frame_ready: false,
        }
    }

    /// A register read, $00 to $0F, `bus` what's on the data bus.
    pub fn read(&mut self, addr: u16, bus: u8) -> u8 {
        let pair = |a: u16, b: u16, bit: u8| {
            if self.collisions & a & b != 0 { bit } else { 0 }
        };
        let value = match addr & 0x0F {
            0x0 => pair(MISSILE_0, PLAYER_1, 0x80) | pair(MISSILE_0, PLAYER_0, 0x40),
            0x1 => pair(MISSILE_1, PLAYER_0, 0x80) | pair(MISSILE_1, PLAYER_1, 0x40),
            0x2 => pair(PLAYER_0, PLAYFIELD, 0x80) | pair(PLAYER_0, BALL, 0x40),
            0x3 => pair(PLAYER_1, PLAYFIELD, 0x80) | pair(PLAYER_1, BALL, 0x40),
            0x4 => pair(MISSILE_0, PLAYFIELD, 0x80) | pair(MISSILE_0, BALL, 0x40),
            0x5 => pair(MISSILE_1, PLAYFIELD, 0x80) | pair(MISSILE_1, BALL, 0x40),
            0x6 => pair(BALL, PLAYFIELD, 0x80),
            0x7 => pair(PLAYER_0, PLAYER_1, 0x80) | pair(MISSILE_0, MISSILE_1, 0x40),
            pot @ 0x8..=0xB => {
                let per_second = self.per_second as f64;
                let pot = &mut self.pots[pot as usize - 8];
                pot.update(self.clocks, per_second);
                let trip = SUPPLY
                    * (1.0 - (-TRIP_LINES * 228.0 / per_second / (1e6 + SERIES) / CAPACITOR).exp());
                if !pot.grounded && pot.charge as f64 / 1e6 > trip {
                    0x80
                } else {
                    0
                }
            }
            0xC => self.latches[0].read(self.fire[0]),
            0xD => self.latches[1].read(self.fire[1]),
            _ => 0,
        };
        value | bus & 0x3F
    }

    /// A register written, $00 to $3F: true if it's WSYNC.
    pub fn write(&mut self, addr: u16, value: u8) -> bool {
        let register = addr as u8 & 0x3F;
        let blank = self.blank;
        let counter = resx_counter(blank, self.hctr);
        match register {
            WSYNC => return true,
            RSYNC => self.rsync(),
            VSYNC => {
                if self.frame.set_vsync(value & 0x02 != 0) {
                    self.frame_complete();
                }
            }
            VBLANK => {
                self.latches
                    .iter_mut()
                    .for_each(|latch| latch.vblank(value));
                let (now, per_second) = (self.clocks, self.per_second as f64);
                for pot in &mut self.pots {
                    pot.update(now, per_second);
                    pot.grounded = value & 0x80 != 0;
                }
                self.queue.push(VBLANK, value, 1);
            }
            AUDV0 => self.sound.channels[0].audv(value),
            AUDV1 => self.sound.channels[1].audv(value),
            AUDF0 => self.sound.channels[0].audf(value),
            AUDF1 => self.sound.channels[1].audf(value),
            AUDC0 => self.sound.channels[0].audc(value),
            AUDC1 => self.sound.channels[1].audc(value),
            HMOVE => self.queue.push(HMOVE, value, 6),
            COLUBK => self.background = value & 0xFE,
            COLUP0 | COLUP1 => {
                let n = (register - COLUP0) as usize;
                let color = value & 0xFE;
                if n == 0 {
                    self.playfield.set_color_p0(color);
                } else {
                    self.playfield.set_color_p1(color);
                }
                self.missiles[n].color = color;
                self.players[n].color = color;
            }
            CTRLPF => {
                self.priority = if value & 0x04 != 0 {
                    Priority::Playfield
                } else if value & 0x02 != 0 {
                    Priority::Score
                } else {
                    Priority::Normal
                };
                self.playfield.ctrlpf(value);
                self.ball.ctrlpf(value);
            }
            COLUPF => {
                self.playfield.set_color(value & 0xFE);
                self.ball.color = value & 0xFE;
            }
            PF0 | PF1 | PF2 => self.queue.push(register, value, 2),
            ENAM0 | ENAM1 | ENABL | REFP0 | REFP1 => self.queue.push(register, value, 1),
            HMP0 | HMP1 | HMM0 | HMM1 | HMBL | HMCLR => self.queue.push(register, value, 2),
            RESM0 | RESM1 => self.missiles[(register - RESM0) as usize].resm(counter, blank),
            RESMP0 | RESMP1 => self.missiles[(register - RESMP0) as usize].resmp(value),
            NUSIZ0 | NUSIZ1 => {
                let n = (register - NUSIZ0) as usize;
                self.missiles[n].nusiz(value);
                self.players[n].nusiz(value, blank);
            }
            GRP0 => {
                self.queue.push(GRP0, value, 1);
                self.queue.push(SHUFFLE_P1, 0, 1);
            }
            GRP1 => {
                self.queue.push(GRP1, value, 1);
                self.queue.push(SHUFFLE_P0, 0, 1);
                self.queue.push(SHUFFLE_BL, 0, 1);
            }
            RESP0 | RESP1 => self.players[(register - RESP0) as usize].resp(counter),
            VDELP0 | VDELP1 => self.players[(register - VDELP0) as usize].vdelp(value),
            RESBL => self.ball.resbl(counter),
            VDELBL => self.ball.vdelbl(value),
            CXCLR => self.collisions = 0,
            _ => {}
        }
        false
    }

    /// A write that was to come, now.
    fn delayed_write(&mut self, register: u8, value: u8) {
        match register {
            VBLANK => self.frame.set_vblank(value & 0x02 != 0),
            HMOVE => {
                self.movement_clock = 0;
                self.moving = true;
                if !self.extended_blank {
                    self.clear_comb();
                    self.extended_blank = true;
                }
                self.missiles.iter_mut().for_each(Missile::start_movement);
                self.players.iter_mut().for_each(Player::start_movement);
                self.ball.start_movement();
            }
            PF0 => self.playfield.pf0(value),
            PF1 => self.playfield.pf1(value),
            PF2 => self.playfield.pf2(value),
            HMM0 => self.missiles[0].hmm(value),
            HMM1 => self.missiles[1].hmm(value),
            HMCLR => {
                self.missiles.iter_mut().for_each(|missile| missile.hmm(0));
                self.players.iter_mut().for_each(|player| player.hmp(0));
                self.ball.hmbl(0);
            }
            GRP0 => self.players[0].grp(value),
            GRP1 => self.players[1].grp(value),
            SHUFFLE_P0 => self.players[0].shuffle(),
            SHUFFLE_P1 => self.players[1].shuffle(),
            SHUFFLE_BL => self.ball.shuffle(),
            HMP0 => self.players[0].hmp(value),
            HMP1 => self.players[1].hmp(value),
            HMBL => self.ball.hmbl(value),
            REFP0 => self.players[0].refp(value),
            REFP1 => self.players[1].refp(value),
            ENABL => self.ball.enabl(value),
            ENAM0 => self.missiles[0].enam(value),
            ENAM1 => self.missiles[1].enam(value),
            _ => {}
        }
    }

    /// A color clock: the writes due on it, movement, the objects, the
    /// collisions, and the sound.
    pub fn clock(&mut self) {
        self.clocks += 1;
        if let Some(due) = self.queue.take() {
            for (register, value) in due {
                self.delayed_write(register, value);
            }
        }
        self.tick_movement();
        if self.blank {
            self.tick_blank();
        } else {
            self.tick_picture();
        }
        if !self.frame.vblank {
            self.collisions |= self.players[0].collision
                & self.players[1].collision
                & self.missiles[0].collision
                & self.missiles[1].collision
                & self.ball.collision
                & self.playfield.collision;
        }
        self.hctr += 1;
        if self.hctr >= LINE {
            self.next_line();
        }
        self.sound.tick();
    }

    /// HMOVE's movement clocks, every 4th color clock, as many as each
    /// object's to move.
    fn tick_movement(&mut self) {
        if !self.moving || self.hctr & 3 != 0 {
            return;
        }
        let (blank, hctr) = (self.blank, self.hctr);
        let clock = if self.movement_clock > 15 {
            0
        } else {
            self.movement_clock
        };
        for missile in &mut self.missiles {
            missile.movement_tick(clock, hctr, blank);
        }
        for player in &mut self.players {
            player.movement_tick(clock, blank);
        }
        self.ball.movement_tick(clock, blank);
        self.moving = self.missiles.iter().any(|missile| missile.moving)
            || self.players.iter().any(|player| player.moving)
            || self.ball.moving;
        self.movement_clock = self.movement_clock.saturating_add(1);
    }

    /// A clock of the blank: the objects latch what they draw, the picture
    /// starting at its end; with HMOVE's 8 more, the playfield ticking
    /// through them.
    fn tick_blank(&mut self) {
        self.missiles.iter_mut().for_each(Missile::tick_in_blank);
        self.players.iter_mut().for_each(Player::tick_in_blank);
        self.ball.tick_in_blank();
        match self.hctr {
            0 => self.extended_blank = false,
            67 if !self.extended_blank => self.blank = false,
            75 if self.extended_blank => self.blank = false,
            _ => {}
        }
        let at = self.hctr as i32 - self.hctr_delta;
        if self.extended_blank && at > BLANK as i32 - 1 {
            self.playfield.tick((at - BLANK as i32) as u32);
        }
    }

    fn tick_picture(&mut self) {
        let x = (self.hctr as i32 - BLANK as i32 - self.hctr_delta) as u32;
        self.playfield.tick(x);
        self.players.iter_mut().for_each(Player::tick);
        let hctr = self.hctr;
        let [p0, p1] = &self.players;
        self.missiles[0].tick_with(hctr, p0);
        self.missiles[1].tick_with(hctr, p1);
        self.ball.tick(true);
        if self.frame.picture() {
            self.draw(x);
        }
    }

    /// Dot `x` of the picture: its colour, of what's drawn there over what.
    fn draw(&mut self, x: u32) {
        if x >= WIDTH as u32 {
            return;
        }
        let [p0, p1] = &self.players;
        let [m0, m1] = &self.missiles;
        let (pf, bl) = (&self.playfield, &self.ball);
        let drawn =
            p0.is_on() || p1.is_on() || m0.is_on() || m1.is_on() || pf.is_on() || bl.is_on();
        let color = if self.frame.vblank {
            0
        } else if !drawn {
            self.background
        } else {
            match self.priority {
                Priority::Playfield => [
                    (pf.is_on(), pf.color()),
                    (bl.is_on(), bl.color),
                    (p0.is_on(), p0.color),
                    (m0.is_on(), m0.color),
                    (p1.is_on(), p1.color),
                    (m1.is_on(), m1.color),
                ],
                Priority::Score => [
                    (p0.is_on(), p0.color),
                    (m0.is_on(), m0.color),
                    (pf.is_on(), pf.color()),
                    (p1.is_on(), p1.color),
                    (m1.is_on(), m1.color),
                    (bl.is_on(), bl.color),
                ],
                Priority::Normal => [
                    (p0.is_on(), p0.color),
                    (m0.is_on(), m0.color),
                    (p1.is_on(), p1.color),
                    (m1.is_on(), m1.color),
                    (pf.is_on(), pf.color()),
                    (bl.is_on(), bl.color),
                ],
            }
            .into_iter()
            .find_map(|(on, color)| on.then_some(color))
            .unwrap_or(self.background)
        };
        let at = self.frame.y as usize * WIDTH + x as usize;
        if let Some(dot) = self.back.get_mut(at) {
            *dot = color;
        }
    }

    /// The line's left 8 dots black, for HMOVE in the blank.
    fn clear_comb(&mut self) {
        if self.frame.picture() && self.blank {
            let at = self.frame.y as usize * WIDTH;
            if let Some(dots) = self.back.get_mut(at..at + 8) {
                dots.fill(0);
            }
        }
    }

    /// RSYNC: the line's clock put back to 3 before its end, the rest of
    /// the line's picture black.
    fn rsync(&mut self) {
        let x = self.hctr.saturating_sub(BLANK) as usize;
        self.hctr_delta = LINE as i32 - 3 - self.hctr as i32;
        if self.frame.picture() {
            let at = self.frame.y as usize * WIDTH;
            if let Some(dots) = self.back.get_mut(at + x.min(WIDTH)..at + WIDTH) {
                dots.fill(0);
            }
        }
        self.hctr = LINE - 3;
    }

    fn next_line(&mut self) {
        self.hctr = 0;
        self.blank = true;
        self.hctr_delta = 0;
        if self.frame.next_line() {
            self.frame_complete();
        }
        self.missiles.iter_mut().for_each(Missile::next_line);
        self.players.iter_mut().for_each(Player::next_line);
        self.ball.next_line();
        self.playfield.next_line();
    }

    /// The frame done: its lines not drawn black, and it's shown.
    fn frame_complete(&mut self) {
        let missing = self.frame.missing_lines() as usize;
        if missing > 0 {
            let at = self.frame.y as usize * WIDTH;
            let end = (at + missing * WIDTH).min(self.back.len());
            if at < end {
                self.back[at..end].fill(0);
            }
        }
        self.picture.copy_from_slice(&self.back);
        self.frame_ready = true;
    }

    /// Paddle `n`'s pot, INPT0 to INPT3's, `resistance` ohms, or none
    /// there.
    pub fn paddle(&mut self, n: usize, resistance: Option<u32>) {
        self.wire(n, resistance, false);
    }

    /// Input `n`, INPT0 to INPT3, grounded by a keypad's key, `shorted`,
    /// or else charged through `resistance` ohms from 5 V.
    pub fn keypad_column(&mut self, n: usize, shorted: bool, resistance: u32) {
        self.wire(n, Some(resistance), shorted);
    }

    fn wire(&mut self, n: usize, resistance: Option<u32>, shorted: bool) {
        let pot = &mut self.pots[n];
        if (pot.resistance, pot.shorted) != (resistance, shorted) {
            pot.update(self.clocks, self.per_second as f64);
            (pot.resistance, pot.shorted) = (resistance, shorted);
        }
    }

    /// The frame buffers, as a state loaded into `running` leaves them.
    pub fn keep(&mut self, running: &mut Tia) {
        self.back = std::mem::take(&mut running.back);
        self.picture = std::mem::take(&mut running.picture);
        self.fire = running.fire;
    }
}
