//! The cartridges: each type's banks and its hotspots, switched before the
//! read; the Superchip's and CBS's RAM; FE's switch by the stack; 3F's by
//! a write to the TIA's; E7's RAM; and the DPC's data fetchers and random
//! number.

#[cfg(test)]
mod tests {
    use crate::machines::atari2600::cartridge::{Cartridge, Kind};

    const CLOCK: u32 = 1_193_182;

    /// A ROM of `banks` banks of `size`, each its number in every byte but
    /// its second, so it's no Superchip's.
    fn rom(banks: usize, size: usize) -> Vec<u8> {
        (0..banks)
            .flat_map(|bank| {
                let mut bank = vec![bank as u8; size];
                bank[1] = 0xEE;
                bank
            })
            .collect()
    }

    fn cart(image: &[u8], kind: Kind) -> Cartridge {
        Cartridge::new(image, Some(kind), CLOCK).unwrap()
    }

    #[test]
    fn f8_f6_and_f4_switch_4k_at_their_hotspots() {
        let mut f8 = cart(&rom(2, 0x1000), Kind::F8);
        assert_eq!(f8.read(0x1000), 1, "F8 starts in its second");
        assert_eq!(f8.read(0x1FF8), 0, "switched before the read");
        assert_eq!(f8.read(0x1234), 0);
        f8.write(0x1FF9, 0);
        assert_eq!(f8.read(0x1234), 1, "by a write too");
        let mut f6 = cart(&rom(4, 0x1000), Kind::F6);
        assert_eq!(f6.read(0x1000), 0, "F6 starts in its first");
        assert_eq!(f6.read(0x1FF8), 2);
        let mut f4 = cart(&rom(8, 0x1000), Kind::F4);
        assert_eq!(f4.read(0x1FFB), 7);
    }

    #[test]
    fn a_superchip_is_written_at_1000_and_read_at_1080() {
        let image: Vec<u8> = (0..2).flat_map(|bank| vec![bank as u8; 0x1000]).collect();
        let mut sc = Cartridge::new(&image, None, CLOCK).unwrap();
        assert!(sc.superchip, "each bank's first 256 bytes, 128 twice");
        assert_eq!(sc.name(), "F8SC 8K");
        sc.write(0x1005, 0x77);
        assert_eq!(sc.read(0x1085), 0x77);
        assert_eq!(
            sc.read(0x1005),
            0x10,
            "its write half read: what's on the bus"
        );
        assert_eq!(sc.read(0x1085), 0x10, "and written");
        assert_eq!(sc.read(0x1200), 1, "ROM past it");
    }

    #[test]
    fn fa_starts_in_its_third_with_256_bytes_of_ram() {
        let mut fa = cart(&rom(3, 0x1000), Kind::Fa);
        assert_eq!(fa.read(0x1300), 2);
        fa.write(0x10AB, 0x55);
        assert_eq!(fa.read(0x11AB), 0x55);
        assert_eq!(fa.read(0x1FF8), 0);
    }

    #[test]
    fn e0_switches_three_1k_slots_the_last_fixed() {
        let mut e0 = cart(&rom(8, 0x400), Kind::E0);
        let slots = |e0: &mut Cartridge| [0x1000, 0x1400, 0x1800, 0x1C00].map(|at| e0.read(at));
        assert_eq!(slots(&mut e0), [4, 5, 6, 7]);
        e0.read(0x1FE1);
        e0.read(0x1FEA);
        e0.read(0x1FF3);
        assert_eq!(slots(&mut e0), [1, 2, 3, 7]);
    }

    #[test]
    fn fe_switches_on_the_access_after_one_at_01fe() {
        let mut fe = cart(&rom(2, 0x1000), Kind::Fe);
        assert_eq!(fe.read(0x1000), 0);
        // A JSR to $D000: its return address's low byte pushed at $01FE,
        // then its target's high byte read, $D0: bank 1.
        fe.access(0x01FE, 0x12, true);
        fe.access(0x1F00, 0xD0, false);
        assert_eq!(fe.read(0x1000), 1);
        fe.access(0x01FE, 0x34, false);
        fe.access(0x01FF, 0xF0, false);
        assert_eq!(fe.read(0x1000), 0, "$F0: bank 0");
    }

    #[test]
    fn tigervisions_2k_switched_by_a_write_to_the_tias() {
        let mut tv = cart(&rom(4, 0x800), Kind::Tigervision);
        assert_eq!([tv.read(0x1000), tv.read(0x1800)], [0, 3]);
        tv.access(0x003F, 2, true);
        assert_eq!([tv.read(0x1000), tv.read(0x1800)], [2, 3]);
        tv.access(0x003F, 1, false);
        assert_eq!(tv.read(0x1000), 2, "not by a read");
    }

    #[test]
    fn e7s_2k_banks_and_its_rams() {
        let mut e7 = cart(&rom(8, 0x800), Kind::E7);
        e7.read(0x1FE2);
        assert_eq!([e7.read(0x1000), e7.read(0x1A00)], [2, 7]);
        // $1FE7: its 1K of RAM at $1000, written there, read at $1400.
        e7.read(0x1FE7);
        e7.write(0x1010, 0x99);
        assert_eq!(e7.read(0x1410), 0x99);
        // A 256 bytes of it at $1800, written there, read at $1900, of 4.
        e7.read(0x1FE9);
        e7.write(0x1820, 0x44);
        assert_eq!(e7.read(0x1920), 0x44);
        e7.read(0x1FE8);
        assert_eq!(e7.read(0x1920), 0, "another of the 4");
    }

    #[test]
    fn dpcs_data_fetchers_count_down_through_its_graphics() {
        let mut image = rom(2, 0x1000);
        image.extend((0..0x800).map(|n| n as u8));
        let mut dpc = Cartridge::new(&image, None, CLOCK).unwrap();
        assert_eq!(dpc.kind, Kind::Dpc);
        assert_eq!(dpc.read(0x1100), 1, "F8, its second bank first");
        // Fetcher 0's counter at $005: its byte, 2047 - 5 from the start.
        dpc.write(0x1050, 0x05); // counter low
        dpc.write(0x1058, 0x00); // counter high
        let bytes: Vec<u8> = (0..3).map(|_| dpc.read(0x1008)).collect();
        let at = |counter: usize| (2047 - counter) as u8;
        assert_eq!(bytes, [at(5), at(4), at(3)]);
        // Its flag: set at its top, cleared at its bottom.
        dpc.write(0x1040, 0x01); // top
        dpc.write(0x1048, 0x00); // bottom
        assert_eq!(dpc.read(0x1038), 0x00, "below its top");
        assert_eq!(dpc.read(0x1038), 0xFF, "at its top, 1");
        assert_eq!(dpc.read(0x1038), 0x00, "at its bottom, 0");
        // Its random number, from 1 again, stepped at each access.
        dpc.write(0x1070, 0);
        let first = dpc.read(0x1000);
        let second = dpc.read(0x1000);
        assert_ne!(first, second);
        dpc.write(0x1070, 0);
        assert_eq!(dpc.read(0x1000), first, "the same again from 1");
    }
}
