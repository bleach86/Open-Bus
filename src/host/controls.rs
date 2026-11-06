//! What's pressed for what: for each player, each of the console's buttons
//! and the way back to the library, what's bound to it. A binding's a word
//! for what's held, the same for all of it: `key:X`, a key by the window's
//! name for it; `pad:a`, a gamepad's button or `pad:lefty-`, a stick pushed
//! that way, all on that player's gamepad.
//!
//! They're kept in a file of a binding a line, its player, its action and
//! the binding, shared by however `cpu_emu` is started; without one, the
//! defaults. The function keys aren't here: they're always what they are,
//! so that nothing can be lost.

use crate::host::files;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    Select,
    Start,
    Library,
}

pub const ACTIONS: [Action; 9] = [
    Action::Up,
    Action::Down,
    Action::Left,
    Action::Right,
    Action::A,
    Action::B,
    Action::Select,
    Action::Start,
    Action::Library,
];

/// The file's word for an action.
fn word(action: Action) -> &'static str {
    match action {
        Action::Up => "up",
        Action::Down => "down",
        Action::Left => "left",
        Action::Right => "right",
        Action::A => "a",
        Action::B => "b",
        Action::Select => "select",
        Action::Start => "start",
        Action::Library => "library",
    }
}

/// Its name on the screen.
pub fn name(action: Action) -> String {
    word(action).to_uppercase()
}

/// What's said of a binding on the screen.
pub fn said(source: &str) -> String {
    match source.split_once(':') {
        Some(("key", key)) => key.to_uppercase(),
        Some(("pad", button)) => format!("PAD {}", button.strip_prefix("dp").unwrap_or(button).to_uppercase()),
        _ => source.to_uppercase(),
    }
}

/// Whether a binding's a stick's: left alone when a button's bound over it,
/// a stick steering whatever the D-pad is.
pub fn is_stick(source: &str) -> bool {
    source.starts_with("pad:left") && source.ends_with(['+', '-'])
}

const GAMEPAD: [(Action, &str); 8] = [
    (Action::Up, "pad:dpup"),
    (Action::Down, "pad:dpdown"),
    (Action::Left, "pad:dpleft"),
    (Action::Right, "pad:dpright"),
    (Action::A, "pad:a"),
    (Action::B, "pad:x"),
    (Action::Select, "pad:back"),
    (Action::Start, "pad:start"),
];
const STICK: [(Action, &str); 4] = [
    (Action::Up, "pad:lefty-"),
    (Action::Down, "pad:lefty+"),
    (Action::Left, "pad:leftx-"),
    (Action::Right, "pad:leftx+"),
];
const KEYBOARD: [(Action, &str); 10] = [
    (Action::Up, "key:Up"),
    (Action::Down, "key:Down"),
    (Action::Left, "key:Left"),
    (Action::Right, "key:Right"),
    (Action::A, "key:X"),
    (Action::B, "key:Z"),
    (Action::Select, "key:Right Shift"),
    (Action::Select, "key:Tab"),
    (Action::Start, "key:Return"),
    (Action::Library, "key:F5"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Controls {
    /// Each binding: its player, 0 or 1; its action; what's held for it.
    bindings: Vec<(usize, Action, String)>,
}

impl Default for Controls {
    /// As the console's own: the first player's on the keyboard too, and
    /// the Guide button for the library.
    fn default() -> Controls {
        let mut bindings = Vec::new();
        for player in 0..2 {
            for (action, source) in GAMEPAD.iter().chain(&STICK) {
                bindings.push((player, *action, source.to_string()));
            }
        }
        for (action, source) in KEYBOARD {
            bindings.push((0, action, source.to_string()));
        }
        bindings.push((0, Action::Library, "pad:guide".to_string()));
        Controls { bindings }
    }
}

impl Controls {
    /// What's bound to `action` for `player`.
    pub fn of(&self, player: usize, action: Action) -> Vec<&str> {
        let this = |&&(who, what, _): &&(usize, Action, String)| who == player && what == action;
        self.bindings.iter().filter(this).map(|(_, _, source)| source.as_str()).collect()
    }

    /// Whether `action`'s held for `player`, by what `held` says of each
    /// binding.
    pub fn holds(&self, player: usize, action: Action, held: impl Fn(&str) -> bool) -> bool {
        self.of(player, action).into_iter().any(held)
    }

    /// `source` bound to `action` for `player`: in place of the same kind
    /// bound to it before, a key's of a key, a button's of a button, a stick
    /// staying; and no longer anything else of that player's.
    pub fn bind(&mut self, player: usize, action: Action, source: &str) {
        let kind = |source: &str| source.split(':').next().unwrap_or("").to_string();
        let new_kind = kind(source);
        self.bindings.retain(|(who, what, old)| {
            let same_slot = *what == action && kind(old) == new_kind && !is_stick(old);
            *who != player || (old != source && !same_slot)
        });
        self.bindings.push((player, action, source.to_string()));
    }

    /// All a player's put back as they were.
    pub fn reset(&mut self, player: usize) {
        self.bindings.retain(|(who, _, _)| *who != player);
        self.bindings.extend(Controls::default().bindings.into_iter().filter(|(who, _, _)| *who == player));
    }

    /// As it's kept: a line each.
    pub fn text(&self) -> String {
        let line = |(who, what, source): &(usize, Action, String)| format!("{} {} {source}\n", who + 1, word(*what));
        self.bindings.iter().map(line).collect()
    }

    /// As `text` has it; any line it doesn't know left out, and with none
    /// it does, the defaults.
    pub fn parse(text: &str) -> Controls {
        let mut bindings = Vec::new();
        for line in text.lines() {
            let mut parts = line.splitn(3, ' ');
            let (Some(who), Some(what), Some(source)) = (parts.next(), parts.next(), parts.next()) else { continue };
            let player = match who {
                "1" => 0,
                "2" => 1,
                _ => continue,
            };
            if let Some(&action) = ACTIONS.iter().find(|&&action| word(action) == what) {
                bindings.push((player, action, source.to_string()));
            }
        }
        if bindings.is_empty() { Controls::default() } else { Controls { bindings } }
    }

    /// What's kept, or the defaults.
    pub fn load() -> Controls {
        files::read(&path()).map_or_else(|_| Controls::default(), |text| Controls::parse(&String::from_utf8_lossy(&text)))
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = path();
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        files::save(&path, self.text().as_bytes())
    }
}

/// Where they're kept: the system's place for a program's settings.
pub fn path() -> PathBuf {
    let home = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let folder = if cfg!(windows) {
        home("APPDATA").map(|data| data.join("Open Bus"))
    } else if cfg!(target_os = "macos") {
        home("HOME").map(|home| home.join("Library/Application Support/Open Bus"))
    } else {
        home("XDG_CONFIG_HOME").or_else(|| home("HOME").map(|home| home.join(".config"))).map(|config| config.join("open-bus"))
    };
    folder.unwrap_or_default().join("controls.txt")
}
