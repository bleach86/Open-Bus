//! The cart slot, `$FF30`: a flash cart, as an EverDrive is. 2M of RAM in
//! 8K banks, two of which show in the cart window, and 32K of non-volatile
//! save memory. The system software loads a cart file into the banks and
//! resets into it; the cart file format is the system software's business
//! (docs/machine.md), not the hardware's.

use std::fs;
use std::path::PathBuf;

/// The system ROM, `$C000-$FFFF`.
pub const ROM_SIZE: usize = 0x4000;

pub const WINDOW: u16 = 0x8000;
pub const WINDOW_END: u16 = 0xBFFF;
const BANK_SIZE: usize = 0x2000;
const BANK_COUNT: usize = 256;
pub const SAVE_SIZE: usize = 0x8000;

const BANK0: u16 = 0xFF30;
const BANK1: u16 = 0xFF31;
const CART_CTL: u16 = 0xFF32;
const SAVE_ADDR: u16 = 0xFF34;
const SAVE_ADDR_HI: u16 = 0xFF35;
const SAVE_DATA: u16 = 0xFF36;

/// `CART_CTL`: the BIOS boots the cart.
const ENABLED: u8 = 0x01;
/// `CART_CTL`: writes to the window go to the banks.
const WRITABLE: u8 = 0x02;

pub struct FlashCart {
    banks: Vec<u8>,
    bank: [u8; 2],
    ctl: u8,
    save: Vec<u8>,
    save_addr: u16,
    /// Where save memory lives on the host, and whether it changed since it
    /// was last written there.
    save_file: Option<PathBuf>,
    save_changed: bool,
}

impl FlashCart {
    /// A flash cart whose save memory is kept in `save_file`.
    pub fn new(save_file: Option<PathBuf>) -> FlashCart {
        let mut save = save_file
            .as_ref()
            .and_then(|path| fs::read(path).ok())
            .unwrap_or_default();
        save.resize(SAVE_SIZE, 0);
        FlashCart {
            banks: vec![0xFF; BANK_SIZE * BANK_COUNT],
            bank: [0, 1],
            ctl: 0,
            save,
            save_addr: 0,
            save_file,
            save_changed: false,
        }
    }

    /// The reset line, and the reset button: banks 0 and 1 in the window and
    /// the cart disabled, so the BIOS starts the system software again.
    pub fn reset(&mut self) {
        self.bank = [0, 1];
        self.ctl = 0;
        self.save_addr = 0;
    }

    fn window_offset(&self, addr: u16) -> usize {
        let offset = (addr - WINDOW) as usize;
        self.bank[offset / BANK_SIZE] as usize * BANK_SIZE + offset % BANK_SIZE
    }

    pub fn read_window(&self, addr: u16) -> u8 {
        self.banks[self.window_offset(addr)]
    }

    pub fn write_window(&mut self, addr: u16, value: u8) {
        if self.ctl & WRITABLE != 0 {
            let offset = self.window_offset(addr);
            self.banks[offset] = value;
        }
    }

    pub fn read(&mut self, addr: u16) -> u8 {
        let value = self.peek(addr);
        if addr == SAVE_DATA {
            self.next_save_addr();
        }
        value
    }

    /// What a read gives, without the read's effect (SAVE_DATA moving on).
    pub fn peek(&self, addr: u16) -> u8 {
        match addr {
            BANK0 => self.bank[0],
            BANK1 => self.bank[1],
            CART_CTL => self.ctl,
            SAVE_ADDR => self.save_addr as u8,
            SAVE_ADDR_HI => (self.save_addr >> 8) as u8,
            SAVE_DATA => self.save[self.save_addr as usize],
            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            BANK0 => self.bank[0] = value,
            BANK1 => self.bank[1] = value,
            CART_CTL => self.ctl = value & (ENABLED | WRITABLE),
            SAVE_ADDR => self.save_addr = self.save_addr & 0xFF00 | value as u16,
            SAVE_ADDR_HI => {
                self.save_addr = (self.save_addr & 0x00FF | (value as u16) << 8) % SAVE_SIZE as u16
            }
            SAVE_DATA => {
                let at = self.save_addr as usize;
                if self.save[at] != value {
                    self.save[at] = value;
                    self.save_changed = true;
                }
                self.next_save_addr();
            }
            _ => {}
        }
    }

    fn next_save_addr(&mut self) {
        self.save_addr = (self.save_addr + 1) % SAVE_SIZE as u16;
    }

    /// Write save memory to its file if it changed. Non-volatile memory keeps
    /// its contents with the power off; this is how the host does that.
    pub fn keep_save(&mut self) {
        if !std::mem::take(&mut self.save_changed) {
            return;
        }
        let Some(path) = &self.save_file else {
            return;
        };
        // Replaced whole, so losing power part way leaves the previous save.
        let partial = path.with_extension("new");
        if let Err(error) = fs::write(&partial, &self.save).and_then(|()| fs::rename(&partial, path)) {
            eprintln!("{}: {error}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_banks_through_the_window_and_switches_them() {
        let mut cart = FlashCart::new(None);
        cart.write_window(0x8000, 0x11);
        assert_eq!(cart.read_window(0x8000), 0xFF, "not writable");
        cart.write(CART_CTL, WRITABLE);
        for bank in 0..4u8 {
            cart.write(BANK0, bank);
            cart.write_window(0x8000, 0x10 + bank);
            cart.write_window(0x9FFF, 0x20 + bank);
        }
        cart.write(CART_CTL, ENABLED);
        cart.write(BANK0, 0);
        cart.write(BANK1, 3);
        assert_eq!(cart.read_window(0x8000), 0x10);
        assert_eq!(cart.read_window(0x9FFF), 0x20);
        assert_eq!(cart.read_window(0xA000), 0x13);
        assert_eq!(cart.read_window(0xBFFF), 0x23);
        cart.write_window(0xA000, 0);
        assert_eq!(cart.read_window(0xA000), 0x13, "read-only once loaded");

        cart.reset();
        assert_eq!((cart.read(BANK0), cart.read(BANK1), cart.read(CART_CTL)), (0, 1, 0));
        assert_eq!(cart.read_window(0xA000), 0x11, "the banks keep what was loaded");
    }

    #[test]
    fn save_memory_survives_power_off() {
        let path = std::path::Path::new(env!("OUT_DIR")).join("test-save-memory");
        let _ = fs::remove_file(&path);
        let mut cart = FlashCart::new(Some(path.clone()));
        cart.keep_save();
        assert!(!path.exists(), "nothing written until something changes");
        cart.write(SAVE_ADDR, 0xFF);
        cart.write(SAVE_ADDR_HI, 0x7F);
        cart.write(SAVE_DATA, 0x42);
        cart.write(SAVE_DATA, 0x43); // wraps to 0
        cart.keep_save();

        let mut cart = FlashCart::new(Some(path));
        cart.write(SAVE_ADDR, 0xFF);
        cart.write(SAVE_ADDR_HI, 0x7F);
        assert_eq!([cart.read(SAVE_DATA), cart.read(SAVE_DATA)], [0x42, 0x43]);
    }
}
