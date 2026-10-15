mod audio;
mod commands;
mod hotkeys;
mod models;
mod pack;
mod state;
mod storage;
mod youtube;

use std::sync::Arc;
use std::thread;

use parking_lot::Mutex;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WindowEvent,
};
use tauri_plugin_autostart::MacosLauncher;

use crate::{
    audio::engine::{self, EngineCmd},
    commands::*,
    hotkeys::HotkeyFire,
    models::Settings,
    state::{preload_library, show_main, AppState, Inner, TrayItems},
    storage::{load_json, Paths, Saver},
};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .setup(|app| {
            let handle = app.handle().clone();
            init_app(handle)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            let app = window.app_handle();
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    if let Some(state) = app.try_state::<AppState>() {
                        let close_to_tray = state.inner.lock().settings.close_to_tray;
                        if close_to_tray {
                            api.prevent_close();
                            let _ = window.hide();
                            state.set_hidden(app, true);
                        } else {
                            state.saver.flush();
                            state.audio.send(EngineCmd::Shutdown);
                            state.hotkeys.shutdown();
                        }
                    }
                }
                WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                    if let Some(state) = app.try_state::<AppState>() {
                        let minimized = window.is_minimized().unwrap_or(false);
                        let visible = window.is_visible().unwrap_or(true);
                        state.set_hidden(app, minimized || !visible);
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            get_playback,
            get_audio,
            list_devices,
            set_active,
            play_sound,
            preview_sound,
            stop_sound,
            stop_all,
            update_sound,
            remove_sound,
            create_folder,
            rename_folder,
            delete_folder,
            update_settings,
            check_hotkey_cmd,
            import_paths,
            pick_import_files,
            export_library,
            import_pack,
            set_compact,
            set_view_mode,
            open_library_folder,
            library_path,
            show_window,
            youtube_preview,
            youtube_prepare,
            youtube_import,
            youtube_cancel,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Cueboard")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app.try_state::<AppState>() {
                    state.saver.flush();
                    state.audio.send(EngineCmd::Shutdown);
                    state.hotkeys.shutdown();
                }
            }
        });
}

fn init_app(app: AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let root = app.path().app_data_dir()?;
    std::fs::create_dir_all(&root)?;
    let paths = Paths::new(root)?;

    let mut settings: Settings = load_json(&paths.settings);
    settings.sanitize();
    let library: crate::models::Library = load_json(&paths.library);
    let saver = Saver::spawn(paths.settings.clone(), paths.library.clone());
    saver.settings(settings.clone());

    let (eng_tx, eng_rx) = crossbeam_channel::unbounded();
    let audio = engine::spawn(
        settings.master_volume,
        settings.monitor_volume,
        settings.max_voices,
        move |ev| {
            let _ = eng_tx.send(ev);
        },
    );
    audio.send(EngineCmd::SetOutputs {
        primary: settings.output_device.clone(),
        primary_name: settings.output_device_name.clone(),
        monitor: settings.monitor_device.clone(),
        monitor_name: settings.monitor_device_name.clone(),
    });
    preload_library(&audio, &paths, &library);

    let (hk_tx, hk_rx) = crossbeam_channel::unbounded::<HotkeyFire>();
    let hotkeys = crate::hotkeys::spawn(move |fire| {
        let _ = hk_tx.send(fire);
    });

    let active = settings.start_active;
    let inner = Arc::new(Mutex::new(Inner {
        library,
        settings: settings.clone(),
        active,
        hidden: false,
        compact: false,
        playback: Vec::new(),
        audio_status: audio.status(),
        warnings: Vec::new(),
    }));

    let state = AppState {
        inner: inner.clone(),
        audio,
        hotkeys,
        saver,
        paths,
    };

    let show = MenuItem::with_id(&app, "show", "Show Cueboard", true, None::<&str>)?;
    let toggle = CheckMenuItem::with_id(
        &app,
        "toggle-active",
        "Soundboard Active",
        true,
        active,
        None::<&str>,
    )?;
    let stop = MenuItem::with_id(&app, "stop-all", "Stop All Sounds", true, None::<&str>)?;
    let quit = MenuItem::with_id(&app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        &app,
        &[
            &show,
            &toggle,
            &stop,
            &PredefinedMenuItem::separator(&app)?,
            &quit,
        ],
    )?;

    let icon = app
        .default_window_icon()
        .cloned()
        .or_else(|| {
            tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png")).ok()
        })
        .expect("app icon");

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Cueboard")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "toggle-active" => {
                if let Some(state) = app.try_state::<AppState>() {
                    state.toggle_active(app);
                }
            }
            "stop-all" => {
                if let Some(state) = app.try_state::<AppState>() {
                    state.stop_all();
                }
            }
            "quit" => {
                if let Some(state) = app.try_state::<AppState>() {
                    state.saver.flush();
                    state.audio.send(EngineCmd::Shutdown);
                    state.hotkeys.shutdown();
                }
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(&app)?;

    app.manage(TrayItems { toggle });
    app.manage(crate::youtube::YoutubeCtl::new());
    app.manage(state);

    let ev_app = app.clone();
    thread::Builder::new()
        .name("cueboard-events".into())
        .spawn(move || {
            loop {
                crossbeam_channel::select! {
                    recv(eng_rx) -> ev => {
                        let Ok(ev) = ev else { break };
                        if let Some(state) = ev_app.try_state::<AppState>() {
                            state.handle_engine(&ev_app, ev);
                        }
                    }
                    recv(hk_rx) -> fire => {
                        let Ok(fire) = fire else { break };
                        if let Some(state) = ev_app.try_state::<AppState>() {
                            state.handle_hotkey(&ev_app, fire);
                        }
                    }
                }
            }
        })?;

    if let Some(state) = app.try_state::<AppState>() {
        state.refresh_hotkeys(&app);
        if let Err(err) = sync_startup(&app, settings.start_with_windows) {
            state.notice(&app, err);
        }
    }

    if settings.start_minimized {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
        }
        if let Some(state) = app.try_state::<AppState>() {
            state.set_hidden(&app, true);
        }
    }

    Ok(())
}

fn sync_startup(app: &AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let auto = app.autolaunch();
    let current = auto.is_enabled().map_err(|e| e.to_string())?;
    if enabled && !current {
        auto.enable()
            .map_err(|e| format!("Could not enable start with Windows: {e}"))?;
    } else if !enabled && current {
        auto.disable()
            .map_err(|e| format!("Could not disable start with Windows: {e}"))?;
    }
    Ok(())
}
