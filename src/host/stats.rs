//! How a machine's running, measured as it runs: for the debug panel, for
//! any machine. Every half second, what that half second came to: frames
//! shown a second, CPU cycles run a second, cycles a frame, and how much of
//! the time went on the work of a frame.

/// How long each measure's taken over.
const WINDOW: f64 = 0.5;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    /// The last measure's.
    pub frames_per_second: f64,
    pub cycles_per_second: f64,
    pub cycles_per_frame: u64,
    /// Of the time, the share spent working: 1 is all of it.
    pub load: f64,
    // This one's so far, and when it began.
    began: Option<f64>,
    frames: u64,
    cycles: u64,
    busy: f64,
}

impl Stats {
    pub fn new() -> Self {
        Stats::default()
    }

    /// A frame ended at `now`, seconds, of `cycles`, `busy` seconds' work.
    pub fn frame(&mut self, now: f64, cycles: u64, busy: f64) {
        let Some(began) = self.began else {
            self.began = Some(now);
            return;
        };
        self.frames += 1;
        self.cycles += cycles;
        self.busy += busy;
        let elapsed = now - began;
        if elapsed < WINDOW {
            return;
        }
        self.frames_per_second = self.frames as f64 / elapsed;
        self.cycles_per_second = self.cycles as f64 / elapsed;
        self.cycles_per_frame = self.cycles / self.frames;
        self.load = self.busy / elapsed;
        (self.began, self.frames, self.cycles, self.busy) = (Some(now), 0, 0, 0.0);
    }

    /// Lines of it, for a CPU of clock `clock_hz`.
    pub fn lines(&self, clock_hz: u32) -> Vec<String> {
        if self.frames_per_second == 0.0 {
            return vec!["MEASURING".to_string()];
        }
        let target = clock_hz as f64 / self.cycles_per_frame.max(1) as f64;
        vec![
            format!("FPS {:.1} OF {:.1}", self.frames_per_second, target),
            format!(
                "CPU {:.3} MHZ {:.0}%",
                self.cycles_per_second / 1e6,
                100.0 * self.cycles_per_second / clock_hz as f64
            ),
            format!("HOST LOAD {:.0}%", self.load * 100.0),
            format!("CYCLES A FRAME {}", self.cycles_per_frame),
        ]
    }
}
