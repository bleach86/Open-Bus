use std::fmt;
use std::io::Write;

#[derive(Clone, Debug)]
pub struct Memory {
    // 64KB of memory
    pub data: [u8; 65536],
}

impl Default for Memory {
    fn default() -> Self {
        Memory { data: [0; 65536] }
    }
}

impl Memory {
    pub fn read(&mut self, addr: u16) -> u8 {
        self.data[addr as usize]
    }

    pub fn write(&mut self, addr: u16, value: u8) {
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
