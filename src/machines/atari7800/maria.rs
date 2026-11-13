//! MARIA, the 7800's graphics chip, as a7800 has it. It draws a line at a
//! time into one of its two line buffers, 160 cells, by DMA from what its
//! display list list (DLL) points to: each of its entries a zone of 1 to
//! 16 lines, the zone's display list (DL) and whether its last line ends
//! with an interrupt (a DLI, the CPU's NMI); each DL's headers an object,
//! its graphics' address, width, palette and place on the line, the next
//! line's graphics a page further on (the zone's offset counting down to 0).
//! Holey DMA leaves out the graphics at addresses A11 or A12 says, so a
//! zone's objects needn't be cut to fit. Indirect mode reads a byte for each
//! character, and the graphics at CHARBASE and it.
//!
//! While it draws the other buffer's shown, a line late: each cell two dots
//! in 160 modes, or a dot each of its two halves in 320 modes, through the
//! palette as it is then. Its DMA takes the CPU's cycles, as many as it
//! needs: 16 to start, 8 or 10 a header, 3 a graphics byte, and more for
//! indirect mode and holes; 8 more after a zone's last line, and 17 more if
//! that's to interrupt. At 430 it's out of time, and the CPU waits for the
//! next line.
//!
//! Its registers, $20 to $3F: the colours, 8 palettes of 3 and the
//! background, every one whose address's low two bits aren't 0; WSYNC; the
//! DLL's address, DPPH and DPPL; CHARBASE; and CTRL: colour on or off, DMA
//! on or off, character width, kangaroo mode (a cell of colour 0 drawn, not
//! left), and the read mode, which 160 or 320 mode it's shown in. MSTAT, its
//! one register read, says whether it's in the vertical blank.

use crate::host::save_state;
use serde::{Deserialize, Serialize};

/// The cells of a line.
pub const CELLS: usize = 160;
/// Where the CPU stops, its DMA out of time.
pub const DMA_LIMIT: u32 = 430;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Maria {
    /// The colours, by palette and colour: those at 0, 4, 8 and so on all
    /// the background.
    pub colours: [u8; 32],
    /// The two line buffers, one after the other.
    #[serde(with = "save_state::bytes")]
    line_buffers: [u8; 2 * CELLS],
    /// The buffer being drawn; the other's shown.
    active: usize,
    /// 160B's way of writing a byte's cells, as the last extended header
    /// said: two of four colours each, not four of one.
    write_mode: bool,
    /// The DLL's entry, the zone's DL, its holey DMA, the line's offset in
    /// it, and whether it interrupts after its last line.
    dll: u16,
    dl: u16,
    holey: u8,
    offset: u8,
    nmi: bool,
    /// MSTAT's bit 7: in the vertical blank.
    pub vblank: bool,
    dma_on: bool,
    /// DPPH and DPPL: the DLL's address, taken at the frame's first line.
    dpp: u16,
    charbase: u16,
    colour_kill: bool,
    character_width: bool,
    kangaroo: bool,
    read_mode: u8,
}

impl Maria {
    pub fn new() -> Maria {
        Maria {
            colours: [0; 32],
            line_buffers: [0; 2 * CELLS],
            active: 0,
            write_mode: false,
            dll: 0,
            dl: 0,
            holey: 0,
            offset: 0,
            nmi: false,
            vblank: true,
            dma_on: false,
            dpp: 0,
            charbase: 0,
            colour_kill: false,
            character_width: false,
            kangaroo: false,
            read_mode: 0,
        }
    }

    /// A read of register `register`, $00 to $1F: MSTAT's, or nothing.
    pub fn read(&self, register: u8) -> u8 {
        match register {
            0x08 => (self.vblank as u8) << 7,
            _ => 0,
        }
    }

    /// A write of register `register`, $00 to $1F: true if it's WSYNC.
    pub fn write(&mut self, register: u8, value: u8) -> bool {
        if register & 3 != 0 {
            self.colours[register as usize] = value;
        }
        match register {
            0x00 => {
                for colour in self.colours.iter_mut().step_by(4) {
                    *colour = value;
                }
            }
            0x04 => return true,
            0x0C => self.dpp = (self.dpp & 0x00FF) | (value as u16) << 8,
            0x10 => self.dpp = (self.dpp & 0xFF00) | value as u16,
            0x14 => self.charbase = (value as u16) << 8,
            0x1C => {
                self.colour_kill = value & 0x80 != 0;
                match (value >> 5) & 3 {
                    2 => self.dma_on = true,
                    3 => self.dma_on = false,
                    // DMA's test modes, never to be used.
                    _ => {}
                }
                self.character_width = value & 0x10 != 0;
                self.kangaroo = value & 0x04 != 0;
                self.read_mode = value & 0x03;
            }
            _ => {}
        }
        false
    }

    /// The vertical blank, as line `line` of a frame of `lines` starts.
    pub fn line_started(&mut self, line: u32, lines: u32) {
        if line == 16 {
            self.vblank = false;
        }
        if line == lines - 5 {
            self.vblank = true;
        }
    }

    /// Line `line`'s DMA, of a frame of `lines`, its memory read by `read`:
    /// the buffers swapped, the line drawn, and the next line's place in
    /// the DLL. Its cycles, of MARIA's clock, if it ran; and whether it
    /// interrupts, the CPU's NMI.
    pub fn dma(&mut self, line: u32, lines: u32, read: &mut impl FnMut(u16) -> u8) -> (u32, bool) {
        self.active ^= 1;
        if line == 16 && self.dma_on {
            self.dll = self.dpp;
            self.zone(read);
        }
        let mut cycles = 0;
        let drawn = line > 15 && line < lines - 5;
        if drawn {
            self.buffer(self.active).fill(0);
            if self.dma_on {
                cycles = self.draw(read);
            }
        }
        if line > 16 && line < lines - 5 && self.dma_on {
            if self.offset == 0 {
                self.dll = self.dll.wrapping_add(3);
                self.zone(read);
            } else {
                self.offset -= 1;
            }
        }
        (cycles, std::mem::take(&mut self.nmi))
    }

    /// The DLL's entry at `dll`, its zone's DL and the rest.
    fn zone(&mut self, read: &mut impl FnMut(u16) -> u8) {
        let flags = read(self.dll);
        self.dl =
            (read(self.dll.wrapping_add(1)) as u16) << 8 | read(self.dll.wrapping_add(2)) as u16;
        self.offset = flags & 0x0F;
        self.holey = (flags & 0x60) >> 5;
        self.nmi = flags & 0x80 != 0;
    }

    /// Whether the graphics at `addr` are a hole, as the zone's holey DMA
    /// says: those where A12 is set, of 4K holes, or A11, of 2K.
    fn is_holey(&self, addr: u16) -> bool {
        (self.holey & 0x02 != 0 && addr & 0x9000 == 0x9000)
            || (self.holey & 0x01 != 0 && addr & 0x8800 == 0x8800)
    }

    /// The zone's line drawn into the active buffer from its DL: its cycles.
    fn draw(&mut self, read: &mut impl FnMut(u16) -> u8) -> u32 {
        let mut cycles = 16;
        let mut dl = self.dl;
        while read(dl.wrapping_add(1)) & 0x5F != 0 && cycles < DMA_LIMIT {
            let mode = read(dl.wrapping_add(1));
            let graphics = (read(dl.wrapping_add(2)) as u16) << 8 | read(dl) as u16;
            let (width, mut place, palette, indirect);
            if mode & 0x1F == 0 {
                // An extended header: 5 bytes, its write mode and indirect
                // mode its own.
                let byte_3 = read(dl.wrapping_add(3));
                width = ((byte_3 ^ 0xFF) & 0x1F) as u16 + 1;
                place = read(dl.wrapping_add(4));
                palette = byte_3 >> 5;
                self.write_mode = mode & 0x80 != 0;
                indirect = mode & 0x20 != 0;
                dl = dl.wrapping_add(5);
                cycles += 10;
            } else {
                width = ((mode ^ 0xFF) & 0x1F) as u16 + 1;
                place = read(dl.wrapping_add(3));
                palette = mode >> 5;
                indirect = false;
                dl = dl.wrapping_add(4);
                cycles += 8;
            }
            let page = (self.offset as u16) << 8;
            let mut hole_counted = false;
            for x in 0..width {
                if cycles >= DMA_LIMIT {
                    break;
                }
                let addr = if indirect {
                    let character = read(graphics.wrapping_add(x));
                    (self.charbase | character as u16).wrapping_add(page)
                } else {
                    graphics.wrapping_add(x).wrapping_add(page)
                };
                if self.is_holey(addr) {
                    if !hole_counted {
                        cycles += 3;
                    }
                    hole_counted = true;
                    continue;
                }
                if indirect {
                    cycles += 3;
                }
                place = self.write_cells(read(addr), place, palette);
                cycles += 3;
                if indirect && self.character_width {
                    place = self.write_cells(read(addr.wrapping_add(1)), place, palette);
                    cycles += 3;
                }
            }
        }
        // A zone's last line: MARIA's shutdown, and the DLI's start.
        if self.offset == 0 {
            cycles += 8;
            if read(self.dll.wrapping_add(3)) & 0x80 != 0 {
                cycles += 17;
            }
        }
        cycles
    }

    /// A graphics byte `data`'s cells written at `place`, of `palette`: four
    /// of a colour each, or in 160B's write mode two of four: where the
    /// line goes next. Colour 0 isn't written, unless in kangaroo mode.
    fn write_cells(&mut self, data: u8, place: u8, palette: u8) -> u8 {
        let palette = palette << 2;
        let cells: [u8; 4];
        let count = if self.write_mode {
            cells = [
                (palette & 0x10) | (data & 0x0C) | data >> 6,
                (palette & 0x10) | (data & 0x03) << 2 | (data & 0x30) >> 4,
                0,
                0,
            ];
            2
        } else {
            cells = std::array::from_fn(|n| palette | (data >> (6 - 2 * n)) & 0x03);
            4
        };
        let kangaroo = self.kangaroo;
        let buffer = self.buffer(self.active);
        let mut place = place;
        for &cell in &cells[..count] {
            if (cell & 3 != 0 || kangaroo) && (place as usize) < CELLS {
                buffer[place as usize] = cell;
            }
            place = place.wrapping_add(1);
        }
        place
    }

    fn buffer(&mut self, n: usize) -> &mut [u8] {
        &mut self.line_buffers[n * CELLS..(n + 1) * CELLS]
    }

    /// Cell `x` of the line shown, as it's shown now: its two dots' colours.
    pub fn dots(&self, x: usize) -> [u8; 2] {
        let cell = self.line_buffers[(self.active ^ 1) * CELLS + x] as usize;
        let dots = match self.read_mode {
            // 160A and 160B.
            0 | 1 => [cell, cell],
            // 320B and 320D.
            2 => [
                (cell & 0x10) | (cell & 0x02) | (cell >> 3) & 1,
                (cell & 0x10) | (cell << 1) & 0x02 | (cell >> 2) & 1,
            ],
            // 320A and 320C.
            _ => [
                (cell & 0x1C) | (cell & 0x02),
                (cell & 0x1C) | (cell << 1) & 0x02,
            ],
        };
        dots.map(|dot| {
            let colour = self.colours[dot];
            if self.colour_kill {
                colour & 0x0F
            } else {
                colour
            }
        })
    }
}

impl Default for Maria {
    fn default() -> Maria {
        Maria::new()
    }
}
