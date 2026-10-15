use serde::{Deserialize, Serialize};

/// A user-assigned shortcut. `code` follows the DOM `KeyboardEvent.code` naming
/// (`KeyA`, `Digit1`, `F5`, `Numpad3`) plus `MouseMiddle`, `MouseX1` and `MouseX2`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hotkey {
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub win: bool,
    pub code: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

/// Identity of a shortcut, ignoring the display label.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HotkeySignature {
    pub mods: Modifiers,
    pub code: String,
}

impl Hotkey {
    pub fn signature(&self) -> HotkeySignature {
        HotkeySignature {
            mods: Modifiers {
                ctrl: self.ctrl,
                alt: self.alt,
                shift: self.shift,
                win: self.win,
            },
            code: self.code.clone(),
        }
    }

    pub fn display(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.win {
            parts.push("Win");
        }
        let key = if self.label.is_empty() {
            self.code.as_str()
        } else {
            self.label.as_str()
        };
        parts.push(key);
        parts.join(" + ")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sound {
    pub id: String,
    pub name: String,
    /// File name inside the managed library folder.
    pub file: String,
    #[serde(default)]
    pub original_name: String,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default = "default_volume")]
    pub volume: f32,
    #[serde(default, rename = "loop")]
    pub looping: bool,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub folder_id: Option<String>,
    #[serde(default)]
    pub hotkey: Option<Hotkey>,
    #[serde(default)]
    pub added_at: u64,
}

fn default_volume() -> f32 {
    1.0
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Library {
    pub version: u32,
    pub sounds: Vec<Sound>,
    pub folders: Vec<Folder>,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            version: 1,
            sounds: Vec::new(),
            folders: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MinimizeBehavior {
    /// Shortcuts are disabled on minimise; the toggle shortcut can re-enable them.
    Disable,
    /// Shortcuts are disabled on minimise and the toggle shortcut is released too.
    Strict,
    /// The soundboard keeps its state when minimised or hidden in the tray.
    KeepActive,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RetriggerMode {
    /// Every trigger starts a new layered instance.
    Overlap,
    /// A trigger stops running instances of the same sound and starts it again.
    Restart,
    /// A trigger stops the sound if it is playing, otherwise starts it.
    Toggle,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub version: u32,
    pub output_device: Option<String>,
    pub output_device_name: Option<String>,
    pub monitor_device: Option<String>,
    pub monitor_device_name: Option<String>,
    pub master_volume: f32,
    pub monitor_volume: f32,
    pub toggle_hotkey: Option<Hotkey>,
    pub stop_all_hotkey: Option<Hotkey>,
    pub minimize_behavior: MinimizeBehavior,
    pub restore_on_show: bool,
    pub close_to_tray: bool,
    pub start_with_windows: bool,
    pub start_minimized: bool,
    pub start_active: bool,
    pub stop_on_disable: bool,
    pub retrigger_mode: RetriggerMode,
    pub max_voices: u32,
    pub compact_always_on_top: bool,
    pub view_mode: String,
    pub tray_notice_shown: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 2,
            output_device: None,
            output_device_name: None,
            monitor_device: None,
            monitor_device_name: None,
            master_volume: 0.8,
            monitor_volume: 1.0,
            toggle_hotkey: Some(Hotkey {
                ctrl: true,
                alt: false,
                shift: true,
                win: false,
                code: "F9".into(),
                label: "F9".into(),
            }),
            stop_all_hotkey: Some(Hotkey {
                ctrl: true,
                alt: false,
                shift: true,
                win: false,
                code: "F10".into(),
                label: "F10".into(),
            }),
            minimize_behavior: MinimizeBehavior::KeepActive,
            restore_on_show: true,
            close_to_tray: true,
            start_with_windows: false,
            start_minimized: false,
            start_active: true,
            stop_on_disable: false,
            retrigger_mode: RetriggerMode::Overlap,
            max_voices: 16,
            compact_always_on_top: true,
            view_mode: "grid".into(),
            tray_notice_shown: false,
        }
    }
}

impl Settings {
    pub fn sanitize(&mut self) {
        self.master_volume = self.master_volume.clamp(0.0, 1.0);
        self.monitor_volume = self.monitor_volume.clamp(0.0, 1.0);
        self.max_voices = self.max_voices.clamp(1, 64);
        if self.view_mode != "grid" && self.view_mode != "list" {
            self.view_mode = "grid".into();
        }
        if self.version < 2 {
            if self.minimize_behavior == MinimizeBehavior::Disable {
                self.minimize_behavior = MinimizeBehavior::KeepActive;
            }
            self.version = 2;
        }
    }
}

/// Which action a registered shortcut performs.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "camelCase")]
pub enum HotkeyTarget {
    Toggle,
    StopAll,
    Sound(String),
}

impl HotkeyTarget {
    pub fn key(&self) -> String {
        match self {
            HotkeyTarget::Toggle => "toggle".into(),
            HotkeyTarget::StopAll => "stopAll".into(),
            HotkeyTarget::Sound(id) => format!("sound:{id}"),
        }
    }
}
