//! Screen blanking (`--blank-after`): on battery, turn the host's backlight
//! off after a while without input, and back on at the next key or button
//! press or when the host goes back on mains. Uses the first
//! `/sys/class/backlight` device, where brightness 0 is off. This is the
//! host's power management, not part of the machine.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub struct Blanker {
    after: Duration,
    brightness: Option<PathBuf>,
    last_input: Instant,
    /// The brightness to restore while the screen is blank.
    saved: Option<String>,
}

impl Blanker {
    pub fn new(after: Duration) -> Self {
        let brightness = fs::read_dir("/sys/class/backlight")
            .ok()
            .and_then(|mut devices| devices.next())
            .and_then(|device| device.ok())
            .map(|device| device.path().join("brightness"));
        Blanker {
            after,
            brightness,
            last_input: Instant::now(),
            saved: None,
        }
    }

    /// A key or button was pressed. True if it only woke the screen, so the
    /// caller can swallow it.
    pub fn input(&mut self) -> bool {
        self.last_input = Instant::now();
        let was_blank = self.saved.is_some();
        self.wake();
        was_blank
    }

    /// Once per tick: blank the screen if it is time to.
    pub fn tick(&mut self, on_battery: bool) {
        if !on_battery {
            // On mains the screen stays on, and unplugging starts a fresh wait.
            self.wake();
            self.last_input = Instant::now();
        } else if self.saved.is_none() && self.last_input.elapsed() >= self.after {
            self.blank();
        }
    }

    fn blank(&mut self) {
        let Some(path) = &self.brightness else {
            return;
        };
        let Ok(current) = fs::read_to_string(path) else {
            return;
        };
        if fs::write(path, "0").is_ok() {
            self.saved = Some(current.trim().to_string());
        }
    }

    fn wake(&mut self) {
        if let (Some(path), Some(saved)) = (&self.brightness, self.saved.take()) {
            let _ = fs::write(path, saved);
        }
    }
}

