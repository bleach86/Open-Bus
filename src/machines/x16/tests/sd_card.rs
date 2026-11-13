//! The SD card: started as the KERNAL starts one; its blocks read, one or
//! on till it's stopped, and written to its image, but a read only one's;
//! out of range; unselected, or with no image, nothing; and reached through
//! VERA's SPI port.

#[cfg(test)]
mod tests {
    use crate::machines::x16::sd_card::{Image, SdCard};
    use crate::machines::x16::vera::Vera;
    use std::path::{Path, PathBuf};

    const BLOCKS: usize = 2048; // 1M

    /// An image of `BLOCKS` blocks in a temporary file, block n's bytes n.
    fn image(name: &str) -> PathBuf {
        let file = std::env::temp_dir().join(format!("cpu_emu_{}_{name}.img", std::process::id()));
        let bytes: Vec<u8> = (0..BLOCKS * 512).map(|at| (at / 512) as u8).collect();
        std::fs::write(&file, bytes).unwrap();
        file
    }

    fn card(file: &Path) -> SdCard {
        let mut card = SdCard::new();
        card.image = Some(Image::open(file).unwrap());
        card.select();
        card
    }

    /// Command `n`, its argument `argument`, sent: what came back, $FFs.
    fn command(card: &mut SdCard, n: u8, argument: u32) -> Vec<u8> {
        let mut bytes = vec![0x40 | n];
        bytes.extend(argument.to_be_bytes());
        bytes.push(0x95);
        bytes.into_iter().map(|byte| card.exchange(byte)).collect()
    }

    /// `count` bytes of its response, $FFs sent for them.
    fn response(card: &mut SdCard, count: usize) -> Vec<u8> {
        (0..count).map(|_| card.exchange(0xFF)).collect()
    }

    #[test]
    fn it_starts_as_an_sdhc_card_its_size_in_its_csd() {
        let file = image("start");
        let mut card = card(&file);
        assert_eq!(command(&mut card, 0, 0), [0xFF; 6]);
        assert_eq!(response(&mut card, 2), [0x01, 0xFF], "idle, then nothing");
        command(&mut card, 8, 0x1AA);
        assert_eq!(response(&mut card, 5), [0x01, 0x00, 0x00, 0x01, 0xAA]);
        command(&mut card, 13, 0);
        assert_eq!(response(&mut card, 2), [0x1F, 0xFF], "not started");
        command(&mut card, 55, 0);
        assert_eq!(response(&mut card, 1), [0x01]);
        command(&mut card, 41, 0x4000_0000);
        assert_eq!(response(&mut card, 1), [0x00], "started");
        command(&mut card, 58, 0);
        assert_eq!(response(&mut card, 4), [0xC0, 0xFF, 0x80, 0x00], "SDHC");
        command(&mut card, 9, 0);
        let csd = response(&mut card, 21);
        assert_eq!(csd[2..5], [0x00, 0xFF, 0xFE]);
        assert_eq!(csd[12..15], [0, 0, 1], "two of 512K, less one");
        std::fs::remove_file(&file).unwrap();
    }

    #[test]
    fn its_blocks_are_read_one_or_on_till_its_stopped() {
        let file = image("read");
        let mut card = card(&file);
        command(&mut card, 17, 5);
        let block = response(&mut card, 1 + 1 + 512 + 2);
        assert_eq!(block[..2], [0x00, 0xFE], "R1, the data token");
        assert!(block[2..514].iter().all(|&byte| byte == 5));
        assert_eq!(response(&mut card, 1), [0xFF], "done");
        command(&mut card, 18, 7);
        assert_eq!(response(&mut card, 3), [0x00, 0xFE, 7]);
        response(&mut card, 511 + 2);
        assert_eq!(response(&mut card, 2), [0xFE, 8], "the next block");
        response(&mut card, 511 + 2);
        command(&mut card, 12, 0);
        assert_eq!(
            response(&mut card, 2),
            [0x01, 0xFF],
            "stopped: idle, never started"
        );
        command(&mut card, 17, BLOCKS as u32);
        assert_eq!(response(&mut card, 3), [0x00, 0x08, 0xFF], "out of range");
        std::fs::remove_file(&file).unwrap();
    }

    #[test]
    fn a_block_written_is_written_to_its_image_but_a_read_only_ones() {
        let file = image("write");
        let mut card = card(&file);
        command(&mut card, 24, 3);
        assert_eq!(response(&mut card, 1), [0x01]);
        for byte in [0xFE].into_iter().chain([0xA5; 512]).chain([0, 0]) {
            card.exchange(byte);
        }
        let written = std::fs::read(&file).unwrap();
        assert!(written[3 * 512..4 * 512].iter().all(|&byte| byte == 0xA5));
        assert_eq!((written[3 * 512 - 1], written[4 * 512]), (2, 4));
        assert!(card.used);

        let mut permissions = std::fs::metadata(&file).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&file, permissions.clone()).unwrap();
        let mut card = self::card(&file);
        command(&mut card, 24, 4);
        response(&mut card, 1);
        for byte in [0xFE].into_iter().chain([0x5A; 512]).chain([0, 0]) {
            card.exchange(byte);
        }
        assert_eq!(std::fs::read(&file).unwrap(), written, "not written");
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        std::fs::set_permissions(&file, permissions).unwrap();
        std::fs::remove_file(&file).unwrap();
    }

    #[test]
    fn unselected_or_with_no_image_its_nothing() {
        let file = image("none");
        let mut card = SdCard::new();
        card.image = Some(Image::open(&file).unwrap());
        command(&mut card, 0, 0);
        assert_eq!(response(&mut card, 1), [0xFF], "unselected");
        let mut card = SdCard::new();
        card.select();
        command(&mut card, 0, 0);
        assert_eq!(response(&mut card, 1), [0xFF], "no image");
        let small = std::env::temp_dir().join(format!("cpu_emu_{}_small", std::process::id()));
        std::fs::write(&small, [0; 1000]).unwrap();
        assert!(Image::open(&small).is_err(), "not whole blocks");
        std::fs::remove_file(&small).unwrap();
        std::fs::remove_file(&file).unwrap();
    }

    #[test]
    fn its_reached_through_veras_spi_port() {
        const SPI_DATA: u8 = 0x1E;
        const SPI_CTRL: u8 = 0x1F;
        let file = image("spi");
        let mut vera = Vera::new();
        vera.sd_card().image = Some(Image::open(&file).unwrap());
        vera.write(SPI_CTRL, 0x01);
        let exchange = |vera: &mut Vera, byte: u8| {
            vera.write(SPI_DATA, byte);
            while vera.read(SPI_CTRL) & 0x80 != 0 {
                vera.tick();
            }
            vera.read(SPI_DATA)
        };
        for byte in [0x40, 0, 0, 0, 0, 0x95] {
            exchange(&mut vera, byte);
        }
        assert_eq!(exchange(&mut vera, 0xFF), 0x01, "CMD0: idle");
        // Auto-TX: each read sends an $FF for the next.
        for byte in [0x51, 0, 0, 0, 9, 0x01] {
            exchange(&mut vera, byte);
        }
        vera.write(SPI_CTRL, 0x05);
        vera.write(SPI_DATA, 0xFF);
        let next = |vera: &mut Vera| {
            while vera.read(SPI_CTRL) & 0x80 != 0 {
                vera.tick();
            }
            vera.read(SPI_DATA)
        };
        assert_eq!(
            [next(&mut vera), next(&mut vera), next(&mut vera)],
            [0x00, 0xFE, 9]
        );
        std::fs::remove_file(&file).unwrap();
    }
}
