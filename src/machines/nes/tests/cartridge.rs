//! The cartridge: an iNES header's sizes, mirroring, battery, trainer and
//! mapper, NES 2.0's and an old header's junk too; NROM's 16K mirrored,
//! its CHR RAM; the nametables' wiring.

#[cfg(test)]
mod tests {
    use crate::machines::nes::cartridge::{Cartridge, Mapper, Mirroring};

    /// An iNES image: its flags 6 to 15, then `prg` 16K banks, each its
    /// number in every byte, and `chr` 8K banks of $C0 and its number.
    fn image(prg: u8, chr: u8, flags: [u8; 10]) -> Vec<u8> {
        let mut image = b"NES\x1A".to_vec();
        image.extend([prg, chr]);
        image.extend(flags);
        if flags[0] & 0x04 != 0 {
            image.extend([0xEE; 512]); // a trainer
        }
        for bank in 0..prg {
            image.extend([bank; 0x4000]);
        }
        for bank in 0..chr {
            image.extend([0xC0 | bank; 0x2000]);
        }
        image
    }

    #[test]
    fn an_ines_header_is_read() {
        let mut cart = Cartridge::new(&image(1, 1, [0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0])).unwrap();
        assert_eq!((cart.mapper.clone(), cart.mirroring, cart.battery), (Mapper::Nrom, Mirroring::Vertical, false));
        assert_eq!((cart.cpu_read(0x8000), cart.cpu_read(0xC000)), (Some(0), Some(0)), "16K at $8000, and again at $C000");
        assert_eq!(cart.chr_read(0x1FFF), 0xC0, "its CHR ROM");
        cart.chr_write(0x0000, 0x12);
        assert_eq!(cart.chr_read(0x0000), 0xC0, "its CHR ROM written");

        // 32K; a battery; a trainer, skipped; horizontal.
        let mut cart = Cartridge::new(&image(2, 1, [0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0])).unwrap();
        assert_eq!((cart.cpu_read(0x8000), cart.cpu_read(0xC000)), (Some(0), Some(1)), "32K, past the trainer");
        assert_eq!((cart.mirroring, cart.battery), (Mirroring::Horizontal, true));
        cart.cpu_write(0x6123, 0x77, 0);
        assert_eq!(cart.cpu_read(0x6123), Some(0x77), "its PRG RAM");
        assert_eq!(cart.cpu_read(0x5000), None, "nothing at $5000");

        // No CHR ROM: 8K of CHR RAM. Four screens.
        let mut cart = Cartridge::new(&image(1, 0, [0x08, 0, 0, 0, 0, 0, 0, 0, 0, 0])).unwrap();
        cart.chr_write(0x1234, 0x56);
        assert_eq!((cart.chr_read(0x1234), cart.mirroring), (0x56, Mirroring::FourScreen), "its CHR RAM");
    }

    #[test]
    fn its_mapper_is_from_flags_6_and_7_or_none_with_junk() {
        let mapper = |flags| Cartridge::new(&image(1, 1, flags)).map(|cart| cart.mapper).map_err(|e| e.to_string());
        assert_eq!(mapper([0x10, 0x40, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap_err(), "mapper 65, which isn't here yet");
        // NES 2.0: byte 8's low nibble its bits 8 to 11.
        assert_eq!(mapper([0x00, 0x08, 0x01, 0, 0, 0, 0, 0, 0, 0]).unwrap_err(), "mapper 256, which isn't here yet");
        // An old header, a ripper's name in its last bytes: flags 7 junk too.
        assert_eq!(mapper([0x00, 0x44, 0, 0, 0, 0, b'D', b'i', b's', b'k']).unwrap(), Mapper::Nrom);
        assert!(Cartridge::new(b"not an image").is_err());
        assert!(Cartridge::new(&image(2, 1, [0; 10])[..0x5000]).is_err(), "cut short");
    }

    #[test]
    fn the_nametables_are_wired_as_the_board_is() {
        let pages = |mirroring: Mirroring| [0x2000, 0x2400, 0x2800, 0x2C00, 0x3000].map(|addr| mirroring.page(addr));
        assert_eq!(pages(Mirroring::Horizontal), [0, 0, 1, 1, 0]);
        assert_eq!(pages(Mirroring::Vertical), [0, 1, 0, 1, 0]);
        assert_eq!(pages(Mirroring::SingleLower), [0; 5]);
        assert_eq!(pages(Mirroring::SingleUpper), [1; 5]);
        assert_eq!(pages(Mirroring::FourScreen), [0, 1, 2, 3, 0]);
    }
}
