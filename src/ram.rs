//! The bus: 32K of RAM, the cart window, the system ROM and the I/O
//! registers, one 16-byte block per device. See docs/machine.md.

use crate::cart::{self, FlashCart};
use crate::drive::Drive;
use crate::input::{Input, InputPort};
use crate::render::FrameSlot;
use crate::sound::Sound;
use crate::status::Status;
use crate::vdp::Vdp;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const RAM_END: u16 = 0x7FFF;
const ROM_BASE: u16 = 0xC000;
const IO_BASE: u16 = 0xFF00;
const IRQ_VECTOR: u16 = 0xFFFE;
const IO_END: u16 = 0xFFF9;

const FRAME_WAIT: u16 = 0xFF00;
const FRAME_COUNT: u16 = 0xFF01;
const POWER: u16 = 0xFF03;
const HOST: u16 = 0xFF04;
const DEBUG_OUT: u16 = 0xFF05;
const RTC_LATCH: u16 = 0xFF08;
const RTC_END: u16 = 0xFF0F;
/// `HOST`: a launcher acts on the `POWER` statuses.
const HOST_LAUNCHER: u8 = 0x01;

/// How often save memory is written out if it changed.
const SAVE_INTERVAL_FRAMES: u32 = 60;

/// What the host provides: where input comes from, where frames go, the
/// status readings, the drive's directory and the file that keeps save
/// memory.
#[derive(Default)]
pub struct Host {
    pub input: Arc<Input>,
    pub status: Arc<Status>,
    pub display: Option<Arc<FrameSlot>>,
    pub drive: Option<PathBuf>,
    pub save_file: Option<PathBuf>,
    pub launcher: bool,
    /// The machine-language monitor's link, if one may attach (monitor.rs).
    pub monitor: Option<Arc<crate::monitor::Link>>,
}

pub struct Memory {
    ram: Box<[u8; 0x8000]>,
    rom: Box<[u8; cart::ROM_SIZE]>,
    pub(crate) vdp: Vdp,
    pub(crate) cart: FlashCart,
    input: InputPort,
    drive: Drive,
    sound: Sound,
    host: Host,
    frame_count: u8,
    frames: u32,
    frame_wait: bool,
    power_off: Option<u8>,
    /// The reset button was pressed: the next power-on is a reset, which
    /// keeps RAM.
    resetting: bool,
    rtc: u64,
    debug: Vec<u8>,
    /// What the monitor is watching for, when there is one.
    monitor: Option<crate::monitor::Watch>,
}

impl Default for Memory {
    fn default() -> Self {
        Memory::new(Host::default())
    }
}

impl Memory {
    pub fn new(host: Host) -> Memory {
        Memory {
            ram: Box::new([0; 0x8000]),
            rom: Box::new([0xFF; cart::ROM_SIZE]),
            vdp: Vdp::default(),
            cart: FlashCart::new(host.save_file.clone()),
            input: InputPort::default(),
            drive: Drive::new(host.drive.clone()),
            sound: Sound::default(),
            monitor: host.monitor.clone().map(crate::monitor::Watch::new),
            host,
            frame_count: 0,
            frames: 0,
            frame_wait: false,
            power_off: None,
            resetting: false,
            rtc: 0,
            debug: Vec::new(),
        }
    }

    /// Power on with `rom` as the system ROM: RAM and the devices cleared.
    /// After the reset button, a reset instead: RAM and VRAM keep their
    /// contents, the devices go back to their reset state.
    pub fn power_on(&mut self, rom: &[u8; cart::ROM_SIZE]) {
        if !std::mem::take(&mut self.resetting) {
            self.ram.fill(0);
            self.vdp = Vdp::default();
            self.frame_count = 0;
        }
        self.rom.copy_from_slice(rom);
        self.vdp.reset();
        self.cart.reset();
        self.sound.reset();
        self.input = InputPort::default();
        self.host.input.clear_keys();
        self.drive = Drive::new(self.host.drive.clone());
        self.frame_wait = false;
    }

    pub fn read(&mut self, addr: u16) -> u8 {
        match addr {
            0..=RAM_END => self.ram[addr as usize],
            cart::WINDOW..=cart::WINDOW_END => self.cart.read_window(addr),
            IO_BASE..=IO_END => match addr & 0xFFF0 {
                0xFF00 => self.read_system(addr),
                0xFF10 => self.vdp.read(addr),
                0xFF20 => self.input.read(&self.host.input, addr),
                0xFF30 => self.cart.read(addr),
                0xFF40 => self.drive.read(addr),
                0xFF50 => self.host.status.read(addr),
                0xFF80..=0xFFA0 => self.sound.read(addr),
                _ => 0,
            },
            _ => self.rom[(addr - ROM_BASE) as usize],
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        if let Some(watch) = &mut self.monitor {
            watch.wrote(addr, value);
        }
        match addr {
            0..=RAM_END => self.ram[addr as usize] = value,
            cart::WINDOW..=cart::WINDOW_END => self.cart.write_window(addr, value),
            IO_BASE..=IO_END => match addr & 0xFFF0 {
                0xFF00 => self.write_system(addr, value),
                0xFF10 => self.vdp.write(addr, value),
                0xFF20 => self.input.write(addr),
                0xFF30 => self.cart.write(addr, value),
                0xFF40 => self.drive.write(addr, value),
                0xFF80..=0xFFA0 => self.sound.write(addr, value),
                _ => {}
            },
            _ => {} // the system ROM
        }
    }

    fn read_system(&self, addr: u16) -> u8 {
        match addr {
            FRAME_COUNT => self.frame_count,
            HOST => {
                if self.host.launcher {
                    HOST_LAUNCHER
                } else {
                    0
                }
            }
            RTC_LATCH..=RTC_END => self.rtc.to_le_bytes()[(addr - RTC_LATCH) as usize],
            _ => 0,
        }
    }

    fn write_system(&mut self, addr: u16, value: u8) {
        match addr {
            FRAME_WAIT => self.frame_wait = true,
            POWER => self.power_off = Some(value),
            DEBUG_OUT => self.debug.push(value),
            // The real-time clock: the host's time, read when latched.
            RTC_LATCH => {
                self.rtc = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |time| time.as_millis() as u64)
            }
            _ => {}
        }
    }

    /// Vblank: count the frame, play its sound, show the picture if it
    /// changed, pass debug output on and keep save memory now and then.
    pub fn end_frame(&mut self) {
        self.frame_count = self.frame_count.wrapping_add(1);
        self.frames += 1;
        self.sound.end_frame();
        self.vdp.end_frame();
        if let Some(display) = &self.host.display {
            if display.is_empty() && self.vdp.take_changed() {
                display.offer(self.vdp.frame());
            }
        }
        self.vdp.start_frame();
        if let Some(watch) = &mut self.monitor {
            watch.frame_ended();
        }
        if !self.debug.is_empty() {
            let mut stdout = std::io::stdout();
            let _ = stdout.write_all(&self.debug);
            let _ = stdout.flush();
            self.debug.clear();
        }
        if self.frames % SAVE_INTERVAL_FRAMES == 0 {
            self.keep_save();
        }
    }

    pub fn keep_save(&mut self) {
        self.cart.keep_save();
    }

    /// The interrupt lines between instructions, `frame_cycle` cycles into
    /// the frame: the vector to take, if a chip raises an interrupt the CPU
    /// takes (IRQ only while `masked` is clear). Only the video chip
    /// interrupts, and only on IRQ.
    pub fn interrupt(&mut self, frame_cycle: u64, masked: bool) -> Option<u16> {
        self.vdp.beam(frame_cycle);
        (!masked && self.vdp.irq()).then_some(IRQ_VECTOR)
    }

    /// The cycle within the frame of the next interrupt due this frame, for
    /// a CPU waiting for the frame's end.
    pub fn next_interrupt(&self, frame_cycle: u64) -> Option<u64> {
        self.vdp.next_interrupt(frame_cycle)
    }

    /// The monitor wants the CPU before the instruction at `pc` (a
    /// breakpoint, a step, a client asking): one check when there is no
    /// monitor.
    pub fn monitor_wants(&mut self, pc: u16) -> bool {
        self.monitor.as_mut().is_some_and(|watch| watch.wants(pc))
    }

    pub(crate) fn monitor_watch(&mut self) -> Option<&mut crate::monitor::Watch> {
        self.monitor.as_mut()
    }

    /// What a read of `addr` gives, without what reading does to a port (a
    /// VRAM address moving on, a byte taken from the drive): for the
    /// monitor. None where a read can't be had without its effect.
    pub fn peek(&self, addr: u16) -> Option<u8> {
        Some(match addr {
            0..=RAM_END => self.ram[addr as usize],
            cart::WINDOW..=cart::WINDOW_END => self.cart.read_window(addr),
            IO_BASE..=IO_END => match addr & 0xFFF0 {
                0xFF00 => self.read_system(addr),
                0xFF10 => self.vdp.peek(addr),
                0xFF20 => match addr {
                    0xFF20 | 0xFF21 => self.host.input.pad.load(std::sync::atomic::Ordering::Relaxed).to_le_bytes()
                        [(addr & 1) as usize],
                    0xFF24 | 0xFF25 => self.host.input.keypad.load(std::sync::atomic::Ordering::Relaxed).to_le_bytes()
                        [(addr & 1) as usize],
                    _ => return None, // a waiting key is taken by reading
                },
                0xFF30 => self.cart.peek(addr),
                0xFF40 => return self.drive.peek(addr),
                0xFF50 => self.host.status.read(addr),
                0xFF80..=0xFFA0 => self.sound.peek(addr),
                _ => 0,
            },
            _ => self.rom[(addr - ROM_BASE) as usize],
        })
    }

    /// True once if the program wrote `FRAME_WAIT`.
    pub fn take_frame_wait(&mut self) -> bool {
        std::mem::take(&mut self.frame_wait)
    }

    /// The status the program powered off with through `POWER`, once.
    pub fn take_host_call(&mut self) -> Option<u8> {
        self.power_off.take()
    }

    /// True once after the reset button was pressed.
    pub fn take_reset(&mut self) -> bool {
        let reset = self.host.input.take_reset();
        self.resetting |= reset;
        reset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn reset_keeps_ram_and_power_on_clears_it() {
        let host = Host::default();
        let input = Arc::clone(&host.input);
        let mut memory = Memory::new(host);
        let rom = [0xEA; cart::ROM_SIZE];
        memory.power_on(&rom);
        memory.write(0x1234, 0x42);
        memory.write(0xC000, 0); // ROM
        assert_eq!((memory.read(0x1234), memory.read(0xC000)), (0x42, 0xEA));

        input.reset.store(true, Ordering::Relaxed);
        assert!(memory.take_reset());
        memory.power_on(&rom);
        assert_eq!(memory.read(0x1234), 0x42, "the reset button keeps RAM");
        memory.power_on(&rom);
        assert_eq!(memory.read(0x1234), 0, "power-on clears it");
    }

    #[test]
    fn system_registers() {
        let mut memory = Memory::default();
        memory.power_on(&[0; cart::ROM_SIZE]);
        memory.end_frame();
        assert_eq!(memory.read(FRAME_COUNT), 1);
        memory.write(FRAME_WAIT, 0);
        assert!(memory.take_frame_wait());
        assert!(!memory.take_frame_wait());
        memory.write(POWER, 20);
        assert_eq!(memory.take_host_call(), Some(20));
        assert_eq!(memory.read(HOST), 0);

        assert_eq!(memory.read(RTC_LATCH), 0, "not latched yet");
        let ms = || SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
        let before = ms();
        memory.write(RTC_LATCH, 0);
        let after = ms();
        let rtc: Vec<u8> = (RTC_LATCH..=RTC_END).map(|addr| memory.read(addr)).collect();
        let rtc = u64::from_le_bytes(rtc.try_into().unwrap());
        assert!((before..=after).contains(&rtc), "the time when latched");
    }
}
