//! Disk images as the drive reads them: each track a ring of bits, as the
//! head passes over it, a bit every 4 microseconds.
//!
//! A .nib holds each track as its bytes on the disk; a .dsk holds only its
//! sectors, in DOS 3.3's order, and its tracks are rebuilt as DOS 3.3
//! formats them. Either way the sync bytes in the gaps between fields are
//! written as on a disk, FF and two 0s: the controller skips 0s before a
//! byte, so a run of them brings it into step. The bytes in a field,
//! including its FFs, are 8 bits.

pub const TRACKS: usize = 35;
const NIB_TRACK: usize = 6656;
const DSK_TRACK: usize = 16 * 256;

/// A disk: its tracks' bits, one a byte (0 or 1).
pub struct Disk {
    pub tracks: Vec<Vec<u8>>,
}

/// DOS 3.3's 64 disk bytes, for 6 bits each: bytes with their top bit set
/// and no two 0 bits together.
pub const TRANSLATE: [u8; 64] = [
    0x96, 0x97, 0x9A, 0x9B, 0x9D, 0x9E, 0x9F, 0xA6, 0xA7, 0xAB, 0xAC, 0xAD, 0xAE, 0xAF, 0xB2, 0xB3,
    0xB4, 0xB5, 0xB6, 0xB7, 0xB9, 0xBA, 0xBB, 0xBC, 0xBD, 0xBE, 0xBF, 0xCB, 0xCD, 0xCE, 0xCF, 0xD3,
    0xD6, 0xD7, 0xD9, 0xDA, 0xDB, 0xDC, 0xDD, 0xDE, 0xDF, 0xE5, 0xE6, 0xE7, 0xE9, 0xEA, 0xEB, 0xEC,
    0xED, 0xEE, 0xEF, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF9, 0xFA, 0xFB, 0xFC, 0xFD, 0xFE, 0xFF,
];

/// The sector of a .dsk track at each physical sector, the order they pass
/// the head: DOS 3.3's interleave.
pub const DOS_ORDER: [usize; 16] = [0x0, 0x7, 0xE, 0x6, 0xD, 0x5, 0xC, 0x4, 0xB, 0x3, 0xA, 0x2, 0x9, 0x1, 0x8, 0xF];

/// The volume a rebuilt disk's address fields have: DOS 3.3's own.
const VOLUME: u8 = 254;

impl Disk {
    /// A .nib: 35 tracks of 6,656 bytes, as they are on the disk.
    pub fn from_nib(image: &[u8]) -> Result<Disk, String> {
        if image.len() != TRACKS * NIB_TRACK {
            return Err(format!("a .nib is {} bytes, not {}", TRACKS * NIB_TRACK, image.len()));
        }
        let tracks = image.chunks(NIB_TRACK).map(|track| {
            let in_field = fields(track);
            let mut bits = Vec::new();
            for (&byte, in_field) in track.iter().zip(in_field) {
                if byte == 0xFF && !in_field {
                    sync(&mut bits, 1);
                } else {
                    push_byte(&mut bits, byte);
                }
            }
            bits
        });
        Ok(Disk { tracks: tracks.collect() })
    }

    /// A .dsk: 35 tracks of 16 sectors of 256 bytes, in DOS 3.3's order;
    /// each track rebuilt as DOS 3.3 formats it.
    pub fn from_dsk(image: &[u8]) -> Result<Disk, String> {
        if image.len() != TRACKS * DSK_TRACK {
            return Err(format!("a .dsk is {} bytes, not {}", TRACKS * DSK_TRACK, image.len()));
        }
        let tracks = image.chunks(DSK_TRACK).enumerate().map(|(track, sectors)| {
            let mut bits = Vec::new();
            sync(&mut bits, 48);
            for (physical, &sector) in DOS_ORDER.iter().enumerate() {
                for byte in address_field(VOLUME, track as u8, physical as u8) {
                    push_byte(&mut bits, byte);
                }
                sync(&mut bits, 6);
                let data: &[u8; 256] = sectors[sector * 256..(sector + 1) * 256].try_into().unwrap();
                for byte in data_field(data) {
                    push_byte(&mut bits, byte);
                }
                sync(&mut bits, 27);
            }
            bits
        });
        Ok(Disk { tracks: tracks.collect() })
    }
}

/// Which of a .nib track's bytes are in a field: from an address field's
/// D5 AA 96 its 14 bytes, from a data field's D5 AA AD its 349, the track
/// going round.
fn fields(track: &[u8]) -> Vec<bool> {
    let len = track.len();
    let at = |i: usize| track[i % len];
    let mut in_field = vec![false; len];
    for start in 0..len {
        if at(start) == 0xD5 && at(start + 1) == 0xAA {
            let length = match at(start + 2) {
                0x96 => 14,
                0xAD => 349,
                _ => continue,
            };
            for i in start..start + length {
                in_field[i % len] = true;
            }
        }
    }
    in_field
}

/// A byte's 8 bits, the highest first.
fn push_byte(bits: &mut Vec<u8>, byte: u8) {
    bits.extend((0..8).rev().map(|bit| (byte >> bit) & 1));
}

/// Sync bytes: FF and two 0s each.
fn sync(bits: &mut Vec<u8>, count: usize) {
    for _ in 0..count {
        push_byte(bits, 0xFF);
        bits.extend([0, 0]);
    }
}

/// A byte in 4-and-4: its odd bits, then its even, each with 1s between.
fn four_and_four(value: u8) -> [u8; 2] {
    [(value >> 1) | 0xAA, value | 0xAA]
}

/// A sector's address field: D5 AA 96, the volume, track, sector and their
/// checksum in 4-and-4, DE AA EB.
pub fn address_field(volume: u8, track: u8, sector: u8) -> Vec<u8> {
    let mut field = vec![0xD5, 0xAA, 0x96];
    for value in [volume, track, sector, volume ^ track ^ sector] {
        field.extend(four_and_four(value));
    }
    field.extend([0xDE, 0xAA, 0xEB]);
    field
}

/// A sector's data field: D5 AA AD, its 256 bytes in 6-and-2, DE AA EB.
/// Each byte's top 6 bits are one of 256 values; its low 2, swapped, go in
/// threes into 86 more, first. Each of the 342 is written XORed with the
/// one before, through TRANSLATE, then the last as a checksum.
pub fn data_field(data: &[u8; 256]) -> Vec<u8> {
    let swap = |b: u8| ((b & 1) << 1) | ((b >> 1) & 1);
    let mut values = [0u8; 342];
    for i in 0..86 {
        values[i] = swap(data[i]) | swap(data[i + 86]) << 2 | if i + 172 < 256 { swap(data[i + 172]) << 4 } else { 0 };
    }
    for i in 0..256 {
        values[86 + i] = data[i] >> 2;
    }
    let mut field = vec![0xD5, 0xAA, 0xAD];
    let mut last = 0;
    for value in values {
        field.push(TRANSLATE[(value ^ last) as usize]);
        last = value;
    }
    field.push(TRANSLATE[last as usize]);
    field.extend([0xDE, 0xAA, 0xEB]);
    field
}
