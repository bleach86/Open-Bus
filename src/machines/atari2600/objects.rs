//! The TIA's objects, each a color clock at a time, as Stella has them (its
//! TIA core, Christian Speckner's): the two players, the two missiles, the
//! ball, and the playfield.
//!
//! Each of the movable ones has a position counter, 0 to 159, that counts
//! the visible line's color clocks; at certain counts (156, the main copy's,
//! and those of the copies NUSIZ asks for) it starts drawing, after a few
//! clocks more. RESxx sets the counter, and HMOVE's movement clocks, one
//! every 4 color clocks during the blank at a line's start, give it extra
//! counts, up to 15, as many as its HMxx says more than the 8 it'd be.
//!
//! What each draws is a collision mask: all ones if it's drawing this
//! clock, or else all but the bits of each pair of objects it's in, and of
//! the visible bit. All six ANDed together, a pair's bit is set where both
//! of its objects draw; ORed into the TIA's latches, the collisions.

use serde::{Deserialize, Serialize};

/// Each object's bits in the collision mask: each pair of the six objects
/// one bit, in each of the pair's masks.
pub const PLAYER_0: u16 = 0b0111_1100_0000_0000;
pub const PLAYER_1: u16 = 0b0100_0011_1100_0000;
pub const MISSILE_0: u16 = 0b0010_0010_0011_1000;
pub const MISSILE_1: u16 = 0b0001_0001_0010_0110;
pub const BALL: u16 = 0b0000_1000_1001_0101;
pub const PLAYFIELD: u16 = 0b0000_0100_0100_1011;
/// The bit that says an object's drawing.
const VISIBLE: u16 = 0x8000;
/// What an object drawing gives.
const DRAWING: u16 = 0xFFFF;

/// The mask of the object of `bits` when it isn't drawing.
const fn silent(bits: u16) -> u16 {
    !bits & !VISIBLE
}

/// The visible line's color clocks.
const PIXELS: u8 = 160;

/// Which copy a player or missile starts drawing at its counter `counter`,
/// with NUSIZ's low three bits `nusiz`, if it starts one: 1, the main, at
/// 156; 2 and 3, the others, 16, 32 or 64 clocks after the line's start.
#[inline(always)]
fn decode(nusiz: u8, counter: u8) -> u8 {
    match (nusiz, counter) {
        (_, 156) => 1,
        (1 | 3, 12) | (2 | 6, 28) | (4, 60) => 2,
        (3, 28) | (6, 60) => 3,
        _ => 0,
    }
}

/// Which of the decode tables NUSIZ's low bits `nusiz` have: 5 and 7, a
/// double and a quadruple width, have one copy, as 0 has.
fn table(nusiz: u8) -> u8 {
    match nusiz {
        5 | 7 => 0,
        nusiz => nusiz,
    }
}

/// The counter `back` counts before `counter`, round the line.
fn before(counter: u8, back: u8) -> u8 {
    ((counter as u16 + PIXELS as u16 - back as u16) % PIXELS as u16) as u8
}

/// How far past 157, where a strobe in the picture sets it, a counter is.
fn past_157(counter: u8) -> i8 {
    (counter as i16 - 157) as i8
}

/// The counter a RESxx strobe sets, as where in the line it lands: in the
/// blank's last clocks, earlier in the blank, or during the picture.
pub fn resx_counter(blank: bool, hctr: u8) -> u8 {
    match (blank, hctr >= 73) {
        (true, true) => 158,
        (true, false) => 159,
        (false, _) => 157,
    }
}

/// HMxx's movement: the clocks it's moved for, its top nibble, flipped.
fn movement(value: u8) -> u8 {
    (value >> 4) ^ 0x08
}

/// A player: its position counter and movement; whether it's drawing, a
/// copy, how far into it (its render counter, from -5, and the bit it's at),
/// and its width (1, 2 or 4 clocks a bit) and a change of that to come; its
/// NUSIZ; its graphics, old and new (VDELP choosing), reflected or not.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    silent: u16,
    pub collision: u16,
    pub color: u8,
    movement: u8,
    counter: u8,
    pub moving: bool,
    rendering: bool,
    render_counter: i8,
    trip_point: i8,
    copy: i8,
    divider: u8,
    divider_pending: u8,
    sample: u8,
    divider_change: i8,
    nusiz: u8,
    pattern_old: u8,
    pattern_new: u8,
    pattern: u8,
    reflected: bool,
    delaying: bool,
}

const PLAYER_START: i8 = -5;

impl Player {
    /// The player whose collision bits are `bits`.
    pub fn new(bits: u16) -> Player {
        let mut player = Player {
            silent: silent(bits),
            collision: silent(bits),
            copy: 1,
            divider_change: -1,
            ..Player::default()
        };
        player.set_divider(1);
        player
    }

    pub fn grp(&mut self, pattern: u8) {
        self.pattern_new = pattern;
        if !self.delaying {
            self.update_pattern();
        }
    }

    pub fn hmp(&mut self, value: u8) {
        self.movement = movement(value);
    }

    /// NUSIZ written, `blank` if in the line's blank: its copies, and its
    /// width, which changes at once or a little later as far as it's into
    /// drawing one.
    pub fn nusiz(&mut self, value: u8, blank: bool) {
        let old = self.nusiz;
        self.nusiz = value & 7;
        self.divider_pending = match self.nusiz {
            5 => 2,
            7 => 4,
            _ => 1,
        };
        // A copy decoded in the same clock.
        let previous = before(self.counter, 1);
        let decoded = decode(self.nusiz, previous);
        if !self.rendering && decoded != 0 {
            self.rendering = true;
            self.sample = 0;
            self.render_counter = PLAYER_START;
            self.copy = decoded as i8;
        }
        if table(self.nusiz) != table(old)
            && self.rendering
            && self.render_counter - PLAYER_START < 2
        {
            let at = (self.counter as i16 - self.render_counter as i16
                + PLAYER_START as i16
                + PIXELS as i16
                - 1)
            .rem_euclid(PIXELS as i16) as u8;
            if decode(self.nusiz, at) == 0 {
                self.rendering = false;
            }
        }
        if self.divider_pending == self.divider {
            return;
        }
        if !self.rendering {
            self.set_divider(self.divider_pending);
            return;
        }
        let delta = self.render_counter - PLAYER_START;
        match (self.divider, self.divider_pending) {
            (1, 2) | (1, 4) => {
                if blank {
                    if delta < 4 {
                        self.set_divider(self.divider_pending);
                    } else {
                        self.divider_change = if delta < 5 { 1 } else { 0 };
                    }
                } else if delta < 3 {
                    self.set_divider(self.divider_pending);
                } else {
                    self.divider_change = 1;
                }
            }
            (2, 1) | (4, 1) => {
                if delta < if blank { 4 } else { 3 } {
                    self.set_divider(self.divider_pending);
                } else if delta < if blank { 6 } else { 5 } {
                    self.set_divider(self.divider_pending);
                    self.render_counter -= 1;
                } else {
                    self.divider_change = if blank { 0 } else { 1 };
                }
            }
            _ => {
                let divider = self.divider as i8;
                if self.render_counter < 1 || (blank && self.render_counter % divider == 1) {
                    self.set_divider(self.divider_pending);
                } else {
                    self.divider_change = divider - (self.render_counter - 1) % divider;
                }
            }
        }
    }

    /// RESP: its counter set, as far as a copy being decoded allows.
    pub fn resp(&mut self, counter: u8) {
        self.counter = counter;
        if self.rendering && self.render_counter - PLAYER_START < 4 {
            self.render_counter = PLAYER_START + past_157(counter);
        }
    }

    pub fn refp(&mut self, value: u8) {
        self.reflected = value & 0x08 != 0;
        self.update_pattern();
    }

    pub fn vdelp(&mut self, value: u8) {
        self.delaying = value & 0x01 != 0;
        self.update_pattern();
    }

    /// GRP of the other player written: its new graphics the old.
    pub fn shuffle(&mut self) {
        self.pattern_old = self.pattern_new;
        if self.delaying {
            self.update_pattern();
        }
    }

    pub fn start_movement(&mut self) {
        self.moving = true;
    }

    /// A movement clock, the `clock`th of HMOVE's: a count more, if it's
    /// still moving, which only counts in the blank.
    pub fn movement_tick(&mut self, clock: u8, blank: bool) {
        if self.moving {
            if clock == self.movement {
                self.moving = false;
            } else if blank {
                self.tick();
            }
        }
    }

    /// A clock of the blank: what it draws latched, its counter not
    /// counting.
    #[inline(always)]
    pub fn tick_in_blank(&mut self) {
        self.latch();
    }

    pub fn next_line(&mut self) {
        self.latch();
    }

    #[inline(always)]
    fn latch(&mut self) {
        self.collision = if !self.rendering || self.render_counter < self.trip_point {
            self.silent
        } else if self.pattern & (1 << self.sample) != 0 {
            DRAWING
        } else {
            self.silent
        };
    }

    /// A clock of the picture: what it draws, then its counter counted, a
    /// copy started where it decodes one.
    #[inline(always)]
    pub fn tick(&mut self) {
        self.latch();
        let decoded = decode(self.nusiz, self.counter);
        if decoded != 0 {
            self.rendering = true;
            self.sample = 0;
            self.render_counter = PLAYER_START;
            self.copy = decoded as i8;
        } else if self.rendering {
            self.render_counter += 1;
            let change_at = if self.divider == 1 { 0 } else { 1 };
            if self.divider == 1 {
                if self.render_counter > 0 {
                    self.sample += 1;
                }
            } else if self.render_counter > 1
                && (self.render_counter - 1) as u8 & (self.divider - 1) == 0
            {
                self.sample += 1;
            }
            if self.render_counter >= change_at && self.divider_change >= 0 {
                if self.divider_change == 0 {
                    self.set_divider(self.divider_pending);
                }
                self.divider_change -= 1;
            }
            if self.sample > 7 {
                self.rendering = false;
            }
        }
        self.counter += 1;
        if self.counter >= PIXELS {
            self.counter = 0;
        }
    }

    #[inline(always)]
    pub fn is_on(&self) -> bool {
        self.collision & VISIBLE != 0
    }

    /// Whether it's at its main copy's fifth bit, where a missile locked to
    /// it by RESMP is moved to it.
    fn at_main_copys_fifth_bit(&self) -> bool {
        self.rendering && self.sample == 4 && self.copy == 1
    }

    /// The counter a missile locked to it is given: where it was set.
    fn resp_clock(&self) -> u8 {
        let back = match self.divider {
            1 => 5,
            2 => 8,
            _ => 12,
        };
        before(self.counter, back)
    }

    fn update_pattern(&mut self) {
        let pattern = if self.delaying {
            self.pattern_old
        } else {
            self.pattern_new
        };
        self.pattern = if self.reflected {
            pattern
        } else {
            pattern.reverse_bits()
        };
        if self.rendering && self.render_counter >= self.trip_point {
            self.latch();
        }
    }

    fn set_divider(&mut self, divider: u8) {
        self.divider = divider;
        self.trip_point = if divider == 1 { 0 } else { 1 };
    }
}

/// A missile: its counter and movement; whether it's drawing, how far in
/// (from -4), and its width, 1 to 8 clocks, and as HMOVE during drawing
/// makes it (the "starfield"); its copies; its enable, and RESMP, which
/// hides it and locks it to its player.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Missile {
    silent: u16,
    pub collision: u16,
    pub color: u8,
    nusiz: u8,
    enabled: bool,
    enam: bool,
    resmp: bool,
    movement: u8,
    counter: u8,
    pub moving: bool,
    width: u8,
    effective_width: u8,
    rendering: bool,
    visible: bool,
    render_counter: i8,
}

const MISSILE_START: i8 = -4;

impl Missile {
    pub fn new(bits: u16) -> Missile {
        Missile {
            silent: silent(bits),
            collision: silent(bits),
            width: 1,
            effective_width: 1,
            ..Missile::default()
        }
    }

    pub fn enam(&mut self, value: u8) {
        self.enam = value & 0x02 != 0;
        self.update_enabled();
    }

    pub fn hmm(&mut self, value: u8) {
        self.movement = movement(value);
    }

    /// RESM: its counter set, and as far as it was drawing, the rest of it
    /// as the width and the blank have it.
    pub fn resm(&mut self, counter: u8, blank: bool) {
        self.counter = counter;
        if !self.rendering {
            return;
        }
        if self.render_counter < 0 {
            self.render_counter = MISSILE_START + past_157(counter);
            return;
        }
        match self.width {
            8 => {
                let on = if self.render_counter >= 4 { 4 } else { 0 };
                self.render_counter = past_157(counter) + on;
            }
            4 => self.render_counter = past_157(counter),
            2 => {
                if blank {
                    self.rendering = self.render_counter > 1;
                } else if self.render_counter == 0 {
                    self.render_counter += 1;
                }
            }
            _ => {
                if blank {
                    self.rendering = self.render_counter > 0;
                }
            }
        }
    }

    pub fn resmp(&mut self, value: u8) {
        self.resmp = value & 0x02 != 0;
        self.update_enabled();
    }

    pub fn nusiz(&mut self, value: u8) {
        self.nusiz = value & 7;
        self.width = 1 << ((value >> 4) & 3);
        if self.rendering && self.render_counter >= self.width as i8 {
            self.rendering = false;
        }
    }

    pub fn start_movement(&mut self) {
        self.moving = true;
    }

    /// A movement clock, `hctr` the line's clock it's on.
    pub fn movement_tick(&mut self, clock: u8, hctr: u8, blank: bool) {
        if self.moving {
            if clock == self.movement {
                self.moving = false;
            } else if blank {
                self.tick(hctr, false);
            }
        }
    }

    #[inline(always)]
    pub fn tick_in_blank(&mut self) {
        self.visible = self.rendering && self.render_counter >= 0;
        self.latch();
    }

    pub fn next_line(&mut self) {
        self.visible = self.rendering && self.render_counter >= 0;
        self.latch();
    }

    #[inline(always)]
    fn latch(&mut self) {
        self.collision = if self.visible && self.enabled {
            DRAWING
        } else {
            self.silent
        };
    }

    /// A clock of the picture, `player` its own, which RESMP locks it to.
    #[inline(always)]
    pub fn tick_with(&mut self, hctr: u8, player: &Player) {
        self.tick(hctr, true);
        if self.resmp && player.at_main_copys_fifth_bit() {
            self.counter = player.resp_clock();
        }
    }

    /// A clock: a `regular` one, or HMOVE's in the blank.
    #[inline(always)]
    fn tick(&mut self, hctr: u8, regular: bool) {
        let starfield = self.moving && regular;
        let phase = (hctr as u16 + 1) % 4;
        self.visible = self.rendering
            && (self.render_counter >= 0
                || (starfield && self.render_counter == -1 && self.width < 4 && phase == 3));
        self.latch();
        let decoded = decode(self.nusiz, self.counter);
        if decoded != 0 && !self.resmp {
            self.rendering = true;
            self.render_counter = MISSILE_START;
        } else if self.rendering {
            if self.render_counter == -1 {
                if starfield {
                    match phase {
                        3 => {
                            self.effective_width = if self.width == 1 { 2 } else { self.width };
                            if self.width < 4 {
                                self.render_counter += 1;
                            }
                        }
                        2 => self.effective_width = 0,
                        _ => self.effective_width = self.width,
                    }
                } else {
                    self.effective_width = self.width;
                }
            }
            self.render_counter += 1;
            let width = if self.moving {
                self.effective_width
            } else {
                self.width
            };
            if self.render_counter >= width as i8 {
                self.rendering = false;
            }
        }
        self.counter += 1;
        if self.counter >= PIXELS {
            self.counter = 0;
        }
    }

    #[inline(always)]
    pub fn is_on(&self) -> bool {
        self.collision & VISIBLE != 0
    }

    fn update_enabled(&mut self) {
        self.enabled = self.enam && !self.resmp;
        self.latch();
    }
}

/// The ball: as a missile, but one copy, its width from CTRLPF, and its
/// enable delayed by VDELBL as a player's graphics are.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ball {
    silent: u16,
    pub collision: u16,
    pub color: u8,
    enabled_old: bool,
    enabled_new: bool,
    enabled: bool,
    delaying: bool,
    signal: bool,
    movement: u8,
    counter: u8,
    pub moving: bool,
    effective_width: u8,
    last_movement_tick: u8,
    width: u8,
    rendering: bool,
    render_counter: i8,
}

const BALL_START: i8 = -4;

impl Ball {
    pub fn new(bits: u16) -> Ball {
        Ball {
            silent: silent(bits),
            collision: silent(bits),
            width: 1,
            effective_width: 1,
            ..Ball::default()
        }
    }

    pub fn enabl(&mut self, value: u8) {
        self.enabled_new = value & 0x02 != 0;
        if !self.delaying {
            self.update_enabled();
        }
    }

    pub fn hmbl(&mut self, value: u8) {
        self.movement = movement(value);
    }

    pub fn resbl(&mut self, counter: u8) {
        self.counter = counter;
        self.rendering = true;
        self.render_counter = BALL_START + past_157(counter);
    }

    pub fn ctrlpf(&mut self, value: u8) {
        self.width = 1 << ((value >> 4) & 3);
    }

    pub fn vdelbl(&mut self, value: u8) {
        self.delaying = value & 0x01 != 0;
        self.update_enabled();
    }

    /// GRP1 written: its new enable the old.
    pub fn shuffle(&mut self) {
        self.enabled_old = self.enabled_new;
        if self.delaying {
            self.update_enabled();
        }
    }

    pub fn start_movement(&mut self) {
        self.moving = true;
    }

    pub fn movement_tick(&mut self, clock: u8, blank: bool) {
        self.last_movement_tick = self.counter;
        if self.moving {
            if clock == self.movement {
                self.moving = false;
            } else if blank {
                self.tick(false);
            }
        }
    }

    #[inline(always)]
    pub fn tick_in_blank(&mut self) {
        self.signal = self.rendering && self.render_counter >= 0;
        self.latch();
    }

    pub fn next_line(&mut self) {
        self.signal = self.rendering && self.render_counter >= 0;
        self.latch();
    }

    #[inline(always)]
    fn latch(&mut self) {
        self.collision = if self.signal && self.enabled {
            DRAWING
        } else {
            self.silent
        };
    }

    /// A clock: a `regular` one, or HMOVE's in the blank.
    #[inline(always)]
    pub fn tick(&mut self, regular: bool) {
        self.signal = self.rendering && self.render_counter >= 0;
        self.latch();
        let starfield = self.moving && regular;
        if self.counter == 156 {
            self.rendering = true;
            self.render_counter = BALL_START;
            let delta = before(self.counter, self.last_movement_tick) % 4;
            if starfield && delta == 3 && self.width < 4 {
                self.render_counter += 1;
            }
            self.effective_width = match delta {
                3 if self.width == 1 => 2,
                2 => 0,
                _ => self.width,
            };
        } else if self.rendering {
            self.render_counter += 1;
            let width = if starfield {
                self.effective_width
            } else {
                self.width
            };
            if self.render_counter >= width as i8 {
                self.rendering = false;
            }
        }
        self.counter += 1;
        if self.counter >= PIXELS {
            self.counter = 0;
        }
    }

    #[inline(always)]
    pub fn is_on(&self) -> bool {
        self.collision & VISIBLE != 0
    }

    fn update_enabled(&mut self) {
        self.enabled = if self.delaying {
            self.enabled_old
        } else {
            self.enabled_new
        };
        self.latch();
    }
}

/// The playfield: its 20 bits, PF0's four, PF1's eight backwards and PF2's
/// eight, a bit every 4 clocks across the line's left half, and its right
/// half again or reflected (as CTRLPF says, at the line's start and its
/// middle); its colour, or in score mode, each half its player's.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playfield {
    silent: u16,
    pub collision: u16,
    pattern: u32,
    reflected: bool,
    reflecting: bool,
    x: u8,
    color: u8,
    color_p0: u8,
    color_p1: u8,
    score: bool,
    color_left: u8,
    color_right: u8,
}

impl Playfield {
    pub fn new() -> Playfield {
        Playfield {
            silent: silent(PLAYFIELD),
            collision: 0,
            ..Playfield::default()
        }
    }

    pub fn pf0(&mut self, value: u8) {
        self.pattern = (self.pattern & 0x000F_FFF0) | (value >> 4) as u32;
    }

    pub fn pf1(&mut self, value: u8) {
        self.pattern = (self.pattern & 0x000F_F00F) | (value.reverse_bits() as u32) << 4;
    }

    pub fn pf2(&mut self, value: u8) {
        self.pattern = (self.pattern & 0x0000_0FFF) | (value as u32) << 12;
    }

    pub fn ctrlpf(&mut self, value: u8) {
        self.reflected = value & 0x01 != 0;
        self.score = value & 0x06 == 0x02;
        self.apply_colors();
    }

    pub fn set_color(&mut self, color: u8) {
        self.color = color;
        self.apply_colors();
    }

    pub fn set_color_p0(&mut self, color: u8) {
        self.color_p0 = color;
        self.apply_colors();
    }

    pub fn set_color_p1(&mut self, color: u8) {
        self.color_p1 = color;
        self.apply_colors();
    }

    fn apply_colors(&mut self) {
        (self.color_left, self.color_right) = if self.score {
            (self.color_p0, self.color_p1)
        } else {
            (self.color, self.color)
        };
    }

    pub fn next_line(&mut self) {
        self.collision = self.silent;
    }

    /// Picture clock `x`: a bit of it every 4 clocks.
    #[inline(always)]
    pub fn tick(&mut self, x: u32) {
        self.x = x as u8;
        if x == PIXELS as u32 / 2 - 1 || x == 0 {
            self.reflecting = self.reflected;
        }
        if x & 3 != 0 {
            return;
        }
        let column = x >> 2;
        let bit = if self.pattern == 0 {
            false
        } else if x < PIXELS as u32 / 2 - 1 {
            self.pattern & (1 << column) != 0
        } else if self.reflecting {
            column <= 39 && self.pattern & (1 << (39 - column)) != 0
        } else {
            (20..40).contains(&column) && self.pattern & (1 << (column - 20)) != 0
        };
        self.collision = if bit { DRAWING } else { self.silent };
    }

    #[inline(always)]
    pub fn is_on(&self) -> bool {
        self.collision & VISIBLE != 0
    }

    pub fn color(&self) -> u8 {
        if self.x < PIXELS / 2 {
            self.color_left
        } else {
            self.color_right
        }
    }
}
