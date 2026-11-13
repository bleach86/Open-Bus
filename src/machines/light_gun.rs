//! What a light gun's photodiode sees, whichever console's it is: a dot
//! bright enough, its red, green and blue adding up to 85 or more, of 765,
//! as Mesen has the Zapper's.

const BRIGHT: u32 = 85;

/// Whether a dot of colour `rgb` (0xRRGGBB) is bright enough to be seen.
pub fn bright(rgb: u32) -> bool {
    (rgb >> 16 & 0xFF) + (rgb >> 8 & 0xFF) + (rgb & 0xFF) >= BRIGHT
}
