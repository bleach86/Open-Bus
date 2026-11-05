//! Zip archives, read: a file in one, found by the archive's central
//! directory, then inflated (by miniz_oxide) if it was deflated, or as it
//! is if it was stored, and its CRC checked. No more of zip than that: not
//! ZIP64, nor encryption, nor an archive in parts.

/// A file in an archive, as its central directory has it.
struct Entry {
    name: String,
    method: usize,
    crc: usize,
    packed: usize,
    size: usize,
    local: usize,
}

/// What's in the zip `archive`, by its central directory.
fn entries(archive: &[u8]) -> Result<Vec<Entry>, String> {
    let u16_at = |at: usize| archive.get(at..at + 2).map(|bytes| u16::from_le_bytes(bytes.try_into().unwrap()) as usize);
    let u32_at = |at: usize| archive.get(at..at + 4).map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()) as usize);
    // The end of the central directory: its record, the last thing in the
    // archive but a comment, up to 64K.
    let end = (0..archive.len().saturating_sub(21))
        .rev()
        .take(22 + 0xFFFF)
        .find(|&at| u32_at(at) == Some(0x0605_4B50))
        .ok_or("not a zip")?;
    let (count, mut at) = (u16_at(end + 10).ok_or("not a zip")?, u32_at(end + 16).ok_or("not a zip")?);
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        if u32_at(at) != Some(0x0201_4B50) {
            return Err("its central directory broken".to_string());
        }
        let field = |offset: usize| u32_at(at + offset).ok_or("its central directory cut short");
        let (method, crc, packed, size) = (u16_at(at + 10).unwrap_or(0), field(16)?, field(20)?, field(24)?);
        let (name_len, extra_len, comment_len) = (u16_at(at + 28).unwrap_or(0), u16_at(at + 30).unwrap_or(0), u16_at(at + 32).unwrap_or(0));
        let local = field(42)?;
        let name = String::from_utf8_lossy(archive.get(at + 46..at + 46 + name_len).ok_or("a name cut short")?).into_owned();
        at += 46 + name_len + extra_len + comment_len;
        entries.push(Entry { name, method, crc, packed, size, local });
    }
    Ok(entries)
}

/// The bytes of `entry`, of `archive`.
fn read(archive: &[u8], entry: &Entry) -> Result<Vec<u8>, String> {
    let name = &entry.name;
    let u16_at = |at: usize| archive.get(at..at + 2).map_or(0, |bytes| u16::from_le_bytes(bytes.try_into().unwrap()) as usize);
    // Its local header, then its data.
    if archive.get(entry.local..entry.local + 4) != Some(&[0x50, 0x4B, 0x03, 0x04]) {
        return Err(format!("{name}: its header broken"));
    }
    let start = entry.local + 30 + u16_at(entry.local + 26) + u16_at(entry.local + 28);
    let data = archive.get(start..start + entry.packed).ok_or_else(|| format!("{name}: cut short"))?;
    let bytes = match entry.method {
        0 => data.to_vec(),
        8 => miniz_oxide::inflate::decompress_to_vec(data).map_err(|e| format!("{name}: {e:?}"))?,
        method => return Err(format!("{name}: compressed by method {method}, not stored or deflated")),
    };
    if bytes.len() != entry.size || crc32(&bytes) as usize != entry.crc {
        return Err(format!("{name}: not as it was zipped, its size or CRC"));
    }
    Ok(bytes)
}

/// The first file in the zip `archive` that `wanted` takes by its name:
/// its name, and its bytes.
pub fn file(archive: &[u8], wanted: impl Fn(&str) -> bool) -> Result<(String, Vec<u8>), String> {
    let entries = entries(archive)?;
    let entry = entries.iter().find(|entry| wanted(&entry.name)).ok_or("none such in it")?;
    Ok((entry.name.clone(), read(archive, entry)?))
}

/// The files of the zip `archive` of at most `most` bytes, by their names
/// and bytes; those it can't read left out.
pub fn small_files(archive: &[u8], most: usize) -> Result<Vec<(String, Vec<u8>)>, String> {
    let entries = entries(archive)?;
    let small = entries.iter().filter(|entry| entry.size <= most);
    Ok(small.filter_map(|entry| Some((entry.name.clone(), read(archive, entry).ok()?))).collect())
}

/// The CRC-32 zip has (a .woz has it too).
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
