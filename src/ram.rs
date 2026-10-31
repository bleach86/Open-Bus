use std::fmt;
use std::io::Write;

#[derive(Clone, Debug)]
pub struct Memory {
    // 64KB of memory
    pub data: [u8; 65536],
    // Where ROM starts, if there is any: from there up, writes don't take,
    // but for the terminal at 0xFFF8 and 0xFFF9.
    pub rom_start: Option<u16>,
    // Every read and write, in order, while there is a log: the address, the
    // value and whether it was a write. A logic analyzer on the bus.
    pub bus_log: Option<Vec<(u16, u8, bool)>>,
}

impl Default for Memory {
    fn default() -> Self {
        Memory {
            data: [0; 65536],
            rom_start: None,
            bus_log: None,
        }
    }
}

impl Memory {
    pub fn read(&mut self, addr: u16) -> u8 {
        let value = self.data[addr as usize];
        if let Some(log) = &mut self.bus_log {
            log.push((addr, value, false));
        }
        value
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        if let Some(log) = &mut self.bus_log {
            log.push((addr, value, true));
        }
        if self.rom_start.is_some_and(|start| addr >= start) && addr != 0xFFF8 && addr != 0xFFF9 {
            return; // ROM
        }
        self.data[addr as usize] = value;

        // Special behavior: if writing to address 0xFFF8, print the character
        if addr == 0xFFF8 {
            print!("{}", value as char);
            std::io::stdout().flush().unwrap();
        }
    }
}

impl fmt::UpperHex for Memory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, byte) in self.data.iter().enumerate() {
            if i % 16 == 0 {
                write!(f, "\n{:04X}: ", i)?;
            }
            write!(f, "{:02X} ", byte)?;
        }
        Ok(())
    }
}
