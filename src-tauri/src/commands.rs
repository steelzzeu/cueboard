use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;

use crate::{
    audio::engine::DeviceInfo,
    hotkeys,
    models::{Folder, Hotkey, Settings, Sound},
    pack,
    state::{
        apply_settings_patch, check_hotkey, file_stem_label, show_main, AppState, AudioPayload,
        HotkeyCheck, ImportReport, Snapshot,
    },
};

#[tauri::command]
pub fn get_snapshot(state: State<AppState>) -> Snapshot {
    state.snapshot()
}

#[tauri::command]
pub fn get_playback(state: State<AppState>) -> Vec<crate::audio::engine::PlayingVoice> {
    state.inner.lock().playback.clone()
}

#[tauri::command]
pub fn get_audio(state: State<AppState>) -> AudioPayload {
    AudioPayload {
        status: state.audio.status(),
        devices: state.audio.list_devices(),
    }
}

#[tauri::command]
pub fn list_devices(state: State<AppState>) -> Vec<DeviceInfo> {
    state.audio.list_devices()
}

#[tauri::command]
pub fn set_active(app: AppHandle, state: State<AppState>, active: bool) {
    state.set_active(&app, active);
}

#[tauri::command]
pub fn play_sound(app: AppHandle, state: State<AppState>, id: String) {
    state.play_sound(&app, &id, false);
}

#[tauri::command]
pub fn preview_sound(app: AppHandle, state: State<AppState>, id: String) {
    state.play_sound(&app, &id, true);
}

#[tauri::command]
pub fn stop_sound(state: State<AppState>, id: String) {
    state.stop_sound(&id);
}

#[tauri::command]
pub fn stop_all(state: State<AppState>) {
    state.stop_all();
}

#[tauri::command]
pub fn update_sound(app: AppHandle, state: State<AppState>, sound: Sound) -> Result<Sound, String> {
    if let Some(ref hk) = sound.hotkey {
        if let Some(reason) = hotkeys::reserved_reason(hk) {
            return Err(reason.to_string());
        }
        let check = check_hotkey(&state.inner.lock(), hk, Some(&sound.id));
        if check.unsupported {
            return Err("This key cannot be registered as a Windows shortcut.".into());
        }
        if let Some(conflict) = check.conflict {
            return Err(format!("That shortcut is already used by {}.", conflict.name));
        }
    }
    state.update_sound(&app, sound)
}

#[tauri::command]
pub fn remove_sound(app: AppHandle, state: State<AppState>, id: String) -> Result<(), String> {
    state.remove_sound(&app, &id)
}

#[tauri::command]
pub fn create_folder(app: AppHandle, state: State<AppState>, name: String) -> Result<Folder, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a folder name.".into());
    }
    let folder = Folder {
        id: Uuid::new_v4().to_string(),
        name: name.chars().take(40).collect(),
    };
    state.inner.lock().library.folders.push(folder.clone());
    state.persist_library();
    state.emit_snapshot(&app);
    Ok(folder)
}

#[tauri::command]
pub fn rename_folder(
    app: AppHandle,
    state: State<AppState>,
    id: String,
    name: String,
) -> Result<(), String> {
    let name = name.trim().chars().take(40).collect::<String>();
    if name.is_empty() {
        return Err("Enter a folder name.".into());
    }
    {
        let mut g = state.inner.lock();
        let folder = g
            .library
            .folders
            .iter_mut()
            .find(|f| f.id == id)
            .ok_or_else(|| "Folder not found.".to_string())?;
        folder.name = name;
    }
    state.persist_library();
    state.emit_snapshot(&app);
    Ok(())
}

#[tauri::command]
pub fn delete_folder(app: AppHandle, state: State<AppState>, id: String) -> Result<(), String> {
    {
        let mut g = state.inner.lock();
        g.library.folders.retain(|f| f.id != id);
        for sound in &mut g.library.sounds {
            if sound.folder_id.as_deref() == Some(id.as_str()) {
                sound.folder_id = None;
            }
        }
    }
    state.persist_library();
    state.emit_snapshot(&app);
    Ok(())
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<AppState>,
    settings: Settings,
) -> Result<Settings, String> {
    if let Some(ref hk) = settings.toggle_hotkey {
        if let Some(reason) = hotkeys::reserved_reason(hk) {
            return Err(reason.to_string());
        }
        let check = check_hotkey(&state.inner.lock(), hk, Some("toggle"));
        if let Some(conflict) = check.conflict {
            return Err(format!("That shortcut is already used by {}.", conflict.name));
        }
        if check.unsupported {
            return Err("This key cannot be registered as a Windows shortcut.".into());
        }
    }
    if let Some(ref hk) = settings.stop_all_hotkey {
        if let Some(reason) = hotkeys::reserved_reason(hk) {
            return Err(reason.to_string());
        }
        let check = check_hotkey(&state.inner.lock(), hk, Some("stopAll"));
        if let Some(conflict) = check.conflict {
            return Err(format!("That shortcut is already used by {}.", conflict.name));
        }
        if check.unsupported {
            return Err("This key cannot be registered as a Windows shortcut.".into());
        }
    }

    let start_with_windows;
    let hotkeys_changed;
    {
        let mut g = state.inner.lock();
        hotkeys_changed = hotkey_opt_changed(&g.settings.toggle_hotkey, &settings.toggle_hotkey)
            || hotkey_opt_changed(&g.settings.stop_all_hotkey, &settings.stop_all_hotkey)
            || g.settings.minimize_behavior != settings.minimize_behavior;
        apply_settings_patch(&mut g.settings, settings);
        start_with_windows = g.settings.start_with_windows;
    }
    state.apply_outputs();
    state.persist_settings();
    if let Err(err) = sync_autostart(&app, start_with_windows) {
        state.notice(&app, err);
    }
    if hotkeys_changed {
        state.refresh_hotkeys(&app);
    } else {
        state.emit_snapshot(&app);
        state.update_tray(&app);
    }
    Ok(state.inner.lock().settings.clone())
}

fn hotkey_opt_changed(a: &Option<crate::models::Hotkey>, b: &Option<crate::models::Hotkey>) -> bool {
    match (a, b) {
        (None, None) => false,
        (Some(x), Some(y)) => x.signature() != y.signature(),
        _ => true,
    }
}

fn sync_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let auto = app.autolaunch();
    let current = auto.is_enabled().map_err(|e| e.to_string())?;
    if enabled && !current {
        auto.enable().map_err(|e| format!("Could not enable start with Windows: {e}"))?;
    } else if !enabled && current {
        auto.disable()
            .map_err(|e| format!("Could not disable start with Windows: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn check_hotkey_cmd(
    state: State<AppState>,
    hotkey: Hotkey,
    ignore: Option<String>,
) -> HotkeyCheck {
    check_hotkey(&state.inner.lock(), &hotkey, ignore.as_deref())
}

#[tauri::command]
pub fn import_paths(app: AppHandle, state: State<AppState>, paths: Vec<String>) -> ImportReport {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    let mut files = Vec::new();
    let mut packs = Vec::new();
    for path in paths {
        if pack::is_zip(&path) {
            packs.push(path);
        } else {
            files.push(path);
        }
    }
    let mut report = state.import_paths(&app, files);
    for pack_path in packs {
        match import_pack_file(&app, &state, &pack_path) {
            Ok(mut extra) => {
                report.imported.append(&mut extra.imported);
                report.failed.append(&mut extra.failed);
            }
            Err(message) => report.failed.push(crate::state::ImportFailure {
                name: file_stem_label(&pack_path),
                message,
            }),
        }
    }
    report
}

#[tauri::command]
pub fn pick_import_files(app: AppHandle, state: State<AppState>) -> Result<ImportReport, String> {
    let picked = app
        .dialog()
        .file()
        .add_filter(
            "Audio",
            &[
                "mp3", "wav", "ogg", "oga", "flac", "aac", "m4a", "mp4", "wma", "opus", "aif",
                "aiff", "caf", "mka", "webm",
            ],
        )
        .add_filter("All files", &["*"])
        .blocking_pick_files();
    let Some(files) = picked else {
        return Ok(ImportReport {
            imported: Vec::new(),
            failed: Vec::new(),
        });
    };
    let paths = files
        .into_iter()
        .filter_map(|f| f.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    Ok(import_paths(app, state, paths))
}

#[tauri::command]
pub fn export_library(app: AppHandle, state: State<AppState>) -> Result<bool, String> {
    let dest = app
        .dialog()
        .file()
        .add_filter("Cueboard pack", &["zip"])
        .set_file_name("cueboard-library.zip")
        .blocking_save_file();
    let Some(dest) = dest else {
        return Ok(false);
    };
    let path = dest.into_path().map_err(|e| e.to_string())?;
    let (sounds, folders) = {
        let g = state.inner.lock();
        (g.library.sounds.clone(), g.library.folders.clone())
    };
    pack::write_pack(&path, &state.paths.sounds, &sounds, &folders)?;
    Ok(true)
}

#[tauri::command]
pub fn import_pack(app: AppHandle, state: State<AppState>) -> Result<ImportReport, String> {
    let picked = app
        .dialog()
        .file()
        .add_filter("Cueboard pack", &["zip"])
        .blocking_pick_file();
    let Some(file) = picked else {
        return Ok(ImportReport {
            imported: Vec::new(),
            failed: Vec::new(),
        });
    };
    let path = file.into_path().map_err(|e| e.to_string())?;
    import_pack_file(&app, &state, &path)
}

fn import_pack_file(
    app: &AppHandle,
    state: &AppState,
    path: &std::path::Path,
) -> Result<ImportReport, String> {
    let staging = state.paths.root.join(format!("import-{}", Uuid::new_v4()));
    let extracted = pack::extract_pack(path, staging.clone())?;
    let mut report = ImportReport {
        imported: Vec::new(),
        failed: Vec::new(),
    };

    if let Some(manifest) = extracted.manifest {
        for folder in manifest.folders {
            let exists = state
                .inner
                .lock()
                .library
                .folders
                .iter()
                .any(|f| f.id == folder.id);
            if !exists {
                state.inner.lock().library.folders.push(Folder {
                    id: folder.id,
                    name: folder.name,
                });
            }
        }
        for sound in manifest.sounds {
            let src = extracted.staging.join("files").join(&sound.file);
            let fallback = extracted.staging.join(&sound.file);
            let file = if src.exists() { src } else { fallback };
            if !file.exists() {
                report.failed.push(crate::state::ImportFailure {
                    name: sound.name,
                    message: "The audio file was missing from the pack.".into(),
                });
                continue;
            }
            match state.import_paths(app, vec![file]) {
                r if r.imported.is_empty() => {
                    if let Some(fail) = r.failed.into_iter().next() {
                        report.failed.push(fail);
                    }
                }
                r => {
                    if let Some(mut imported) = r.imported.into_iter().next() {
                        imported.name = sound.name;
                        imported.volume = sound.volume;
                        imported.looping = sound.looping;
                        imported.favorite = sound.favorite;
                        imported.color = sound.color;
                        imported.folder_id = sound.folder_id;
                        imported.hotkey = sound.hotkey;
                        let _ = state.update_sound(app, imported.clone());
                        report.imported.push(imported);
                    }
                }
            }
        }
    } else {
        report = state.import_paths(app, extracted.loose_audio);
    }

    let _ = std::fs::remove_dir_all(staging);
    state.persist_library();
    state.refresh_hotkeys(app);
    Ok(report)
}

#[tauri::command]
pub fn set_compact(app: AppHandle, state: State<AppState>, compact: bool) -> Result<(), String> {
    state.inner.lock().compact = compact;
    if let Some(window) = app.get_webview_window("main") {
        use tauri::{LogicalSize, Size};
        if compact {
            let on_top = state.inner.lock().settings.compact_always_on_top;
            let _ = window.set_size(Size::Logical(LogicalSize::new(400.0, 680.0)));
            let _ = window.set_always_on_top(on_top);
        } else {
            let _ = window.set_size(Size::Logical(LogicalSize::new(1120.0, 740.0)));
            let _ = window.set_always_on_top(false);
        }
    }
    state.emit_snapshot(&app);
    Ok(())
}

#[tauri::command]
pub fn set_view_mode(app: AppHandle, state: State<AppState>, mode: String) {
    {
        let mut g = state.inner.lock();
        g.settings.view_mode = if mode == "list" {
            "list".into()
        } else {
            "grid".into()
        };
    }
    state.persist_settings();
    state.emit_snapshot(&app);
}

#[tauri::command]
pub fn open_library_folder(state: State<AppState>) {
    crate::state::open_in_explorer(&state.paths.root);
}

#[tauri::command]
pub fn library_path(state: State<AppState>) -> String {
    state.paths.root.to_string_lossy().into_owned()
}

#[tauri::command]
pub fn show_window(app: AppHandle) {
    show_main(&app);
}

#[tauri::command]
pub fn youtube_preview(url: String) -> Result<crate::youtube::YoutubePreview, String> {
    crate::youtube::preview(&url)
}

#[tauri::command]
pub fn youtube_prepare(app: AppHandle) -> Result<(), String> {
    crate::youtube::prepare(&app)
}

#[tauri::command]
pub async fn youtube_import(
    app: AppHandle,
    url: String,
    folder_id: Option<String>,
) -> Result<crate::models::Sound, String> {
    tauri::async_runtime::spawn_blocking(move || crate::youtube::import_url(&app, &url, folder_id))
        .await
        .map_err(|e| format!("The converter stopped: {e}"))?
}

#[tauri::command]
pub fn youtube_cancel(app: AppHandle) {
    crate::youtube::cancel(&app);
}

