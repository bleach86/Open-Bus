//! Sunsoft-4: its 16K bank, the last fixed; its 2K banks of CHR; its
//! nametables CHR ROM's from its last 128K, as the mirroring has them; its
//! PRG RAM's enable; and Nantettatte!! Baseball's option cartridge, there
//! only while it's licensed.

#[cfg(test)]
mod tests {
    use crate::machines::nes::cartridge::{Cartridge, Mirroring};

    /// Mapper 68: `prg` 16K banks, each its number in every byte, and 256K
    /// of CHR ROM, each 1K its number.
    fn sunsoft4(prg: u8) -> Cartridge {
        let mut image = b"NES\x1A".to_vec();
        image.extend([prg, 32, 0x40, 0x40]);
        image.extend([0; 8]);
        for bank in 0..prg {
            image.extend([bank; 0x4000]);
        }
        for page in 0..256 {
            image.extend([page as u8; 0x400]);
        }
        Cartridge::new(&image).unwrap()
    }

    fn prg(cart: &mut Cartridge) -> [Option<u8>; 2] {
        [0x8000, 0xC000].map(|addr| cart.cpu_read(addr))
    }

    #[test]
    fn its_16k_bank_and_2k_banks_of_chr() {
        let mut cart = sunsoft4(8);
        assert_eq!(cart.mapper_name(), "MAPPER 68 SUNSOFT-4");
        assert_eq!(prg(&mut cart), [Some(0), Some(7)]);
        cart.cpu_write(0xF000, 0x03, 0);
        assert_eq!(prg(&mut cart), [Some(3), Some(7)]);
        for (at, bank) in [0x8000, 0x9000, 0xA000, 0xB000]
            .into_iter()
            .zip([5, 6, 0x40, 0x7F])
        {
            cart.cpu_write(at, bank, 0);
        }
        let pages: Vec<u8> = (0..8).map(|page| cart.chr_read(page * 0x400)).collect();
        assert_eq!(pages, [10, 11, 12, 13, 0x80, 0x81, 0xFE, 0xFF]);
        // Its RAM, there only once it's enabled.
        cart.cpu_write(0x6000, 0x5A, 0);
        assert_eq!(cart.cpu_read(0x6000), None);
        cart.cpu_write(0xF000, 0x10, 0);
        cart.cpu_write(0x6000, 0x5A, 0);
        assert_eq!(cart.cpu_read(0x6000), Some(0x5A));
    }

    #[test]
    fn its_nametables_chr_roms_from_its_last_128k() {
        let mut cart = sunsoft4(8);
        let tables = |cart: &mut Cartridge| {
            [0x2000, 0x2400, 0x2800, 0x2C00].map(|addr| cart.nametable_read(addr))
        };
        assert_eq!(tables(&mut cart), [None; 4], "the PPU's");
        assert!(!cart.watches_fetches());
        cart.cpu_write(0xC000, 0x01, 0);
        cart.cpu_write(0xD000, 0x82, 0);
        cart.cpu_write(0xE000, 0x10, 0);
        assert_eq!(cart.mirroring, Mirroring::Vertical);
        assert_eq!(tables(&mut cart), [0x81, 0x82, 0x81, 0x82].map(Some));
        assert!(cart.watches_fetches(), "its own, not what the PPU has");
        assert!(cart.nametable_write(0x2000, 0x33), "ROM, not written");
        for (value, mirroring, pages) in [
            (0x11, Mirroring::Horizontal, [0x81, 0x81, 0x82, 0x82]),
            (0x12, Mirroring::SingleLower, [0x81; 4]),
            (0x13, Mirroring::SingleUpper, [0x82; 4]),
        ] {
            cart.cpu_write(0xE000, value, 0);
            assert_eq!(cart.mirroring, mirroring);
            assert_eq!(tables(&mut cart), pages.map(Some));
        }
        cart.cpu_write(0xE000, 0x01, 0);
        assert_eq!(tables(&mut cart), [None; 4]);
        assert!(!cart.nametable_write(0x2000, 0x33));
    }

    #[test]
    fn an_option_cartridge_only_while_its_licensed() {
        // Nantettatte!! Baseball's: 128K, and an option cartridge's 16K.
        let mut cart = sunsoft4(9);
        cart.cpu_write(0xF000, 0x0B, 0);
        assert_eq!(prg(&mut cart), [Some(3), Some(7)], "its own");
        cart.cpu_write(0xF000, 0x03, 0);
        assert_eq!(prg(&mut cart), [None, Some(7)], "not licensed");
        cart.cpu_write(0x6000, 0x00, 0);
        assert_eq!(prg(&mut cart), [Some(8), Some(7)], "licensed");
        for _ in 0..1024 * 105 - 1 {
            cart.cycle();
        }
        assert_eq!(prg(&mut cart), [Some(8), Some(7)]);
        cart.cycle();
        assert_eq!(prg(&mut cart), [None, Some(7)], "lapsed");
        // With its RAM on, a write's the RAM's, not a licence.
        cart.cpu_write(0xF000, 0x13, 0);
        cart.cpu_write(0x6000, 0x00, 0);
        assert_eq!(prg(&mut cart), [None, Some(7)]);
    }
}
