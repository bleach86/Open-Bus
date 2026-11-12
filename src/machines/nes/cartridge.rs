//! The cartridge: its PRG ROM, on the CPU's side from $8000; its CHR, ROM
//! or RAM, the PPU's pattern tables at $0000 to $1FFF; its PRG RAM at
//! $6000, battery backed in some, a game's saves; and its mapper, the
//! board's logic that switches banks of them in, and wires the PPU's
//! nametables to its 2K of RAM (or the cartridge's own) as the board does.
//!
//! An image is an iNES file: a 16-byte header, "NES" and $1A; its PRG
//! ROM's size in 16K, its CHR ROM's in 8K (none, 8K of CHR RAM); flags 6,
//! the nametables' mirroring (bit 0 vertical, bit 3 four screens), a
//! battery (bit 1), a 512-byte trainer before the PRG (bit 2), and the
//! mapper's low nibble; flags 7, its high nibble, and in bits 2 and 3 a 2
//! for NES 2.0, whose byte 8 has the mapper's bits 8 to 11 and bytes 10
//! and 11 its RAMs' sizes, if its ROMs as it has them fit in the image. An
//! old header with junk in its last bytes (a ripper's name) has none in
//! flags 7 either, taken as 0.
//!
//! The mappers:
//!
//! 0, NROM: no switching, 16K of PRG (at $8000 and again at $C000) or 32K.
//!
//! 1, MMC1: its registers written a bit at a time, bit 0 of five writes to
//! $8000 up, the fifth's address saying which; a write with bit 7 set
//! starts over, and sets PRG mode 3. Writes on consecutive cycles (a
//! read-modify-write's two) are taken once. Control ($8000): mirroring
//! (one screen, lower or upper; vertical; horizontal), the PRG mode (0 or
//! 1, 32K at $8000; 2, the first 16K fixed at $8000, a bank at $C000; 3, a
//! bank at $8000, the last fixed at $C000), the CHR mode (8K, or two 4K).
//! CHR banks 0 and 1 ($A000, $C000), 4K each; the PRG bank ($E000), its
//! bit 4 the PRG RAM's disable. Boards with 512K of PRG take its top bit
//! from CHR bank 0's bit 4; with 32K of PRG RAM, its 8K bank from bits 2
//! and 3.
//!
//! 105, NES-EVENT, Nintendo World Championships 1990's: an MMC1, its 256K
//! two chips of 128K, its 8K of CHR RAM not banked. Its first 32K's fixed
//! till CHR bank 0's bit 4, set at power on, is cleared and set again.
//! Then that register's bit 3 says which chip: the first's 32K banks by its
//! bits 1 and 2, or the second's as MMC1 banks them, by the PRG bank's low
//! three bits, the second's own last fixed. Its bit 4 is a timer's too:
//! cleared, a count up every CPU cycle; set, the count back at 0 and held,
//! and the IRQ acknowledged; and at the time its DIP switches say, the IRQ
//! line held, the game over. As the championships had them set: 6 minutes
//! and 15 seconds.
//!
//! 2, UxROM: a 16K bank at $8000, the last fixed at $C000.
//!
//! 3, CNROM: an 8K bank of CHR.
//!
//! 4, MMC3: bank select ($8000, even): the register to write next, the PRG
//! mode (bit 6: the second-last 8K at $8000, not $C000) and the CHR's
//! inversion (bit 7: its 2K banks at $1000, its 1K at $0000); bank data
//! ($8001, odd): R0 and R1, 2K of CHR, R2 to R5, 1K, R6 and R7, 8K of PRG,
//! the last 8K fixed at $E000; mirroring ($A000); PRG RAM enabled and
//! protected ($A001). Its IRQ counts the lines: clocked by the PPU's
//! address line A12 rising after being low a while (as it does once a line
//! as sprites' patterns are fetched after the background's), reloaded from
//! its latch ($C000) at 0 or when told ($C001), and at 0, enabled ($E001,
//! $E000 disabling and acknowledging), holding the IRQ line. The MMC6, its
//! cousin (StarTropics' and its sequel's, known by their ROMs, an iNES
//! header not saying, or NES 2.0's submapper 1): its RAM 1K, at $7000 and
//! over again to $7FFF, enabled by bank select's bit 5; each 512 bytes'
//! reads and writes enabled by $A001's top bits, a half written only if
//! it's read too, a half not read reading 0 while the other is.
//!
//! 5, the MMC5: in mmc5.rs, there being so much of it.
//!
//! 21, 22, 23 and 25, Konami's VRC2 and VRC4; 75, VRC1; 73, VRC3; 24 and
//! 26, VRC6, with its sound: in vrc.rs.
//!
//! 20, the Famicom Disk System: not a cartridge, but it's in the slot, and
//! all of it, in fds.rs.
//!
//! 7, AxROM: a 32K bank, and which 1K all four nametables are.
//!
//! 9, MMC2: an 8K bank at $8000 ($A000), the last three fixed after it; 4K
//! of CHR at $0000 and at $1000, of two banks each ($B000 and $C000, $D000
//! and $E000), which of the two by a latch: set by the PPU reading tile
//! $FD or $FE, after the read, so the tile's the old bank's; at $0000 on
//! that tile's top row's second plane only, at $1000 on any of its rows.
//! Mirroring ($F000).
//!
//! 10, MMC4: MMC2 with a 16K bank at $8000 ($A000), the last 16K fixed,
//! and its left table's latch set on any of tile $FD's or $FE's rows, as
//! its right's is. 8K of PRG RAM, kept by a battery on Fire Emblem's.
//!
//! 11, Color Dreams: a 32K bank (bits 0 and 1) and an 8K bank of CHR (bits
//! 4 to 7). Its bus conflicts only as NES 2.0's submapper 2 says: a plain
//! header doesn't say which board it is, and a game of a board without
//! them doesn't run with them.
//!
//! 34: two boards unrelated. BNROM (NES 2.0's submapper 2, or up to 8K of
//! CHR): a 32K bank, with bus conflicts. NINA-001 (submapper 1, or more
//! CHR): at $7FFD a 32K bank, at $7FFE and $7FFF the 4K banks of CHR at
//! $0000 and $1000, the writes going to the PRG RAM under them too.
//!
//! 66, GxROM: a 32K bank (bits 4 and 5) and an 8K bank of CHR (bits 0 and
//! 1). Nintendo's GNROM board has bus conflicts, but as Mesen has it, none:
//! its games don't need them, and a hack of Jajamaru no Daibouken for 66
//! only runs without.
//!
//! 71, Camerica: a 16K bank at $8000, written at $C000 up, the last fixed
//! at $C000. Fire Hawk's board has which 1K all four nametables are too,
//! written at $9000: not at $8000, which games on the others write.
//!
//! 232, Camerica's Quattro multicarts: as 71, in one of four 64K blocks,
//! bits 3 and 4 written at $8000 to $BFFF, its bank's 2 bits at $C000 up,
//! the block's last 16K fixed at $C000. The Aladdin Deck Enhancer's (NES
//! 2.0's submapper 1) has the block's two bits the other way round.
//!
//! 79, NINA-03 and 06: a register where the address is 010x xxx1 xxxx
//! xxxx, $4100 and every other 256 bytes to $5FFF: a 32K bank (bit 3) and
//! an 8K bank of CHR (bits 0 to 2).
//!
//! 144: Color Dreams' board with a resistor put in the way of D0, Death
//! Race's, its ROM's bit 0 always winning; known by its ROM too, its
//! header often saying 11.
//!
//! 118, TxSROM: MMC3 with the top bit of a 1K bank's number wired to the
//! nametables' address line. Its CHR banking ignores PPU A13, so a
//! nametable's address is banked as if it were $0000 to $0FFF's, and each
//! 1K of the nametables is whichever page that bit of the same bank says:
//! R0 and R1's for 2K each, or with the CHR inverted R2 to R5's for 1K;
//! $A000's mirroring does nothing.
//!
//! 119, TQROM: MMC3 with both CHR ROM and 8K of CHR RAM, bit 6 of each
//! bank's number choosing, 6 bits left to say which bank of it.
//!
//! 37, Nintendo of Europe's SMB, Tetris and World Cup, and 47, NES-QJ's
//! Super Spike V'Ball and World Cup: MMC3 multicarts, a register at $6000
//! up in place of its PRG RAM, written only as that is, not read, and
//! cleared by Reset. 47's bit 0 says which 128K of PRG and of CHR. 37's
//! bit 2 is the 128K of CHR and PRG's A17; its low bits, both set, force
//! PRG's A16 high, else it's the MMC3's if bit 2's set, else low.
//!
//! 206, Namco 108, and its boards: the MMC3's ancestor, its bank select and
//! bank data and nothing else, every write from $8000 up one of those two
//! by its A0. No PRG mode or CHR inversion (bank select's top bits not
//! there): R6 and R7 at $8000 and $A000, the last two 8K fixed; R0 and R1
//! the left pattern table, R2 to R5 the right. No IRQ, no PRG RAM, and its
//! mirroring wired. 76, NAMCOT-3446: R2 to R5 four 2K banks, all the CHR
//! there is. 88, NAMCOT-3443: the left pattern table from the CHR's first
//! 64K, the right from the second. 95, NAMCOT-3425: R0's bit 5 says which
//! 1K the top two nametables are, R1's the bottom two. 154, NAMCOT-3453:
//! as 88, every write's bit 6 saying which 1K all four nametables are.
//!
//! 64, RAMBO-1, Tengen's: MMC3's registers at the same addresses, and
//! more. Bank select's bit 5 makes $0000 to $0FFF's banks four of 1K, R0,
//! R8, R1 and R9, not R0 and R1's 2K each; it has a third 8K of PRG, RF, at
//! $C000, or with bit 6 $8000 and R6 at $C000; only the last 8K is fixed.
//! Its IRQ counts A12's rises as MMC3's does, or ($C001's bit 0) every 4
//! CPU cycles: at each count, reloaded from its latch at 0 or if $C001 was
//! written, then ORed with 1, else taken down, and at 0 enabled, the line
//! held a little later, 4 cycles counting lines and 1 counting cycles: Klax
//! sets its scroll every 8 lines from the IRQ, its writes only safely past
//! dot 256 of the line if it's as late as that. No PRG RAM.
//!
//! 69, Sunsoft's FME-7: a command written at $8000 up, its parameter at
//! $A000 up. 0 to 7, eight 1K banks of CHR; 9 to B, 8K of PRG at $8000,
//! $A000 and $C000, the last fixed; C, the nametables (0, A10; 1, A11; 2
//! and 3 one screen). 8, what's at $6000: an 8K bank of ROM, or with bit 6
//! its PRG RAM, there if bit 7's set too. Its IRQ counts down every CPU
//! cycle, its 16 bits set directly by E and F, D enabling the count and
//! the line and acknowledging, the line held on going past 0. The
//! Famicom's 5B has sound at $C000 up too; carts of this have none.
//!
//! 228, Active Enterprises' Action 52 and Cheetahmen II: what's written is
//! mostly in the address. A13 the mirroring (0 vertical); A12 and A11
//! which 512K chip of PRG, the third there isn't, so open bus, the images
//! having the fourth straight after the second; A10 to A6 its 16K bank; A5
//! that bank at $8000 and $C000, not 32K of it and the next; and of the CHR's 8K
//! bank, A3 to A0 the high bits, the data's two the low.
//!
//! Boards of one register, written at $8000 up, that's all there is to
//! them: what was last written, each its bits' meaning.
//!
//! 94, UN1ROM: as UxROM, its bank in bits 2 to 4. 180, Crazy Climber's
//! UNROM: the first 16K fixed at $8000, a bank at $C000.
//!
//! 13, CPROM: 16K of CHR RAM, its first 4K at $0000, a 4K bank at $1000
//! (bits 0 and 1).
//!
//! 185, CNROM with its CHR ROM as copy protection: CHR there only if what's
//! written, with bus conflicts, says so; NES 2.0's submappers 4 to 7, its
//! bits 0 and 1 being theirs, and else its low four bits not 0 and it not
//! $13. Without it, the pattern tables are open bus, the address's low
//! byte, but with D0 pulled up, as Mighty Bomb Jack needs to start.
//!
//! 70, Bandai's: a 16K bank at $8000 (bits 4 to 6), the last fixed, and an
//! 8K bank of CHR (bits 0 to 3). 152: as 70, bit 7 which 1K all four
//! nametables are.
//!
//! 89, Sunsoft-2 on Mito Koumon: as 152, but the nametables' 1K bit 3,
//! and bit 7 the CHR bank's bit 3. 93, Sunsoft-2 on Fantasy Zone: a 16K bank
//! (bits 4 to 6), its CHR RAM there only with bit 0 set.
//!
//! 68, Sunsoft-4: four 2K banks of CHR ($8000, $9000, $A000, $B000); a
//! 16K bank at $8000 ($F000, bits 0 to 2), the last fixed, bit 4 its PRG
//! RAM's enable; the mirroring ($E000: vertical, horizontal, one screen or
//! the other), and in its bit 4, the nametables 1K of CHR ROM each, from
//! its last 128K ($C000 and $D000, their top bit set), not the PPU's RAM.
//! Nantettatte!! Baseball's option cartridge after its 128K: its ROM at
//! $8000 while bit 3's clear, but only for a while after a write at $6000
//! up with the RAM off, its licence; after that, open bus.
//!
//! Jaleco's: 86, JF-13, a register at $6000 up to $6FFF, a 32K bank (bits
//! 4 and 5) and an 8K bank of CHR (bits 0 and 1, and 6 its bit 2); its
//! sound at $7000 up, a speech chip, not here. 87, its CHR's 8K bank in
//! bits 0 and 1 the other way round, written at $6000 up. 140, JF-11 and
//! JF-14: at $6000 up, a 32K bank (bits 4 and 5) and an 8K of CHR (bits 0
//! to 3). 78, JF-16 and Irem's IF-12 on Holy Diver, with bus conflicts: a
//! 16K bank at $8000 (bits 0 to 2), the last fixed, and an 8K of CHR (bits
//! 4 to 7); bit 3 which 1K all four nametables are, or Holy Diver's (NES
//! 2.0's submapper 3), vertical or horizontal.
//!
//! 97, Irem's TAM-S1: a 16K bank at $C000 (bits 0 to 3), the last fixed at
//! $8000; bits 6 and 7 the nametables, one screen, horizontal, vertical,
//! or the other screen. 184, Sunsoft-1: at $6000 up, two 4K banks of CHR,
//! at $0000 (bits 0 to 2) and $1000 (bits 4 to 6, its top bit always set).
//! 77, Irem's on Napoleon Senki, with bus conflicts: a 32K bank (bits 0 to
//! 3) and a 2K bank of CHR ROM at $0000 (bits 4 to 7), 6K of CHR RAM after
//! it; its own 4 screens of nametables.
//!
//! 41, Caltron's 6 in 1: its 32K bank, the CHR's 8K bank's top two bits,
//! and its mirroring (A5: horizontal) in the address written at $6000 to
//! $67FF; the CHR bank's bottom two in what's written at $8000 up, but
//! only while the 32K bank's 4 to 7. All cleared by Reset, its menu again.
//!
//! 234, the Maxi 15: two registers, written or read, the ROM's what's
//! there, with bus conflicts: the first ($FF80 to $FF9F) once only, until
//! its low six bits are set, the second ($FFE8 to $FFF8) any time. As
//! CNROM, the first's bits 0 to 3 its 32K bank, and its 0 and 1 with the
//! second's 4 and 5 its CHR's 8K; as NINA-03, the first's bit 6 set, the
//! first's bits 1 to 3 and the second's 0 its 32K, the first's 1 to 3 and
//! the second's 4 to 6 its CHR. The first's bit 7 its mirroring, horizontal.
//!
//! 72, Jaleco's JF-17, and 92, JF-19: with bus conflicts, its banks taken
//! only as a write's bits 7 and 6 rise, from the last's: 7 its PRG's 16K at
//! $8000, the last fixed (bits 0 to 2); JF-19's at $C000, the first fixed
//! (bits 0 to 3); 6 its CHR's 8K (bits 0 to 3). Their speech chips not
//! here.
//!
//! Bus conflicts: on a board whose register is written with its ROM still
//! driving the bus, the bits it holds low win; the register's given what
//! was written ANDed with the ROM's byte there.

use super::bandai::{self, Bandai};
use super::database::{self, Known};
use super::fds::Fds;
use super::mmc5::{self, Mmc5};
use super::namco163::{CHIP_RAM, Chip, Namco163};
use super::region::Region;
use super::vrc::{Vrc, Vrc1, Vrc3, Vrc6, Wiring};
use serde::{Deserialize, Serialize};

/// How the PPU's four nametables, $2000, $2400, $2800 and $2C00, are
/// wired to its 2K: horizontal, $2000 and $2400 the first 1K and the other
/// two the second, for a game scrolling up and down; vertical, $2000 and
/// $2800 the first, for one scrolling across; all to one 1K, the first or
/// the second; or four screens, the cartridge's RAM the other 2K.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mirroring {
    Horizontal,
    Vertical,
    SingleLower,
    SingleUpper,
    FourScreen,
}

impl Mirroring {
    /// Which 1K of the nametables' RAM, 0 to 3, PPU address `addr` is in.
    pub fn page(self, addr: u16) -> usize {
        let table = (addr as usize >> 10) & 3;
        match self {
            Mirroring::Horizontal => table >> 1,
            Mirroring::Vertical => table & 1,
            Mirroring::SingleLower => 0,
            Mirroring::SingleUpper => 1,
            Mirroring::FourScreen => table,
        }
    }
}

/// MMC1's registers: its shift register and the bits in it; control, the
/// CHR banks, the PRG bank; and the cycle it was last written on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mmc1 {
    shift: u8,
    bits: u8,
    control: u8,
    chr: [u8; 2],
    prg: u8,
    written: Option<u64>,
}

impl Mmc1 {
    /// As it is at power on: control's PRG mode 3, the last 16K fixed.
    fn power_on() -> Mmc1 {
        Mmc1 {
            shift: 0,
            bits: 0,
            control: 0x0C,
            chr: [0; 2],
            prg: 0,
            written: None,
        }
    }

    /// `value` written at `addr` on CPU cycle `cycle`, a bit of a register:
    /// whether a register's filled, or it's started over; and if it filled
    /// control, the mirroring that says.
    fn write(&mut self, addr: u16, value: u8, cycle: u64) -> (bool, Option<Mirroring>) {
        let consecutive = self.written == Some(cycle.wrapping_sub(1));
        self.written = Some(cycle);
        if consecutive {
            return (false, None);
        }
        if value & 0x80 != 0 {
            (self.shift, self.bits) = (0, 0);
            self.control |= 0x0C;
            return (true, None);
        }
        self.shift |= (value & 1) << self.bits;
        self.bits += 1;
        if self.bits < 5 {
            return (false, None);
        }
        let register = self.shift;
        (self.shift, self.bits) = (0, 0);
        match addr {
            0x8000..=0x9FFF => {
                self.control = register;
                let mirroring = [
                    Mirroring::SingleLower,
                    Mirroring::SingleUpper,
                    Mirroring::Vertical,
                    Mirroring::Horizontal,
                ][register as usize & 3];
                return (true, Some(mirroring));
            }
            0xA000..=0xBFFF => self.chr[0] = register,
            0xC000..=0xDFFF => self.chr[1] = register,
            _ => self.prg = register,
        }
        (true, None)
    }
}

/// NES-EVENT's MMC1; how far it's been started, CHR bank 0's bit 4 cleared
/// then set again; and its timer's count, whether it's counting, and its
/// IRQ line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nwc {
    mmc1: Mmc1,
    started: u8,
    pub(super) count: u32,
    counting: bool,
    irq: bool,
}

/// NES-EVENT's time, in CPU cycles: bit 29 always, and the DIP switches
/// bits 25 to 28, as the championships had them, only bit 27's closed.
pub(super) const NWC_TIME: u32 = 0x2000_0000 | 0x0800_0000;

impl Nwc {
    fn new() -> Nwc {
        let mut mmc1 = Mmc1::power_on();
        mmc1.chr[0] = 0x10;
        Nwc {
            mmc1,
            started: 0,
            count: 0,
            counting: false,
            irq: false,
        }
    }

    /// A register filled, or the MMC1 started over: CHR bank 0's bit 4,
    /// its start, and its timer's reset or run.
    fn written(&mut self) {
        let reset = self.mmc1.chr[0] & 0x10 != 0;
        self.started = match (self.started, reset) {
            (0, false) => 1,
            (1, true) => 2,
            (started, _) => started,
        };
        self.counting = !reset;
        if reset {
            (self.count, self.irq) = (0, false);
        }
    }

    /// A CPU cycle: a count, and at its time, the line held.
    fn cycle(&mut self) {
        if self.counting {
            self.count += 1;
            if self.count >= NWC_TIME {
                (self.irq, self.counting) = (true, false);
            }
        }
    }
}

/// Sunsoft-4's registers: its four 2K banks of CHR, and the two 1K of it
/// that can be the nametables, and whether they are; its 16K bank, whether
/// that's an option cartridge's, and its PRG RAM's enable; and the cycles
/// left of the option cartridge's licence.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sunsoft4 {
    chr: [u8; 4],
    tables: [u8; 2],
    chr_tables: bool,
    prg: u8,
    external: bool,
    ram_on: bool,
    licence: u32,
}

/// How long Nantettatte!! Baseball's option cartridge stays licensed, in
/// CPU cycles, after a write: as the game checks it, 1024 times 105.
const SUNSOFT4_LICENCE: u32 = 1024 * 105;

impl Sunsoft4 {
    /// Whether its 16K at $8000 is open bus: an option cartridge's, lapsed.
    fn lapsed(&self) -> bool {
        self.external && self.licence == 0
    }
}

/// MMC3's registers: bank select, the 8 banks, the PRG RAM's enable and
/// protection; the IRQ's latch, counter, reload, enable and line; and
/// the PPU dot A12 was last seen low from. Whether it's an MMC6, and its
/// RAM's halves' enables.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mmc3 {
    select: u8,
    banks: [u8; 8],
    ram_enabled: bool,
    ram_protected: bool,
    mmc6: bool,
    halves: u8,
    irq_latch: u8,
    irq_counter: u8,
    irq_reload: bool,
    irq_enabled: bool,
    irq: bool,
    a12_low_since: Option<u64>,
}

/// What a board's register gets of a write, with its ROM's `rom` there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Conflicts {
    None,
    And,
    /// As And, but the ROM's bit 0 is all that bit gets: mapper 144's.
    RomD0,
}

impl Conflicts {
    fn on(self, value: u8, rom: u8) -> u8 {
        match self {
            Conflicts::None => value,
            Conflicts::And => value & rom,
            Conflicts::RomD0 => (value | 1) & rom,
        }
    }
}

/// A board's two banks: 32K of PRG and 8K of CHR; and writing them's bus
/// conflicts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Banks {
    prg: u8,
    chr: u8,
    conflicts: Conflicts,
}

impl Banks {
    fn new(conflicts: Conflicts) -> Banks {
        Banks {
            prg: 0,
            chr: 0,
            conflicts,
        }
    }
}

/// Jaleco's JF-17's and JF-19's banks, and what was last written, their
/// bits 7 and 6 latching them as they rise.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Jf17 {
    prg: u8,
    chr: u8,
    last: u8,
}

/// Caltron's 6 in 1's banks: its 32K, and its CHR's 8K.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caltron {
    prg: u8,
    chr: u8,
}

/// The Maxi 15's two registers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Maxi15 {
    regs: [u8; 2],
}

impl Maxi15 {
    /// `addr` read or written, `value` there: a register set if it's one
    /// that can be; its mirroring then.
    fn access(&mut self, addr: u16, value: u8) -> Option<Mirroring> {
        match addr {
            0xFF80..=0xFF9F if self.regs[0] & 0x3F == 0 => self.regs[0] = value,
            0xFFE8..=0xFFF8 => self.regs[1] = value & 0x71,
            _ => return None,
        }
        Some(if self.regs[0] & 0x80 != 0 {
            Mirroring::Horizontal
        } else {
            Mirroring::Vertical
        })
    }

    /// Its 32K bank, and its CHR's 8K.
    fn banks(&self) -> (usize, usize) {
        let [first, second] = self.regs.map(|reg| reg as usize);
        if first & 0x40 != 0 {
            (
                (first & 0x0E) | (second & 1),
                (first << 2) & 0x38 | (second >> 4) & 7,
            )
        } else {
            (first & 0x0F, (first << 2) & 0x3C | (second >> 4) & 3)
        }
    }
}

/// NINA-001's registers: its 32K bank, and its two 4K banks of CHR.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nina001 {
    prg: u8,
    chr: [u8; 2],
}

/// RAMBO-1's registers: bank select, and R0 to R9 and RF, by their numbers;
/// the IRQ's latch, counter and reload, whether it's counting cycles and
/// how many of the 4, enable, line, the cycles till that's held; and the
/// PPU dot A12 was last seen low from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rambo1 {
    select: u8,
    banks: [u8; 16],
    irq_latch: u8,
    irq_counter: u8,
    irq_reload: bool,
    counting_cycles: bool,
    prescaler: u8,
    irq_enabled: bool,
    irq: bool,
    irq_delay: u8,
    a12_low_since: Option<u64>,
}

impl Rambo1 {
    /// The 1K bank that CHR address `addr` is in, by the banks, the mode
    /// and their inversion.
    fn chr_bank(&self, addr: usize) -> usize {
        let addr = if self.select & 0x80 != 0 {
            addr ^ 0x1000
        } else {
            addr
        };
        let full = self.select & 0x20 != 0;
        let bank = |register: usize| self.banks[register] as usize;
        match addr >> 10 {
            0 if full => bank(0),
            1 if full => bank(8),
            2 if full => bank(1),
            3 if full => bank(9),
            slot @ (0 | 1) => (bank(0) & 0xFE) | slot,
            slot @ (2 | 3) => (bank(1) & 0xFE) | (slot & 1),
            slot => bank(slot - 2),
        }
    }

    /// A count of the IRQ's, the line held `delay` cycles on if it's due.
    fn count(&mut self, delay: u8) {
        if self.irq_reload {
            self.irq_counter = if self.irq_latch == 0 {
                0
            } else {
                self.irq_latch | 1
            };
            self.irq_reload = false;
        } else if self.irq_counter == 0 {
            self.irq_counter = self.irq_latch;
        } else {
            self.irq_counter -= 1;
        }
        if self.irq_counter == 0 && self.irq_enabled {
            self.irq_delay = delay;
        }
    }
}

/// FME-7's registers: the command; the CHR banks, 8K of PRG, and what's at
/// $6000; its IRQ's counter, the count's and the line's enables, the line.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fme7 {
    command: u8,
    chr: [u8; 8],
    prg: [u8; 3],
    window: u8,
    counter: u16,
    counting: bool,
    irq_enabled: bool,
    irq: bool,
}

/// A Quattro's registers: its 64K block and 16K bank; and whether it's the
/// Aladdin's, the block's bits swapped.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quattro {
    block: u8,
    bank: u8,
    aladdin: bool,
}

/// Action 52's register: its chip of PRG and 16K bank, whether that's at
/// both places, and its 8K of CHR.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Active {
    chip: u8,
    bank: u8,
    one_bank: bool,
    chr: u8,
}

/// The chip of Action 52 that isn't there.
const ACTIVE_NO_CHIP: u8 = 2;

/// The boards MMC3's wired to differently.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Board {
    #[default]
    Plain,
    Txsrom,
    Tqrom,
    Multicart37,
    Multicart47,
    Namco108,
    Namco3446,
    Namco3443,
    Namco3425,
    Namco3453,
    Mmc4,
    Un1rom,
    Unrom180,
    Cprom,
    /// 185's, its submapper saying when its CHR's there.
    Cnrom185(u8),
    Bandai70,
    Bandai152,
    Sunsoft89,
    Sunsoft93,
    Jf13,
    Jaleco87,
    Jf11,
    /// 78's, its submapper saying if it's Holy Diver's.
    Jf16(u8),
    Jf17,
    Jf19,
    TamS1,
    Sunsoft184,
    Irem77,
    /// A VRC2's or 4's, and whether that's a guess, its header's an old one.
    Vrc(Wiring, bool),
}

impl Board {
    /// Whether its one register's at $6000 up, where RAM would be.
    fn register_at_6000(self) -> bool {
        matches!(
            self,
            Board::Jf13 | Board::Jaleco87 | Board::Jf11 | Board::Sunsoft184
        )
    }
}

impl Board {
    fn is_multicart(self) -> bool {
        matches!(self, Board::Multicart37 | Board::Multicart47)
    }

    /// Whether it's Namco 108's, or one of its boards'.
    fn is_namco_108(self) -> bool {
        matches!(
            self,
            Board::Namco108
                | Board::Namco3446
                | Board::Namco3443
                | Board::Namco3425
                | Board::Namco3453
        )
    }
}

impl Mmc3 {
    /// The 1K bank that CHR address `addr` is in, by the banks and their
    /// inversion: R0 and R1 2K each, R2 to R5 1K.
    fn chr_bank(&self, addr: usize) -> usize {
        let addr = if self.select & 0x80 != 0 {
            addr ^ 0x1000
        } else {
            addr
        };
        match addr >> 10 {
            slot @ (0 | 1) => (self.banks[0] as usize & 0xFE) | slot,
            slot @ (2 | 3) => (self.banks[1] as usize & 0xFE) | (slot & 1),
            slot => self.banks[slot - 2] as usize,
        }
    }
}

/// MMC2's registers: its 8K bank; of the pattern tables' two 4K banks each,
/// those for tiles $FD and $FE; and each table's latch, which of them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mmc2 {
    prg: u8,
    chr: [[u8; 2]; 2],
    latch: [usize; 2],
}

/// The MMC6's games, by their ROMs' CRC-32: StarTropics, and StarTropics
/// II.
const MMC6_GAMES: [u32; 2] = [0x8891_29CB, 0xD054_FFB0];

/// Mapper 144's game by its ROM's CRC-32, for a header saying 11: Death
/// Race.
const MAPPER_144_GAMES: [u32; 1] = [0x5CAA_3E61];

/// Mapper 140's games by their ROMs' CRC-32, whatever an old header says:
/// Jajamaru no Daibouken, its dumps' headers saying 66, the database not
/// knowing it. Its register's at $6000, JF-14's; a hack of it writes $8000
/// instead, and is 66.
const MAPPER_140_GAMES: [u32; 1] = [0x6BBE_2BE5];

/// How long A12 must be low, in PPU dots, for its rise to clock MMC3's
/// IRQ counter: about three CPU cycles. Not between a line's sprites'
/// fetches, only as the background's give way to theirs.
const A12_LOW_DOTS: u64 = 10;

/// The cycles after a count RAMBO-1's IRQ line's held: counting lines, and
/// counting cycles.
const RAMBO1_LINE_DELAY: u8 = 4;
const RAMBO1_CYCLE_DELAY: u8 = 1;

/// A board's mapper, and its registers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mapper {
    Nrom,
    Mmc1(Mmc1),
    Uxrom(u8),
    Cnrom(u8),
    Mmc3(Mmc3),
    Axrom(u8),
    ColorDreams(Banks),
    Camerica(u8),
    Nina03(Banks),
    Gxrom(Banks),
    Bnrom(u8),
    Nina001(Nina001),
    Mmc2(Mmc2),
    Rambo1(Rambo1),
    Fme7(Fme7),
    Quattro(Quattro),
    Mmc5(Box<Mmc5>),
    Active(Active),
    Fds(Box<Fds>),
    Latch(u8),
    Jf17(Jf17),
    Caltron(Caltron),
    Maxi15(Maxi15),
    Vrc(Vrc),
    Vrc1(Vrc1),
    Vrc3(Vrc3),
    Namco163(Box<Namco163>),
    Bandai(Box<Bandai>),
    Nwc(Nwc),
    Sunsoft4(Sunsoft4),
    Vrc6(Box<Vrc6>),
}

/// Its ROMs aren't in a save state: the machine it's loaded into has
/// them. Its RAM is.
#[derive(Serialize, Deserialize)]
pub struct Cartridge {
    #[serde(skip)]
    pub prg_rom: Vec<u8>,
    #[serde(skip)]
    pub chr_rom: Vec<u8>,
    /// Its CHR RAM, if it has no CHR ROM.
    pub chr_ram: Vec<u8>,
    pub prg_ram: Vec<u8>,
    /// Whether its PRG RAM is battery backed, its saves kept; and whether
    /// it's been written since they last were.
    pub battery: bool,
    #[serde(skip)]
    pub written: bool,
    /// Its nametables' wiring, as the board is soldered, or the mapper
    /// switches it.
    pub mirroring: Mirroring,
    pub mapper: Mapper,
    /// The region its header says it's for, if it says one.
    #[serde(skip)]
    pub region: Option<Region>,
    /// Whether the database knows it as a Famicom's game; a Disk System's
    /// always is.
    #[serde(skip)]
    pub famicom: bool,
    /// Whether the database knows it as a Zapper's game.
    #[serde(skip)]
    pub zapper: bool,
    /// The board its MMC3 is on: as the ROMs are, not in a save state.
    #[serde(skip)]
    pub board: Board,
    /// The PPU's fetching a sprite: for the MMC5, which keeps count.
    #[serde(skip)]
    pub sprite_fetch: bool,
    /// Where each 4K of $8000 up is in the PRG ROM, and each 1K of the
    /// pattern tables in the CHR, if they're known: what a mapper has them
    /// as is a lot to work out each read, and only a write changes it.
    #[serde(skip)]
    prg_pages: Option<[u32; 8]>,
    #[serde(skip)]
    chr_pages: Option<[u32; 8]>,
}

const HEADER: usize = 16;
const TRAINER: usize = 512;
const PRG_BANK: usize = 0x4000;
const CHR_BANK: usize = 0x2000;

impl Cartridge {
    /// The cartridge in the iNES image `image`: as the database has it if
    /// it knows the game, unless that's a board that isn't here yet and the
    /// header's is, as many a game on a near enough board's has been run.
    pub fn new(image: &[u8]) -> Result<Cartridge, String> {
        Cartridge::known_as(image, database::find)
    }

    /// The cartridge in `image`, as `find` knows its game by its ROMs'
    /// CRC-32, or as its header has it.
    pub(super) fn known_as(
        image: &[u8],
        find: fn(u32) -> Option<&'static Known>,
    ) -> Result<Cartridge, String> {
        Cartridge::read(image, find)
            .or_else(|known| Cartridge::read(image, |_| None).map_err(|_| known))
    }

    /// The cartridge in `image`, as `find` knows it, if it does.
    fn read(image: &[u8], find: fn(u32) -> Option<&'static Known>) -> Result<Cartridge, String> {
        let header = image
            .get(..HEADER)
            .filter(|header| header.starts_with(b"NES\x1A"))
            .ok_or("not an iNES image")?;
        // NES 2.0, unless the ROMs it says there are wouldn't fit in the
        // image: then it's an old header's junk that looks like it.
        let trainer = if header[6] & 0x04 != 0 { TRAINER } else { 0 };
        let nes2 = header[7] & 0x0C == 0x08 && {
            let prg = (header[4] as usize | ((header[9] & 0x0F) as usize) << 8) * PRG_BANK;
            let chr = (header[5] as usize | ((header[9] >> 4) as usize) << 8) * CHR_BANK;
            HEADER + trainer + prg + chr <= image.len()
        };
        let junk = !nes2 && header[12..16].iter().any(|&byte| byte != 0);
        let high = if junk { 0 } else { header[7] & 0xF0 };
        let mut number = (header[6] >> 4) as u16 | high as u16;
        let (mut prg_size, mut chr_size) =
            (header[4] as usize * PRG_BANK, header[5] as usize * CHR_BANK);
        let mut prg_ram_size = 0x2000;
        // Of that, its NVRAM, as NES 2.0 says it: Bandai's EEPROMs go by it.
        let mut nvram = 0;
        let mut chr_ram_size = CHR_BANK;
        // RAM, and NVRAM, each 64 << its shift, if there's any.
        let sizes = |byte: u8| {
            [byte & 0x0F, byte >> 4]
                .iter()
                .map(|&shift| if shift == 0 { 0 } else { 64 << shift })
                .sum::<usize>()
        };
        if nes2 {
            number |= ((header[8] & 0x0F) as u16) << 8;
            prg_size += ((header[9] & 0x0F) as usize) << 8 << 14;
            chr_size += ((header[9] >> 4) as usize) << 8 << 13;
            prg_ram_size = sizes(header[10]);
            nvram = if header[10] >> 4 == 0 {
                0
            } else {
                64 << (header[10] >> 4)
            };
            chr_ram_size = sizes(header[11]);
        }
        let mut submapper = if nes2 { header[8] >> 4 } else { 0 };
        let start = HEADER + trainer;
        // A game the database knows is as it has it. One it's checked is
        // as sure as a NES 2.0 header; one it hasn't only adds RAM.
        let known = image
            .get(start..)
            .and_then(|roms| find(crate::host::zip::crc32(roms)));
        let mut battery = header[6] & 0x02 != 0;
        if let Some(known) = known {
            number = known.mapper;
            submapper = known.submapper.unwrap_or(0);
            (prg_size, chr_size) = (known.prg_rom, known.chr_rom);
            if known.checked() || known.chr_ram > 0 {
                chr_ram_size = known.chr_ram;
            }
            if known.checked() || known.prg_ram > 0 {
                prg_ram_size = known.prg_ram;
            }
            // A header's battery bit is often wrongly set (Super Mario Bros
            // 3 (J)'s, its RAM then saved every second it's played).
            battery = if known.checked() {
                known.battery
            } else {
                battery || known.battery
            };
        }
        let told = nes2 || known.is_some_and(Known::checked);
        let prg_rom = image
            .get(start..start + prg_size)
            .ok_or("its PRG ROM cut short")?
            .to_vec();
        let chr_rom = image
            .get(start + prg_size..start + prg_size + chr_size)
            .ok_or("its CHR ROM cut short")?
            .to_vec();
        if prg_rom.is_empty() {
            return Err("no PRG ROM".to_string());
        }
        // NES 2.0's byte 12: 0 NTSC, 1 PAL, 2 either, 3 a Dendy; an old
        // header's bytes 9 or 10, on the rare one that has them.
        let region = match (nes2, header[12] & 3) {
            (true, 0) => Some(Region::Ntsc),
            (true, 1) => Some(Region::Pal),
            (false, _) if !junk && (header[9] & 1 != 0 || header[10] & 3 == 2) => Some(Region::Pal),
            _ => None,
        };
        let rom_crc = crate::host::zip::crc32(&[&prg_rom[..], &chr_rom[..]].concat());
        if !told && MAPPER_140_GAMES.contains(&rom_crc) {
            number = 140;
        }
        let mmc6 = number == 4 && ((told && submapper == 1) || MMC6_GAMES.contains(&rom_crc));
        if mmc6 && !told {
            prg_ram_size = 0x400;
        }
        if number == 64 && !told {
            prg_ram_size = 0;
        }
        if number == 5 {
            prg_ram_size = mmc5::PRG_RAM;
        }
        // The Namcos' 128 bytes of their own after their RAM, saved with
        // it, as Mesen saves them.
        if matches!(number, 19 | 210) {
            prg_ram_size += CHIP_RAM;
        }
        let mirroring = match header[6] & 0x09 {
            0x00 => Mirroring::Horizontal,
            0x01 => Mirroring::Vertical,
            _ => Mirroring::FourScreen,
        };
        let mirroring = known.and_then(|known| known.mirroring).unwrap_or(mirroring);
        let mapper = match number {
            0 => Mapper::Nrom,
            1 => Mapper::Mmc1(Mmc1::power_on()),
            105 => Mapper::Nwc(Nwc::new()),
            68 => Mapper::Sunsoft4(Sunsoft4::default()),
            2 => Mapper::Uxrom(0),
            3 => Mapper::Cnrom(0),
            4 | 37 | 47 | 118 | 119 | 206 | 76 | 88 | 95 | 154 => Mapper::Mmc3(Mmc3 {
                select: 0,
                banks: [0, 2, 4, 5, 6, 7, 0, 1],
                ram_enabled: true,
                ram_protected: false,
                mmc6,
                halves: 0,
                irq_latch: 0,
                irq_counter: 0,
                irq_reload: false,
                irq_enabled: false,
                irq: false,
                a12_low_since: None,
            }),
            5 => Mapper::Mmc5(Box::new(Mmc5::new())),
            7 => Mapper::Axrom(0),
            // Its CHR there at first, as it'd be with nothing written.
            185 if submapper >= 4 => Mapper::Latch(submapper & 3),
            185 | 93 => Mapper::Latch(1),
            94 | 180 | 13 | 70 | 152 | 89 | 86 | 87 | 140 | 78 | 184 | 77 => Mapper::Latch(0),
            // The last 16K at $C000 too at first, as TAM-S1's is.
            97 => Mapper::Latch(0x0F),
            41 => Mapper::Caltron(Caltron::default()),
            21 | 22 | 23 | 25 => Mapper::Vrc(Vrc::default()),
            75 => Mapper::Vrc1(Vrc1::default()),
            73 => Mapper::Vrc3(Vrc3::default()),
            24 | 26 => Mapper::Vrc6(Box::new(Vrc6::new(number == 26))),
            // An old header's 16 with 512K of PRG and CHR RAM: Famicom Jump
            // II's 153, the only board so, and the only game.
            16 if !told && prg_size == 0x80000 && chr_size == 0 => {
                Mapper::Bandai(Box::new(Bandai::new(153, 0, battery, nvram, told)))
            }
            16 | 153 | 157 | 159 => Mapper::Bandai(Box::new(Bandai::new(
                number, submapper, battery, nvram, told,
            ))),
            19 | 210 => Mapper::Namco163(Box::new(Namco163::new(
                number,
                submapper,
                known.map_or("", |known| known.board),
            ))),
            234 => Mapper::Maxi15(Maxi15::default()),
            72 => Mapper::Jf17(Jf17::default()),
            // The last 16K at $C000 at first, as JF-19's is.
            92 => Mapper::Jf17(Jf17 {
                prg: 0xFF,
                ..Jf17::default()
            }),
            9 | 10 => Mapper::Mmc2(Mmc2 {
                prg: 0,
                chr: [[0; 2]; 2],
                latch: [1; 2],
            }),
            11 if MAPPER_144_GAMES.contains(&rom_crc) => {
                Mapper::ColorDreams(Banks::new(Conflicts::RomD0))
            }
            11 if told && submapper == 2 => Mapper::ColorDreams(Banks::new(Conflicts::And)),
            11 => Mapper::ColorDreams(Banks::new(Conflicts::None)),
            144 => Mapper::ColorDreams(Banks::new(Conflicts::RomD0)),
            34 if submapper == 1 || (submapper != 2 && chr_size > CHR_BANK) => {
                Mapper::Nina001(Nina001::default())
            }
            34 => Mapper::Bnrom(0),
            64 => Mapper::Rambo1(Rambo1::default()),
            69 => Mapper::Fme7(Fme7::default()),
            66 => Mapper::Gxrom(Banks::new(Conflicts::None)),
            71 => Mapper::Camerica(0),
            228 => Mapper::Active(Active::default()),
            232 => Mapper::Quattro(Quattro {
                aladdin: submapper == 1,
                ..Quattro::default()
            }),
            79 => Mapper::Nina03(Banks::new(Conflicts::None)),
            _ => return Err(format!("mapper {number}, which isn't here yet")),
        };
        // Bandai's EEPROMs' bytes, as its RAM, saved as a battery's: it has
        // no other, but Famicom Jump II's 8K.
        if let Mapper::Bandai(bandai) = &mapper {
            if bandai.board == bandai::Board::Jump2 {
                prg_ram_size = prg_ram_size.max(0x2000);
            } else {
                prg_ram_size = bandai.eeprom_bytes();
                battery = prg_ram_size > 0;
            }
        }
        let board = match number {
            118 => Board::Txsrom,
            119 => Board::Tqrom,
            37 => Board::Multicart37,
            47 => Board::Multicart47,
            206 => Board::Namco108,
            76 => Board::Namco3446,
            88 => Board::Namco3443,
            95 => Board::Namco3425,
            154 => Board::Namco3453,
            10 => Board::Mmc4,
            94 => Board::Un1rom,
            180 => Board::Unrom180,
            13 => Board::Cprom,
            185 => Board::Cnrom185(submapper),
            70 => Board::Bandai70,
            152 => Board::Bandai152,
            89 => Board::Sunsoft89,
            93 => Board::Sunsoft93,
            86 => Board::Jf13,
            87 => Board::Jaleco87,
            140 => Board::Jf11,
            78 => Board::Jf16(submapper),
            72 => Board::Jf17,
            92 => Board::Jf19,
            97 => Board::TamS1,
            184 => Board::Sunsoft184,
            77 => Board::Irem77,
            21 | 22 | 23 | 25 => {
                let (wiring, guessed) = Wiring::of(number, submapper);
                Board::Vrc(wiring, guessed)
            }
            _ => Board::Plain,
        };
        // As these start: 77's own four screens; 41's and 234's registers
        // all 0, vertical.
        let mirroring = match number {
            77 => Mirroring::FourScreen,
            41 | 234 => Mirroring::Vertical,
            _ => mirroring,
        };
        if board == Board::Cprom {
            chr_ram_size = chr_ram_size.max(0x4000);
        }
        let has_ram = chr_rom.is_empty() || matches!(board, Board::Tqrom | Board::Irem77);
        let chr_ram = if has_ram {
            vec![0; chr_ram_size.max(CHR_BANK)]
        } else {
            Vec::new()
        };
        Ok(Cartridge {
            prg_rom,
            chr_rom,
            chr_ram,
            // A multicart's outer register, in place of its RAM.
            prg_ram: vec![
                0;
                if board.is_multicart() {
                    1
                } else {
                    prg_ram_size
                }
            ],
            battery,
            written: false,
            mirroring,
            mapper,
            region,
            famicom: known.is_some_and(|known| known.famicom),
            zapper: known.is_some_and(|known| known.zapper),
            board,
            sprite_fetch: false,
            prg_pages: None,
            chr_pages: None,
        })
    }

    /// The Disk System, with the disk whose sides, as in the image `image`,
    /// are `sides`, and `bios`.
    pub fn disk(image: &[u8], sides: &[u8], bios: &[u8]) -> Cartridge {
        let (fds, disk) = Fds::new(sides, crate::host::zip::crc32(image));
        Cartridge {
            prg_rom: bios.to_vec(),
            chr_rom: Vec::new(),
            chr_ram: vec![0; CHR_BANK],
            prg_ram: disk,
            battery: true,
            written: false,
            mirroring: Mirroring::Horizontal,
            mapper: Mapper::Fds(Box::new(fds)),
            region: None,
            famicom: true,
            zapper: false,
            board: Board::Plain,
            sprite_fetch: false,
            prg_pages: None,
            chr_pages: None,
        }
    }

    /// What tells a game from another, if its ROMs don't.
    pub fn disk_crc(&self) -> Option<u32> {
        match &self.mapper {
            Mapper::Fds(fds) => Some(fds.crc),
            _ => None,
        }
    }

    /// The disk changed, if it's a Disk System: what's going in, in words.
    pub fn change_disk(&mut self) -> Option<String> {
        match &mut self.mapper {
            Mapper::Fds(fds) => Some(fds.change_side()),
            _ => None,
        }
    }

    /// Its own sound, added to the APU's.
    pub fn output(&self) -> f32 {
        match &self.mapper {
            Mapper::Fds(fds) => fds.output(),
            Mapper::Namco163(namco) => namco.output(),
            Mapper::Vrc6(vrc6) => vrc6.output(),
            _ => 0.0,
        }
    }

    /// A read of $4020 to $7FFF, `open_bus` what's there of it if it's
    /// nothing, or not all of it.
    pub fn register_read(&mut self, addr: u16, open_bus: u8) -> u8 {
        if let Some(latch) = self.vrc2_latch(addr) {
            return *latch | open_bus & 0xFE;
        }
        match &mut self.mapper {
            Mapper::Fds(fds) if addr < 0x4100 => fds.read(addr, open_bus),
            Mapper::Bandai(bandai) if bandai.board == bandai::Board::Jump2 => match addr {
                0x6000.. if bandai.ram_on => self.prg_ram[addr as usize & 0x1FFF],
                _ => open_bus,
            },
            Mapper::Bandai(bandai) if addr >= 0x6000 => bandai.read(open_bus),
            Mapper::Namco163(namco) if (0x4800..0x6000).contains(&addr) => {
                let chip_ram = self.prg_ram.len() - CHIP_RAM;
                namco
                    .read(addr, &self.prg_ram[chip_ram..])
                    .unwrap_or(open_bus)
            }
            _ => self.cpu_read(addr).unwrap_or(open_bus),
        }
    }

    /// A VRC2's latch, if `addr` is it: a VRC2's, known to be one, with no
    /// RAM, at $6000 up.
    fn vrc2_latch(&mut self, addr: u16) -> Option<&mut u8> {
        match (&mut self.mapper, self.board) {
            (Mapper::Vrc(vrc), Board::Vrc(wiring, false))
                if (0x6000..=0x7FFF).contains(&addr)
                    && !wiring.vrc4()
                    && self.prg_ram.is_empty() =>
            {
                Some(&mut vrc.latch)
            }
            _ => None,
        }
    }

    /// The ROMs taken from `cartridge`, the one a save state's is loaded
    /// into.
    pub fn take_roms(&mut self, cartridge: &mut Cartridge) {
        self.prg_rom = std::mem::take(&mut cartridge.prg_rom);
        self.chr_rom = std::mem::take(&mut cartridge.chr_rom);
        self.board = cartridge.board;
    }

    /// MMC3's 8K bank, as a multicart's register has it: bit 3 of it A16,
    /// bit 4 A17.
    fn prg_block(&self, bank: usize) -> usize {
        match self.board {
            Board::Multicart37 => {
                let register = self.prg_ram[0] as usize;
                let (block, low_bits) = ((register >> 2) & 1, register & 3);
                let a16 = if low_bits == 3 {
                    1
                } else {
                    block & (bank >> 3)
                };
                (bank & 7) | a16 << 3 | block << 4
            }
            Board::Multicart47 => (bank & 0x0F) | (self.prg_ram[0] as usize & 1) << 4,
            _ => bank,
        }
    }

    /// Reset: a multicart's outer register cleared, back to its menu.
    pub fn reset(&mut self) {
        self.prg_pages = None;
        self.chr_pages = None;
        if self.board.is_multicart() {
            self.prg_ram[0] = 0;
        }
        if let Mapper::Mmc5(mmc5) = &mut self.mapper {
            mmc5.reset();
        }
        if let Mapper::Active(active) = &mut self.mapper {
            *active = Active::default();
        }
        if let Mapper::Caltron(caltron) = &mut self.mapper {
            *caltron = Caltron::default();
            self.mirroring = Mirroring::Vertical;
        }
    }

    /// Where `addr`, $8000 up, is in the PRG ROM, as the mapper has its
    /// banks.
    fn prg_offset(&self, addr: u16) -> usize {
        let addr = addr as usize;
        let len = self.prg_rom.len();
        let last_16k = len / PRG_BANK - 1;
        let in_16k = |bank: usize| bank * PRG_BANK + (addr & 0x3FFF);
        let offset = match &self.mapper {
            Mapper::Nrom | Mapper::Cnrom(_) => addr - 0x8000,
            Mapper::Latch(_)
                if matches!(
                    self.board,
                    Board::Cprom | Board::Cnrom185(_) | Board::Jaleco87 | Board::Sunsoft184
                ) =>
            {
                addr - 0x8000
            }
            Mapper::Latch(latch) if self.board == Board::Irem77 => {
                (*latch as usize & 0x0F) * 0x8000 + (addr & 0x7FFF)
            }
            Mapper::Caltron(caltron) => caltron.prg as usize * 0x8000 + (addr & 0x7FFF),
            Mapper::Vrc(vrc) => vrc.prg_bank(addr as u16, len / 0x2000) * 0x2000 + (addr & 0x1FFF),
            Mapper::Vrc1(vrc1) => {
                vrc1.prg_bank(addr as u16, len / 0x2000) * 0x2000 + (addr & 0x1FFF)
            }
            Mapper::Namco163(namco) => {
                namco.prg_bank(addr as u16, len / 0x2000) * 0x2000 + (addr & 0x1FFF)
            }
            Mapper::Vrc6(vrc6) => {
                vrc6.prg_bank(addr as u16, len / 0x2000) * 0x2000 + (addr & 0x1FFF)
            }
            Mapper::Bandai(bandai) => in_16k(bandai.prg_bank(addr as u16, len / PRG_BANK)),
            Mapper::Vrc3(vrc3) => in_16k(if addr < 0xC000 {
                vrc3.prg_bank()
            } else {
                last_16k
            }),
            Mapper::Maxi15(maxi) => maxi.banks().0 * 0x8000 + (addr & 0x7FFF),
            Mapper::Latch(latch) if matches!(self.board, Board::Jf13 | Board::Jf11) => {
                ((*latch as usize >> 4) & 3) * 0x8000 + (addr & 0x7FFF)
            }
            Mapper::Jf17(jf) if addr >= 0xC000 => in_16k(match self.board {
                Board::Jf19 => jf.prg as usize,
                _ => last_16k,
            }),
            Mapper::Jf17(jf) => in_16k(match self.board {
                Board::Jf19 => 0,
                _ => jf.prg as usize & 7,
            }),
            Mapper::Latch(latch) if addr >= 0xC000 => in_16k(match self.board {
                Board::Unrom180 => *latch as usize & 7,
                Board::TamS1 => *latch as usize & 0x0F,
                _ => last_16k,
            }),
            Mapper::Latch(latch) => in_16k(match self.board {
                Board::Un1rom => (*latch as usize >> 2) & 7,
                Board::Unrom180 => 0,
                Board::Jf16(_) => *latch as usize & 7,
                Board::TamS1 => last_16k,
                _ => (*latch as usize >> 4) & 7,
            }),
            Mapper::Uxrom(bank) | Mapper::Camerica(bank) => in_16k(if addr < 0xC000 {
                *bank as usize
            } else {
                last_16k
            }),
            Mapper::Axrom(bank) | Mapper::Bnrom(bank) => {
                (*bank as usize) * 0x8000 + (addr & 0x7FFF)
            }
            Mapper::ColorDreams(banks) | Mapper::Nina03(banks) | Mapper::Gxrom(banks) => {
                banks.prg as usize * 0x8000 + (addr & 0x7FFF)
            }
            Mapper::Nina001(nina) => nina.prg as usize * 0x8000 + (addr & 0x7FFF),
            Mapper::Rambo1(rambo) => {
                let last_8k = len / 0x2000 - 1;
                let swapped = rambo.select & 0x40 != 0;
                let bank = match ((addr - 0x8000) / 0x2000, swapped) {
                    (0, false) | (2, true) => rambo.banks[6] as usize,
                    (0, true) | (2, false) => rambo.banks[15] as usize,
                    (1, _) => rambo.banks[7] as usize,
                    _ => last_8k,
                };
                bank * 0x2000 + (addr & 0x1FFF)
            }
            Mapper::Mmc5(_) | Mapper::Fds(_) => addr,
            Mapper::Active(active) => {
                let chip = [0, 1, 0, 2][active.chip as usize] * 0x80000;
                let high = (addr >= 0xC000) as usize;
                let bank = if active.one_bank {
                    active.bank as usize
                } else {
                    (active.bank as usize & !1) | high
                };
                chip + in_16k(bank)
            }
            Mapper::Quattro(quattro) => {
                let block = quattro.block as usize * 4;
                in_16k(
                    block
                        + if addr < 0xC000 {
                            quattro.bank as usize
                        } else {
                            3
                        },
                )
            }
            Mapper::Fme7(fme7) => {
                let last_8k = len / 0x2000 - 1;
                let bank = fme7
                    .prg
                    .get((addr - 0x8000) / 0x2000)
                    .map_or(last_8k, |&bank| bank as usize & 0x3F);
                bank * 0x2000 + (addr & 0x1FFF)
            }
            Mapper::Mmc2(mmc2) if self.board == Board::Mmc4 && addr < 0xC000 => {
                mmc2.prg as usize * PRG_BANK + (addr & 0x3FFF)
            }
            Mapper::Mmc2(_) if self.board == Board::Mmc4 => len - PRG_BANK + (addr - 0xC000),
            Mapper::Mmc2(mmc2) if addr < 0xA000 => mmc2.prg as usize * 0x2000 + (addr & 0x1FFF),
            Mapper::Mmc2(_) => len - 0x6000 + (addr - 0xA000),
            Mapper::Nwc(nwc) if nwc.started < 2 => addr - 0x8000,
            Mapper::Nwc(nwc) if nwc.mmc1.chr[0] & 0x08 == 0 => {
                (nwc.mmc1.chr[0] as usize >> 1 & 3) * 0x8000 + (addr & 0x7FFF)
            }
            Mapper::Nwc(nwc) => {
                let bank = (nwc.mmc1.prg as usize & 7) | 8;
                match (nwc.mmc1.control >> 2) & 3 {
                    0 | 1 => (bank & !1) * PRG_BANK + (addr & 0x7FFF),
                    2 => in_16k(if addr < 0xC000 { 8 } else { bank }),
                    _ => in_16k(if addr < 0xC000 { bank } else { 0x0F }),
                }
            }
            Mapper::Sunsoft4(sunsoft) if addr < 0xC000 => in_16k(sunsoft.prg as usize),
            // Its own last: an option cartridge's is after it.
            Mapper::Sunsoft4(_) => in_16k(last_16k.min(7)),
            Mapper::Mmc1(mmc1) => {
                // On 512K boards, CHR bank 0's bit 4 picks which 256K.
                let outer = if len > 0x40000 {
                    mmc1.chr[0] as usize & 0x10
                } else {
                    0
                };
                let bank = (mmc1.prg as usize & 0x0F) | outer;
                match (mmc1.control >> 2) & 3 {
                    0 | 1 => (bank & !1) * PRG_BANK + (addr & 0x7FFF),
                    2 => in_16k(if addr < 0xC000 { outer } else { bank }),
                    _ => in_16k(if addr < 0xC000 { bank } else { 0x0F | outer }),
                }
            }
            Mapper::Mmc3(mmc3) => {
                let last_8k = len / 0x2000 - 1;
                let swapped = mmc3.select & 0x40 != 0;
                let bank = match ((addr - 0x8000) / 0x2000, swapped) {
                    (0, false) | (2, true) => mmc3.banks[6] as usize,
                    (0, true) | (2, false) => last_8k - 1,
                    (1, _) => mmc3.banks[7] as usize,
                    _ => last_8k,
                };
                self.prg_block(bank) * 0x2000 + (addr & 0x1FFF)
            }
        };
        offset % len
    }

    /// Where `addr`, $0000 to $1FFF, is in its CHR, as the mapper has its
    /// banks.
    fn chr_offset(&mut self, addr: u16) -> usize {
        let addr = addr as usize;
        if let Mapper::Mmc5(mmc5) = &mut self.mapper {
            let chr = if self.chr_rom.is_empty() {
                self.chr_ram.len()
            } else {
                self.chr_rom.len()
            };
            return mmc5.chr_offset(addr, self.sprite_fetch) % chr;
        }
        let offset = match &self.mapper {
            Mapper::Nrom
            | Mapper::Uxrom(_)
            | Mapper::Axrom(_)
            | Mapper::Camerica(_)
            | Mapper::Bnrom(_)
            | Mapper::Quattro(_)
            | Mapper::Mmc5(_)
            | Mapper::Fds(_)
            | Mapper::Vrc3(_)
            | Mapper::Nwc(_) => addr,
            Mapper::Sunsoft4(sunsoft) => sunsoft.chr[addr >> 11] as usize * 0x800 + (addr & 0x7FF),
            Mapper::Cnrom(bank) => (*bank as usize) * CHR_BANK + addr,
            Mapper::Jf17(jf) => jf.chr as usize * CHR_BANK + addr,
            Mapper::Caltron(caltron) => caltron.chr as usize * CHR_BANK + addr,
            Mapper::Vrc(vrc) => match self.board {
                Board::Vrc(wiring, _) => vrc.chr_bank(addr as u16, wiring) * 0x400 + (addr & 0x3FF),
                _ => addr,
            },
            Mapper::Maxi15(maxi) => maxi.banks().1 * CHR_BANK + addr,
            Mapper::Vrc1(vrc1) => vrc1.chr_bank(addr as u16) * 0x1000 + (addr & 0x0FFF),
            Mapper::Namco163(namco) => namco.chr_bank(addr as u16) * 0x400 + (addr & 0x3FF),
            Mapper::Vrc6(vrc6) => vrc6.chr_bank(addr as u16) * 0x400 + (addr & 0x3FF),
            Mapper::Bandai(bandai) => bandai.chr_offset(addr as u16),
            Mapper::Latch(latch) => {
                let latch = *latch as usize;
                match self.board {
                    Board::Cprom if addr >= 0x1000 => (latch & 3) * 0x1000 + (addr & 0x0FFF),
                    Board::Bandai70 | Board::Bandai152 => (latch & 0x0F) * CHR_BANK + addr,
                    Board::Sunsoft89 => ((latch & 7) | (latch & 0x80) >> 4) * CHR_BANK + addr,
                    Board::Jf13 => ((latch & 3) | (latch >> 4) & 4) * CHR_BANK + addr,
                    Board::Jaleco87 => ((latch & 1) << 1 | (latch & 2) >> 1) * CHR_BANK + addr,
                    Board::Jf11 => (latch & 0x0F) * CHR_BANK + addr,
                    Board::Jf16(_) => (latch >> 4) * CHR_BANK + addr,
                    Board::Sunsoft184 if addr < 0x1000 => (latch & 7) * 0x1000 + addr,
                    Board::Sunsoft184 => (0x80 | (latch >> 4) & 7) * 0x1000 + (addr & 0x0FFF),
                    Board::Irem77 if addr < 0x0800 => (latch >> 4) * 0x0800 + addr,
                    Board::Irem77 => addr - 0x0800,
                    _ => addr,
                }
            }
            Mapper::Active(active) => active.chr as usize * CHR_BANK + addr,
            Mapper::ColorDreams(banks) | Mapper::Nina03(banks) | Mapper::Gxrom(banks) => {
                banks.chr as usize * CHR_BANK + addr
            }
            Mapper::Nina001(nina) => nina.chr[addr >> 12] as usize * 0x1000 + (addr & 0x0FFF),
            Mapper::Rambo1(rambo) => rambo.chr_bank(addr) * 0x400 + (addr & 0x3FF),
            Mapper::Fme7(fme7) => fme7.chr[addr >> 10] as usize * 0x400 + (addr & 0x3FF),
            Mapper::Mmc2(mmc2) => {
                let table = addr >> 12;
                mmc2.chr[table][mmc2.latch[table]] as usize * 0x1000 + (addr & 0x0FFF)
            }
            Mapper::Mmc1(mmc1) => match mmc1.control & 0x10 {
                0 => (mmc1.chr[0] as usize & 0x1E) * 0x1000 + addr,
                _ => mmc1.chr[addr >> 12] as usize * 0x1000 + (addr & 0x0FFF),
            },
            Mapper::Mmc3(mmc3) if self.board == Board::Namco3446 => {
                mmc3.banks[2 + (addr >> 11)] as usize * 0x800 + (addr & 0x7FF)
            }
            Mapper::Mmc3(mmc3) => {
                let bank = mmc3.chr_bank(addr);
                let bank = match self.board {
                    Board::Namco3443 | Board::Namco3453 if addr < 0x1000 => bank & 0x3F,
                    Board::Namco3443 | Board::Namco3453 => bank | 0x40,
                    Board::Tqrom => bank & 0x3F,
                    Board::Multicart37 => (bank & 0x7F) | (self.prg_ram[0] as usize & 4) << 5,
                    Board::Multicart47 => (bank & 0x7F) | (self.prg_ram[0] as usize & 1) << 7,
                    _ => bank,
                };
                bank * 0x400 + (addr & 0x3FF)
            }
        };
        let chr = if self.chr_in_ram(addr as u16) {
            &self.chr_ram
        } else {
            &self.chr_rom
        };
        offset % chr.len()
    }

    /// Whether CHR address `addr` is in its CHR RAM.
    fn chr_in_ram(&self, addr: u16) -> bool {
        match &self.mapper {
            _ if self.chr_rom.is_empty() => true,
            Mapper::Mmc3(mmc3) if self.board == Board::Tqrom => {
                mmc3.chr_bank(addr as usize) & 0x40 != 0
            }
            Mapper::Latch(_) if self.board == Board::Irem77 => addr >= 0x0800,
            _ => false,
        }
    }

    /// Where `addr`, $6000 to $7FFF, is in its PRG RAM, if it's there to
    /// read, or `writing`, to write.
    fn prg_ram_offset(&self, addr: u16, writing: bool) -> Option<usize> {
        if self.prg_ram.is_empty() {
            return None;
        }
        let addr = addr as usize - 0x6000;
        match &self.mapper {
            Mapper::Mmc1(mmc1) if mmc1.prg & 0x10 != 0 => None,
            Mapper::Nwc(nwc) if nwc.mmc1.prg & 0x10 != 0 => None,
            Mapper::Sunsoft4(sunsoft) if !sunsoft.ram_on => None,
            Mapper::Vrc6(vrc6) if !vrc6.ram_on() => None,
            Mapper::Mmc1(mmc1) if self.prg_ram.len() == 0x8000 => {
                Some(((mmc1.chr[0] as usize >> 2) & 3) * 0x2000 + addr)
            }
            Mapper::Mmc3(mmc3) if !mmc3.ram_enabled || (writing && mmc3.ram_protected) => None,
            Mapper::Fme7(fme7) if fme7.window & 0xC0 != 0xC0 => None,
            _ => Some(addr % self.prg_ram.len()),
        }
    }

    /// The MMC6's RAM, `addr` $6000 to $7FFF, read or `writing`: where it
    /// is, if it's there, or 0 for a half not read; none if neither is.
    fn mmc6_ram(&self, mmc3: &Mmc3, addr: u16, writing: bool) -> Option<Option<usize>> {
        if addr < 0x7000 || mmc3.select & 0x20 == 0 || mmc3.halves & 0xA0 == 0 {
            return None;
        }
        let half = (addr >> 9) & 1;
        let (read, write) = (
            mmc3.halves & (0x20 << (2 * half)) != 0,
            mmc3.halves & (0x10 << (2 * half)) != 0,
        );
        let at = addr as usize & 0x3FF;
        Some((read && (write || !writing)).then_some(at))
    }

    /// A read of $4020 to $FFFF: what the cartridge drives on the bus, if it
    /// drives anything.
    pub fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        // Not the vectors: for the MMC5, reading those is an event.
        if let (0x8000..0xFFFA, Some(pages)) = (addr, &self.prg_pages) {
            return Some(
                self.prg_rom[pages[addr as usize >> 12 & 7] as usize + (addr as usize & 0xFFF)],
            );
        }
        if let Mapper::Mmc5(mmc5) = &mut self.mapper {
            let value = mmc5.cpu_read(addr, &self.prg_rom, &self.prg_ram);
            self.prg_pages = mmc5.rom_pages(self.prg_rom.len());
            return value;
        }
        if let Mapper::Fds(fds) = &self.mapper {
            return match addr {
                0x6000..=0xDFFF => Some(fds.ram(addr)),
                0xE000..=0xFFFF => Some(self.prg_rom[addr as usize - 0xE000]),
                _ => None,
            };
        }
        match addr {
            0x6000..=0x7FFF => match &self.mapper {
                _ if self.board.is_multicart() => None,
                Mapper::Mmc3(mmc3) if mmc3.mmc6 => self
                    .mmc6_ram(mmc3, addr, false)
                    .map(|at| at.map_or(0, |at| self.prg_ram[at])),
                Mapper::Fme7(fme7) if fme7.window & 0x40 == 0 => {
                    let at = (fme7.window as usize & 0x3F) * 0x2000 + (addr as usize & 0x1FFF);
                    Some(self.prg_rom[at % self.prg_rom.len()])
                }
                Mapper::Namco163(namco) => namco
                    .ram_offset(addr, self.prg_ram.len() - CHIP_RAM, false)
                    .map(|at| self.prg_ram[at]),
                _ => self.prg_ram_offset(addr, false).map(|at| self.prg_ram[at]),
            },
            0x8000..=0xFFFF => match &self.mapper {
                Mapper::Active(active) if active.chip == ACTIVE_NO_CHIP => None,
                // An option cartridge's, its pages not kept: its licence
                // lapses with no write.
                Mapper::Sunsoft4(sunsoft) if sunsoft.external => (addr >= 0xC000
                    || !sunsoft.lapsed())
                .then(|| self.prg_rom[self.prg_offset(addr)]),
                // Its registers read as written, its pages not to be kept
                // for reads to miss them.
                Mapper::Maxi15(_) => {
                    let value = self.prg_rom[self.prg_offset(addr)];
                    if let Mapper::Maxi15(maxi) = &mut self.mapper {
                        if let Some(mirroring) = maxi.access(addr, value) {
                            self.mirroring = mirroring;
                            self.chr_pages = None; // a write's would be too
                        }
                    }
                    Some(value)
                }
                _ => {
                    if self.prg_rom.len() % 0x1000 == 0 {
                        self.prg_pages = Some(std::array::from_fn(|page| {
                            self.prg_offset(0x8000 + page as u16 * 0x1000) as u32
                        }));
                    }
                    Some(self.prg_rom[self.prg_offset(addr)])
                }
            },
            _ => None,
        }
    }

    /// A write to $4020 to $FFFF, on CPU cycle `cycle`: its PRG RAM, or its
    /// mapper's registers.
    pub fn cpu_write(&mut self, addr: u16, value: u8, cycle: u64) {
        self.prg_pages = None;
        self.chr_pages = None;
        if let Mapper::Mmc5(mmc5) = &mut self.mapper {
            self.written |= mmc5.cpu_write(addr, value, &mut self.prg_ram) && self.battery;
            return;
        }
        if let Mapper::Fds(fds) = &mut self.mapper {
            match addr {
                0x4020..=0x40FF => {
                    fds.write(addr, value);
                    self.mirroring = fds.mirroring();
                }
                0x6000..=0xDFFF => fds.write_ram(addr, value),
                _ => {}
            }
            return;
        }
        // Bandai's registers, where its board has them; its RAM, its
        // EEPROMs', not at $6000 up, only talked to through them.
        if let Mapper::Bandai(bandai) = &mut self.mapper {
            if bandai.registers_at(addr) {
                let (mirroring, written) = bandai.write(addr, value, &mut self.prg_ram);
                if let Some(mirroring) = mirroring {
                    self.mirroring = mirroring;
                }
                self.written |= written && self.battery;
            } else if (0x6000..0x8000).contains(&addr) && bandai.ram_on {
                self.prg_ram[addr as usize & 0x1FFF] = value;
                self.written |= self.battery;
            }
            return;
        }
        // A Namco's registers, from $4800 up but for its RAM. Its own 128
        // bytes, written, are saved only if they're all the RAM there is:
        // a game with more keeps its saves there, and the sound writes
        // these every frame.
        if let Mapper::Namco163(namco) = &mut self.mapper {
            if (0x4800..0x6000).contains(&addr) || addr >= 0x8000 {
                let chip_ram = self.prg_ram.len() - CHIP_RAM;
                if let Some(mirroring) = namco.write(addr, value, &mut self.prg_ram[chip_ram..]) {
                    self.mirroring = mirroring;
                }
                self.written |= self.battery && chip_ram == 0 && addr & 0xF800 == 0x4800;
                return;
            }
        }
        if let (0x4020..=0x5FFF, Mapper::Nina03(banks)) = (addr, &mut self.mapper) {
            if addr & 0xE100 == 0x4100 {
                (banks.prg, banks.chr) = ((value >> 3) & 1, value & 7);
            }
            return;
        }
        if let Some(latch) = self.vrc2_latch(addr) {
            *latch = value & 1;
            return;
        }
        if let (0x6000..=0x67FF, Mapper::Caltron(caltron)) = (addr, &mut self.mapper) {
            caltron.prg = addr as u8 & 7;
            caltron.chr = (caltron.chr & 3) | (addr >> 1) as u8 & 0x0C;
            self.mirroring = if addr & 0x20 != 0 {
                Mirroring::Horizontal
            } else {
                Mirroring::Vertical
            };
            return;
        }
        if let (0x6000..=0x7FFF, Mapper::Latch(latch)) = (addr, &mut self.mapper) {
            if self.board.register_at_6000() {
                // JF-13's sound at $7000 up, not here.
                if !(self.board == Board::Jf13 && addr >= 0x7000) {
                    *latch = value;
                }
                return;
            }
        }
        if let 0x6000..=0x7FFF = addr {
            if let Mapper::Nina001(nina) = &mut self.mapper {
                match addr {
                    0x7FFD => nina.prg = value & 1,
                    0x7FFE | 0x7FFF => nina.chr[addr as usize & 1] = value & 0x0F,
                    _ => {}
                }
            }
            if let Mapper::Namco163(namco) = &mut self.mapper {
                namco.ram_written();
            }
            if let Mapper::Sunsoft4(sunsoft) = &mut self.mapper {
                if !sunsoft.ram_on {
                    sunsoft.licence = SUNSOFT4_LICENCE;
                }
            }
            let at = match &self.mapper {
                Mapper::Mmc3(mmc3) if mmc3.mmc6 => self.mmc6_ram(mmc3, addr, true).flatten(),
                Mapper::Namco163(namco) => {
                    namco.ram_offset(addr, self.prg_ram.len() - CHIP_RAM, true)
                }
                _ => self.prg_ram_offset(addr, true),
            };
            if let Some(at) = at {
                self.prg_ram[at] = value;
                self.written |= self.battery;
            }
            return;
        }
        if addr < 0x8000 {
            return;
        }
        let four_screen = self.mirroring == Mirroring::FourScreen;
        let rom = self.prg_rom[self.prg_offset(addr)];
        let banks = self.prg_rom.len() / PRG_BANK;
        match &mut self.mapper {
            Mapper::Nrom
            | Mapper::Nina03(_)
            | Mapper::Nina001(_)
            | Mapper::Mmc5(_)
            | Mapper::Fds(_)
            | Mapper::Namco163(_)
            | Mapper::Bandai(_) => {}
            Mapper::Uxrom(bank) | Mapper::Cnrom(bank) => *bank = value,
            Mapper::Latch(_) if self.board.register_at_6000() => {}
            Mapper::Vrc(vrc) => {
                if let Board::Vrc(wiring, guessed) = self.board {
                    let mirroring = vrc.write(addr, value, wiring, guessed);
                    if let Some(mirroring) = mirroring.filter(|_| !four_screen) {
                        self.mirroring = mirroring;
                    }
                }
            }
            Mapper::Vrc1(vrc1) => {
                if let Some(mirroring) = vrc1.write(addr, value).filter(|_| !four_screen) {
                    self.mirroring = mirroring;
                }
            }
            Mapper::Vrc3(vrc3) => vrc3.write(addr, value),
            Mapper::Vrc6(vrc6) => vrc6.write(addr, value),
            Mapper::Caltron(caltron) => {
                if caltron.prg >= 4 {
                    caltron.chr = (caltron.chr & 0x0C) | (value & 3);
                }
            }
            Mapper::Maxi15(maxi) => {
                if let Some(mirroring) = maxi.access(addr, value & rom) {
                    self.mirroring = mirroring;
                }
            }
            Mapper::Jf17(jf) => {
                let value = value & rom;
                if value & 0x80 != 0 && jf.last & 0x80 == 0 {
                    jf.prg = value & 0x0F;
                }
                if value & 0x40 != 0 && jf.last & 0x40 == 0 {
                    jf.chr = value & 0x0F;
                }
                jf.last = value;
            }
            Mapper::Latch(latch) => {
                let value = match self.board {
                    Board::Cnrom185(_) | Board::Jf16(_) | Board::Irem77 => value & rom,
                    _ => value,
                };
                *latch = value;
                if self.board == Board::TamS1 {
                    self.mirroring = [
                        Mirroring::SingleLower,
                        Mirroring::Horizontal,
                        Mirroring::Vertical,
                        Mirroring::SingleUpper,
                    ][value as usize >> 6];
                }
                if self.board == Board::Jf16(3) {
                    self.mirroring = if value & 0x08 != 0 {
                        Mirroring::Vertical
                    } else {
                        Mirroring::Horizontal
                    };
                }
                let upper = match self.board {
                    Board::Bandai152 => Some(value & 0x80 != 0),
                    Board::Sunsoft89 | Board::Jf16(_) if self.board != Board::Jf16(3) => {
                        Some(value & 0x08 != 0)
                    }
                    _ => None,
                };
                if let Some(upper) = upper {
                    self.mirroring = if upper {
                        Mirroring::SingleUpper
                    } else {
                        Mirroring::SingleLower
                    };
                }
            }
            Mapper::Axrom(bank) => {
                *bank = value & 7;
                self.mirroring = if value & 0x10 != 0 {
                    Mirroring::SingleUpper
                } else {
                    Mirroring::SingleLower
                };
            }
            Mapper::ColorDreams(banks) => {
                let value = banks.conflicts.on(value, rom);
                (banks.prg, banks.chr) = (value & 3, value >> 4);
            }
            Mapper::Gxrom(banks) => {
                let value = banks.conflicts.on(value, rom);
                (banks.prg, banks.chr) = ((value >> 4) & 3, value & 3);
            }
            Mapper::Bnrom(bank) => *bank = Conflicts::And.on(value, rom),
            Mapper::Rambo1(rambo) => match addr & 0xE001 {
                0x8000 => rambo.select = value,
                0x8001 => rambo.banks[rambo.select as usize & 0x0F] = value,
                0xA000 if !four_screen => {
                    self.mirroring = if value & 1 == 0 {
                        Mirroring::Vertical
                    } else {
                        Mirroring::Horizontal
                    };
                }
                0xC000 => rambo.irq_latch = value,
                0xC001 => {
                    rambo.counting_cycles = value & 1 != 0;
                    (rambo.irq_counter, rambo.irq_reload, rambo.prescaler) = (0, true, 0);
                }
                0xE000 => (rambo.irq_enabled, rambo.irq, rambo.irq_delay) = (false, false, 0),
                0xE001 => rambo.irq_enabled = true,
                _ => {}
            },
            Mapper::Active(active) => {
                let bits = |shift: u16, mask: u16| ((addr >> shift) & mask) as u8;
                (active.chip, active.bank, active.one_bank) =
                    (bits(11, 3), bits(6, 0x1F), bits(5, 1) != 0);
                active.chr = bits(0, 0x0F) << 2 | value & 3;
                self.mirroring = if bits(13, 1) == 0 {
                    Mirroring::Vertical
                } else {
                    Mirroring::Horizontal
                };
            }
            Mapper::Quattro(quattro) => match addr {
                0x8000..=0xBFFF => {
                    let block = (value >> 3) & 3;
                    quattro.block = if quattro.aladdin {
                        (block & 1) << 1 | block >> 1
                    } else {
                        block
                    };
                }
                _ => quattro.bank = value & 3,
            },
            Mapper::Fme7(fme7) => match addr {
                0x8000..=0x9FFF => fme7.command = value & 0x0F,
                0xA000..=0xBFFF => match fme7.command {
                    command @ 0..=7 => fme7.chr[command as usize] = value,
                    8 => fme7.window = value,
                    command @ 9..=0xB => fme7.prg[command as usize - 9] = value,
                    0xC => {
                        self.mirroring = [
                            Mirroring::Vertical,
                            Mirroring::Horizontal,
                            Mirroring::SingleLower,
                            Mirroring::SingleUpper,
                        ][value as usize & 3];
                    }
                    0xD => {
                        (fme7.counting, fme7.irq_enabled) = (value & 0x80 != 0, value & 1 != 0);
                        fme7.irq = false;
                    }
                    0xE => fme7.counter = (fme7.counter & 0xFF00) | value as u16,
                    _ => fme7.counter = (fme7.counter & 0x00FF) | (value as u16) << 8,
                },
                _ => {}
            },
            Mapper::Mmc2(mmc2) => match addr >> 12 {
                0xA => mmc2.prg = value & 0x0F,
                0xB..=0xE => {
                    let register = (addr >> 12) as usize - 0xB;
                    mmc2.chr[register / 2][register % 2] = value & 0x1F;
                }
                0xF if !four_screen => {
                    self.mirroring = if value & 1 == 0 {
                        Mirroring::Vertical
                    } else {
                        Mirroring::Horizontal
                    };
                }
                _ => {}
            },
            Mapper::Camerica(bank) => match addr {
                0x9000..=0x9FFF => {
                    self.mirroring = if value & 0x10 != 0 {
                        Mirroring::SingleUpper
                    } else {
                        Mirroring::SingleLower
                    };
                }
                0xC000.. => *bank = value & 0x0F,
                _ => {}
            },
            Mapper::Mmc1(mmc1) => {
                if let (_, Some(mirroring)) = mmc1.write(addr, value, cycle) {
                    self.mirroring = mirroring;
                }
            }
            Mapper::Nwc(nwc) => {
                let (filled, mirroring) = nwc.mmc1.write(addr, value, cycle);
                if let Some(mirroring) = mirroring {
                    self.mirroring = mirroring;
                }
                if filled {
                    nwc.written();
                }
            }
            Mapper::Sunsoft4(sunsoft) => match addr >> 12 {
                register @ 0x8..=0xB => sunsoft.chr[register as usize - 8] = value,
                register @ (0xC | 0xD) => sunsoft.tables[register as usize - 0xC] = value,
                0xE => {
                    self.mirroring = [
                        Mirroring::Vertical,
                        Mirroring::Horizontal,
                        Mirroring::SingleLower,
                        Mirroring::SingleUpper,
                    ][value as usize & 3];
                    sunsoft.chr_tables = value & 0x10 != 0;
                }
                _ => {
                    // Bit 3 clear, the option cartridge's, if there's one.
                    sunsoft.external = banks > 8 && value & 0x08 == 0;
                    sunsoft.prg = if sunsoft.external {
                        8 + (value & 7) % (banks - 8) as u8
                    } else {
                        value & 7
                    };
                    sunsoft.ram_on = value & 0x10 != 0;
                }
            },
            Mapper::Mmc3(mmc3) if self.board.is_namco_108() => {
                if self.board == Board::Namco3453 {
                    self.mirroring = if value & 0x40 != 0 {
                        Mirroring::SingleUpper
                    } else {
                        Mirroring::SingleLower
                    };
                }
                match addr & 1 {
                    0 => mmc3.select = value & 0x07,
                    _ => mmc3.banks[mmc3.select as usize] = value,
                }
            }
            Mapper::Mmc3(mmc3) => match addr & 0xE001 {
                0x8000 => mmc3.select = value,
                0x8001 => mmc3.banks[mmc3.select as usize & 7] = value,
                0xA000 if !four_screen => {
                    self.mirroring = if value & 1 == 0 {
                        Mirroring::Vertical
                    } else {
                        Mirroring::Horizontal
                    };
                }
                0xA001 if mmc3.mmc6 => {
                    if mmc3.select & 0x20 != 0 {
                        mmc3.halves = value & 0xF0;
                    }
                }
                0xA001 => {
                    mmc3.ram_enabled = value & 0x80 != 0;
                    mmc3.ram_protected = value & 0x40 != 0;
                }
                0xC000 => mmc3.irq_latch = value,
                0xC001 => {
                    mmc3.irq_counter = 0;
                    mmc3.irq_reload = true;
                }
                0xE000 => {
                    mmc3.irq_enabled = false;
                    mmc3.irq = false;
                }
                0xE001 => mmc3.irq_enabled = true,
                _ => {}
            },
        }
    }

    /// Which 1K of the PPU's nametable RAM, 0 to 3, PPU address `addr` is
    /// in: as the board's mirroring wires it, or on TxSROM, the top bit of
    /// the CHR bank that address falls in.
    pub fn nametable_page(&self, addr: u16) -> usize {
        match &self.mapper {
            Mapper::Mmc3(mmc3) if self.board == Board::Txsrom => {
                mmc3.chr_bank(addr as usize & 0x0FFF) >> 7
            }
            Mapper::Mmc3(mmc3) if self.board == Board::Namco3425 => {
                (mmc3.banks[(addr as usize >> 11) & 1] as usize >> 5) & 1
            }
            Mapper::Mmc5(mmc5) => mmc5.nametable_page(addr),
            Mapper::Vrc6(vrc6) => vrc6.table(addr) & 1,
            Mapper::Namco163(namco) => match namco.nametable(addr) {
                Some(Ok(page)) => page,
                _ => self.mirroring.page(addr),
            },
            _ => self.mirroring.page(addr),
        }
    }

    /// The PPU fetching this cycle, whatever's been seen of it.
    #[inline]
    pub fn fetching(&mut self) {
        if let Mapper::Mmc5(mmc5) = &mut self.mapper {
            mmc5.fetching();
        }
    }

    /// Whether its mapper's to see each of the PPU's fetches, not just that
    /// there were some.
    pub fn watches_fetches(&self) -> bool {
        match &self.mapper {
            Mapper::Mmc2(_) => true,
            Mapper::Mmc5(mmc5) => !mmc5.plain_fetches(),
            Mapper::Namco163(namco) => namco.moves_tables(),
            // Its nametables CHR ROM, not what the PPU has.
            Mapper::Sunsoft4(sunsoft) => sunsoft.chr_tables,
            Mapper::Vrc6(vrc6) => vrc6.rom_tables(),
            _ => false,
        }
    }

    /// Whether its mapper counts the lines by what's fetched.
    pub fn counts_lines(&self) -> bool {
        matches!(self.mapper, Mapper::Mmc5(_))
    }

    /// A PPU read of a nametable, if its mapper answers it, not the PPU's
    /// RAM.
    pub fn nametable_read(&mut self, addr: u16) -> Option<u8> {
        match &mut self.mapper {
            Mapper::Mmc5(mmc5) => mmc5.nametable_read(addr),
            Mapper::Vrc6(vrc6) if vrc6.rom_tables() => {
                let chr = if self.chr_rom.is_empty() {
                    &self.chr_ram
                } else {
                    &self.chr_rom
                };
                Some(chr[(vrc6.table(addr) * 0x400 + (addr as usize & 0x3FF)) % chr.len()])
            }
            Mapper::Sunsoft4(sunsoft) if sunsoft.chr_tables => {
                let table = sunsoft.tables[self.mirroring.page(addr) & 1] as usize | 0x80;
                let at = table * 0x400 + (addr as usize & 0x3FF);
                Some(self.chr_rom[at % self.chr_rom.len()])
            }
            Mapper::Namco163(namco) => match namco.nametable(addr) {
                Some(Err(bank)) => {
                    let chr = if self.chr_rom.is_empty() {
                        &self.chr_ram
                    } else {
                        &self.chr_rom
                    };
                    Some(chr[(bank * 0x400 + (addr as usize & 0x3FF)) % chr.len()])
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// A PPU write of a nametable: true if it's its mapper's.
    pub fn nametable_write(&mut self, addr: u16, value: u8) -> bool {
        match &mut self.mapper {
            Mapper::Mmc5(mmc5) => mmc5.nametable_write(addr, value),
            // CHR ROM's, as a Namco can have, not written.
            Mapper::Namco163(namco) => matches!(namco.nametable(addr), Some(Err(_))),
            Mapper::Sunsoft4(sunsoft) => sunsoft.chr_tables,
            Mapper::Vrc6(vrc6) => vrc6.rom_tables(),
            _ => false,
        }
    }

    /// Which 1K of the PPU's nametable RAM pattern table address `addr` is
    /// in, if that's where its mapper has it: a Namco 163's can be.
    #[inline]
    pub fn chr_nametable(&self, addr: u16) -> Option<usize> {
        match &self.mapper {
            Mapper::Namco163(namco) => namco.chr_nametable(addr),
            _ => None,
        }
    }

    /// A PPU read of `addr`, for a mapper that watches them.
    pub fn ppu_read(&mut self, addr: u16) {
        if let Mapper::Mmc5(mmc5) = &mut self.mapper {
            mmc5.ppu_read(addr);
        }
    }

    /// A CPU write of a PPU register, or $4014, for a mapper that watches.
    pub fn ppu_register_written(&mut self, addr: u16, value: u8) {
        if let Mapper::Mmc5(mmc5) = &mut self.mapper {
            mmc5.ppu_register_written(addr, value);
        }
    }

    /// A read of the pattern tables, $0000 to $1FFF.
    pub fn chr_read(&mut self, addr: u16) -> u8 {
        if let Some(pages) = &self.chr_pages {
            let at = pages[addr as usize >> 10 & 7] as usize + (addr as usize & 0x3FF);
            return if self.chr_rom.is_empty() {
                self.chr_ram[at]
            } else {
                self.chr_rom[at]
            };
        }
        self.chr_pages = self.chr_pages_now();
        if let Some(open_bus) = self.chr_off(addr) {
            return open_bus;
        }
        let at = self.chr_offset(addr);
        let value = if self.chr_in_ram(addr) {
            self.chr_ram[at]
        } else {
            self.chr_rom[at]
        };
        if let Mapper::Mmc2(mmc2) = &mut self.mapper {
            let mmc4 = self.board == Board::Mmc4;
            match addr {
                0x0FD8 => mmc2.latch[0] = 0,
                0x0FE8 => mmc2.latch[0] = 1,
                0x0FD9..=0x0FDF if mmc4 => mmc2.latch[0] = 0,
                0x0FE9..=0x0FEF if mmc4 => mmc2.latch[0] = 1,
                0x1FD8..=0x1FDF => mmc2.latch[1] = 0,
                0x1FE8..=0x1FEF => mmc2.latch[1] = 1,
                _ => {}
            }
        }
        value
    }

    /// Where its pages are, if that's a thing they have: a read moves the
    /// MMC2's and the MMC5's, and TQROM's are in two places.
    fn chr_pages_now(&mut self) -> Option<[u32; 8]> {
        let chr = if self.chr_rom.is_empty() {
            self.chr_ram.len()
        } else {
            self.chr_rom.len()
        };
        let moves = matches!(
            self.mapper,
            Mapper::Mmc2(_) | Mapper::Mmc5(_) | Mapper::Fds(_)
        ) || matches!(self.board, Board::Tqrom | Board::Irem77)
            || self.chr_off(0).is_some();
        (!moves && chr > 0 && chr % 0x400 == 0)
            .then(|| std::array::from_fn(|page| self.chr_offset(page as u16 * 0x400) as u32))
    }

    /// What a read of CHR address `addr` gets if its board has its CHR off:
    /// open bus, the address's low byte, 185's D0 pulled up.
    fn chr_off(&self, addr: u16) -> Option<u8> {
        let Mapper::Latch(latch) = self.mapper else {
            return None;
        };
        let on = match self.board {
            Board::Sunsoft93 => latch & 1 != 0,
            Board::Cnrom185(submapper @ 4..=7) => latch & 3 == submapper & 3,
            Board::Cnrom185(_) => latch & 0x0F != 0 && latch != 0x13,
            _ => true,
        };
        let pulled_up = matches!(self.board, Board::Cnrom185(_)) as u8;
        (!on).then_some(addr as u8 | pulled_up)
    }

    /// A write to them: to its CHR RAM, if it has that.
    pub fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_in_ram(addr) {
            let at = self.chr_offset(addr);
            self.chr_ram[at] = value;
        }
    }

    /// The PPU's address on its bus, at PPU dot `dot`: for MMC3 and RAMBO-1,
    /// A12 rising after it's been low a while clocking its IRQ counter.
    pub fn ppu_bus(&mut self, addr: u16, dot: u64) {
        let low_since = match &mut self.mapper {
            Mapper::Mmc3(mmc3) => &mut mmc3.a12_low_since,
            Mapper::Rambo1(rambo) if !rambo.counting_cycles => &mut rambo.a12_low_since,
            _ => return,
        };
        if addr & 0x1000 == 0 {
            low_since.get_or_insert(dot);
            return;
        }
        if !low_since
            .take()
            .is_some_and(|since| dot - since >= A12_LOW_DOTS)
        {
            return;
        }
        if let Mapper::Rambo1(rambo) = &mut self.mapper {
            rambo.count(RAMBO1_LINE_DELAY);
        } else if let Mapper::Mmc3(mmc3) = &mut self.mapper {
            if mmc3.irq_counter == 0 || mmc3.irq_reload {
                mmc3.irq_counter = mmc3.irq_latch;
                mmc3.irq_reload = false;
            } else {
                mmc3.irq_counter -= 1;
            }
            if mmc3.irq_counter == 0 && mmc3.irq_enabled {
                mmc3.irq = true;
            }
        }
    }

    /// Its mapper, by its number and name, as a person knows it.
    pub fn mapper_name(&self) -> String {
        let (number, name) = match &self.mapper {
            Mapper::Nrom => (0, "NROM"),
            Mapper::Mmc1(_) => (1, "MMC1"),
            Mapper::Uxrom(_) => (2, "UXROM"),
            Mapper::Cnrom(_) => (3, "CNROM"),
            Mapper::Mmc3(mmc3) if mmc3.mmc6 => (4, "MMC6"),
            Mapper::Mmc3(_) => match self.board {
                Board::Txsrom => (118, "TXSROM"),
                Board::Tqrom => (119, "TQROM"),
                Board::Multicart37 => (37, "MULTICART"),
                Board::Multicart47 => (47, "MULTICART"),
                Board::Namco108 => (206, "NAMCO 108"),
                Board::Namco3446 => (76, "NAMCOT-3446"),
                Board::Namco3443 => (88, "NAMCOT-3443"),
                Board::Namco3425 => (95, "NAMCOT-3425"),
                Board::Namco3453 => (154, "NAMCOT-3453"),
                _ => (4, "MMC3"),
            },
            Mapper::Mmc5(_) => (5, "MMC5"),
            Mapper::Axrom(_) => (7, "AXROM"),
            Mapper::Mmc2(_) if self.board == Board::Mmc4 => (10, "MMC4"),
            Mapper::Mmc2(_) => (9, "MMC2"),
            Mapper::ColorDreams(_) => (11, "COLOR DREAMS"),
            Mapper::Bnrom(_) => (34, "BNROM"),
            Mapper::Nina001(_) => (34, "NINA-001"),
            Mapper::Rambo1(_) => (64, "RAMBO-1"),
            Mapper::Gxrom(_) => (66, "GXROM"),
            Mapper::Fme7(_) => (69, "FME-7"),
            Mapper::Camerica(_) => (71, "CAMERICA"),
            Mapper::Nina03(_) => (79, "NINA-03"),
            Mapper::Active(_) => (228, "ACTIVE"),
            Mapper::Fds(_) => (20, "FDS"),
            Mapper::Jf17(_) if self.board == Board::Jf19 => (92, "JALECO JF-19"),
            Mapper::Jf17(_) => (72, "JALECO JF-17"),
            Mapper::Caltron(_) => (41, "CALTRON"),
            Mapper::Vrc(_) => match self.board {
                Board::Vrc(Wiring::Vrc2a, _) => (22, "VRC2"),
                Board::Vrc(Wiring::Vrc2b, _) => (23, "VRC2"),
                Board::Vrc(Wiring::Vrc4e, _) => (23, "VRC4"),
                Board::Vrc(Wiring::Vrc4a | Wiring::Vrc4c, _) => (21, "VRC4"),
                Board::Vrc(Wiring::Vrc2c, _) => (25, "VRC2"),
                _ => (25, "VRC4"),
            },
            Mapper::Maxi15(_) => (234, "MAXI 15"),
            Mapper::Vrc1(_) => (75, "VRC1"),
            Mapper::Vrc3(_) => (73, "VRC3"),
            Mapper::Namco163(namco) => match namco.chip {
                Chip::N175 => (namco.number, "NAMCO 175"),
                Chip::N340 => (namco.number, "NAMCO 340"),
                Chip::N163 => (namco.number, "NAMCO 163"),
                Chip::Unknown => (namco.number, "NAMCO 175 OR 340"),
            },
            Mapper::Bandai(bandai) => match bandai.board {
                bandai::Board::Fcg => (16, "BANDAI FCG"),
                bandai::Board::Lz93d50 => (bandai.number, "BANDAI LZ93D50"),
                bandai::Board::Either => (16, "BANDAI FCG OR LZ93D50"),
                bandai::Board::Datach => (157, "DATACH"),
                bandai::Board::Jump2 => (153, "BANDAI LZ93D50 WITH RAM"),
            },
            Mapper::Latch(_) => match self.board {
                Board::Un1rom => (94, "UN1ROM"),
                Board::Unrom180 => (180, "UNROM"),
                Board::Cprom => (13, "CPROM"),
                Board::Cnrom185(_) => (185, "CNROM"),
                Board::Bandai70 => (70, "BANDAI"),
                Board::Bandai152 => (152, "BANDAI"),
                Board::Sunsoft89 => (89, "SUNSOFT-2"),
                Board::Jf13 => (86, "JALECO JF-13"),
                Board::Jaleco87 => (87, "JALECO"),
                Board::Jf11 => (140, "JALECO JF-11"),
                Board::Jf16(3) => (78, "IREM IF-12"),
                Board::TamS1 => (97, "IREM TAM-S1"),
                Board::Sunsoft184 => (184, "SUNSOFT-1"),
                Board::Irem77 => (77, "IREM"),
                Board::Jf16(_) => (78, "JALECO JF-16"),
                _ => (93, "SUNSOFT-2"),
            },
            Mapper::Quattro(_) => (232, "QUATTRO"),
            Mapper::Nwc(_) => (105, "NES-EVENT"),
            Mapper::Sunsoft4(_) => (68, "SUNSOFT-4"),
            Mapper::Vrc6(vrc6) => (if vrc6.swapped() { 26 } else { 24 }, "VRC6"),
        };
        format!("MAPPER {number} {name}")
    }

    /// What's in it, for the debug panel.
    pub fn debug(&self) -> Vec<String> {
        let kilobytes = |bytes: usize| bytes / 1024;
        let chr = match (self.chr_rom.len(), self.chr_ram.len()) {
            (0, ram) => format!("CHR RAM {}K", kilobytes(ram)),
            (rom, 0) => format!("CHR {}K", kilobytes(rom)),
            (rom, ram) => format!("CHR {}K+{}K RAM", kilobytes(rom), kilobytes(ram)),
        };
        let battery = if self.battery { " BATTERY" } else { "" };
        let mut lines = vec![
            self.mapper_name(),
            format!("PRG {}K {chr}", kilobytes(self.prg_rom.len())),
            format!(
                "RAM {}K{battery}  {:?}",
                kilobytes(self.prg_ram.len()),
                self.mirroring
            ),
        ];
        if let Mapper::Fds(fds) = &self.mapper {
            lines.push(fds.debug());
        }
        lines
    }

    /// A CPU cycle: the IRQ counters of those that count them; RAMBO-1's
    /// line held if it's due.
    pub fn cycle(&mut self) {
        if let Mapper::Mmc5(mmc5) = &mut self.mapper {
            mmc5.cycle();
            return;
        }
        if let Mapper::Fds(fds) = &mut self.mapper {
            self.written |= fds.cycle(&mut self.prg_ram);
            return;
        }
        if let (Mapper::Vrc(vrc), Board::Vrc(wiring, guessed)) = (&mut self.mapper, self.board) {
            if wiring.counts(guessed) {
                vrc.cycle();
            }
            return;
        }
        if let Mapper::Vrc3(vrc3) = &mut self.mapper {
            vrc3.cycle();
            return;
        }
        if let Mapper::Vrc6(vrc6) = &mut self.mapper {
            vrc6.cycle();
            return;
        }
        if let Mapper::Namco163(namco) = &mut self.mapper {
            let chip_ram = self.prg_ram.len() - CHIP_RAM;
            namco.cycle(&mut self.prg_ram[chip_ram..]);
            return;
        }
        if let Mapper::Bandai(bandai) = &mut self.mapper {
            bandai.cycle();
            return;
        }
        if let Mapper::Nwc(nwc) = &mut self.mapper {
            nwc.cycle();
            return;
        }
        if let Mapper::Sunsoft4(sunsoft) = &mut self.mapper {
            sunsoft.licence = sunsoft.licence.saturating_sub(1);
            return;
        }
        if let Mapper::Fme7(fme7) = &mut self.mapper {
            if fme7.counting {
                fme7.irq |= fme7.counter == 0 && fme7.irq_enabled;
                fme7.counter = fme7.counter.wrapping_sub(1);
            }
            return;
        }
        let Mapper::Rambo1(rambo) = &mut self.mapper else {
            return;
        };
        if rambo.irq_delay > 0 {
            rambo.irq_delay -= 1;
            rambo.irq |= rambo.irq_delay == 0;
        }
        if rambo.counting_cycles {
            rambo.prescaler = (rambo.prescaler + 1) & 3;
            if rambo.prescaler == 0 {
                rambo.count(RAMBO1_CYCLE_DELAY);
            }
        }
    }

    /// Whether its mapper holds the IRQ line.
    pub fn irq(&self) -> bool {
        match &self.mapper {
            Mapper::Mmc3(mmc3) => mmc3.irq,
            Mapper::Rambo1(rambo) => rambo.irq,
            Mapper::Fme7(fme7) => fme7.irq,
            Mapper::Mmc5(mmc5) => mmc5.irq(),
            Mapper::Fds(fds) => fds.irq(),
            Mapper::Vrc(vrc) => vrc.irq(),
            Mapper::Vrc3(vrc3) => vrc3.irq(),
            Mapper::Vrc6(vrc6) => vrc6.irq(),
            Mapper::Namco163(namco) => namco.irq(),
            Mapper::Bandai(bandai) => bandai.irq(),
            Mapper::Nwc(nwc) => nwc.irq,
            _ => false,
        }
    }
}
