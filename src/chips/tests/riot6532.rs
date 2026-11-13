//! The RIOT: its timer, a cycle after it's written and every 8 cycles
//! after, past 0 every cycle with its flag set, cleared by a read; the
//! joysticks' port, its output bits; and its RAM.

#[cfg(test)]
mod tests {
    use crate::chips::riot6532::Riot;

    const INTIM: u16 = 0x284;
    const TIMINT: u16 = 0x285;
    const TIM8T: u16 = 0x295;
    const SWCHA: u16 = 0x280;
    const SWACNT: u16 = 0x281;

    fn cycles(riot: &mut Riot, cycles: usize) {
        for _ in 0..cycles {
            riot.cycle();
        }
    }

    #[test]
    fn its_timer_counts_down_then_on_past_0_every_cycle() {
        let mut riot = Riot::new(vec![0; 128], 0);
        riot.write(TIM8T, 3);
        cycles(&mut riot, 1);
        assert_eq!(riot.read(INTIM), 2, "a cycle after it's written");
        cycles(&mut riot, 8);
        assert_eq!(riot.read(INTIM), 1);
        cycles(&mut riot, 16);
        assert_eq!(riot.read(TIMINT) & 0x80, 0x80, "gone past 0");
        cycles(&mut riot, 1);
        assert_eq!(riot.read(INTIM), 0xFE, "every cycle");
        assert_eq!(riot.read(TIMINT) & 0x80, 0, "cleared by reading it");
        cycles(&mut riot, 1);
        assert_eq!(riot.read(INTIM), 0xFE, "every 8 cycles again");
    }

    #[test]
    fn its_ports_read_their_pins_or_as_written() {
        let mut riot = Riot::new(vec![0; 128], 0);
        riot.pins_a = 0b1110_1111; // the left joystick's up
        assert_eq!(riot.read(SWCHA), 0b1110_1111);
        riot.write(SWACNT, 0x0F);
        riot.write(SWCHA, 0x05);
        assert_eq!(riot.read(SWCHA), 0b1110_0101, "the right's bits written");
        riot.write(0x80, 0x42);
        assert_eq!(riot.read(0x180), 0x42, "its RAM, at the stack's too");
    }
}
