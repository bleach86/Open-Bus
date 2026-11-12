//! Where a frame is, as a TV finds it, as Stella's frame manager has it: the
//! 2600 has no frame of its own, only the lines a game makes, and its VSYNC,
//! held for some lines to say a frame's starting. A pulse of 2 lines or more
//! is a TV's VSYNC; from its end, a count of lines (23 on NTSC, 32 on PAL)
//! to the picture's top, then 228 lines of it (274 on PAL). A game that
//! never sends one (or too long a one) has its frame ended anyway, after
//! more lines than any TV would wait for.

use crate::machines::region::Region;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
enum State {
    #[default]
    WaitingForVsync,
    InVsync,
    WaitingForPicture,
    Picture,
}

/// The lines a TV waits for a VSYNC before it gives up.
const MOST_VSYNC_LINES: u32 = 50;
/// PAL's lines a frame, and twice them: too long a frame for any TV.
const PAL_LINES: u32 = 312;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    state: State,
    lines_in_state: u32,
    frames: u64,
    vsync_lines: u32,
    /// The picture's line being drawn.
    pub y: u32,
    last_y: u32,
    vsync_pending: bool,
    vsync_pending_lines: u32,
    vsync: bool,
    pub vblank: bool,
    lines: u32,
    /// The lines of the frame before; and between the last two VSYNCs, and
    /// since the last, however short, as Stella's layout detector counts.
    pub lines_last_frame: u32,
    pub lines_between_vsyncs: u32,
    lines_since_vsync: u32,
    frame_lines: u32,
    start: u32,
    pub height: u32,
}

impl Frame {
    pub fn new(region: Region) -> Frame {
        let (frame_lines, start, height) = match region {
            Region::Ntsc => (262, 23, 228),
            Region::Pal => (312, 32, 274),
        };
        Frame {
            frame_lines,
            start,
            height,
            ..Frame::default()
        }
    }

    pub fn picture(&self) -> bool {
        self.state == State::Picture
    }

    pub fn set_vblank(&mut self, vblank: bool) {
        self.vblank = vblank;
    }

    /// VSYNC set or cleared: its start pending till it's lasted 2 lines;
    /// its end, if it did, the frame's.
    pub fn set_vsync(&mut self, vsync: bool) -> bool {
        if vsync == self.vsync {
            return false;
        }
        self.vsync = vsync;
        if vsync {
            self.lines_between_vsyncs = self.lines_since_vsync;
            self.lines_since_vsync = 0;
            if self.state != State::InVsync && !self.vsync_pending {
                (self.vsync_pending, self.vsync_pending_lines) = (true, 0);
            }
            return false;
        }
        self.vsync_pending = false;
        if self.state != State::InVsync {
            return false;
        }
        self.set_state(State::WaitingForPicture)
    }

    /// The lines in the picture this frame didn't get to.
    pub fn missing_lines(&self) -> u32 {
        if self.last_y == self.start + self.y {
            0
        } else {
            self.height.saturating_sub(self.y)
        }
    }

    /// A line over: true if that ends a frame.
    pub fn next_line(&mut self) -> bool {
        self.lines += 1;
        self.lines_since_vsync += 1;
        let before = self.state;
        self.lines_in_state += 1;
        let mut ended = false;
        if self.vsync_pending {
            self.vsync_pending_lines += 1;
            if self.vsync_pending_lines >= 2 {
                self.vsync_pending = false;
                ended |= self.set_state(State::InVsync);
            }
        }
        match self.state {
            State::WaitingForVsync => {
                let late = self.lines > self.frame_lines - 3 || self.frames == 0;
                if late && (self.vblank || self.lines > PAL_LINES * 2) {
                    self.vsync_lines += 1;
                }
                if self.vsync_lines > MOST_VSYNC_LINES {
                    ended |= self.set_state(State::WaitingForPicture);
                }
            }
            State::InVsync => {
                if self.vblank || self.lines > PAL_LINES * 2 {
                    self.vsync_lines += 1;
                }
                if self.vsync_lines > MOST_VSYNC_LINES {
                    ended |= self.set_state(State::WaitingForPicture);
                }
            }
            State::WaitingForPicture => {
                if self.lines_in_state >= self.start {
                    ended |= self.set_state(State::Picture);
                }
            }
            State::Picture => {
                if self.lines_in_state >= self.height {
                    self.last_y = self.start + self.y;
                    ended |= self.set_state(State::WaitingForVsync);
                }
            }
        }
        if self.state == State::Picture && before == State::Picture {
            self.y += 1;
        }
        ended
    }

    /// The state changed: true if that ends a frame.
    fn set_state(&mut self, state: State) -> bool {
        if self.state == state {
            return false;
        }
        self.state = state;
        self.lines_in_state = 0;
        match state {
            State::WaitingForPicture => {
                self.lines_last_frame = self.lines;
                self.lines = 0;
                self.frames += 1;
                self.vsync_lines = 0;
                true
            }
            State::Picture => {
                (self.vsync_lines, self.y) = (0, 0);
                false
            }
            _ => false,
        }
    }
}
