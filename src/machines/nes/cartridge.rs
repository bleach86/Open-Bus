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
//! and 11 its RAMs' sizes. An old header with junk in its last bytes (a
//! ripper's name) has none in flags 7 either, taken as 0.
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
//! 1), with bus conflicts.
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
//! Bus conflicts: on a board whose register is written with its ROM still
//! driving the bus, the bits it holds low win; the register's given what
//! was written ANDed with the ROM's byte there.

use super::fds::Fds;
use super::mmc5::{self, Mmc5};
use super::region::Region;
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
        Banks { prg: 0, chr: 0, conflicts }
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
        let addr = if self.select & 0x80 != 0 { addr ^ 0x1000 } else { addr };
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
            self.irq_counter = if self.irq_latch == 0 { 0 } else { self.irq_latch | 1 };
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
}

impl Board {
    fn is_multicart(self) -> bool {
        matches!(self, Board::Multicart37 | Board::Multicart47)
    }
}

impl Mmc3 {
    /// The 1K bank that CHR address `addr` is in, by the banks and their
    /// inversion: R0 and R1 2K each, R2 to R5 1K.
    fn chr_bank(&self, addr: usize) -> usize {
        let addr = if self.select & 0x80 != 0 { addr ^ 0x1000 } else { addr };
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
    /// The board its MMC3 is on: as the ROMs are, not in a save state.
    #[serde(skip)]
    pub board: Board,
    /// The PPU's fetching a sprite: for the MMC5, which keeps count.
    #[serde(skip)]
    pub sprite_fetch: bool,
}

const HEADER: usize = 16;
const TRAINER: usize = 512;
const PRG_BANK: usize = 0x4000;
const CHR_BANK: usize = 0x2000;

impl Cartridge {
    /// The cartridge in the iNES image `image`.
    pub fn new(image: &[u8]) -> Result<Cartridge, String> {
        let header = image.get(..HEADER).filter(|header| header.starts_with(b"NES\x1A")).ok_or("not an iNES image")?;
        let nes2 = header[7] & 0x0C == 0x08;
        let junk = !nes2 && header[12..16].iter().any(|&byte| byte != 0);
        let high = if junk { 0 } else { header[7] & 0xF0 };
        let mut number = (header[6] >> 4) as u16 | high as u16;
        let (mut prg_size, mut chr_size) = (header[4] as usize * PRG_BANK, header[5] as usize * CHR_BANK);
        let mut prg_ram_size = 0x2000;
        let mut chr_ram_size = CHR_BANK;
        // RAM, and NVRAM, each 64 << its shift, if there's any.
        let sizes = |byte: u8| [byte & 0x0F, byte >> 4].iter().map(|&shift| if shift == 0 { 0 } else { 64 << shift }).sum::<usize>();
        if nes2 {
            number |= ((header[8] & 0x0F) as u16) << 8;
            prg_size += ((header[9] & 0x0F) as usize) << 8 << 14;
            chr_size += ((header[9] >> 4) as usize) << 8 << 13;
            prg_ram_size = sizes(header[10]);
            chr_ram_size = sizes(header[11]);
        }
        let submapper = if nes2 { header[8] >> 4 } else { 0 };
        let start = HEADER + if header[6] & 0x04 != 0 { TRAINER } else { 0 };
        let prg_rom = image.get(start..start + prg_size).ok_or("its PRG ROM cut short")?.to_vec();
        let chr_rom = image.get(start + prg_size..start + prg_size + chr_size).ok_or("its CHR ROM cut short")?.to_vec();
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
        let mmc6 = number == 4 && ((nes2 && header[8] >> 4 == 1) || MMC6_GAMES.contains(&rom_crc));
        if mmc6 && !nes2 {
            prg_ram_size = 0x400;
        }
        if number == 64 && !nes2 {
            prg_ram_size = 0;
        }
        if number == 5 {
            prg_ram_size = mmc5::PRG_RAM;
        }
        let mirroring = match header[6] & 0x09 {
            0x00 => Mirroring::Horizontal,
            0x01 => Mirroring::Vertical,
            _ => Mirroring::FourScreen,
        };
        let mapper = match number {
            0 => Mapper::Nrom,
            1 => Mapper::Mmc1(Mmc1 { shift: 0, bits: 0, control: 0x0C, chr: [0; 2], prg: 0, written: None }),
            2 => Mapper::Uxrom(0),
            3 => Mapper::Cnrom(0),
            4 | 37 | 47 | 118 | 119 => Mapper::Mmc3(Mmc3 {
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
            9 => Mapper::Mmc2(Mmc2 { prg: 0, chr: [[0; 2]; 2], latch: [1; 2] }),
            11 if MAPPER_144_GAMES.contains(&rom_crc) => Mapper::ColorDreams(Banks::new(Conflicts::RomD0)),
            11 if nes2 && submapper == 2 => Mapper::ColorDreams(Banks::new(Conflicts::And)),
            11 => Mapper::ColorDreams(Banks::new(Conflicts::None)),
            144 => Mapper::ColorDreams(Banks::new(Conflicts::RomD0)),
            34 if submapper == 1 || (submapper != 2 && chr_size > CHR_BANK) => Mapper::Nina001(Nina001::default()),
            34 => Mapper::Bnrom(0),
            64 => Mapper::Rambo1(Rambo1::default()),
            69 => Mapper::Fme7(Fme7::default()),
            66 => Mapper::Gxrom(Banks::new(Conflicts::And)),
            71 => Mapper::Camerica(0),
            228 => Mapper::Active(Active::default()),
            232 => Mapper::Quattro(Quattro { aladdin: submapper == 1, ..Quattro::default() }),
            79 => Mapper::Nina03(Banks::new(Conflicts::None)),
            _ => return Err(format!("mapper {number}, which isn't here yet")),
        };
        let board = match number {
            118 => Board::Txsrom,
            119 => Board::Tqrom,
            37 => Board::Multicart37,
            47 => Board::Multicart47,
            _ => Board::Plain,
        };
        let has_ram = chr_rom.is_empty() || board == Board::Tqrom;
        let chr_ram = if has_ram { vec![0; chr_ram_size.max(CHR_BANK)] } else { Vec::new() };
        Ok(Cartridge {
            prg_rom,
            chr_rom,
            chr_ram,
            // A multicart's outer register, in place of its RAM.
            prg_ram: vec![0; if board.is_multicart() { 1 } else { prg_ram_size }],
            battery: header[6] & 0x02 != 0,
            written: false,
            mirroring,
            mapper,
            region,
            board,
            sprite_fetch: false,
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
            board: Board::Plain,
            sprite_fetch: false,
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
            _ => 0.0,
        }
    }

    /// A read of $4020 to $40FF, `open_bus` what's there if it's nothing.
    pub fn register_read(&mut self, addr: u16, open_bus: u8) -> u8 {
        match &mut self.mapper {
            Mapper::Fds(fds) => fds.read(addr, open_bus),
            _ => self.cpu_read(addr).unwrap_or(open_bus),
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
                let a16 = if low_bits == 3 { 1 } else { block & (bank >> 3) };
                (bank & 7) | a16 << 3 | block << 4
            }
            Board::Multicart47 => (bank & 0x0F) | (self.prg_ram[0] as usize & 1) << 4,
            _ => bank,
        }
    }

    /// Reset: a multicart's outer register cleared, back to its menu.
    pub fn reset(&mut self) {
        if self.board.is_multicart() {
            self.prg_ram[0] = 0;
        }
        if let Mapper::Mmc5(mmc5) = &mut self.mapper {
            mmc5.reset();
        }
        if let Mapper::Active(active) = &mut self.mapper {
            *active = Active::default();
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
            Mapper::Uxrom(bank) | Mapper::Camerica(bank) => in_16k(if addr < 0xC000 { *bank as usize } else { last_16k }),
            Mapper::Axrom(bank) | Mapper::Bnrom(bank) => (*bank as usize) * 0x8000 + (addr & 0x7FFF),
            Mapper::ColorDreams(banks) | Mapper::Nina03(banks) | Mapper::Gxrom(banks) => banks.prg as usize * 0x8000 + (addr & 0x7FFF),
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
                let bank = if active.one_bank { active.bank as usize } else { (active.bank as usize & !1) | high };
                chip + in_16k(bank)
            }
            Mapper::Quattro(quattro) => {
                let block = quattro.block as usize * 4;
                in_16k(block + if addr < 0xC000 { quattro.bank as usize } else { 3 })
            }
            Mapper::Fme7(fme7) => {
                let last_8k = len / 0x2000 - 1;
                let bank = fme7.prg.get((addr - 0x8000) / 0x2000).map_or(last_8k, |&bank| bank as usize & 0x3F);
                bank * 0x2000 + (addr & 0x1FFF)
            }
            Mapper::Mmc2(mmc2) if addr < 0xA000 => mmc2.prg as usize * 0x2000 + (addr & 0x1FFF),
            Mapper::Mmc2(_) => len - 0x6000 + (addr - 0xA000),
            Mapper::Mmc1(mmc1) => {
                // On 512K boards, CHR bank 0's bit 4 picks which 256K.
                let outer = if len > 0x40000 { mmc1.chr[0] as usize & 0x10 } else { 0 };
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
            let chr = if self.chr_rom.is_empty() { self.chr_ram.len() } else { self.chr_rom.len() };
            return mmc5.chr_offset(addr, self.sprite_fetch) % chr;
        }
        let offset = match &self.mapper {
            Mapper::Nrom | Mapper::Uxrom(_) | Mapper::Axrom(_) | Mapper::Camerica(_) | Mapper::Bnrom(_) | Mapper::Quattro(_) | Mapper::Mmc5(_) | Mapper::Fds(_) => addr,
            Mapper::Cnrom(bank) => (*bank as usize) * CHR_BANK + addr,
            Mapper::Active(active) => active.chr as usize * CHR_BANK + addr,
            Mapper::ColorDreams(banks) | Mapper::Nina03(banks) | Mapper::Gxrom(banks) => banks.chr as usize * CHR_BANK + addr,
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
            Mapper::Mmc3(mmc3) => {
                let bank = mmc3.chr_bank(addr);
                let bank = match self.board {
                    Board::Tqrom => bank & 0x3F,
                    Board::Multicart37 => (bank & 0x7F) | (self.prg_ram[0] as usize & 4) << 5,
                    Board::Multicart47 => (bank & 0x7F) | (self.prg_ram[0] as usize & 1) << 7,
                    _ => bank,
                };
                bank * 0x400 + (addr & 0x3FF)
            }
        };
        let chr = if self.chr_in_ram(addr as u16) { &self.chr_ram } else { &self.chr_rom };
        offset % chr.len()
    }

    /// Whether CHR address `addr` is in its CHR RAM.
    fn chr_in_ram(&self, addr: u16) -> bool {
        match &self.mapper {
            _ if self.chr_rom.is_empty() => true,
            Mapper::Mmc3(mmc3) if self.board == Board::Tqrom => mmc3.chr_bank(addr as usize) & 0x40 != 0,
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
            Mapper::Mmc1(mmc1) if self.prg_ram.len() == 0x8000 => Some(((mmc1.chr[0] as usize >> 2) & 3) * 0x2000 + addr),
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
        let (read, write) = (mmc3.halves & (0x20 << (2 * half)) != 0, mmc3.halves & (0x10 << (2 * half)) != 0);
        let at = addr as usize & 0x3FF;
        Some((read && (write || !writing)).then_some(at))
    }

    /// A read of $4020 to $FFFF: what the cartridge drives on the bus, if it
    /// drives anything.
    pub fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        if let Mapper::Mmc5(mmc5) = &mut self.mapper {
            return mmc5.cpu_read(addr, &self.prg_rom, &self.prg_ram);
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
                Mapper::Mmc3(mmc3) if mmc3.mmc6 => self.mmc6_ram(mmc3, addr, false).map(|at| at.map_or(0, |at| self.prg_ram[at])),
                Mapper::Fme7(fme7) if fme7.window & 0x40 == 0 => {
                    let at = (fme7.window as usize & 0x3F) * 0x2000 + (addr as usize & 0x1FFF);
                    Some(self.prg_rom[at % self.prg_rom.len()])
                }
                _ => self.prg_ram_offset(addr, false).map(|at| self.prg_ram[at]),
            },
            0x8000..=0xFFFF => match &self.mapper {
                Mapper::Active(active) if active.chip == ACTIVE_NO_CHIP => None,
                _ => Some(self.prg_rom[self.prg_offset(addr)]),
            },
            _ => None,
        }
    }

    /// A write to $4020 to $FFFF, on CPU cycle `cycle`: its PRG RAM, or its
    /// mapper's registers.
    pub fn cpu_write(&mut self, addr: u16, value: u8, cycle: u64) {
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
        if let (0x4020..=0x5FFF, Mapper::Nina03(banks)) = (addr, &mut self.mapper) {
            if addr & 0xE100 == 0x4100 {
                (banks.prg, banks.chr) = ((value >> 3) & 1, value & 7);
            }
            return;
        }
        if let 0x6000..=0x7FFF = addr {
            if let Mapper::Nina001(nina) = &mut self.mapper {
                match addr {
                    0x7FFD => nina.prg = value & 1,
                    0x7FFE | 0x7FFF => nina.chr[addr as usize & 1] = value & 0x0F,
                    _ => {}
                }
            }
            let at = match &self.mapper {
                Mapper::Mmc3(mmc3) if mmc3.mmc6 => self.mmc6_ram(mmc3, addr, true).flatten(),
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
        match &mut self.mapper {
            Mapper::Nrom | Mapper::Nina03(_) | Mapper::Nina001(_) | Mapper::Mmc5(_) | Mapper::Fds(_) => {}
            Mapper::Uxrom(bank) | Mapper::Cnrom(bank) => *bank = value,
            Mapper::Axrom(bank) => {
                *bank = value & 7;
                self.mirroring = if value & 0x10 != 0 { Mirroring::SingleUpper } else { Mirroring::SingleLower };
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
                    self.mirroring = if value & 1 == 0 { Mirroring::Vertical } else { Mirroring::Horizontal };
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
                (active.chip, active.bank, active.one_bank) = (bits(11, 3), bits(6, 0x1F), bits(5, 1) != 0);
                active.chr = bits(0, 0x0F) << 2 | value & 3;
                self.mirroring = if bits(13, 1) == 0 { Mirroring::Vertical } else { Mirroring::Horizontal };
            }
            Mapper::Quattro(quattro) => match addr {
                0x8000..=0xBFFF => {
                    let block = (value >> 3) & 3;
                    quattro.block = if quattro.aladdin { (block & 1) << 1 | block >> 1 } else { block };
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
                        self.mirroring = [Mirroring::Vertical, Mirroring::Horizontal, Mirroring::SingleLower, Mirroring::SingleUpper]
                            [value as usize & 3];
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
                    self.mirroring = if value & 1 == 0 { Mirroring::Vertical } else { Mirroring::Horizontal };
                }
                _ => {}
            },
            Mapper::Camerica(bank) => match addr {
                0x9000..=0x9FFF => {
                    self.mirroring = if value & 0x10 != 0 { Mirroring::SingleUpper } else { Mirroring::SingleLower };
                }
                0xC000.. => *bank = value & 0x0F,
                _ => {}
            },
            Mapper::Mmc1(mmc1) => {
                let consecutive = mmc1.written == Some(cycle.wrapping_sub(1));
                mmc1.written = Some(cycle);
                if consecutive {
                    return;
                }
                if value & 0x80 != 0 {
                    (mmc1.shift, mmc1.bits) = (0, 0);
                    mmc1.control |= 0x0C;
                    return;
                }
                mmc1.shift |= (value & 1) << mmc1.bits;
                mmc1.bits += 1;
                if mmc1.bits < 5 {
                    return;
                }
                let register = mmc1.shift;
                (mmc1.shift, mmc1.bits) = (0, 0);
                match addr {
                    0x8000..=0x9FFF => {
                        mmc1.control = register;
                        self.mirroring = [Mirroring::SingleLower, Mirroring::SingleUpper, Mirroring::Vertical, Mirroring::Horizontal]
                            [register as usize & 3];
                    }
                    0xA000..=0xBFFF => mmc1.chr[0] = register,
                    0xC000..=0xDFFF => mmc1.chr[1] = register,
                    _ => mmc1.prg = register,
                }
            }
            Mapper::Mmc3(mmc3) => match addr & 0xE001 {
                0x8000 => mmc3.select = value,
                0x8001 => mmc3.banks[mmc3.select as usize & 7] = value,
                0xA000 if !four_screen => {
                    self.mirroring = if value & 1 == 0 { Mirroring::Vertical } else { Mirroring::Horizontal };
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
            Mapper::Mmc3(mmc3) if self.board == Board::Txsrom => mmc3.chr_bank(addr as usize & 0x0FFF) >> 7,
            Mapper::Mmc5(mmc5) => mmc5.nametable_page(addr),
            _ => self.mirroring.page(addr),
        }
    }

    /// A PPU read of a nametable, if its mapper answers it, not the PPU's
    /// RAM.
    pub fn nametable_read(&mut self, addr: u16) -> Option<u8> {
        match &mut self.mapper {
            Mapper::Mmc5(mmc5) => mmc5.nametable_read(addr),
            _ => None,
        }
    }

    /// A PPU write of a nametable: true if it's its mapper's.
    pub fn nametable_write(&mut self, addr: u16, value: u8) -> bool {
        match &mut self.mapper {
            Mapper::Mmc5(mmc5) => mmc5.nametable_write(addr, value),
            _ => false,
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
        let at = self.chr_offset(addr);
        let value = if self.chr_in_ram(addr) { self.chr_ram[at] } else { self.chr_rom[at] };
        if let Mapper::Mmc2(mmc2) = &mut self.mapper {
            match addr {
                0x0FD8 => mmc2.latch[0] = 0,
                0x0FE8 => mmc2.latch[0] = 1,
                0x1FD8..=0x1FDF => mmc2.latch[1] = 0,
                0x1FE8..=0x1FEF => mmc2.latch[1] = 1,
                _ => {}
            }
        }
        value
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
        if !low_since.take().is_some_and(|since| dot - since >= A12_LOW_DOTS) {
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
    fn mapper_name(&self) -> String {
        let (number, name) = match &self.mapper {
            Mapper::Nrom => (0, "NROM"),
            Mapper::Mmc1(_) => (1, "MMC1"),
            Mapper::Uxrom(_) => (2, "UXROM"),
            Mapper::Cnrom(_) => (3, "CNROM"),
            Mapper::Mmc3(mmc3) if mmc3.mmc6 => (4, "MMC6"),
            Mapper::Mmc3(_) => match self.board {
                Board::Plain => (4, "MMC3"),
                Board::Txsrom => (118, "TXSROM"),
                Board::Tqrom => (119, "TQROM"),
                Board::Multicart37 => (37, "MULTICART"),
                Board::Multicart47 => (47, "MULTICART"),
            },
            Mapper::Mmc5(_) => (5, "MMC5"),
            Mapper::Axrom(_) => (7, "AXROM"),
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
            Mapper::Quattro(_) => (232, "QUATTRO"),
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
            format!("RAM {}K{battery}  {:?}", kilobytes(self.prg_ram.len()), self.mirroring),
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
        if let Mapper::Fme7(fme7) = &mut self.mapper {
            if fme7.counting {
                fme7.irq |= fme7.counter == 0 && fme7.irq_enabled;
                fme7.counter = fme7.counter.wrapping_sub(1);
            }
            return;
        }
        let Mapper::Rambo1(rambo) = &mut self.mapper else { return };
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
            _ => false,
        }
    }
}
