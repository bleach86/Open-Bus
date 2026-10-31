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
}
