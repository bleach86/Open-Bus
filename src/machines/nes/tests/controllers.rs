//! The controllers: each latched and read a bit at a time from its own
//! register, whichever's held, and the second no different from the first;
//! and when a write of $4016 reaches them.

#[cfg(test)]
mod tests {
    use crate::cpu::bus::Bus;
    use crate::host::display::Gamepad;
    use crate::machines::nes::Nes;
    use crate::machines::nes::cartridge::Cartridge;

    fn nes() -> Nes {
        let mut image = b"NES\x1A\x01\x01".to_vec();
        image.extend([0; 10]);
        image.extend([0xEA; 0x4000 + 0x2000]);
        Nes::new(Cartridge::new(&image).unwrap())
    }

    /// $4016 written, and the time for it to reach the controllers.
    fn write(nes: &mut Nes, value: u8) {
        nes.write(0x4016, value);
        nes.tick();
        nes.tick();
    }

    /// $4016 strobed, then eight reads of `register`, a bit each.
    fn read(nes: &mut Nes, register: u16) -> [u8; 8] {
        write(nes, 1);
        write(nes, 0);
        [0; 8].map(|_| nes.read(register) & 1)
    }

    #[test]
    fn each_controller_is_read_from_its_own_register() {
        let mut nes = nes();
        nes.gamepad = Gamepad { a: true, select: true, up: true, ..Gamepad::default() };
        nes.gamepad_2 = Gamepad { b: true, start: true, right: true, ..Gamepad::default() };
        // A, B, Select, Start, Up, Down, Left, Right.
        assert_eq!(read(&mut nes, 0x4016), [1, 0, 1, 0, 1, 0, 0, 0]);
        assert_eq!(read(&mut nes, 0x4017), [0, 1, 0, 1, 0, 0, 0, 1]);
    }

    #[test]
    fn neither_shifts_the_other() {
        let mut nes = nes();
        nes.gamepad = Gamepad { a: true, ..Gamepad::default() };
        nes.gamepad_2 = Gamepad { a: true, b: true, ..Gamepad::default() };
        write(&mut nes, 1);
        write(&mut nes, 0);
        assert_eq!(nes.read(0x4016) & 1, 1, "the first's A");
        assert_eq!(nes.read(0x4016) & 1, 0, "the first's B");
        assert_eq!(nes.read(0x4017) & 1, 1, "the second's A, after the first's B");
        assert_eq!(nes.read(0x4017) & 1, 1, "the second's B");
    }

    #[test]
    fn the_second_reads_1s_past_its_8_as_the_first_does() {
        let mut nes = nes();
        for register in [0x4016, 0x4017] {
            read(&mut nes, register);
            assert_eq!(nes.read(register) & 1, 1, "${register:04X} past its 8");
        }
    }

    #[test]
    fn the_seconds_d_pads_opposites_cant_both_be_held() {
        let mut nes = nes();
        nes.gamepad_2 = Gamepad { up: true, down: true, left: true, right: true, ..Gamepad::default() };
        assert_eq!(read(&mut nes, 0x4017), [0; 8]);
    }

    #[test]
    fn while_latching_each_reads_its_a() {
        let mut nes = nes();
        nes.gamepad_2 = Gamepad { a: true, ..Gamepad::default() };
        write(&mut nes, 1);
        for _ in 0..4 {
            assert_eq!((nes.read(0x4016) & 1, nes.read(0x4017) & 1), (0, 1));
        }
    }

    /// A 1 written to $4016 for a cycle, as DEC $4016 does, its value's
    /// bit 0 set, starting on a get or not: what's then read of A, held.
    fn strobe_for_a_cycle(on_a_get: bool) -> u8 {
        let mut nes = nes();
        read(&mut nes, 0x4016); // nothing latched but 1s from here
        nes.gamepad = Gamepad { a: true, ..Gamepad::default() };
        if (nes.cycles % 2 == 1) != on_a_get {
            nes.tick();
        }
        nes.write(0x4016, 1);
        nes.tick();
        nes.write(0x4016, 0);
        nes.tick();
        nes.tick();
        nes.read(0x4016) & 1
    }

    #[test]
    fn a_strobe_of_a_cycle_from_a_get_to_a_put_is_seen() {
        assert_eq!(strobe_for_a_cycle(true), 1, "A latched");
    }

    #[test]
    fn a_strobe_of_a_cycle_from_a_put_to_a_get_is_not() {
        let unlatched = {
            let mut nes = nes();
            read(&mut nes, 0x4016);
            nes.read(0x4016) & 1
        };
        assert_eq!(strobe_for_a_cycle(false), unlatched, "the 1s after the 8th");
    }
}
