//! The Famicom Disk System: an adapter in the cartridge slot, and a drive.
//! The adapter has 32K of RAM, $6000 to $DFFF, where a game's loaded; 8K of
//! CHR RAM; and an 8K BIOS at $E000, that isn't a game's, and that loads
//! one. So a disk is a machine's two files, the BIOS being Nintendo's own.
//!
//! A disk's sides, in an image, are 65500 bytes each, after a 16-byte
//! header ("FDS", $1A) if it has one. Those are its blocks, one after
//! another, as the BIOS reads them: on a disk each is after a gap of 0s and
//! a $80 that marks its start, and before a CRC, which an image hasn't got.
//! They're put back here. The disk as it's spinning, all its sides, is the
//! cartridge's PRG RAM, with a battery: so what a game writes to it is kept
//! as any game's saves are, and in save states, with nothing more.
//!
//! The drive: told to by $4025, it runs to the start of a side, and passes
//! a byte under the head each 150 cycles or so, the transfer flag raised
//! for each and, if asked, an IRQ. Reading, nothing's seen until the first
//! non-zero byte after the game says it's ready for one. At the end of the
//! side the head goes back. $4032 says whether there's a disk in, and it's
//! ready. A game wanting another side waits to see that one's gone and one
//! has come: so changing one takes a while, the drive empty meanwhile.
//!
//! The registers: $4020 and $4021, the IRQ timer's reload; $4022, its
//! repeat and enable; $4023, the disk's and sound's registers on or off;
//! $4024, a byte to write; $4025, the drive's controls and the mirroring;
//! $4026, the expansion port, read back at $4033. $4030 is the IRQs' and
//! transfer's flags, reading it clearing them; $4031 the byte read.
//!
//! Its sound, a 64-sample wavetable of 6 bits, $4040 to $407F, played by a
//! 12-bit pitch; a volume envelope; and a modulator, a table of 64 steps
//! bending the pitch, with an envelope of its own, for the vibrato that
//! makes the games' music theirs. All clocked by the CPU's cycles.

use super::cartridge::Mirroring;
use serde::{Deserialize, Serialize};

/// A side's bytes in an image; an image's header.
const SIDE: usize = 65500;
const HEADER: usize = 16;
const IMAGE_MAGIC: &[u8] = b"FDS\x1A";
/// The gaps of 0s, before a side's first block and after each: 28300 and
/// 976 bits.
const LEAD_IN: usize = 28300 / 8;
const GAP: usize = 976 / 8;
/// What marks a block's start; and a CRC, not checked of one in an image.
const BLOCK_START: u8 = 0x80;
const NO_CRC: [u8; 2] = [0x4D, 0x62];
/// Cycles to get up to speed; and between bytes, which on a real drive
/// vary, a game's timing not able to rest on them. 150 is said to break a
/// game's stage 3 by an NMI landing badly (Ai Senshi Nicol); 149 doesn't.
const SPIN_UP: u32 = 50000;
const BYTE_CYCLES: u32 = 149;
/// How long a change of disk leaves the drive empty: a second.
pub const CHANGE_CYCLES: u32 = 1_789_773;
/// $4089's four master volumes, as a share of 1152.
const MASTER_VOLUMES: [u32; 4] = [36, 24, 17, 14];
/// The BIOS's value for $408A.
const ENVELOPE_SPEED: u8 = 0xE8;
/// What a step of the modulator adds to its counter; 4 resets it.
const MOD_STEPS: [i8; 8] = [0, 1, 2, 4, 0, -4, -2, -1];
const MOD_RESET: u8 = 4;

/// Whether `image` is a disk's; its sides, if so.
pub fn sides_of(image: &[u8]) -> Option<&[u8]> {
    if image.starts_with(b"NES\x1A") {
        return None;
    }
    let sides = if image.starts_with(IMAGE_MAGIC) {
        image.get(HEADER..)?
    } else {
        image
    };
    (!sides.is_empty() && sides.len() % SIDE == 0).then_some(sides)
}

/// A side as it's on the disk: its gaps put back around its blocks: disk
/// info (1), the file count (2), and a header (3) and data (4) for each
/// file, the header saying how long. Anything else, the rest as it is.
fn spread(side: &[u8], disk: &mut Vec<u8>) {
    disk.resize(disk.len() + LEAD_IN, 0);
    let (mut at, mut file_size) = (0, 0);
    while at < SIDE {
        let length = match side[at] {
            1 => 56,
            2 => 2,
            3 => {
                file_size = side.get(at + 13).copied().unwrap_or(0) as usize | (side.get(at + 14).copied().unwrap_or(0) as usize) << 8;
                16
            }
            4 => 1 + file_size,
            _ => {
                disk.push(BLOCK_START);
                disk.extend(&side[at..]);
                return;
            }
        };
        if at + length >= SIDE {
            return;
        }
        disk.push(BLOCK_START);
        disk.extend(&side[at..at + length]);
        disk.extend(NO_CRC);
        disk.resize(disk.len() + GAP, 0);
        at += length;
    }
}

/// A volume envelope, the wave's or the modulator's, and a pitch.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Channel {
    speed: u8,
    gain: u8,
    fixed: bool,
    rising: bool,
    pitch: u16,
    timer: u32,
}

impl Channel {
    fn restart(&mut self, master: u8) {
        self.timer = 8 * (self.speed as u32 + 1) * master as u32;
    }

    fn write(&mut self, register: u16, value: u8, master: u8) {
        match register & 3 {
            0 => {
                self.speed = value & 0x3F;
                self.rising = value & 0x40 != 0;
                self.fixed = value & 0x80 != 0;
                self.restart(master);
                if self.fixed {
                    self.gain = self.speed;
                }
            }
            2 => self.pitch = (self.pitch & 0x0F00) | value as u16,
            3 => self.pitch = (self.pitch & 0x00FF) | (value as u16 & 0x0F) << 8,
            _ => {}
        }
    }

    /// A cycle of its envelope: whether its gain's stepped.
    fn tick(&mut self, master: u8) -> bool {
        if self.fixed || master == 0 {
            return false;
        }
        self.timer = self.timer.wrapping_sub(1);
        if self.timer != 0 {
            return false;
        }
        self.restart(master);
        if self.rising && self.gain < 32 {
            self.gain += 1;
        } else if !self.rising && self.gain > 0 {
            self.gain -= 1;
        }
        true
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Sound {
    wave: Vec<u8>,
    wave_at: u8,
    wave_written: bool,
    wave_overflow: u16,
    halted: bool,
    envelopes_off: bool,
    master_volume: u8,
    envelope_speed: u8,
    volume: Channel,
    modulator: Channel,
    mod_table: Vec<u8>,
    mod_at: u8,
    mod_off: bool,
    mod_overflow: u16,
    /// The modulator's 7-bit signed counter; and what it adds to the pitch.
    counter: i8,
    bend: i32,
    output: u8,
}

impl Sound {
    fn new() -> Sound {
        Sound {
            wave: vec![0; 64],
            wave_at: 0,
            wave_written: false,
            wave_overflow: 0,
            halted: false,
            envelopes_off: false,
            master_volume: 0,
            envelope_speed: ENVELOPE_SPEED,
            volume: Channel::default(),
            modulator: Channel::default(),
            mod_table: vec![0; 64],
            mod_at: 0,
            mod_off: false,
            mod_overflow: 0,
            counter: 0,
            bend: 0,
            output: 0,
        }
    }

    fn set_counter(&mut self, value: i32) {
        let wrapped = value.rem_euclid(128);
        self.counter = (if wrapped >= 64 { wrapped - 128 } else { wrapped }) as i8;
    }

    /// The pitch's bend, from the counter and gain, by the chip's odd
    /// rounding: the product's low bits dropped, it nudged if any were set.
    fn bend(&mut self) {
        let product = self.counter as i32 * self.modulator.gain as i32;
        let mut scaled = product >> 4;
        if product & 0x0F > 0 && scaled & 0x80 == 0 {
            scaled += if self.counter < 0 { -1 } else { 2 };
        }
        if scaled >= 192 {
            scaled -= 256;
        } else if scaled < -64 {
            scaled += 256;
        }
        let pitched = self.volume.pitch as i32 * scaled;
        self.bend = (pitched >> 6) + (pitched & 0x3F >= 32) as i32;
    }

    fn cycle(&mut self) {
        if !self.halted && !self.envelopes_off {
            self.volume.tick(self.envelope_speed);
            if self.modulator.tick(self.envelope_speed) {
                self.bend();
            }
        }
        if !self.mod_off && self.modulator.pitch > 0 {
            let (sum, carried) = self.mod_overflow.overflowing_add(self.modulator.pitch);
            self.mod_overflow = sum;
            if carried {
                let step = self.mod_table[self.mod_at as usize];
                let counter = if step == MOD_RESET { 0 } else { self.counter as i32 + MOD_STEPS[step as usize] as i32 };
                self.set_counter(counter);
                self.mod_at = (self.mod_at + 1) & 0x3F;
                self.bend();
            }
        }
        if !self.wave_written {
            let level = self.volume.gain.min(32) as u32 * MASTER_VOLUMES[self.master_volume as usize];
            self.output = (self.wave[self.wave_at as usize] as u32 * level / 1152) as u8;
        }
        let pitch = self.volume.pitch as i32 + self.bend;
        if !self.halted && pitch > 0 {
            let (sum, carried) = self.wave_overflow.overflowing_add(pitch as u16);
            self.wave_overflow = sum;
            if carried {
                self.wave_at = (self.wave_at + 1) & 0x3F;
            }
        }
    }

    fn read(&self, addr: u16, open_bus: u8) -> u8 {
        let high = open_bus & 0xC0;
        match addr {
            0x4040..=0x407F if self.wave_written => high | self.wave[addr as usize & 0x3F],
            0x4040..=0x407F => high | self.wave[self.wave_at as usize],
            0x4090 => high | self.volume.gain,
            0x4092 => high | self.modulator.gain,
            _ => open_bus,
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        let master = self.envelope_speed;
        match addr {
            0x4040..=0x407F if self.wave_written => self.wave[addr as usize & 0x3F] = value & 0x3F,
            0x4080 | 0x4082 => {
                self.volume.write(addr, value, master);
                self.bend();
            }
            0x4083 => {
                self.envelopes_off = value & 0x40 != 0;
                self.halted = value & 0x80 != 0;
                if self.halted {
                    self.wave_at = 0;
                }
                if self.envelopes_off {
                    self.volume.restart(master);
                    self.modulator.restart(master);
                }
                self.volume.write(addr, value, master);
                self.bend();
            }
            0x4084 => {
                self.modulator.write(addr, value, master);
                self.bend();
            }
            0x4085 => {
                self.set_counter((value & 0x7F) as i32);
                self.bend();
            }
            0x4086 => self.modulator.write(addr, value, master),
            0x4087 => {
                self.modulator.write(addr, value, master);
                self.mod_off = value & 0x80 != 0;
                if self.mod_off {
                    self.mod_overflow = 0;
                }
            }
            // A step's the same twice: written only with the modulator off.
            0x4088 if self.mod_off => {
                for _ in 0..2 {
                    self.mod_table[self.mod_at as usize] = value & 7;
                    self.mod_at = (self.mod_at + 1) & 0x3F;
                }
            }
            0x4089 => {
                self.master_volume = value & 3;
                self.wave_written = value & 0x80 != 0;
            }
            0x408A => self.envelope_speed = value,
            _ => {}
        }
    }
}

/// The adapter and the drive.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fds {
    /// Its 32K of RAM, the disk being the cartridge's PRG RAM.
    ram: Vec<u8>,
    /// Where each side is on the disk, and how long.
    sides: Vec<(usize, usize)>,
    /// The image's CRC-32: every game's BIOS being the same, what tells
    /// one's save state from another's.
    pub crc: u32,
    side: Option<usize>,
    /// The side going in, and the cycles till it does.
    next_side: usize,
    change_in: u32,
    irq_reload: u16,
    irq_counter: u16,
    irq_enabled: bool,
    irq_repeats: bool,
    timer_irq: bool,
    disk_irq: bool,
    disk_registers: bool,
    sound_registers: bool,
    motor: bool,
    transfer_reset: bool,
    reading: bool,
    crc_control: bool,
    was_crc_control: bool,
    ready: bool,
    disk_irq_enabled: bool,
    horizontal: bool,
    expansion: u8,
    position: usize,
    delay: u32,
    end_of_head: bool,
    scanning: bool,
    gap_ended: bool,
    transferred: bool,
    read_byte: u8,
    write_byte: u8,
    crc_sum: u16,
    sound: Sound,
}

impl Fds {
    /// A drive with `sides`, an image's, its first side in, and the disk
    /// they make, to be the cartridge's PRG RAM.
    pub fn new(sides: &[u8], crc: u32) -> (Fds, Vec<u8>) {
        let mut disk = Vec::new();
        let mut places = Vec::new();
        for side in sides.chunks(SIDE) {
            let start = disk.len();
            spread(side, &mut disk);
            places.push((start, disk.len() - start));
        }
        let fds = Fds {
            ram: vec![0; 0x8000],
            sides: places,
            crc,
            side: Some(0),
            next_side: 0,
            change_in: 0,
            irq_reload: 0,
            irq_counter: 0,
            irq_enabled: false,
            irq_repeats: false,
            timer_irq: false,
            disk_irq: false,
            disk_registers: false,
            sound_registers: false,
            motor: false,
            transfer_reset: false,
            reading: false,
            crc_control: false,
            was_crc_control: false,
            ready: false,
            disk_irq_enabled: false,
            horizontal: false,
            expansion: 0,
            position: 0,
            delay: 0,
            end_of_head: true,
            scanning: false,
            gap_ended: false,
            transferred: false,
            read_byte: 0,
            write_byte: 0,
            crc_sum: 0,
            sound: Sound::new(),
        };
        (fds, disk)
    }

    /// The next side going in, after the first if they've run out: what's
    /// in comes out at once, and it goes in a second on. What it's going to
    /// be, in words.
    pub fn change_side(&mut self) -> String {
        self.next_side = (self.side.unwrap_or(self.next_side) + 1) % self.sides.len();
        self.side = None;
        self.change_in = CHANGE_CYCLES;
        self.side_name(self.next_side)
    }

    fn side_name(&self, side: usize) -> String {
        format!("Disk {} side {}", side / 2 + 1, if side % 2 == 0 { 'A' } else { 'B' })
    }

    pub fn debug(&self) -> String {
        match self.side {
            Some(side) => format!("{} POSITION {}", self.side_name(side), self.position),
            None => format!("NO DISK, {} NEXT", self.side_name(self.next_side)),
        }
    }

    pub fn irq(&self) -> bool {
        self.timer_irq || self.disk_irq
    }

    /// The mirroring $4025 has set.
    pub fn mirroring(&self) -> Mirroring {
        if self.horizontal { Mirroring::Horizontal } else { Mirroring::Vertical }
    }

    /// The sound's output, as a share of the NES's own: 0 to about .25.
    pub fn output(&self) -> f32 {
        self.sound.output as f32 * 0.004
    }

    pub fn ram(&self, addr: u16) -> u8 {
        self.ram[addr as usize - 0x6000]
    }

    pub fn write_ram(&mut self, addr: u16, value: u8) {
        self.ram[addr as usize - 0x6000] = value;
    }

    /// A read of its registers, $4020 to $40FF.
    pub fn read(&mut self, addr: u16, open_bus: u8) -> u8 {
        if self.sound_registers && addr >= 0x4040 {
            return self.sound.read(addr, open_bus);
        }
        if !self.disk_registers {
            return open_bus;
        }
        match addr {
            0x4030 => {
                let flags = open_bus & 0x24
                    | self.timer_irq as u8
                    | (self.transferred as u8) << 1
                    | (self.horizontal as u8) << 3;
                self.transferred = false;
                self.timer_irq = false;
                self.disk_irq = false;
                flags
            }
            0x4031 => {
                self.transferred = false;
                self.disk_irq = false;
                self.read_byte
            }
            0x4032 => {
                let empty = self.side.is_none() as u8;
                let not_ready = (self.side.is_none() || !self.scanning) as u8;
                open_bus & 0xF8 | empty | not_ready << 1 | empty << 2
            }
            0x4033 => self.expansion,
            _ => open_bus,
        }
    }

    /// A write of its registers, $4020 to $40FF.
    pub fn write(&mut self, addr: u16, value: u8) {
        if !self.disk_registers && (0x4024..=0x4026).contains(&addr) {
            return;
        }
        match addr {
            0x4020 => self.irq_reload = (self.irq_reload & 0xFF00) | value as u16,
            0x4021 => self.irq_reload = (self.irq_reload & 0x00FF) | (value as u16) << 8,
            0x4022 => {
                self.irq_repeats = value & 1 != 0;
                self.irq_enabled = value & 2 != 0 && self.disk_registers;
                if self.irq_enabled {
                    self.irq_counter = self.irq_reload;
                } else {
                    self.timer_irq = false;
                }
            }
            0x4023 => {
                self.disk_registers = value & 1 != 0;
                self.sound_registers = value & 2 != 0;
                if !self.disk_registers {
                    self.irq_enabled = false;
                    self.timer_irq = false;
                    self.disk_irq = false;
                }
            }
            0x4024 => {
                self.write_byte = value;
                self.transferred = false;
                self.disk_irq = false;
            }
            0x4025 => {
                self.motor = value & 0x01 != 0;
                self.transfer_reset = value & 0x02 != 0;
                self.reading = value & 0x04 != 0;
                self.horizontal = value & 0x08 != 0;
                self.crc_control = value & 0x10 != 0;
                self.ready = value & 0x40 != 0;
                self.disk_irq_enabled = value & 0x80 != 0;
                self.disk_irq = false;
            }
            0x4026 => self.expansion = value,
            0x4040.. if self.sound_registers => self.sound.write(addr, value),
            _ => {}
        }
    }

    fn crc_add(&mut self, byte: u8) {
        self.crc_sum ^= byte as u16;
        for _ in 0..8 {
            let carry = self.crc_sum & 1 != 0;
            self.crc_sum >>= 1;
            if carry {
                self.crc_sum ^= 0x8408;
            }
        }
    }

    /// A CPU cycle, `disk` the disk: whether it's been written.
    pub fn cycle(&mut self, disk: &mut [u8]) -> bool {
        if self.irq_enabled {
            if self.irq_counter == 0 {
                self.timer_irq = true;
                self.irq_counter = self.irq_reload;
                self.irq_enabled = self.irq_repeats;
            } else {
                self.irq_counter -= 1;
            }
        }
        self.sound.cycle();
        if self.change_in > 0 {
            self.change_in -= 1;
            if self.change_in == 0 {
                self.side = Some(self.next_side);
            }
        }
        self.drive(disk)
    }

    fn drive(&mut self, disk: &mut [u8]) -> bool {
        let (true, Some(side)) = (self.motor, self.side) else {
            self.end_of_head = true;
            self.scanning = false;
            return false;
        };
        if self.transfer_reset && !self.scanning {
            return false;
        }
        if self.end_of_head {
            self.delay = SPIN_UP;
            self.end_of_head = false;
            self.position = 0;
            self.gap_ended = false;
            return false;
        }
        if self.delay > 0 {
            self.delay -= 1;
            return false;
        }
        self.scanning = true;
        let (start, length) = self.sides[side];
        let at = start + self.position;
        let mut irq = self.disk_irq_enabled;
        let mut written = false;
        if self.reading {
            let byte = disk[at];
            if !self.was_crc_control {
                self.crc_add(byte);
            }
            if !self.ready {
                self.gap_ended = false;
                self.crc_sum = 0;
            } else if byte != 0 && !self.gap_ended {
                self.gap_ended = true;
                irq = false;
            }
            if self.gap_ended {
                self.transferred = true;
                self.read_byte = byte;
                self.disk_irq |= irq;
            }
        } else {
            let mut byte = 0;
            if !self.crc_control {
                self.transferred = true;
                byte = self.write_byte;
                self.disk_irq |= irq;
            }
            if !self.ready {
                byte = 0;
                self.crc_sum = 0;
            }
            if self.crc_control {
                byte = self.crc_sum as u8;
                self.crc_sum >>= 8;
            } else {
                self.crc_add(byte);
            }
            written = disk[at] != byte;
            disk[at] = byte;
            self.gap_ended = false;
        }
        self.was_crc_control = self.crc_control;
        self.position += 1;
        if self.position >= length {
            self.motor = false;
            self.disk_irq |= self.disk_irq_enabled;
        } else {
            self.delay = BYTE_CYCLES;
        }
        written
    }
}
