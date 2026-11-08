//! The game database: a line read as Mesen reads it, and a game found by
//! its ROMs' CRC-32.

#[cfg(test)]
mod tests {
    use crate::machines::nes::cartridge::Mirroring;
    use crate::machines::nes::database::{self, Known};

    #[test]
    fn a_line_is_a_board() {
        assert_eq!(
            database::game("00890F1E,NesNtsc,,,,4,512,0,8,0,8,1,h,1,,0,,"),
            Some((
                0x0089_0F1E,
                Known {
                    mapper: 4,
                    submapper: Some(0),
                    prg_rom: 512 * 1024,
                    chr_rom: 0,
                    chr_ram: 8 * 1024,
                    prg_ram: 8 * 1024,
                    battery: true,
                    mirroring: Some(Mirroring::Horizontal),
                    famicom: false,
                }
            ))
        );
        // No submapper: not checked, its RAMs only what it adds.
        let (_, unchecked) = database::game("0FB244C8,Famicom,,,,4,128,128,,8,0,0,,,,,,").unwrap();
        assert!(!unchecked.checked());
        assert!(unchecked.famicom, "the Famicom's");
        assert_eq!(
            (unchecked.prg_ram, unchecked.chr_ram, unchecked.mirroring),
            (8 * 1024, 0, None)
        );
        // A comment, and a board known only by its UNIF name: no game.
        assert_eq!(database::game("# Mesen Game Database"), None);
        assert_eq!(
            database::game("01B07343,NesNtsc,BADROM,,,65000,0,,,0,0,0,,,,,,"),
            None
        );
    }

    #[test]
    fn a_game_is_found_by_its_roms_crc() {
        let known = database::find(0x0089_0F1E).expect("in the database");
        assert_eq!((known.mapper, known.prg_rom), (4, 512 * 1024));
        assert!(database::find(0x1234_5678).is_none());
    }
}
