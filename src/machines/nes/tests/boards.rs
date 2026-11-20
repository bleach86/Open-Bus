//! The Famicom's boards with registers of their own, a few of each: Jaleco's
//! SS88006, Taito's TC0190, TC0690, X1-005 and X1-017, Irem's G-101 and
//! H3001, and Sunsoft-3. Their banks, mirroring, RAM and IRQs.

#[cfg(test)]
mod tests {
    use crate::machines::nes::cartridge::{Cartridge, Mirroring};

    /// Mapper `number`, submapper `submapper`, by a NES 2.0 header: 256K of
    /// PRG ROM, each 8K its number in every byte; 256K of CHR ROM, each 1K
    /// its number.
    fn cart(number: u16, submapper: u8) -> Cartridge {
        let mut image = b"NES\x1A".to_vec();
        image.extend([
            16,
            32,
            (number as u8 & 0x0F) << 4,
            (number as u8 & 0xF0) | 0x08,
        ]);
        image.extend([submapper << 4 | (number >> 8) as u8, 0, 0, 0, 0, 0, 0, 0]);
        for bank in 0..32 {
            image.extend([bank; 0x2000]);
        }
        for page in 0..=255 {
            image.extend([page; 0x400]);
        }
        Cartridge::new(&image).unwrap()
    }

    /// Its four 8K banks, as their numbers.
    fn prg(cart: &mut Cartridge) -> [Option<u8>; 4] {
        [0x8000, 0xA000, 0xC000, 0xE000].map(|addr| cart.cpu_read(addr))
    }

    /// Its eight 1K pages of CHR, as their numbers.
    fn chr(cart: &mut Cartridge) -> [u8; 8] {
        std::array::from_fn(|page| cart.chr_read(page as u16 * 0x400))
    }

    /// The cycles till its IRQ, up to `most`.
    fn cycles_to_irq(cart: &mut Cartridge, most: usize) -> Option<usize> {
        (1..=most).find(|_| {
            cart.cycle();
            cart.irq()
        })
    }

    #[test]
    fn ss88006s_banks_a_nibble_at_a_time() {
        let mut cart = cart(18, 0);
        assert_eq!(cart.mapper_name(), "MAPPER 18 JALECO SS88006");
        for (addr, value) in [(0x8000, 3), (0x8001, 1), (0x8002, 5), (0x9000, 7)] {
            cart.cpu_write(addr, value, 0);
        }
        assert_eq!(prg(&mut cart), [Some(0x13), Some(5), Some(7), Some(31)]);
        for (addr, value) in [(0xA000, 9), (0xA001, 1), (0xD002, 4), (0xD003, 2)] {
            cart.cpu_write(addr, value, 0);
        }
        assert_eq!(chr(&mut cart), [0x19, 0, 0, 0, 0, 0, 0, 0x24]);
        cart.cpu_write(0xF002, 1, 0);
        assert_eq!(cart.mirroring, Mirroring::Vertical);
        cart.cpu_write(0xF002, 3, 0);
        assert_eq!(cart.mirroring, Mirroring::SingleUpper);
    }

    #[test]
    fn ss88006s_irq_counts_cycles_down_all_16_bits_or_4() {
        let mut cart = cart(18, 0);
        // Its latch a nibble a register: $0110.
        for (addr, value) in [(0xE000, 0), (0xE001, 1), (0xE002, 1), (0xE003, 0)] {
            cart.cpu_write(addr, value, 0);
        }
        cart.cpu_write(0xF000, 0, 0);
        cart.cpu_write(0xF001, 0x01, 0);
        assert_eq!(cycles_to_irq(&mut cart, 0x200), Some(0x110));
        cart.cpu_write(0xF000, 0, 0);
        assert!(!cart.irq(), "let go of");
        // Only its low 4 bits, the rest left: $0 to 0 again in 16.
        cart.cpu_write(0xF001, 0x09, 0);
        assert_eq!(cycles_to_irq(&mut cart, 0x200), Some(16));
    }

    #[test]
    fn tc0190s_banks_and_mirroring() {
        let mut cart = cart(33, 0);
        assert_eq!(cart.mapper_name(), "MAPPER 33 TAITO TC0190");
        cart.cpu_write(0x8000, 0x45, 0);
        cart.cpu_write(0x8001, 0x07, 0);
        assert_eq!(prg(&mut cart), [Some(5), Some(7), Some(30), Some(31)]);
        assert_eq!(cart.mirroring, Mirroring::Horizontal);
        for (addr, value) in [(0x8002, 3), (0x8003, 4), (0xA000, 9), (0xA003, 0x20)] {
            cart.cpu_write(addr, value, 0);
        }
        assert_eq!(chr(&mut cart), [6, 7, 8, 9, 9, 0, 0, 0x20]);
        // Its registers again at $C000 and $E000.
        cart.cpu_write(0xC000, 0x02, 0);
        assert_eq!(
            (cart.cpu_read(0x8000), cart.mirroring),
            (Some(2), Mirroring::Vertical)
        );
    }

    /// A line of the PPU's fetches as A12 has them: low, then high, as the
    /// sprites' come from $1000.
    fn line(cart: &mut Cartridge, dot: &mut u64) {
        cart.ppu_bus(0x0000, *dot);
        cart.ppu_bus(0x1000, *dot + 260);
        *dot += 341;
    }

    #[test]
    fn tc0690s_irq_an_mmc3s_held_back() {
        let mut cart = cart(48, 0);
        assert_eq!(cart.mapper_name(), "MAPPER 48 TAITO TC0690");
        cart.cpu_write(0xE000, 0x40, 0);
        assert_eq!(cart.mirroring, Mirroring::Horizontal);
        // Its latch written inverted: 2 lines.
        cart.cpu_write(0xC000, !2, 0);
        cart.cpu_write(0xC001, 0, 0);
        cart.cpu_write(0xC002, 0, 0);
        let mut dot = 0;
        for _ in 0..2 {
            line(&mut cart, &mut dot);
            assert_eq!(cycles_to_irq(&mut cart, 100), None);
        }
        line(&mut cart, &mut dot);
        assert_eq!(cycles_to_irq(&mut cart, 100), Some(22), "held back");
        cart.cpu_write(0xC003, 0, 0);
        assert!(!cart.irq());
    }

    #[test]
    fn x1_005s_registers_and_ram_are_at_7ef0_up() {
        let mut cart = cart(80, 0);
        assert_eq!(cart.mapper_name(), "MAPPER 80 TAITO X1-005");
        for (addr, value) in [
            (0x7EFA, 3),
            (0x7EFD, 4),
            (0x7EFE, 5),
            (0x7EF0, 6),
            (0x7EF5, 9),
        ] {
            cart.cpu_write(addr, value, 0);
        }
        assert_eq!(prg(&mut cart), [Some(3), Some(4), Some(5), Some(31)]);
        assert_eq!(chr(&mut cart), [6, 7, 0, 1, 0, 0, 0, 9]);
        cart.cpu_write(0x7EF6, 1, 0);
        assert_eq!(cart.mirroring, Mirroring::Vertical);
        // Its 128 bytes of RAM, there only with its key, mirrored once.
        cart.cpu_write(0x7F00, 0x5A, 0);
        assert_eq!(cart.cpu_read(0x7F00), None);
        cart.cpu_write(0x7EF8, 0xA3, 0);
        cart.cpu_write(0x7F00, 0x5A, 0);
        assert_eq!(cart.cpu_read(0x7F80), Some(0x5A));
        assert_eq!(cart.prg_ram.len(), 0x80);
    }

    #[test]
    fn mapper_207s_nametables_are_its_chr_banks_top_bits() {
        let mut cart = cart(207, 0);
        cart.cpu_write(0x7EF0, 0x80, 0);
        cart.cpu_write(0x7EF1, 0x00, 0);
        let pages = [0x2000, 0x2400, 0x2800, 0x2C00].map(|addr| cart.nametable_page(addr));
        assert_eq!(pages, [1, 1, 0, 0]);
        // Its mirroring register isn't there.
        cart.cpu_write(0x7EF6, 1, 0);
        assert_eq!(cart.nametable_page(0x2000), 1);
    }

    #[test]
    fn x1_017s_ram_in_three_parts_each_with_its_key() {
        let mut cart = cart(82, 0);
        assert_eq!(cart.mapper_name(), "MAPPER 82 TAITO X1-017");
        cart.cpu_write(0x7EFA, 0x0C, 0);
        assert_eq!(cart.cpu_read(0x8000), Some(3), "its bank in bits 2 up");
        for (addr, key) in [(0x6000, 0xCA), (0x6800, 0x69), (0x7000, 0x84)] {
            cart.cpu_write(addr, 0x11, 0);
            assert_eq!(cart.cpu_read(addr), None, "{addr:04X} locked");
            cart.cpu_write(0x7EF7 + (addr - 0x6000) / 0x800, key, 0);
            cart.cpu_write(addr, 0x11, 0);
            assert_eq!(cart.cpu_read(addr), Some(0x11), "{addr:04X} open");
        }
        assert_eq!(cart.cpu_read(0x7400), None, "past its 5K");
        // Its 2K banks at $1000, its 1K at $0000, swapped.
        cart.cpu_write(0x7EF0, 0x05, 0);
        cart.cpu_write(0x7EF2, 0x30, 0);
        cart.cpu_write(0x7EF6, 0x02, 0);
        assert_eq!(chr(&mut cart), [0x30, 0, 0, 0, 4, 5, 0, 1]);
    }

    #[test]
    fn g101s_first_bank_at_8000_or_c000() {
        let mut cart = cart(32, 0);
        assert_eq!(cart.mapper_name(), "MAPPER 32 IREM G-101");
        cart.cpu_write(0x8000, 3, 0);
        cart.cpu_write(0xA000, 4, 0);
        assert_eq!(prg(&mut cart), [Some(3), Some(4), Some(30), Some(31)]);
        cart.cpu_write(0x9000, 0x03, 0);
        assert_eq!(prg(&mut cart), [Some(30), Some(4), Some(3), Some(31)]);
        assert_eq!(cart.mirroring, Mirroring::Horizontal);
        cart.cpu_write(0xB007, 0x42, 0);
        assert_eq!(chr(&mut cart)[7], 0x42);
    }

    #[test]
    fn major_leagues_g101_is_one_screen_its_banks_fixed_so() {
        let mut cart = cart(32, 1);
        assert_eq!(cart.mirroring, Mirroring::SingleLower);
        cart.cpu_write(0x8000, 3, 0);
        cart.cpu_write(0x9000, 0x03, 0);
        assert_eq!(cart.mirroring, Mirroring::SingleLower);
        assert_eq!(cart.cpu_read(0x8000), Some(3));
    }

    #[test]
    fn h3001s_banks_and_its_irq_once() {
        let mut cart = cart(65, 0);
        assert_eq!(cart.mapper_name(), "MAPPER 65 IREM H3001");
        assert_eq!(prg(&mut cart), [Some(0), Some(1), Some(30), Some(31)]);
        cart.cpu_write(0xC000, 6, 0);
        cart.cpu_write(0xB003, 0x21, 0);
        assert_eq!((cart.cpu_read(0xC000), chr(&mut cart)[3]), (Some(6), 0x21));
        cart.cpu_write(0x9001, 0x80, 0);
        assert_eq!(cart.mirroring, Mirroring::Horizontal);
        cart.cpu_write(0x9005, 0x01, 0);
        cart.cpu_write(0x9006, 0x02, 0);
        cart.cpu_write(0x9004, 0, 0);
        cart.cpu_write(0x9003, 0x80, 0);
        assert_eq!(cycles_to_irq(&mut cart, 0x200), Some(0x102));
        cart.cpu_write(0x9004, 0, 0);
        assert_eq!(
            cycles_to_irq(&mut cart, 0x200),
            None,
            "once, till enabled again"
        );
    }

    #[test]
    fn sunsoft3s_16k_bank_2k_chr_and_irq_past_0() {
        let mut cart = cart(67, 0);
        assert_eq!(cart.mapper_name(), "MAPPER 67 SUNSOFT-3");
        cart.cpu_write(0xF800, 2, 0);
        assert_eq!(prg(&mut cart), [Some(4), Some(5), Some(30), Some(31)]);
        cart.cpu_write(0x8800, 3, 0);
        cart.cpu_write(0xB800, 0x10, 0);
        assert_eq!(chr(&mut cart), [6, 7, 0, 1, 0, 1, 0x20, 0x21]);
        cart.cpu_write(0xE800, 1, 0);
        assert_eq!(cart.mirroring, Mirroring::Horizontal);
        // Its high byte, then its low.
        cart.cpu_write(0xC800, 0x00, 0);
        cart.cpu_write(0xC800, 0x02, 0);
        cart.cpu_write(0xD800, 0x10, 0);
        assert_eq!(cycles_to_irq(&mut cart, 100), Some(3));
        cart.cpu_write(0xD800, 0x00, 0);
        assert!(!cart.irq());
    }
}
