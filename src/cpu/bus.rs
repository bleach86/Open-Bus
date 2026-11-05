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
    /// taken between instructions, unless I is set. As it was at the end
    /// of the cycle before the one just ticked, if the machine is as
    /// exact; as it is, if not.
    fn irq(&self) -> bool {
        false
    }

    /// The NMI line, held by the machine's devices: an NMI taken on its
    /// edge, as it's pulled. As the IRQ line, at the end of the cycle
    /// before.
    fn nmi(&self) -> bool {
        false
    }

    /// The RDY line, held by the machine for a DMA of its own: the CPU
    /// halted on its read of `addr`, a cycle at a time, for as long as it
    /// is; the machine's DMA has those cycles, its own work done in their
    /// ticks.
    fn halt(&mut self, _addr: u16) -> bool {
        false
    }

    /// Whether a frame has ended since it was last asked: the CPU's run
    /// loop handed back then, between instructions, to the host.
    fn hand_back(&mut self) -> bool {
        false
    }
}
