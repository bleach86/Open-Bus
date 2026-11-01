//! Zip archives, read: a file in one, found by the archive's central
//! directory, then inflated (by miniz_oxide) if it was deflated, or as it
//! is if it was stored, and its CRC checked. No more of zip than that: not
//! ZIP64, nor encryption, nor an archive in parts.

/// The first file in the zip `archive` that `wanted` takes by its name:
/// its name, and its bytes.
pub fn file(archive: &[u8], wanted: impl Fn(&str) -> bool) -> Result<(String, Vec<u8>), String> {
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
        if !wanted(&name) {
            continue;
        }
        // Its local header, then its data.
        if u32_at(local) != Some(0x0403_4B50) {
            return Err(format!("{name}: its header broken"));
        }
        let start = local + 30 + u16_at(local + 26).unwrap_or(0) + u16_at(local + 28).unwrap_or(0);
        let data = archive.get(start..start + packed).ok_or_else(|| format!("{name}: cut short"))?;
        let bytes = match method {
            0 => data.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec(data).map_err(|e| format!("{name}: {e:?}"))?,
            _ => return Err(format!("{name}: compressed by method {method}, not stored or deflated")),
        };
        if bytes.len() != size || crc32(&bytes) as usize != crc {
            return Err(format!("{name}: not as it was zipped, its size or CRC"));
        }
        return Ok((name, bytes));
    }
    Err("none such in it".to_string())
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
