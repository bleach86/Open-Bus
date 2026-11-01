/// What the CPU is wired to: everything it reads and writes, one access a
/// cycle, as the 6502 drives its address and data lines and the machine
/// decides what answers. Each machine is a Bus: its RAM, ROM and chips at
/// their addresses.
pub trait Bus {
    fn read(&mut self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, value: u8);

    /// A cycle gone by, as the CPU makes its access on it: for the chips
    /// clocked with the CPU.
    fn tick(&mut self) {}

    /// The RESET line, held by the machine (its Reset key, say): the CPU
    /// does nothing while it is, and resets when it's let go.
    fn reset_held(&self) -> bool {
        false
    }

    /// The IRQ line, held by the machine's devices until they're served:
    /// taken between instructions, unless I is set.
    fn irq(&self) -> bool {
        false
    }

    /// Whether the machine wants the CPU's run loop handed back to it,
    /// between instructions: for the machine's state to be saved, say, or
    /// another put in its place.
    fn hand_back(&self) -> bool {
        false
    }
}
