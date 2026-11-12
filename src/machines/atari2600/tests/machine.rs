//! The 2600's bus: what answers where in its 13 bits, the TIA's registers,
//! the RIOT's RAM and its mirrors, and the cartridge.

#[cfg(test)]
mod tests {
    use crate::cpu::bus::Bus;
    use crate::machines::atari2600::Atari2600;
    use crate::machines::atari2600::cartridge::Cartridge;
    use crate::machines::region::Region;

    /// A console with a 4K cartridge of `fill` in every byte.
    fn console(fill: u8) -> Atari2600 {
        let cartridge = Cartridge::new(&[fill; 0x1000], None, 1_193_182).unwrap();
        Atari2600::new(cartridge, Region::Ntsc, Region::Ntsc)
    }

    #[test]
    fn its_ram_is_at_80_and_the_stacks_page_and_the_cartridge_at_a12() {
        let mut atari = console(0x5A);
        atari.write(0x0080, 0x11);
        assert_eq!(atari.read(0x0180), 0x11, "the stack's page, the same RAM");
        assert_eq!(atari.read(0x2080), 0x11, "13 bits only");
        assert_eq!(atari.read(0x1000), 0x5A);
        assert_eq!(atari.read(0xF123), 0x5A);
    }

    #[test]
    fn a_tia_read_drives_only_its_top_bits() {
        let mut atari = console(0x3F);
        atari.read(0x1000); // $3F on the data bus
        assert_eq!(atari.read(0x000C), 0x80 | 0x3F, "INPT4, not pressed");
    }
}

/// WSYNC through the CPU: its next read waits for the line's end.
#[cfg(test)]
mod wsync {
    use crate::cpu::CPU;
    use crate::cpu::bus::Bus;
    use crate::machines::atari2600::Atari2600;
    use crate::machines::atari2600::cartridge::Cartridge;
    use crate::machines::region::Region;

    /// The console, the writes it's made: each RAM write's cycle and the
    /// TIA's color clock then.
    struct Watched {
        atari: Atari2600,
        writes: Vec<(u64, u8)>,
    }

    impl Bus for Watched {
        fn read(&mut self, addr: u16) -> u8 {
            self.atari.read(addr)
        }
        fn write(&mut self, addr: u16, value: u8) {
            if addr == 0x80 {
                self.writes.push((self.atari.cycles, self.atari.tia.hctr));
            }
            self.atari.write(addr, value);
        }
        fn tick(&mut self) {
            self.atari.tick();
        }
        fn halt(&mut self, addr: u16) -> bool {
            self.atari.halt(addr)
        }
    }

    #[test]
    fn the_next_read_is_on_the_next_lines_first_cycle() {
        // STA WSYNC; STA $80; JMP $F000.
        let mut rom = vec![0xEA; 0x1000];
        rom[..7].copy_from_slice(&[0x85, 0x02, 0x85, 0x80, 0x4C, 0x00, 0xF0]);
        rom[0xFFC..].copy_from_slice(&[0x00, 0xF0, 0x00, 0xF0]);
        let cartridge = Cartridge::new(&rom, None, 1_193_182).unwrap();
        let atari = Atari2600::new(cartridge, Region::Ntsc, Region::Ntsc);
        let mut cpu = CPU::with_bus(Watched {
            atari,
            writes: Vec::new(),
        });
        cpu.reset();
        cpu.execute(76 * 5);
        let writes = &cpu.memory.writes;
        assert!(writes.len() >= 4);
        for pair in writes.windows(2) {
            assert_eq!(pair[1].0 - pair[0].0, 76, "a line apart");
        }
        // STA $80's three cycles from the line's start: its write's clocks
        // 6 to 8.
        assert!(writes.iter().all(|&(_, hctr)| hctr == 9), "{writes:?}");
    }
}

/// The phosphor: each dot the brighter of what's drawn and what was, half
/// faded, once a frame.
#[cfg(test)]
mod phosphor {
    use crate::host::machine::Machine;
    use crate::machines::atari2600::Atari2600;
    use crate::machines::atari2600::cartridge::Cartridge;
    use crate::machines::region::Region;

    #[test]
    fn what_was_drawn_fades_by_half_a_frame() {
        let cartridge = Cartridge::new(&[0xEA; 0x1000], None, 1_193_182).unwrap();
        let mut atari = Atari2600::new(cartridge, Region::Ntsc, Region::Ntsc);
        atari.phosphor = Some(0.5);
        atari.tia.picture[0] = 0x0E; // NTSC's white, $ECECEC
        assert_eq!(atari.picture().pixels[0], 0xECECEC);
        atari.frames += 1;
        atari.tia.picture[0] = 0x00;
        assert_eq!(atari.picture().pixels[0], 0x767676, "half of it left");
        assert_eq!(
            atari.picture().pixels[0],
            0x767676,
            "not again till a frame"
        );
        atari.frames += 1;
        assert_eq!(atari.picture().pixels[0], 0x3B3B3B);
    }
}
