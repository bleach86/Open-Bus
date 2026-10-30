//! The video chip, `$FF10`: 64K of VRAM behind an address and a data port,
//! a scrolling background of 8x8 tiles and 64 sprites, 320x240 at 60 Hz.
//! See docs/machine.md for the registers and the VRAM layout. The picture
//! itself is drawn on the GPU from a copy of VRAM (render.rs).
//!
//! The beam: a frame is 262 lines of `CYCLES_PER_LINE` CPU cycles, vblank
//! (lines 240-261) first, then the 240 lines on screen. The chip keeps each
//! line's scroll as the beam passes it, so a scroll changed partway down
//! (from a raster interrupt, say) shows from that line on. It interrupts at
//! vblank and at a chosen line, as enabled; a handler that sets the next
//! line splits the screen again further down.
//!
//! Two background planes: A (map at $8000) and B (map at $A000, in front,
//! when VCTRL bit 3 turns it on), each with its own scroll. Each sprite has
//! a priority (a byte each at $9300): in front of both planes, behind B, or
//! behind both.

use crate::render::Frame;

pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 240;

const VADDR: u16 = 0xFF10;
const VADDR_HI: u16 = 0xFF11;
const VDATA: u16 = 0xFF12;
const VCTRL: u16 = 0xFF13;
const SCROLLX: u16 = 0xFF14;
const SCROLLX_HI: u16 = 0xFF15;
const SCROLLY: u16 = 0xFF16;
const SCROLLY_HI: u16 = 0xFF17;
const BACKDROP: u16 = 0xFF18;
const VIRQ: u16 = 0xFF19;
const VSTATUS: u16 = 0xFF1A;
const RASTER: u16 = 0xFF1B;
const VLINE: u16 = 0xFF1C;
const SCROLLX2: u16 = 0xFF1D;
const SCROLLX2_HI: u16 = 0xFF1E;
const SCROLLY2: u16 = 0xFF1F;

/// VIRQ and VSTATUS bits.
const IRQ_VBLANK: u8 = 0x01;
const IRQ_RASTER: u8 = 0x02;

pub const CYCLES_PER_LINE: u64 = 509;
/// Each line's scroll for plane A, then for plane B.
const LINES_SIZE: usize = HEIGHT * 4 * 2;
const VBLANK_LINES: u64 = 22;

// Where tests look in VRAM.
#[cfg(test)]
const MAP: usize = 0x8000;
#[cfg(test)]
const MAP_COLUMNS: usize = 64;
#[cfg(test)]
const SPRITES: usize = 0x9000;

pub struct Vdp {
    vram: Box<[u8; 0x10000]>,
    addr: u16,
    ctrl: u8,
    scroll_x: u16,
    scroll_y: u16,
    scroll_bx: u16,
    scroll_by: u8,
    backdrop: u8,
    /// Something that shows changed since the last frame was drawn.
    changed: bool,
    irq_enable: u8,
    irq_status: u8,
    raster: u8,
    /// The beam: the line being drawn (0-239, or 240-261 in vblank), the
    /// frame cycle at which it moves on, and whether it has passed the
    /// raster line this frame.
    line: u16,
    next_line_at: u64,
    raster_passed: bool,
    /// Each line's scroll, X then Y, little-endian, as the beam passed it;
    /// `captured` of them so far this frame. And last frame's.
    lines: Vec<u8>,
    captured: usize,
    shown_lines: Vec<u8>,
}

impl Default for Vdp {
    fn default() -> Self {
        Vdp {
            vram: Box::new([0; 0x10000]),
            addr: 0,
            ctrl: 0,
            scroll_x: 0,
            scroll_y: 0,
            scroll_bx: 0,
            scroll_by: 0,
            backdrop: 0,
            changed: true,
            irq_enable: 0,
            irq_status: 0,
            raster: 0,
            line: 240,
            next_line_at: CYCLES_PER_LINE,
            raster_passed: false,
            lines: vec![0; LINES_SIZE],
            captured: 0,
            shown_lines: vec![0; LINES_SIZE],
        }
    }
}

impl Vdp {
    /// The reset line: registers cleared, VRAM kept.
    pub fn reset(&mut self) {
        let vram = std::mem::replace(&mut self.vram, Box::new([0; 0x10000]));
        *self = Vdp { vram, ..Vdp::default() };
    }

    pub fn read(&mut self, addr: u16) -> u8 {
        let value = self.peek(addr);
        if addr == VDATA {
            self.addr = self.addr.wrapping_add(1);
        }
        value
    }

    /// What a read gives, without the read's effect (VDATA moving on).
    pub fn peek(&self, addr: u16) -> u8 {
        match addr {
            VADDR => self.addr as u8,
            VADDR_HI => (self.addr >> 8) as u8,
            VDATA => self.vram[self.addr as usize],
            VCTRL => self.ctrl,
            SCROLLX => self.scroll_x as u8,
            SCROLLX_HI => (self.scroll_x >> 8) as u8,
            SCROLLY => self.scroll_y as u8,
            SCROLLY_HI => (self.scroll_y >> 8) as u8,
            SCROLLX2 => self.scroll_bx as u8,
            SCROLLX2_HI => (self.scroll_bx >> 8) as u8,
            SCROLLY2 => self.scroll_by,
            BACKDROP => self.backdrop,
            VIRQ => self.irq_enable,
            VSTATUS => self.irq_status,
            RASTER => self.raster,
            VLINE => self.line as u8,
            _ => 0,
        }
    }

    /// Only a write that changes what shows marks the picture changed: a
    /// program that rewrites the same sprites every frame costs no redraw.
    pub fn write(&mut self, addr: u16, value: u8) {
        let low = |word: u16| word & 0xFF00 | value as u16;
        let high = |word: u16| word & 0x00FF | (value as u16) << 8;
        let registers = (self.ctrl, self.backdrop);
        match addr {
            VADDR => self.addr = low(self.addr),
            VADDR_HI => self.addr = high(self.addr),
            VDATA => {
                let cell = &mut self.vram[self.addr as usize];
                self.changed |= *cell != value;
                *cell = value;
                self.addr = self.addr.wrapping_add(1);
            }
            VCTRL => self.ctrl = value,
            SCROLLX => self.scroll_x = low(self.scroll_x),
            SCROLLX_HI => self.scroll_x = high(self.scroll_x) & 0x1FF,
            SCROLLY => self.scroll_y = low(self.scroll_y),
            SCROLLY_HI => self.scroll_y = high(self.scroll_y) & 0xFF,
            SCROLLX2 => self.scroll_bx = low(self.scroll_bx),
            SCROLLX2_HI => self.scroll_bx = high(self.scroll_bx) & 0x1FF,
            SCROLLY2 => self.scroll_by = value,
            BACKDROP => self.backdrop = value & 0x0F,
            VIRQ => self.irq_enable = value & (IRQ_VBLANK | IRQ_RASTER),
            VSTATUS => self.irq_status &= !value, // acknowledged
            RASTER => {
                // Armed for the new line if the beam has yet to reach it
                // this frame; a line already passed interrupts next frame.
                self.raster = value;
                self.raster_passed = self.line < HEIGHT as u16 && self.line >= value as u16;
            }
            _ => {}
        }
        // A scroll shows through the lines it reaches (end_frame).
        self.changed |= registers != (self.ctrl, self.backdrop);
    }

    /// The beam at `frame_cycle` cycles into the frame: the lines it has
    /// left behind take the scroll as it stands, and the raster line, when
    /// reached, raises its interrupt. Called between instructions; a
    /// comparison, mostly.
    pub fn beam(&mut self, frame_cycle: u64) {
        if frame_cycle < self.next_line_at {
            return;
        }
        let line = frame_cycle / CYCLES_PER_LINE;
        self.next_line_at = (line + 1) * CYCLES_PER_LINE;
        self.line = if line < VBLANK_LINES { (240 + line) as u16 } else { (line - VBLANK_LINES) as u16 };
        let visible = line.saturating_sub(VBLANK_LINES) as usize;
        self.capture(visible.min(HEIGHT));
        if !self.raster_passed && line >= VBLANK_LINES && visible >= self.raster as usize {
            self.raster_passed = true;
            if (self.raster as usize) < HEIGHT {
                self.irq_status |= IRQ_RASTER;
            }
        }
    }

    /// Lines up to `until` take the scrolls as they stand: plane A's in the
    /// first half of `lines`, B's in the second, 4 bytes a line each.
    fn capture(&mut self, until: usize) {
        let a = [self.scroll_x.to_le_bytes(), self.scroll_y.to_le_bytes()].concat();
        let b = [self.scroll_bx.to_le_bytes(), [self.scroll_by, 0]].concat();
        while self.captured < until {
            let at = self.captured * 4;
            self.lines[at..at + 4].copy_from_slice(&a);
            self.lines[HEIGHT * 4 + at..HEIGHT * 4 + at + 4].copy_from_slice(&b);
            self.captured += 1;
        }
    }

    /// The cycle within the frame of the next interrupt this frame, if one
    /// is enabled: for a CPU waiting for the frame to end.
    pub fn next_interrupt(&self, frame_cycle: u64) -> Option<u64> {
        let at = (VBLANK_LINES + self.raster as u64) * CYCLES_PER_LINE;
        (self.irq_enable & IRQ_RASTER != 0 && !self.raster_passed && (self.raster as usize) < HEIGHT && at > frame_cycle)
            .then_some(at)
    }

    /// The interrupt line: up while an enabled interrupt is unacknowledged.
    pub fn irq(&self) -> bool {
        self.irq_status & self.irq_enable != 0
    }

    /// The frame drawn: the lines the beam has yet to pass take the scroll
    /// as it stands, and the picture changed if any line's scroll did.
    pub fn end_frame(&mut self) {
        self.capture(HEIGHT);
        if self.lines != self.shown_lines {
            self.shown_lines.copy_from_slice(&self.lines);
            self.changed = true;
        }
    }

    /// Vblank: the next frame starts.
    pub fn start_frame(&mut self) {
        self.captured = 0;
        self.line = 240;
        self.next_line_at = CYCLES_PER_LINE;
        self.raster_passed = false;
        self.irq_status |= IRQ_VBLANK;
    }

    /// True once if the picture changed since the last call.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    pub fn peek_vram(&self, addr: u16) -> u8 {
        self.vram[addr as usize]
    }

    /// What the chip shows this frame, for the GPU to draw (render.rs).
    pub fn frame(&self) -> Frame {
        Frame {
            vram: self.vram.to_vec(),
            ctrl: self.ctrl,
            lines: self.lines.clone(),
            backdrop: self.backdrop,
        }
    }

    /// The tile at map cell (`column`, `row`), for tests that read text.
    #[cfg(test)]
    pub fn map_tile(&self, column: usize, row: usize) -> u16 {
        let at = MAP + (row * MAP_COLUMNS + column) * 2;
        u16::from_le_bytes([self.vram[at], self.vram[at + 1]]) & 0x3FF
    }

    /// The palette of map cell (`column`, `row`).
    #[cfg(test)]
    pub fn map_palette(&self, column: usize, row: usize) -> u8 {
        (self.vram[MAP + (row * MAP_COLUMNS + column) * 2 + 1] >> 4) as u8
    }

    #[cfg(test)]
    pub fn sprite(&self, sprite: usize) -> [u8; 4] {
        self.vram[SPRITES + sprite * 4..SPRITES + sprite * 4 + 4].try_into().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn poke(vdp: &mut Vdp, addr: u16, bytes: &[u8]) {
        vdp.write(VADDR, addr as u8);
        vdp.write(VADDR_HI, (addr >> 8) as u8);
        for &byte in bytes {
            vdp.write(VDATA, byte);
        }
    }

    #[test]
    fn ports_read_and_write_vram_in_sequence() {
        let mut vdp = Vdp::default();
        poke(&mut vdp, 0x1234, &[1, 2, 3]);
        assert_eq!((vdp.read(VADDR), vdp.read(VADDR_HI)), (0x37, 0x12));
        vdp.write(VADDR, 0x34);
        assert_eq!([vdp.read(VDATA), vdp.read(VDATA), vdp.read(VDATA)], [1, 2, 3]);
    }

    #[test]
    fn the_beam_keeps_each_lines_scroll_and_interrupts_at_the_raster_line() {
        let mut vdp = Vdp::default();
        let line_start = |n: u64| (VBLANK_LINES + n) * CYCLES_PER_LINE;
        let scroll_x = |vdp: &Vdp, line: usize| u16::from_le_bytes([vdp.lines[line * 4], vdp.lines[line * 4 + 1]]);
        vdp.write(RASTER, 100);
        vdp.write(VIRQ, IRQ_RASTER);
        vdp.start_frame();
        assert!(!vdp.irq(), "vblank is not enabled");
        assert_eq!(vdp.next_interrupt(0), Some(line_start(100)));

        vdp.beam(line_start(99) + 10);
        assert!(!vdp.irq());
        vdp.beam(line_start(100));
        assert!(vdp.irq(), "at line 100");
        assert_eq!(vdp.read(VLINE), 100);
        vdp.write(SCROLLX, 40); // the handler moves the scroll
        vdp.write(VSTATUS, IRQ_RASTER); // and acknowledges
        assert!(!vdp.irq());
        assert_eq!(vdp.next_interrupt(line_start(100)), None, "once a frame");

        vdp.end_frame();
        assert_eq!((scroll_x(&vdp, 99), scroll_x(&vdp, 100), scroll_x(&vdp, 239)), (0, 40, 40));
        assert!(vdp.take_changed());
        vdp.start_frame();
        assert_eq!(vdp.read(VSTATUS), IRQ_VBLANK);
        vdp.write(VIRQ, IRQ_VBLANK);
        assert!(vdp.irq(), "vblank, now enabled");
    }

    /// A handler that sets the raster line further down interrupts again
    /// this frame: three splits, three scrolls. A line the beam has passed
    /// waits for the next frame.
    #[test]
    fn a_new_raster_line_splits_the_frame_again() {
        let mut vdp = Vdp::default();
        let line_start = |n: u64| (VBLANK_LINES + n) * CYCLES_PER_LINE;
        let scroll_x = |vdp: &Vdp, line: usize| u16::from_le_bytes([vdp.lines[line * 4], vdp.lines[line * 4 + 1]]);
        vdp.write(VIRQ, IRQ_RASTER);
        vdp.start_frame();
        vdp.write(RASTER, 16);
        for (line, next, scroll) in [(16, 112, 10), (112, 160, 20), (160, 16, 30)] {
            assert_eq!(vdp.next_interrupt(line_start(line) - 1), Some(line_start(line)));
            vdp.beam(line_start(line));
            assert!(vdp.irq(), "at line {line}");
            vdp.write(SCROLLX, scroll);
            vdp.write(RASTER, next);
            vdp.write(VSTATUS, IRQ_RASTER);
        }
        assert_eq!(vdp.next_interrupt(line_start(160)), None, "line 16 is behind the beam: next frame");
        vdp.end_frame();
        let at = |line| scroll_x(&vdp, line);
        assert_eq!([at(15), at(16), at(111), at(112), at(159), at(160), at(239)], [0, 10, 10, 20, 20, 30, 30]);
        vdp.start_frame();
        assert_eq!(vdp.next_interrupt(0), Some(line_start(16)));
    }

    #[test]
    fn plane_b_keeps_its_own_scroll_line_by_line() {
        let mut vdp = Vdp::default();
        let line_start = |n: u64| (VBLANK_LINES + n) * CYCLES_PER_LINE;
        let scroll_b = |vdp: &Vdp, line: usize| {
            let at = HEIGHT * 4 + line * 4;
            (u16::from_le_bytes([vdp.lines[at], vdp.lines[at + 1]]), vdp.lines[at + 2])
        };
        vdp.start_frame();
        vdp.write(SCROLLX2, 0x34);
        vdp.write(SCROLLX2_HI, 0x01);
        vdp.write(SCROLLY2, 9);
        vdp.write(SCROLLX, 7);
        vdp.beam(line_start(50));
        vdp.write(SCROLLX2, 0);
        vdp.write(SCROLLX2_HI, 0);
        vdp.end_frame();
        assert_eq!(scroll_b(&vdp, 49), (0x134, 9));
        assert_eq!(scroll_b(&vdp, 50), (0, 9));
        assert_eq!(u16::from_le_bytes([vdp.lines[50 * 4], vdp.lines[50 * 4 + 1]]), 7, "plane A's, apart");
        assert_eq!((vdp.read(SCROLLX2), vdp.read(SCROLLY2)), (0, 9));
    }
}
