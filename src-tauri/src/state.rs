use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use crate::{
    audio::{
        decode::{decode_file, is_supported_path, SUPPORTED_EXTENSIONS},
        engine::{AudioHandle, AudioStatus, DeviceInfo, EngineCmd, EngineEvent, PlayRequest, PlayingVoice},
    },
    hotkeys::{self, Binding, HotkeyHandle, HotkeyIssue, HotkeyFire},
    models::{
        Hotkey, HotkeyTarget, Library, MinimizeBehavior, Settings, Sound,
    },
    pack,
    storage::{self, Paths, Saver},
};

const MAX_IMPORT_BYTES: u64 = 200 * 1024 * 1024;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub library: Library,
    pub settings: Settings,
    pub active: bool,
    pub hidden: bool,
    pub compact: bool,
    pub sounds_live: bool,
    pub warnings: Vec<HotkeyIssue>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioPayload {
    pub status: AudioStatus,
    pub devices: Vec<DeviceInfo>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFailure {
    pub name: String,
    pub message: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub imported: Vec<Sound>,
    pub failed: Vec<ImportFailure>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyCheck {
    pub reserved: Option<String>,
    pub risky: Option<String>,
    pub conflict: Option<HotkeyConflict>,
    pub unsupported: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyConflict {
    pub kind: String,
    pub name: String,
}

pub struct Inner {
    pub library: Library,
    pub settings: Settings,
    pub active: bool,
    pub hidden: bool,
    pub compact: bool,
    pub playback: Vec<PlayingVoice>,
    pub audio_status: AudioStatus,
    pub warnings: Vec<HotkeyIssue>,
}

pub struct AppState {
    pub inner: Arc<Mutex<Inner>>,
    pub audio: AudioHandle,
    pub hotkeys: HotkeyHandle,
    pub saver: Saver,
    pub paths: Paths,
}

impl AppState {
    pub fn snapshot(&self) -> Snapshot {
        let g = self.inner.lock();
        Snapshot {
            library: g.library.clone(),
            settings: g.settings.clone(),
            active: g.active,
            hidden: g.hidden,
            compact: g.compact,
            sounds_live: sounds_live(g.active, g.hidden, g.settings.minimize_behavior),
            warnings: g.warnings.clone(),
        }
    }

    pub fn persist_settings(&self) {
        let settings = self.inner.lock().settings.clone();
        self.saver.settings(settings);
    }

    pub fn persist_library(&self) {
        let library = self.inner.lock().library.clone();
        self.saver.library(library);
    }

    pub fn emit_snapshot(&self, app: &AppHandle) {
        let _ = app.emit("snapshot", self.snapshot());
    }

    pub fn emit_playback(&self, app: &AppHandle) {
        let playback = self.inner.lock().playback.clone();
        let _ = app.emit("playback", playback);
    }

    pub fn emit_audio(&self, app: &AppHandle) {
        let status = self.inner.lock().audio_status.clone();
        let devices = self.audio.list_devices();
        let _ = app.emit(
            "audio-status",
            AudioPayload {
                status,
                devices,
            },
        );
    }

    pub fn notice(&self, app: &AppHandle, message: impl Into<String>) {
        let _ = app.emit("notice", message.into());
    }

    pub fn refresh_hotkeys(&self, app: &AppHandle) {
        let bindings = {
            let g = self.inner.lock();
            live_bindings(&g)
        };
        let result = self.hotkeys.sync(bindings);
        self.inner.lock().warnings = result.issues.clone();
        for issue in &result.issues {
            self.notice(app, issue.message.clone());
        }
        self.emit_snapshot(app);
        self.update_tray(app);
    }

    pub fn set_active(&self, app: &AppHandle, active: bool) {
        let stop = {
            let mut g = self.inner.lock();
            if g.active == active {
                return;
            }
            g.active = active;
            !active && g.settings.stop_on_disable
        };
        if stop {
            self.audio.send(EngineCmd::StopAll);
        }
        self.refresh_hotkeys(app);
    }

    pub fn toggle_active(&self, app: &AppHandle) {
        let next = !self.inner.lock().active;
        self.set_active(app, next);
    }

    pub fn set_hidden(&self, app: &AppHandle, hidden: bool) {
        let mut stop = false;
        {
            let mut g = self.inner.lock();
            if g.hidden == hidden {
                return;
            }
            g.hidden = hidden;
            if hidden {
                match g.settings.minimize_behavior {
                    MinimizeBehavior::KeepActive => {}
                    MinimizeBehavior::Disable | MinimizeBehavior::Strict => {
                        if g.active {
                            g.active = false;
                            stop = g.settings.stop_on_disable;
                        }
                    }
                }
            } else if g.settings.restore_on_show {
                g.active = true;
            }
        }
        if stop {
            self.audio.send(EngineCmd::StopAll);
        }
        self.refresh_hotkeys(app);
    }

    pub fn play_sound(&self, app: &AppHandle, id: &str, preview: bool) {
        let req = {
            let g = self.inner.lock();
            let Some(sound) = g.library.sounds.iter().find(|s| s.id == id) else {
                return;
            };
            PlayRequest {
                sound_id: sound.id.clone(),
                path: self.paths.sounds.join(&sound.file),
                volume: sound.volume,
                looping: sound.looping,
                preview,
                mode: g.settings.retrigger_mode,
            }
        };
        self.audio.send(EngineCmd::Play(req));
        let _ = app;
    }

    pub fn stop_sound(&self, id: &str) {
        self.audio.send(EngineCmd::Stop(id.to_string()));
    }

    pub fn stop_all(&self) {
        self.audio.send(EngineCmd::StopAll);
    }

    pub fn apply_outputs(&self) {
        let (primary, primary_name, monitor, monitor_name, master, monitor_vol, max_voices) = {
            let g = self.inner.lock();
            (
                g.settings.output_device.clone(),
                g.settings.output_device_name.clone(),
                g.settings.monitor_device.clone(),
                g.settings.monitor_device_name.clone(),
                g.settings.master_volume,
                g.settings.monitor_volume,
                g.settings.max_voices,
            )
        };
        self.audio.send(EngineCmd::SetOutputs {
            primary,
            primary_name,
            monitor,
            monitor_name,
        });
        self.audio.send(EngineCmd::SetMaster(master));
        self.audio.send(EngineCmd::SetMonitorVolume(monitor_vol));
        self.audio.send(EngineCmd::SetMaxVoices(max_voices));
    }

    pub fn update_tray(&self, app: &AppHandle) {
        let (active, hidden) = {
            let g = self.inner.lock();
            (g.active, g.hidden)
        };
        let live = {
            let g = self.inner.lock();
            sounds_live(g.active, g.hidden, g.settings.minimize_behavior)
        };
        if let Some(tray) = app.tray_by_id("main") {
            let tip = if live {
                "Cueboard  Active"
            } else if hidden {
                "Cueboard  Disabled (window hidden)"
            } else {
                "Cueboard  Disabled"
            };
            let _ = tray.set_tooltip(Some(tip));
        }
        if let Some(item) = app.try_state::<TrayItems>() {
            let _ = item.toggle.set_checked(active);
        }
        let _ = hidden;
    }

    pub fn import_paths(&self, app: &AppHandle, paths: Vec<PathBuf>) -> ImportReport {
        let mut imported = Vec::new();
        let mut failed = Vec::new();
        for path in paths {
            match self.import_one(&path, None, None) {
                Ok(sound) => imported.push(sound),
                Err(message) => failed.push(ImportFailure {
                    name: path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("file")
                        .to_string(),
                    message,
                }),
            }
        }
        if !imported.is_empty() {
            self.persist_library();
            self.emit_snapshot(app);
        }
        ImportReport { imported, failed }
    }

    pub(crate) fn import_one(
        &self,
        path: &Path,
        forced_name: Option<String>,
        folder_id: Option<String>,
    ) -> Result<Sound, String> {
        if pack::is_zip(path) {
            return Err("Use Import pack for .zip libraries.".into());
        }
        if !is_supported_path(path) {
            return Err(format!(
                "Unsupported format. Use {}.",
                SUPPORTED_EXTENSIONS.join(", ")
            ));
        }
        let meta = fs::metadata(path).map_err(|e| format!("Could not read the file: {e}"))?;
        if !meta.is_file() {
            return Err("That path is not a file.".into());
        }
        if meta.len() > MAX_IMPORT_BYTES {
            return Err("The file is larger than 200 MB.".into());
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin")
            .to_ascii_lowercase();
        let id = Uuid::new_v4().to_string();
        let stored = format!("{id}.{ext}");
        let dest = self.paths.sounds.join(&stored);
        fs::copy(path, &dest).map_err(|e| format!("Could not copy the file: {e}"))?;

        let clip = match decode_file(&dest) {
            Ok(clip) => clip,
            Err(err) => {
                let _ = fs::remove_file(&dest);
                return Err(err);
            }
        };
        let duration_ms = clip.duration_ms();
        self.audio.insert_clip(id.clone(), Arc::new(clip));

        let original = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("sound")
            .to_string();
        let name = forced_name.unwrap_or_else(|| stem_name(&original));

        let sound = Sound {
            id,
            name,
            file: stored,
            original_name: original,
            duration_ms,
            volume: 1.0,
            looping: false,
            favorite: false,
            color: None,
            folder_id,
            hotkey: None,
            added_at: storage::now_ms(),
        };
        self.inner.lock().library.sounds.push(sound.clone());
        Ok(sound)
    }

    pub fn remove_sound(&self, app: &AppHandle, id: &str) -> Result<(), String> {
        let file = {
            let mut g = self.inner.lock();
            let idx = g
                .library
                .sounds
                .iter()
                .position(|s| s.id == id)
                .ok_or_else(|| "Sound not found.".to_string())?;
            g.library.sounds.remove(idx).file
        };
        self.audio.send(EngineCmd::Stop(id.to_string()));
        self.audio.evict(id);
        let path = self.paths.sounds.join(file);
        let _ = fs::remove_file(path);
        self.persist_library();
        self.refresh_hotkeys(app);
        Ok(())
    }

    pub fn update_sound(&self, app: &AppHandle, next: Sound) -> Result<Sound, String> {
        let mut volume_change = None;
        let mut loop_change = None;
        let hotkey_changed;
        {
            let mut g = self.inner.lock();
            let Some(existing) = g.library.sounds.iter_mut().find(|s| s.id == next.id) else {
                return Err("Sound not found.".into());
            };
            if (existing.volume - next.volume).abs() > f32::EPSILON {
                volume_change = Some(next.volume.clamp(0.0, 1.0));
            }
            if existing.looping != next.looping {
                loop_change = Some(next.looping);
            }
            hotkey_changed = match (&existing.hotkey, &next.hotkey) {
                (None, None) => false,
                (Some(a), Some(b)) => a.signature() != b.signature(),
                _ => true,
            };
            existing.name = next.name.trim().chars().take(80).collect();
            if existing.name.is_empty() {
                existing.name = stem_name(&existing.original_name);
            }
            existing.volume = next.volume.clamp(0.0, 1.0);
            existing.looping = next.looping;
            existing.favorite = next.favorite;
            existing.color = next.color;
            existing.folder_id = next.folder_id;
            existing.hotkey = next.hotkey;
        }
        if let Some(vol) = volume_change {
            self.audio
                .send(EngineCmd::SetSoundVolume(next.id.clone(), vol));
        }
        if let Some(looping) = loop_change {
            self.audio
                .send(EngineCmd::SetSoundLoop(next.id.clone(), looping));
        }
        self.persist_library();
        if hotkey_changed {
            self.refresh_hotkeys(app);
        } else {
            self.emit_snapshot(app);
        }
        let sound = self
            .inner
            .lock()
            .library
            .sounds
            .iter()
            .find(|s| s.id == next.id)
            .cloned()
            .ok_or_else(|| "Sound not found.".to_string())?;
        Ok(sound)
    }

    pub fn handle_engine(&self, app: &AppHandle, event: EngineEvent) {
        match event {
            EngineEvent::Playback(list) => {
                self.inner.lock().playback = list;
                self.emit_playback(app);
            }
            EngineEvent::Status(status) => {
                self.inner.lock().audio_status = status;
                self.emit_audio(app);
            }
            EngineEvent::Notice(message) => self.notice(app, message),
            EngineEvent::LoadError { sound_id, message } => {
                let name = self
                    .inner
                    .lock()
                    .library
                    .sounds
                    .iter()
                    .find(|s| s.id == sound_id)
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| "A sound".into());
                self.notice(app, format!("{name}: {message}"));
            }
        }
    }

    pub fn handle_hotkey(&self, app: &AppHandle, fire: HotkeyFire) {
        match fire {
            HotkeyFire::Target(HotkeyTarget::Toggle) => self.toggle_active(app),
            HotkeyFire::Target(HotkeyTarget::StopAll) => self.stop_all(),
            HotkeyFire::Target(HotkeyTarget::Sound(id)) => {
                let live = {
                    let g = self.inner.lock();
                    sounds_live(g.active, g.hidden, g.settings.minimize_behavior)
                };
                if live {
                    self.play_sound(app, &id, false);
                }
            }
        }
    }
}

pub struct TrayItems {
    pub toggle: tauri::menu::CheckMenuItem<tauri::Wry>,
}

fn sounds_live(active: bool, hidden: bool, behavior: MinimizeBehavior) -> bool {
    if hidden && behavior == MinimizeBehavior::Strict {
        false
    } else {
        active
    }
}

fn toggle_live(hidden: bool, behavior: MinimizeBehavior) -> bool {
    !(hidden && behavior == MinimizeBehavior::Strict)
}

fn live_bindings(inner: &Inner) -> Vec<Binding> {
    let mut out = Vec::new();
    if toggle_live(inner.hidden, inner.settings.minimize_behavior) {
        if let Some(hk) = inner.settings.toggle_hotkey.clone() {
            out.push(Binding {
                hotkey: hk,
                target: HotkeyTarget::Toggle,
            });
        }
    }
    let sounds = sounds_live(inner.active, inner.hidden, inner.settings.minimize_behavior);
    if sounds {
        if let Some(hk) = inner.settings.stop_all_hotkey.clone() {
            out.push(Binding {
                hotkey: hk,
                target: HotkeyTarget::StopAll,
            });
        }
        for sound in &inner.library.sounds {
            if let Some(hk) = sound.hotkey.clone() {
                out.push(Binding {
                    hotkey: hk,
                    target: HotkeyTarget::Sound(sound.id.clone()),
                });
            }
        }
    }
    out
}

fn stem_name(file: &str) -> String {
    Path::new(file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file)
        .chars()
        .take(80)
        .collect()
}

pub fn check_hotkey(inner: &Inner, hotkey: &Hotkey, ignore: Option<&str>) -> HotkeyCheck {
    let unsupported = hotkeys::to_vk(&hotkey.code).is_none()
        && !matches!(hotkey.code.as_str(), "MouseMiddle" | "MouseX1" | "MouseX2");
    let mut conflict = None;
    if let Some(toggle) = &inner.settings.toggle_hotkey {
        if ignore != Some("toggle") && hotkeys::same_signature(toggle, hotkey) {
            conflict = Some(HotkeyConflict {
                kind: "toggle".into(),
                name: "Enable / disable soundboard".into(),
            });
        }
    }
    if conflict.is_none() {
        if let Some(stop) = &inner.settings.stop_all_hotkey {
            if ignore != Some("stopAll") && hotkeys::same_signature(stop, hotkey) {
                conflict = Some(HotkeyConflict {
                    kind: "stopAll".into(),
                    name: "Stop all sounds".into(),
                });
            }
        }
    }
    if conflict.is_none() {
        for sound in &inner.library.sounds {
            if Some(sound.id.as_str()) == ignore {
                continue;
            }
            if let Some(hk) = &sound.hotkey {
                if hotkeys::same_signature(hk, hotkey) {
                    conflict = Some(HotkeyConflict {
                        kind: "sound".into(),
                        name: sound.name.clone(),
                    });
                    break;
                }
            }
        }
    }
    HotkeyCheck {
        reserved: hotkeys::reserved_reason(hotkey).map(|s| s.to_string()),
        risky: hotkeys::risky_reason(hotkey).map(|s| s.to_string()),
        conflict,
        unsupported,
    }
}

pub fn apply_settings_patch(current: &mut Settings, patch: Settings) {
    *current = patch;
    current.sanitize();
}

pub fn preload_library(audio: &AudioHandle, paths: &Paths, library: &Library) {
    let items = library
        .sounds
        .iter()
        .map(|s| (s.id.clone(), paths.sounds.join(&s.file)))
        .collect();
    audio.preload(items);
}

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    if let Some(state) = app.try_state::<AppState>() {
        state.set_hidden(app, false);
    }
}

pub fn open_in_explorer(path: &Path) {
    let _ = std::process::Command::new("explorer").arg(path).spawn();
}

pub fn file_stem_label(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string()
}
