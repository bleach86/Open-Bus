//! The 7800's cartridges: their boards as a header names them, and each
//! board's banks, at $4000 to $FFFF.

#[cfg(test)]
mod tests {
    use crate::machines::atari7800::cartridge::{Cartridge, Kind};

    /// A ROM of `banks` 16K banks, each byte its bank's number.
    fn banked(banks: usize) -> Vec<u8> {
        (0..banks * 0x4000).map(|at| (at / 0x4000) as u8).collect()
    }

    #[test]
    fn a_header_names_its_board_and_its_pokey() {
        assert_eq!(Kind::headed(0x0000), (Kind::Plain, None));
        assert_eq!(Kind::headed(0x0001), (Kind::Plain, Some(0x4000)));
        assert_eq!(Kind::headed(0x0002), (Kind::SuperGame, None));
        assert_eq!(Kind::headed(0x0003), (Kind::SuperGame, Some(0x4000)));
        assert_eq!(Kind::headed(0x0006), (Kind::SuperGameRam, None));
        assert_eq!(Kind::headed(0x000A), (Kind::SuperGame9, None));
        assert_eq!(Kind::headed(0x0100), (Kind::Activision, None));
        assert_eq!(Kind::headed(0x0200), (Kind::Absolute, None));
        assert_eq!(Kind::headed(0x0080), (Kind::MirrorRam, None));
        assert_eq!(Kind::headed(0x0040), (Kind::Plain, Some(0x0450)));
        assert_eq!(Kind::headed(0x8002), (Kind::SuperGame, Some(0x0800)));
        assert_eq!(
            Kind::headed(0x0007),
            (Kind::SuperGameRam, None),
            "a POKEY and RAM at $4000: the RAM"
        );
        assert_eq!(
            Kind::headed(0x2000),
            (Kind::Plain, None),
            "a bankset's, unknown"
        );
    }

    #[test]
    fn plain_is_at_the_top_up_to_48k() {
        let rom: Vec<u8> = (0..0x4000).map(|at| (at >> 8) as u8).collect();
        let cartridge = Cartridge::new(&rom, Kind::Plain, None).unwrap();
        assert_eq!(cartridge.read(0xC000), 0x00);
        assert_eq!(cartridge.read(0xFFFF), 0x3F);
        assert_eq!(cartridge.read(0x8000), 0xFF, "nothing below it");
        let big = Cartridge::new(&banked(4), Kind::Plain, None).unwrap();
        assert_eq!(
            (big.read(0x4000), big.read(0xC000)),
            (1, 3),
            "64K: its top 48K"
        );
    }

    #[test]
    fn supergame_switches_8000_its_last_bank_at_c000_and_the_one_before_at_4000() {
        let mut cartridge = Cartridge::new(&banked(8), Kind::SuperGame, None).unwrap();
        assert_eq!(cartridge.read(0xC000), 7);
        assert_eq!(cartridge.read(0x4000), 6);
        assert_eq!(cartridge.read(0x8000), 0);
        cartridge.write(0x8000, 3);
        assert_eq!(cartridge.read(0xBFFF), 3);
        cartridge.write(0xA123, 0x0D);
        assert_eq!(cartridge.read(0x8000), 5, "as many bits as there are banks");
    }

    #[test]
    fn supergame_with_ram_has_it_at_4000() {
        let mut cartridge = Cartridge::new(&banked(8), Kind::SuperGameRam, None).unwrap();
        cartridge.write(0x4000, 0x5A);
        cartridge.write(0x7FFF, 0xA5);
        assert_eq!(
            (cartridge.read(0x4000), cartridge.read(0x7FFF)),
            (0x5A, 0xA5)
        );
        cartridge.write(0x8000, 2);
        assert_eq!(cartridge.read(0x8000), 2);
    }

    #[test]
    fn supergame_of_9_banks_has_bank_0_at_4000() {
        let mut cartridge = Cartridge::new(&banked(9), Kind::SuperGame9, None).unwrap();
        assert_eq!(cartridge.read(0x4000), 0);
        assert_eq!(cartridge.read(0xC000), 8);
        cartridge.write(0x8000, 0);
        assert_eq!(cartridge.read(0x8000), 1, "banks 1 to 8 switched");
        cartridge.write(0x8000, 6);
        assert_eq!(cartridge.read(0x8000), 7);
    }

    #[test]
    fn absolute_switches_4000_by_a_write_at_8000() {
        let mut cartridge = Cartridge::new(&banked(4), Kind::Absolute, None).unwrap();
        assert_eq!(cartridge.read(0x4000), 0);
        assert_eq!((cartridge.read(0x8000), cartridge.read(0xC000)), (2, 3));
        cartridge.write(0x8000, 2);
        assert_eq!(cartridge.read(0x4000), 1);
        cartridge.write(0x8000, 1);
        assert_eq!(cartridge.read(0x4000), 0);
    }

    #[test]
    fn activision_has_its_8k_pieces_in_their_own_order() {
        // Each byte its 8K piece's number.
        let rom: Vec<u8> = (0..0x20000).map(|at| (at / 0x2000) as u8).collect();
        let mut cartridge = Cartridge::new(&rom, Kind::Activision, None).unwrap();
        let pieces = |cartridge: &Cartridge| {
            [0x4000, 0x6000, 0x8000, 0xA000, 0xC000, 0xE000].map(|addr| cartridge.read(addr))
        };
        assert_eq!(pieces(&cartridge), [13, 12, 15, 0, 1, 14]);
        cartridge.write(0xFF83, 0);
        assert_eq!(pieces(&cartridge), [13, 12, 15, 6, 7, 14], "bank 3");
    }

    #[test]
    fn rescue_on_fractalus_has_ram_at_4000_every_other_page_the_same() {
        let mut cartridge = Cartridge::new(&[0xEA; 0x8000], Kind::MirrorRam, None).unwrap();
        cartridge.write(0x4010, 0x42);
        assert_eq!(cartridge.read(0x4110), 0x42);
        assert_eq!(cartridge.read(0x4210), 0x00);
        assert_eq!(cartridge.read(0x8000), 0xEA);
    }
}
