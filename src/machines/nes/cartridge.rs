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
//! 7, AxROM: a 32K bank, and which 1K all four nametables are.

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

/// The MMC6's games, by their ROMs' CRC-32: StarTropics, and StarTropics
/// II.
const MMC6_GAMES: [u32; 2] = [0x8891_29CB, 0xD054_FFB0];

/// How long A12 must be low, in PPU dots, for its rise to clock MMC3's
/// IRQ counter: about three CPU cycles. Not between a line's sprites'
/// fetches, only as the background's give way to theirs.
const A12_LOW_DOTS: u64 = 10;

/// A board's mapper, and its registers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mapper {
    Nrom,
    Mmc1(Mmc1),
    Uxrom(u8),
    Cnrom(u8),
    Mmc3(Mmc3),
    Axrom(u8),
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
        let start = HEADER + if header[6] & 0x04 != 0 { TRAINER } else { 0 };
        let prg_rom = image.get(start..start + prg_size).ok_or("its PRG ROM cut short")?.to_vec();
        let chr_rom = image.get(start + prg_size..start + prg_size + chr_size).ok_or("its CHR ROM cut short")?.to_vec();
        if prg_rom.is_empty() {
            return Err("no PRG ROM".to_string());
        }
        let mmc6 = number == 4
            && ((nes2 && header[8] >> 4 == 1) || MMC6_GAMES.contains(&crate::host::zip::crc32(&[&prg_rom[..], &chr_rom[..]].concat())));
        if mmc6 && !nes2 {
            prg_ram_size = 0x400;
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
            4 => Mapper::Mmc3(Mmc3 {
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
            7 => Mapper::Axrom(0),
            _ => return Err(format!("mapper {number}, which isn't here yet")),
        };
        let chr_ram = if chr_rom.is_empty() { vec![0; chr_ram_size.max(CHR_BANK)] } else { Vec::new() };
        Ok(Cartridge {
            prg_rom,
            chr_rom,
            chr_ram,
            prg_ram: vec![0; prg_ram_size],
            battery: header[6] & 0x02 != 0,
            written: false,
            mirroring,
            mapper,
        })
    }

    /// The ROMs taken from `cartridge`, the one a save state's is loaded
    /// into.
    pub fn take_roms(&mut self, cartridge: &mut Cartridge) {
        self.prg_rom = std::mem::take(&mut cartridge.prg_rom);
        self.chr_rom = std::mem::take(&mut cartridge.chr_rom);
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
            Mapper::Uxrom(bank) => in_16k(if addr < 0xC000 { *bank as usize } else { last_16k }),
            Mapper::Axrom(bank) => (*bank as usize) * 0x8000 + (addr & 0x7FFF),
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
                bank * 0x2000 + (addr & 0x1FFF)
            }
        };
        offset % len
    }

    /// Where `addr`, $0000 to $1FFF, is in its CHR, as the mapper has its
    /// banks.
    fn chr_offset(&self, addr: u16) -> usize {
        let addr = addr as usize;
        let offset = match &self.mapper {
            Mapper::Nrom | Mapper::Uxrom(_) | Mapper::Axrom(_) => addr,
            Mapper::Cnrom(bank) => (*bank as usize) * CHR_BANK + addr,
            Mapper::Mmc1(mmc1) => match mmc1.control & 0x10 {
                0 => (mmc1.chr[0] as usize & 0x1E) * 0x1000 + addr,
                _ => mmc1.chr[addr >> 12] as usize * 0x1000 + (addr & 0x0FFF),
            },
            Mapper::Mmc3(mmc3) => {
                let addr = if mmc3.select & 0x80 != 0 { addr ^ 0x1000 } else { addr };
                match addr >> 10 {
                    0 | 1 => (mmc3.banks[0] as usize & 0xFE) * 0x400 + (addr & 0x7FF),
                    2 | 3 => (mmc3.banks[1] as usize & 0xFE) * 0x400 + (addr & 0x7FF),
                    slot => mmc3.banks[slot - 2] as usize * 0x400 + (addr & 0x3FF),
                }
            }
        };
        let chr = if self.chr_rom.is_empty() { &self.chr_ram } else { &self.chr_rom };
        offset % chr.len()
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
        match addr {
            0x6000..=0x7FFF => match &self.mapper {
                Mapper::Mmc3(mmc3) if mmc3.mmc6 => self.mmc6_ram(mmc3, addr, false).map(|at| at.map_or(0, |at| self.prg_ram[at])),
                _ => self.prg_ram_offset(addr, false).map(|at| self.prg_ram[at]),
            },
            0x8000..=0xFFFF => Some(self.prg_rom[self.prg_offset(addr)]),
            _ => None,
        }
    }

    /// A write to $4020 to $FFFF, on CPU cycle `cycle`: its PRG RAM, or its
    /// mapper's registers.
    pub fn cpu_write(&mut self, addr: u16, value: u8, cycle: u64) {
        if let 0x6000..=0x7FFF = addr {
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
        match &mut self.mapper {
            Mapper::Nrom => {}
            Mapper::Uxrom(bank) | Mapper::Cnrom(bank) => *bank = value,
            Mapper::Axrom(bank) => {
                *bank = value & 7;
                self.mirroring = if value & 0x10 != 0 { Mirroring::SingleUpper } else { Mirroring::SingleLower };
            }
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

    /// A read of the pattern tables, $0000 to $1FFF.
    pub fn chr_read(&mut self, addr: u16) -> u8 {
        let at = self.chr_offset(addr);
        if self.chr_rom.is_empty() { self.chr_ram[at] } else { self.chr_rom[at] }
    }

    /// A write to them: to its CHR RAM, if it has that.
    pub fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_rom.is_empty() {
            let at = self.chr_offset(addr);
            self.chr_ram[at] = value;
        }
    }

    /// The PPU's address on its bus, at PPU dot `dot`: for MMC3, A12 rising
    /// after it's been low a while clocking its IRQ counter.
    pub fn ppu_bus(&mut self, addr: u16, dot: u64) {
        let Mapper::Mmc3(mmc3) = &mut self.mapper else { return };
        if addr & 0x1000 == 0 {
            mmc3.a12_low_since.get_or_insert(dot);
            return;
        }
        if mmc3.a12_low_since.take().is_some_and(|since| dot - since >= A12_LOW_DOTS) {
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

    /// Whether its mapper holds the IRQ line.
    pub fn irq(&self) -> bool {
        matches!(&self.mapper, Mapper::Mmc3(mmc3) if mmc3.irq)
    }
}
