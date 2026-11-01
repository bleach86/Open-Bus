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
//! for NES 2.0, whose byte 8 has the mapper's bits 8 to 11. An old header
//! with junk in its last bytes (a ripper's name) has none in flags 7
//! either, taken as 0.
//!
//! The mappers: 0, NROM, no switching, 16K of PRG (at $8000 and again at
//! $C000) or 32K.

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

/// A board's mapper, and its registers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mapper {
    Nrom,
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
    /// Whether its PRG RAM is battery backed, its saves kept.
    pub battery: bool,
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
        if nes2 {
            number |= ((header[8] & 0x0F) as u16) << 8;
            prg_size += ((header[9] & 0x0F) as usize) << 8 << 14;
            chr_size += ((header[9] >> 4) as usize) << 8 << 13;
            // PRG RAM and NVRAM, each 64 << its shift, if it has any.
            let shifts = [header[10] & 0x0F, header[10] >> 4];
            prg_ram_size = shifts.iter().map(|&shift| if shift == 0 { 0 } else { 64 << shift }).sum();
        }
        let start = HEADER + if header[6] & 0x04 != 0 { TRAINER } else { 0 };
        let prg_rom = image.get(start..start + prg_size).ok_or("its PRG ROM cut short")?.to_vec();
        let chr_rom = image.get(start + prg_size..start + prg_size + chr_size).ok_or("its CHR ROM cut short")?.to_vec();
        if prg_rom.is_empty() {
            return Err("no PRG ROM".to_string());
        }
        let mirroring = match header[6] & 0x09 {
            0x00 => Mirroring::Horizontal,
            0x01 => Mirroring::Vertical,
            _ => Mirroring::FourScreen,
        };
        let mapper = match number {
            0 => Mapper::Nrom,
            _ => return Err(format!("mapper {number}, which isn't here yet")),
        };
        let chr_ram = if chr_rom.is_empty() { vec![0; CHR_BANK] } else { Vec::new() };
        Ok(Cartridge {
            prg_rom,
            chr_rom,
            chr_ram,
            prg_ram: vec![0; prg_ram_size],
            battery: header[6] & 0x02 != 0,
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

    /// A read of $4020 to $FFFF: what the cartridge drives on the bus, if it
    /// drives anything.
    pub fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        match addr {
            0x6000..=0x7FFF if !self.prg_ram.is_empty() => Some(self.prg_ram[(addr as usize - 0x6000) % self.prg_ram.len()]),
            0x8000..=0xFFFF => Some(match self.mapper {
                Mapper::Nrom => self.prg_rom[(addr as usize - 0x8000) % self.prg_rom.len()],
            }),
            _ => None,
        }
    }

    /// A write to $4020 to $FFFF: its PRG RAM, or its mapper's registers.
    pub fn cpu_write(&mut self, addr: u16, value: u8) {
        if let 0x6000..=0x7FFF = addr {
            if !self.prg_ram.is_empty() {
                let len = self.prg_ram.len();
                self.prg_ram[(addr as usize - 0x6000) % len] = value;
            }
        }
    }

    /// A read of the pattern tables, $0000 to $1FFF.
    pub fn chr_read(&mut self, addr: u16) -> u8 {
        let chr = if self.chr_rom.is_empty() { &self.chr_ram } else { &self.chr_rom };
        match self.mapper {
            Mapper::Nrom => chr[addr as usize % chr.len()],
        }
    }

    /// A write to them: to its CHR RAM, if it has that.
    pub fn chr_write(&mut self, addr: u16, value: u8) {
        if self.chr_rom.is_empty() {
            let len = self.chr_ram.len();
            self.chr_ram[addr as usize % len] = value;
        }
    }

    /// Whether its mapper holds the IRQ line.
    pub fn irq(&self) -> bool {
        false
    }
}
