//! The Zapper: where it's aimed, the light it sees as the PPU draws, and
//! its bits at $4017, a NES's or a Famicom's.

#[cfg(test)]
mod tests {
    use crate::cpu::bus::Bus;
    use crate::host::display::Pointer;
    use crate::host::machine::{Input, Machine};
    use crate::machines::nes::Nes;
    use crate::machines::nes::cartridge::Cartridge;
    use crate::machines::nes::palette::palette;
    use crate::machines::nes::ppu::{HEIGHT, WIDTH};
    use crate::machines::nes::region::Region;
    use crate::machines::nes::zapper::Zapper;

    /// A pointer at `x`, `y`, its buttons `buttons`.
    fn pointer(x: f32, y: f32, buttons: [bool; 2]) -> Pointer {
        Pointer {
            at: Some((x, y)),
            buttons,
        }
    }

    #[test]
    fn its_aimed_where_the_pointer_is_on_the_picture() {
        let mut zapper = Zapper::default();
        zapper.aim(pointer(10.5, 20.9, [true, false]), 8);
        assert_eq!(
            (zapper.at, zapper.trigger),
            (Some((18, 28)), true),
            "past the overscan"
        );
        zapper.aim(pointer(-1.0, 20.0, [false; 2]), 0);
        assert_eq!(
            (zapper.at, zapper.trigger),
            (None, false),
            "off the picture"
        );
        zapper.aim(pointer(10.0, 20.0, [false, true]), 0);
        assert_eq!(
            (zapper.at, zapper.trigger),
            (None, true),
            "the second button, aimed away"
        );
        zapper.aim(Pointer::default(), 0);
        assert_eq!(zapper.at, None, "no pointer");
    }

    #[test]
    fn it_sees_a_bright_dot_once_its_drawn_and_for_20_lines() {
        let colours = palette(Region::Ntsc);
        let mut frame = vec![0x0F; WIDTH * HEIGHT]; // black
        frame[50 * WIDTH + 100] = 0x30; // white
        let mut zapper = Zapper {
            at: Some((100, 50)),
            trigger: false,
        };
        let seen =
            |zapper: &Zapper, line, dot| zapper.read(line, dot, &frame, &colours) & 0x08 == 0;
        assert!(!seen(&zapper, 49, 300), "a line before");
        assert!(!seen(&zapper, 50, 100), "the dot before it's drawn");
        assert!(seen(&zapper, 50, 101), "as it's drawn");
        assert!(seen(&zapper, 70, 0), "20 lines on");
        assert!(!seen(&zapper, 71, 0), "21");
        zapper.at = Some((101, 50));
        assert!(!seen(&zapper, 55, 0), "a black dot");
        zapper.trigger = true;
        assert_eq!(
            zapper.read(55, 0, &frame, &colours),
            0x18,
            "the trigger's bit"
        );
    }

    /// A NES of NOPs, its game a Zapper's.
    fn nes() -> Nes {
        let mut image = b"NES\x1A\x01\x01".to_vec();
        image.extend([0; 10]);
        image.extend([0xEA; 0x4000 + 0x2000]);
        let mut cartridge = Cartridge::new(&image).unwrap();
        cartridge.zapper = true;
        Nes::of(cartridge, Region::Ntsc)
    }

    /// A register written, a cycle each, as the CPU's are.
    fn put(nes: &mut Nes, addr: u16, value: u8) {
        nes.write(addr, value);
        nes.tick();
    }

    /// The controllers latched: $4016 written 1 then 0, each reaching them
    /// as a put cycle begins, every other cycle.
    fn strobe(nes: &mut Nes) {
        for value in [1, 0] {
            put(nes, 0x4016, value);
            nes.tick();
        }
    }

    /// The screen all `colour`, the PPU's backdrop with nothing drawn over
    /// it; then $4017 read on line 60, the Zapper aimed at line 50.
    fn read_at_line_60(colour: u8) -> u8 {
        let mut nes = nes();
        for _ in 0..3 {
            while !nes.hand_back() {
                nes.tick();
            }
        }
        for (addr, value) in [
            (0x2006, 0x3F),
            (0x2006, 0x00),
            (0x2007, colour),
            (0x2006, 0),
            (0x2006, 0),
        ] {
            put(&mut nes, addr, value);
        }
        nes.input(Input {
            pointer: pointer(100.0, 50.0, [false; 2]),
            ..Input::default()
        });
        while nes.hand_back() || nes.ppu.line != 60 {
            nes.tick();
        }
        nes.read(0x4017)
    }

    #[test]
    fn it_sees_the_screen_the_ppu_draws() {
        assert_eq!(read_at_line_60(0x30) & 0x18, 0x00, "white, and seen");
        assert_eq!(read_at_line_60(0x0F) & 0x18, 0x08, "black, and not");
    }

    #[test]
    fn its_in_the_second_port_a_famicoms_second_controller_still_there() {
        let mut nes = nes();
        assert!(nes.aimed_at());
        assert!(!nes.leave_out_picture(true), "its picture left out");
        nes.input(Input {
            gamepad_2: crate::host::display::Gamepad {
                a: true,
                ..Default::default()
            },
            ..Input::default()
        });
        strobe(&mut nes);
        assert_eq!(nes.read(0x4017) & 0x1F, 0x08, "a NES's: the Zapper's alone");
        nes.as_famicom();
        strobe(&mut nes);
        assert_eq!(
            nes.read(0x4017) & 0x1F,
            0x09,
            "a Famicom's: its controller's A too"
        );
        assert_eq!(nes.read(0x4016) & 0x1F, 0x00, "the first's, as it was");
    }
}
