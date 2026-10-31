//! Disk images as the drive reads them: each track a ring of bits, as the
//! head passes over it, a bit every 4 microseconds.
//!
//! A .nib holds each track as its bytes on the disk; a .dsk (or .do) holds
//! only its sectors, in DOS 3.3's order, and a .po the same in ProDOS's,
//! and their tracks are rebuilt as DOS 3.3 formats them (ProDOS formats
//! them the same). Either way the sync bytes in the gaps between fields are
//! written as on a disk, FF and two 0s: the controller skips 0s before a
//! byte, so a run of them brings it into step. The bytes in a field,
//! including its FFs, are 8 bits. Their tracks are under the head at whole
//! tracks only.
//!
//! A .woz holds each track's bits as they were read off a disk, copy
//! protection and all, and which track is under the head at each
//! quarter-track; at those between tracks, often none.

pub const TRACKS: usize = 35;
const NIB_TRACK: usize = 6656;
const SECTORS_TRACK: usize = 16 * 256;
/// The half-tracks a .woz maps: 40 tracks' worth.
const WOZ_HALF_TRACKS: usize = 80;
/// A WOZ1 track: its bits in 6,646 bytes, then how many, and the rest.
const WOZ1_TRACK: usize = 6656;
/// WOZ2's TRKS: where its 160 tracks are, 8 bytes each, then their bits.
const WOZ2_ENTRIES: usize = 160;

/// A disk: its tracks' bits, one a byte (0 or 1); and at each half-track,
/// which track is under the head, if one is.
pub struct Disk {
    pub tracks: Vec<Vec<u8>>,
    pub half_tracks: Vec<Option<usize>>,
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

/// The same for a .po: ProDOS's, its 512-byte blocks each two sectors, a
/// physical sector apart.
pub const PRODOS_ORDER: [usize; 16] = [0x0, 0x8, 0x1, 0x9, 0x2, 0xA, 0x3, 0xB, 0x4, 0xC, 0x5, 0xD, 0x6, 0xE, 0x7, 0xF];

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
        Ok(Disk::whole_tracks(tracks.collect()))
    }

    /// A .dsk: 35 tracks of 16 sectors of 256 bytes, in DOS 3.3's order;
    /// each track rebuilt as DOS 3.3 formats it.
    pub fn from_dsk(image: &[u8]) -> Result<Disk, String> {
        Disk::from_sectors(image, &DOS_ORDER, ".dsk")
    }

    /// A .po: the same, in ProDOS's order.
    pub fn from_po(image: &[u8]) -> Result<Disk, String> {
        Disk::from_sectors(image, &PRODOS_ORDER, ".po")
    }

    /// 35 tracks of 16 sectors, in `order`: a `kind` of image.
    fn from_sectors(image: &[u8], order: &[usize; 16], kind: &str) -> Result<Disk, String> {
        if image.len() != TRACKS * SECTORS_TRACK {
            return Err(format!("a {kind} is {} bytes, not {}", TRACKS * SECTORS_TRACK, image.len()));
        }
        let tracks = image.chunks(SECTORS_TRACK).enumerate().map(|(track, sectors)| {
            let mut bits = Vec::new();
            sync(&mut bits, 48);
            for (physical, &sector) in order.iter().enumerate() {
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
        Ok(Disk::whole_tracks(tracks.collect()))
    }

    /// The track under the head at `half_track`, if one is: its bits.
    pub fn track(&self, half_track: usize) -> Option<&Vec<u8>> {
        self.half_tracks.get(half_track).copied().flatten().map(|track| &self.tracks[track])
    }

    /// A disk with `tracks` under the head at whole tracks, none between.
    fn whole_tracks(tracks: Vec<Vec<u8>>) -> Disk {
        let half_tracks = (0..2 * tracks.len()).map(|half| (half % 2 == 0).then_some(half / 2)).collect();
        Disk { tracks, half_tracks }
    }

    /// A .woz, WOZ1 or WOZ2: after its header and CRC, chunks, each an ID
    /// and a length. INFO says what disk it is; TMAP, which track is at
    /// each of 160 quarter-tracks, FF for none; TRKS, the tracks: in WOZ1,
    /// 6,656 bytes each, their bits from the first byte's top bit, and how
    /// many at 6,648; in WOZ2, where each one's bits are in the file, in
    /// 512-byte blocks, and how many, then the bits. The stepper stops at half-tracks:
    /// their quarter-tracks' are the ones used.
    pub fn from_woz(image: &[u8]) -> Result<Disk, String> {
        let version = match image.get(..8) {
            Some(b"WOZ1\xFF\n\r\n") => 1,
            Some(b"WOZ2\xFF\n\r\n") => 2,
            _ => return Err("not a WOZ1 or WOZ2".to_string()),
        };
        let crc = u32::from_le_bytes(image.get(8..12).ok_or("no CRC")?.try_into().unwrap());
        if crc != 0 && crc != crc32(&image[12..]) {
            return Err("its CRC is wrong".to_string());
        }
        let (mut info, mut tmap, mut trks) = (None, None, None);
        let mut at = 12;
        while at + 8 <= image.len() {
            let size = u32::from_le_bytes(image[at + 4..at + 8].try_into().unwrap()) as usize;
            let body = image.get(at + 8..at + 8 + size).ok_or("a chunk cut short")?;
            match &image[at..at + 4] {
                b"INFO" => info = Some(body),
                b"TMAP" => tmap = Some(body),
                b"TRKS" => trks = Some(body),
                _ => {}
            }
            at += 8 + size;
        }
        let (Some(info), Some(tmap), Some(trks)) = (info, tmap, trks) else {
            return Err("no INFO, TMAP or TRKS".to_string());
        };
        if info.get(1) != Some(&1) {
            return Err("not a 5.25\" disk".to_string());
        }
        let u16_at = |bytes: &[u8], at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]) as usize;

        let tracks: Option<Vec<Vec<u8>>> = if version == 1 {
            trks.chunks_exact(WOZ1_TRACK).map(|track| bits(&track[..6646], u16_at(track, 6648))).collect()
        } else {
            trks.get(..WOZ2_ENTRIES * 8)
                .ok_or("its TRKS cut short")?
                .chunks_exact(8)
                .map(|entry| {
                    let (start, length) = (u16_at(entry, 0) * 512, u16_at(entry, 2) * 512);
                    let count = u32::from_le_bytes(entry[4..8].try_into().unwrap()) as usize;
                    bits(image.get(start..start + length)?, count)
                })
                .collect()
        };
        let tracks = tracks.ok_or("a track's bits past its bytes")?;

        let half_tracks = (0..WOZ_HALF_TRACKS)
            .map(|half| match tmap.get(2 * half).copied() {
                None | Some(0xFF) => Ok(None),
                Some(track) if tracks.get(track as usize).is_some_and(|bits| !bits.is_empty()) => Ok(Some(track as usize)),
                Some(track) => Err(format!("its map has track {track}, which it hasn't")),
            })
            .collect::<Result<_, _>>()?;
        Ok(Disk { tracks, half_tracks })
    }
}

/// The first `count` bits of `bytes`, from the first byte's top bit; none
/// if it hasn't as many.
fn bits(bytes: &[u8], count: usize) -> Option<Vec<u8>> {
    (count <= bytes.len() * 8).then(|| (0..count).map(|i| (bytes[i / 8] >> (7 - i % 8)) & 1).collect())
}

/// The CRC-32 a .woz has for all after its header: the one zip has.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
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
