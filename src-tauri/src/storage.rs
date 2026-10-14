use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use serde::{de::DeserializeOwned, Serialize};

use crate::models::{Library, Settings};

pub struct Paths {
    pub root: PathBuf,
    pub sounds: PathBuf,
    pub settings: PathBuf,
    pub library: PathBuf,
}

impl Paths {
    pub fn new(root: PathBuf) -> std::io::Result<Self> {
        let sounds = root.join("sounds");
        fs::create_dir_all(&sounds)?;
        Ok(Self {
            settings: root.join("settings.json"),
            library: root.join("library.json"),
            sounds,
            root,
        })
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Loads a JSON document. A corrupted file is kept aside as `*.corrupt-<time>` so the
/// user does not silently lose data, and defaults are returned.
pub fn load_json<T: DeserializeOwned + Default>(path: &Path) -> T {
    match fs::read(path) {
        Ok(bytes) => match serde_json::from_slice::<T>(&bytes) {
            Ok(value) => value,
            Err(err) => {
                log::error!("failed to parse {}: {err}", path.display());
                let backup = path.with_extension(format!("corrupt-{}", now_ms()));
                let _ = fs::rename(path, backup);
                T::default()
            }
        },
        Err(_) => T::default(),
    }
}

pub fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let data = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, data).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub enum SaveRequest {
    Settings(Settings),
    Library(Library),
    Flush(Sender<()>),
}

/// Coalesces rapid edits (dragging a volume slider, for example) into a single disk write.
#[derive(Clone)]
pub struct Saver {
    tx: Sender<SaveRequest>,
}

impl Saver {
    pub fn spawn(settings_path: PathBuf, library_path: PathBuf) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        thread::Builder::new()
            .name("cueboard-saver".into())
            .spawn(move || saver_loop(rx, settings_path, library_path))
            .expect("failed to spawn saver thread");
        Self { tx }
    }

    pub fn settings(&self, s: Settings) {
        let _ = self.tx.send(SaveRequest::Settings(s));
    }

    pub fn library(&self, l: Library) {
        let _ = self.tx.send(SaveRequest::Library(l));
    }

    pub fn flush(&self) {
        let (tx, rx) = crossbeam_channel::bounded(1);
        if self.tx.send(SaveRequest::Flush(tx)).is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(3));
        }
    }
}

fn saver_loop(rx: Receiver<SaveRequest>, settings_path: PathBuf, library_path: PathBuf) {
    let mut pending_settings: Option<Settings> = None;
    let mut pending_library: Option<Library> = None;
    let write = |s: &mut Option<Settings>, l: &mut Option<Library>| {
        if let Some(s) = s.take() {
            if let Err(e) = save_json(&settings_path, &s) {
                log::error!("saving settings failed: {e}");
            }
        }
        if let Some(l) = l.take() {
            if let Err(e) = save_json(&library_path, &l) {
                log::error!("saving library failed: {e}");
            }
        }
    };
    loop {
        let timeout = if pending_settings.is_some() || pending_library.is_some() {
            Duration::from_millis(350)
        } else {
            Duration::from_secs(3600)
        };
        match rx.recv_timeout(timeout) {
            Ok(SaveRequest::Settings(s)) => pending_settings = Some(s),
            Ok(SaveRequest::Library(l)) => pending_library = Some(l),
            Ok(SaveRequest::Flush(done)) => {
                write(&mut pending_settings, &mut pending_library);
                let _ = done.send(());
            }
            Err(RecvTimeoutError::Timeout) => write(&mut pending_settings, &mut pending_library),
            Err(RecvTimeoutError::Disconnected) => {
                write(&mut pending_settings, &mut pending_library);
                return;
            }
        }
    }
}
