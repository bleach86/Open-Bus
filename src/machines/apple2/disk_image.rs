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
//!
//! An image file that can't be written, read only, is a disk write
//! protected, its notch covered. Written to, a disk is saved as it was read: a .dsk or .po by its
//! sectors, read back off its tracks, every one of which must read; a .nib
//! by its tracks' bytes; a .woz by its bits, as a WOZ2.

use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::path::{Path, PathBuf};

pub const TRACKS: usize = 35;
const NIB_TRACK: usize = 6656;
const SECTORS_TRACK: usize = 16 * 256;
/// The half-tracks a .woz maps: 40 tracks' worth.
const WOZ_HALF_TRACKS: usize = 80;
/// A WOZ1 track: its bits in 6,646 bytes, then how many, and the rest.
const WOZ1_TRACK: usize = 6656;
/// WOZ2's TRKS: where its 160 tracks are, 8 bytes each, then their bits.
const WOZ2_ENTRIES: usize = 160;

/// The bits in a turn of the disk, where there is no track yet: at 300
/// rpm, a bit every 4 microseconds.
pub const REVOLUTION_BITS: usize = 50_000;

/// A disk: its tracks' bits, one a byte (0 or 1); and at each half-track,
/// which track is under the head, if one is.
#[derive(Serialize, Deserialize)]
pub struct Disk {
    pub tracks: Vec<Vec<u8>>,
    pub half_tracks: Vec<Option<usize>>,
    /// Its write-protect notch covered: the drive won't write it.
    pub write_protected: bool,
    /// Written since it was read from its image, or last saved to it.
    pub written: bool,
    /// What it was read from, to be saved as; and the image file, to be
    /// saved to, if it was opened from one.
    format: Format,
    pub path: Option<PathBuf>,
}

/// The kind of image a disk was read from, and what saving it needs that
/// its tracks don't keep.
#[derive(Serialize)]
enum Format {
    Nib,
    /// A .dsk or .po: its sectors' order, and which it is; in a save
    /// state, which it is.
    #[serde(serialize_with = "save_sectors")]
    Sectors(&'static [usize; 16], &'static str),
    /// A .woz: its map's quarter-tracks between the half-tracks, its INFO
    /// and its META, if it had one.
    Woz { quarters: Vec<u8>, info: Vec<u8>, meta: Option<Vec<u8>> },
}

/// A .dsk's or .po's format, saved: which it is.
fn save_sectors<S: Serializer>(_order: &&'static [usize; 16], kind: &&'static str, serializer: S) -> Result<S::Ok, S::Error> {
    kind.serialize(serializer)
}

/// A format as a save state has it, a .dsk's or .po's by which it is:
/// loaded as this, then made the format, as serde would borrow a
/// `&'static str` from the state.
#[derive(Deserialize)]
enum SavedFormat {
    Nib,
    Sectors(String),
    Woz { quarters: Vec<u8>, info: Vec<u8>, meta: Option<Vec<u8>> },
}

impl<'de> Deserialize<'de> for Format {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Format, D::Error> {
        Ok(match SavedFormat::deserialize(deserializer)? {
            SavedFormat::Nib => Format::Nib,
            SavedFormat::Sectors(kind) => match kind.as_str() {
                ".dsk" => Format::Sectors(&DOS_ORDER, ".dsk"),
                ".po" => Format::Sectors(&PRODOS_ORDER, ".po"),
                _ => return Err(D::Error::custom(format!("{kind}: not a .dsk or .po"))),
            },
            SavedFormat::Woz { quarters, info, meta } => Format::Woz { quarters, info, meta },
        })
    }
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

/// The kinds of image a disk is read from, by their files' extensions.
const KINDS: [&str; 5] = ["dsk", "do", "po", "nib", "woz"];

/// The disk images `path` means: an image; a folder's images, in their
/// names' order (their numbers as numbers: disk 2 before disk 10), its
/// other files left out; or an .m3u playlist's, an image a line, from the
/// playlist's folder, lines starting # left out.
pub fn images(path: &Path) -> Result<Vec<PathBuf>, String> {
    let extension = path.extension().map_or(String::new(), |kind| kind.to_string_lossy().to_lowercase());
    if path.is_dir() {
        let entries = std::fs::read_dir(path).map_err(|e| e.to_string())?;
        let mut images: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|file| file.extension().is_some_and(|kind| KINDS.contains(&kind.to_string_lossy().to_lowercase().as_str())))
            .collect();
        images.sort_by_key(|file| name_order(&file.file_name().unwrap_or_default().to_string_lossy()));
        Ok(images)
    } else if extension == "m3u" {
        let list = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let folder = path.parent().unwrap_or(Path::new(""));
        Ok(list.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')).map(|line| folder.join(line)).collect())
    } else {
        Ok(vec![path.to_path_buf()])
    }
}

/// A name's order: its runs of digits as numbers, the rest as letters,
/// case aside.
fn name_order(name: &str) -> Vec<(u64, String)> {
    let mut order = Vec::new();
    let mut chars = name.chars().peekable();
    while let Some(&c) = chars.peek() {
        let digits = c.is_ascii_digit();
        let mut run = String::new();
        while let Some(&c) = chars.peek().filter(|c| c.is_ascii_digit() == digits) {
            run.push(c);
            chars.next();
        }
        order.push(if digits { (run.parse().unwrap_or(u64::MAX), String::new()) } else { (0, run.to_lowercase()) });
    }
    order
}

impl Disk {
    /// The disk in the image at `path`, by its kind: a .dsk or .do, .po,
    /// .nib or .woz; write protected if the file is read only.
    pub fn open(path: &Path) -> Result<Disk, String> {
        let image = std::fs::read(path).map_err(|e| e.to_string())?;
        let kind = path.extension().map_or(String::new(), |kind| kind.to_string_lossy().to_lowercase());
        let mut disk = match kind.as_str() {
            "dsk" | "do" => Disk::from_dsk(&image),
            "po" => Disk::from_po(&image),
            "nib" => Disk::from_nib(&image),
            "woz" => Disk::from_woz(&image),
            _ => Err("not a .dsk, .do, .po, .nib or .woz".to_string()),
        }?;
        let read_only = std::fs::metadata(path).map_err(|e| e.to_string())?.permissions().readonly();
        disk.write_protected |= read_only;
        disk.path = Some(path.to_path_buf());
        Ok(disk)
    }

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
        Ok(Disk::whole_tracks(tracks.collect(), Format::Nib))
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
    fn from_sectors(image: &[u8], order: &'static [usize; 16], kind: &'static str) -> Result<Disk, String> {
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
        Ok(Disk::whole_tracks(tracks.collect(), Format::Sectors(order, kind)))
    }

    /// The track under the head at `half_track`, if one is: its bits.
    pub fn track(&self, half_track: usize) -> Option<&Vec<u8>> {
        self.half_tracks.get(half_track).copied().flatten().map(|track| &self.tracks[track])
    }

    /// Bit `bit` of the track at `half_track` written: a flux change, or
    /// none. Where there's no track, one is begun, a turn of no flux
    /// changes, in a track left empty, or a new one.
    pub fn write_bit(&mut self, half_track: usize, bit: usize, value: u8) {
        let track = match self.half_tracks[half_track] {
            Some(track) => track,
            None => {
                let track = self.tracks.iter().position(Vec::is_empty).unwrap_or_else(|| {
                    self.tracks.push(Vec::new());
                    self.tracks.len() - 1
                });
                self.tracks[track] = vec![0; REVOLUTION_BITS];
                self.half_tracks[half_track] = Some(track);
                track
            }
        };
        let bits = &mut self.tracks[track];
        let len = bits.len();
        bits[bit % len] = value;
        self.written = true;
    }

    /// A disk with `tracks` under the head at whole tracks, none between.
    fn whole_tracks(tracks: Vec<Vec<u8>>, format: Format) -> Disk {
        let half_tracks = (0..2 * tracks.len()).map(|half| (half % 2 == 0).then_some(half / 2)).collect();
        Disk { tracks, half_tracks, write_protected: false, written: false, format, path: None }
    }

    /// The disk as an image of the kind it was read from.
    pub fn to_image(&self) -> Result<Vec<u8>, String> {
        match &self.format {
            Format::Nib => self.to_nib(),
            Format::Sectors(order, kind) => self.to_sectors(order, kind),
            Format::Woz { quarters, info, meta } => Ok(self.to_woz(quarters, info, meta.as_deref())),
        }
    }

    /// Each track's bytes, a turn of them, padded with FFs to 6,656.
    fn to_nib(&self) -> Result<Vec<u8>, String> {
        let mut image = Vec::new();
        for track in 0..TRACKS {
            let bytes = self.track(2 * track).map_or(Vec::new(), |bits| turn_of_bytes(bits));
            if bytes.len() > NIB_TRACK {
                return Err(format!("track {track} is {} bytes, more than a .nib's {NIB_TRACK}", bytes.len()));
            }
            image.extend(&bytes);
            image.resize((track + 1) * NIB_TRACK, 0xFF);
        }
        Ok(image)
    }

    /// Each track's 16 sectors read back off it, put in `order`.
    fn to_sectors(&self, order: &[usize; 16], kind: &str) -> Result<Vec<u8>, String> {
        let mut image = vec![0; TRACKS * SECTORS_TRACK];
        for track in 0..TRACKS {
            let sectors = self.track(2 * track).map_or([None; 16], |bits| read_sectors(bits, track as u8));
            for (physical, data) in sectors.iter().enumerate() {
                let data = data.ok_or(format!("track {track}, sector {physical}, doesn't read: not a {kind}'s"))?;
                let at = track * SECTORS_TRACK + order[physical] * 256;
                image[at..at + 256].copy_from_slice(&data);
            }
        }
        Ok(image)
    }

    /// A WOZ2: INFO, as it was but for its version, write protect, its
    /// creator and its largest track; TMAP, the half-tracks' and the
    /// quarter-tracks' between; TRKS, from block 3; and META, if it had one.
    fn to_woz(&self, quarters: &[u8], old_info: &[u8], meta: Option<&[u8]>) -> Vec<u8> {
        let tmap: Vec<u8> = (0..160)
            .map(|quarter| match quarter % 2 {
                0 => self.half_tracks.get(quarter / 2).copied().flatten().map_or(0xFF, |track| track as u8),
                _ => quarters[quarter],
            })
            .collect();
        let mut entries = Vec::new();
        let mut data = Vec::new();
        for n in 0..WOZ2_ENTRIES {
            let bits = self.tracks.get(n).map_or(&[][..], Vec::as_slice);
            let mut bytes: Vec<u8> = bits.chunks(8).map(|byte| byte.iter().enumerate().map(|(i, &bit)| bit << (7 - i)).sum()).collect();
            bytes.resize(bytes.len().div_ceil(512) * 512, 0);
            let start = if bits.is_empty() { 0 } else { 3 + data.len() / 512 };
            entries.extend((start as u16).to_le_bytes());
            entries.extend(((bytes.len() / 512) as u16).to_le_bytes());
            entries.extend((bits.len() as u32).to_le_bytes());
            data.extend(bytes);
        }
        let largest = self.tracks.iter().map(|bits| bits.len().div_ceil(8).div_ceil(512)).max().unwrap_or(0);
        let mut info = vec![0; 60];
        info[..5].copy_from_slice(&[2, 1, self.write_protected as u8, old_info[3], old_info[4]]);
        info[5..37].copy_from_slice(&format!("{:<32}", "cpu_emu").into_bytes());
        info[37] = 1; // a side
        if old_info[0] >= 2 {
            info[38..44].copy_from_slice(&old_info[38..44]);
        } else {
            info[39] = 32; // a bit every 4 microseconds, in 125 ns
        }
        info[44..46].copy_from_slice(&(largest as u16).to_le_bytes());

        let chunk = |id: &[u8], body: &[u8]| [id, &(body.len() as u32).to_le_bytes(), body].concat();
        let mut body = [chunk(b"INFO", &info), chunk(b"TMAP", &tmap), chunk(b"TRKS", &[entries, data].concat())].concat();
        if let Some(meta) = meta {
            body.extend(chunk(b"META", meta));
        }
        [&b"WOZ2\xFF\n\r\n"[..], &crc32(&body).to_le_bytes(), &body].concat()
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
        let (mut info, mut tmap, mut trks, mut meta) = (None, None, None, None);
        let mut at = 12;
        while at + 8 <= image.len() {
            let size = u32::from_le_bytes(image[at + 4..at + 8].try_into().unwrap()) as usize;
            let body = image.get(at + 8..at + 8 + size).ok_or("a chunk cut short")?;
            match &image[at..at + 4] {
                b"INFO" => info = Some(body),
                b"TMAP" => tmap = Some(body),
                b"TRKS" => trks = Some(body),
                b"META" => meta = Some(body.to_vec()),
                _ => {}
            }
            at += 8 + size;
        }
        let (Some(info), Some(tmap), Some(trks)) = (info, tmap, trks) else {
            return Err("no INFO, TMAP or TRKS".to_string());
        };
        if info.len() < 60 || tmap.len() < 160 {
            return Err("its INFO or TMAP cut short".to_string());
        }
        if info[1] != 1 {
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
        let format = Format::Woz { quarters: tmap[..160].to_vec(), info: info.to_vec(), meta };
        Ok(Disk { tracks, half_tracks, write_protected: info[2] != 0, written: false, format, path: None })
    }
}

/// A track's bytes as the card reads them, a turn of them: each from a 1
/// after any 0s, 8 bits; the second time round, so begun in step.
fn turn_of_bytes(bits: &[u8]) -> Vec<u8> {
    let len = bits.len();
    let (mut bytes, mut register) = (Vec::new(), 0u8);
    for at in 0..2 * len {
        register = register << 1 | bits[at % len];
        if register & 0x80 != 0 {
            if at >= len + 7 {
                bytes.push(register);
            }
            register = 0;
        }
    }
    bytes
}

/// The 16 sectors on a track's bits, by their address fields' sector
/// numbers, each that reads: an address field for this track, its checksum
/// right, then its data field, its checksum right and its epilogue's first
/// two bytes there, as DOS 3.3 reads them.
fn read_sectors(bits: &[u8], track: u8) -> [Option<[u8; 256]>; 16] {
    // Twice round, so a sector across the track's start is whole.
    let mut twice = bits.to_vec();
    twice.extend_from_slice(bits);
    let bytes = turn_of_bytes(&twice);
    let four_and_four = |at: usize| ((bytes[at] << 1) | 1) & bytes[at + 1];
    let mut sectors = [None; 16];
    for at in 0..bytes.len().saturating_sub(14) {
        if bytes[at..at + 3] != [0xD5, 0xAA, 0x96] {
            continue;
        }
        let (volume, on, sector, sum) = (four_and_four(at + 3), four_and_four(at + 5), four_and_four(at + 7), four_and_four(at + 9));
        if volume ^ on ^ sector != sum || on != track || sector > 15 || sectors[sector as usize].is_some() {
            continue;
        }
        // The data field: within the gap after, as DOS looks for it.
        let data = (at + 14..(at + 14 + 32).min(bytes.len().saturating_sub(348))).find(|&d| bytes[d..d + 3] == [0xD5, 0xAA, 0xAD]);
        if let Some(data) = data.and_then(|d| data_from_field(&bytes[d + 3..d + 348])) {
            sectors[sector as usize] = Some(data);
        }
    }
    sectors
}

/// A data field's 345 bytes after its prologue back to its 256: 6-and-2
/// undone, data_field's way back; none if a byte isn't a disk byte, its
/// checksum is wrong or its epilogue isn't DE AA.
fn data_from_field(field: &[u8]) -> Option<[u8; 256]> {
    let mut values = [0u8; 342];
    let mut last = 0;
    for (value, &byte) in values.iter_mut().zip(&field[..342]) {
        last ^= TRANSLATE.iter().position(|&t| t == byte)? as u8;
        *value = last;
    }
    if TRANSLATE[last as usize] != field[342] || field[343..345] != [0xDE, 0xAA] {
        return None;
    }
    let swap = |b: u8| ((b & 1) << 1) | ((b >> 1) & 1);
    let mut data = [0u8; 256];
    for (i, byte) in data.iter_mut().enumerate() {
        *byte = values[86 + i] << 2 | swap((values[i % 86] >> (2 * (i / 86))) & 3);
    }
    Some(data)
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
