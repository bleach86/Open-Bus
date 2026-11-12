//! NES-EVENT, Nintendo World Championships 1990's board: its first 32K
//! fixed till it's started, CHR bank 0's bit 4 cleared then set; then its
//! two chips banked, the first's 32K at a time, the second's as MMC1 does;
//! and its timer, run while that bit's clear, the IRQ line held at its
//! time till the bit's set again.

#[cfg(test)]
mod tests {
    use crate::machines::nes::cartridge::{Cartridge, Mapper, NWC_TIME};

    /// Its 256K of PRG, each 16K its number in every byte, CHR RAM.
    fn nwc() -> Cartridge {
        let mut image = b"NES\x1A\x10\x00\x90\x60".to_vec();
        image.extend([0; 8]);
        for bank in 0..16 {
            image.extend([bank; 0x4000]);
        }
        Cartridge::new(&image).unwrap()
    }

    /// An MMC1 register at `addr` written a bit at a time, a cycle between
    /// the writes, from cycle `*cycle` on.
    fn write(cart: &mut Cartridge, addr: u16, value: u8, cycle: &mut u64) {
        for bit in 0..5 {
            cart.cpu_write(addr, value >> bit & 1, *cycle);
            *cycle += 2;
        }
    }

    fn prg(cart: &mut Cartridge) -> [u8; 2] {
        [0x8000, 0xC000].map(|addr| cart.cpu_read(addr).unwrap())
    }

    /// Started: bit 4 cleared, then set.
    fn start(cart: &mut Cartridge, cycle: &mut u64) {
        write(cart, 0xA000, 0x00, cycle);
        write(cart, 0xA000, 0x10, cycle);
    }

    #[test]
    fn its_first_32k_till_its_started() {
        let mut cart = nwc();
        assert_eq!(cart.mapper_name(), "MAPPER 105 NES-EVENT");
        let mut cycle = 0;
        assert_eq!(prg(&mut cart), [0, 1]);
        write(&mut cart, 0xA000, 0x1C, &mut cycle);
        write(&mut cart, 0xE000, 0x03, &mut cycle);
        assert_eq!(prg(&mut cart), [0, 1], "banks set, not started");
        write(&mut cart, 0xA000, 0x0C, &mut cycle);
        assert_eq!(prg(&mut cart), [0, 1], "bit 4 cleared, not set again");
        write(&mut cart, 0xA000, 0x1C, &mut cycle);
        assert_eq!(prg(&mut cart), [11, 15], "started");
    }

    #[test]
    fn the_first_chips_32k_and_the_seconds_as_mmc1s() {
        let mut cart = nwc();
        let mut cycle = 0;
        start(&mut cart, &mut cycle);
        assert_eq!(prg(&mut cart), [0, 1]);
        write(&mut cart, 0xA000, 0x14, &mut cycle);
        assert_eq!(prg(&mut cart), [4, 5], "the first's third 32K");
        // The second's: its bank's low 3 bits, by MMC1's PRG mode.
        write(&mut cart, 0xA000, 0x18, &mut cycle);
        write(&mut cart, 0xE000, 0x0B, &mut cycle);
        assert_eq!(prg(&mut cart), [11, 15], "mode 3, its last fixed");
        write(&mut cart, 0x8000, 0x08, &mut cycle);
        assert_eq!(prg(&mut cart), [8, 11], "mode 2, its first fixed");
        write(&mut cart, 0x8000, 0x00, &mut cycle);
        assert_eq!(prg(&mut cart), [10, 11], "32K");
        // Its CHR RAM; its PRG RAM, and that disabled.
        cart.chr_write(0x1234, 0x77);
        assert_eq!(cart.chr_read(0x1234), 0x77);
        cart.cpu_write(0x6000, 0x5A, cycle);
        assert_eq!(cart.cpu_read(0x6000), Some(0x5A));
        write(&mut cart, 0xE000, 0x10, &mut cycle);
        assert_eq!(cart.cpu_read(0x6000), None);
    }

    fn count(cart: &mut Cartridge) -> &mut u32 {
        match &mut cart.mapper {
            Mapper::Nwc(nwc) => &mut nwc.count,
            _ => unreachable!(),
        }
    }

    #[test]
    fn its_time_up_while_bit_4s_clear() {
        let mut cart = nwc();
        let mut cycle = 0;
        start(&mut cart, &mut cycle);
        for _ in 0..100 {
            cart.cycle();
        }
        assert_eq!(*count(&mut cart), 0, "held while bit 4's set");
        write(&mut cart, 0xA000, 0x00, &mut cycle);
        cart.cycle();
        assert_eq!(*count(&mut cart), 1, "counting");
        // Nearly up: 6:15, as the championships' were set.
        assert_eq!(NWC_TIME, 0x2800_0000);
        *count(&mut cart) = NWC_TIME - 2;
        cart.cycle();
        assert!(!cart.irq());
        cart.cycle();
        assert!(cart.irq(), "time up");
        write(&mut cart, 0xE000, 0x00, &mut cycle);
        cart.cycle();
        assert!(cart.irq(), "held, the other registers written");
        write(&mut cart, 0xA000, 0x10, &mut cycle);
        assert!(!cart.irq(), "acknowledged");
        assert_eq!(*count(&mut cart), 0, "and reset");
    }
}
