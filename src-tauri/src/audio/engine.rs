use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{select, Receiver, Sender};
use parking_lot::RwLock;
use serde::Serialize;

use super::{
    decode::{decode_file, Clip},
    mixer::{open_stream, MixCmd, MixEvent},
};
use crate::models::RetriggerMode;
use crate::storage::now_ms;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutputStatus {
    /// Device that was requested; `None` means "follow the Windows default".
    pub requested: Option<String>,
    pub requested_name: Option<String>,
    /// Device that is actually open right now.
    pub active_name: Option<String>,
    pub sample_rate: u32,
    pub channels: u16,
    pub fallback: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AudioStatus {
    pub primary: OutputStatus,
    pub monitor: Option<OutputStatus>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayingVoice {
    pub voice_id: u64,
    pub sound_id: String,
    pub started_at: u64,
    pub duration_ms: u64,
    pub looping: bool,
    pub preview: bool,
}

pub enum EngineEvent {
    Playback(Vec<PlayingVoice>),
    Status(AudioStatus),
    Notice(String),
    LoadError { sound_id: String, message: String },
}

pub struct PlayRequest {
    pub sound_id: String,
    pub path: PathBuf,
    pub volume: f32,
    pub looping: bool,
    pub preview: bool,
    pub mode: RetriggerMode,
}

pub enum EngineCmd {
    Play(PlayRequest),
    LoadFailed { sound_id: String, message: String },
    Stop(String),
    StopAll,
    SetSoundVolume(String, f32),
    SetSoundLoop(String, bool),
    SetMaster(f32),
    SetMonitorVolume(f32),
    SetOutputs {
        primary: Option<String>,
        primary_name: Option<String>,
        monitor: Option<String>,
        monitor_name: Option<String>,
    },
    SetMaxVoices(u32),
    ListDevices(Sender<Vec<DeviceInfo>>),
    GetStatus(Sender<AudioStatus>),
    Shutdown,
}

pub type ClipCache = Arc<RwLock<HashMap<String, Arc<Clip>>>>;

#[derive(Clone)]
pub struct AudioHandle {
    tx: Sender<EngineCmd>,
    pub cache: ClipCache,
    loader: Sender<LoadJob>,
}

impl AudioHandle {
    pub fn send(&self, cmd: EngineCmd) {
        let _ = self.tx.send(cmd);
    }

    pub fn list_devices(&self) -> Vec<DeviceInfo> {
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.send(EngineCmd::ListDevices(tx));
        rx.recv_timeout(Duration::from_secs(5)).unwrap_or_default()
    }

    pub fn status(&self) -> AudioStatus {
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.send(EngineCmd::GetStatus(tx));
        rx.recv_timeout(Duration::from_secs(5)).unwrap_or_default()
    }

    /// Decodes clips in the background so the first trigger is instant.
    pub fn preload(&self, items: Vec<(String, PathBuf)>) {
        for (id, path) in items {
            let _ = self.loader.send(LoadJob { id, path, then_play: None });
        }
    }

    pub fn insert_clip(&self, id: String, clip: Arc<Clip>) {
        self.cache.write().insert(id, clip);
    }

    pub fn evict(&self, id: &str) {
        self.cache.write().remove(id);
    }
}

struct LoadJob {
    id: String,
    path: PathBuf,
    then_play: Option<PlayRequest>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Role {
    Primary,
    Monitor,
}

struct Output {
    generation: u64,
    #[allow(dead_code)]
    stream: cpal::Stream,
    tx: Sender<MixCmd>,
    status: OutputStatus,
}

struct VoiceInfo {
    sound_id: String,
    started_at: u64,
    duration_ms: u64,
    looping: bool,
    preview: bool,
    /// Output generations still rendering this voice.
    outputs: Vec<u64>,
}

struct Engine {
    host: cpal::Host,
    primary: Option<Output>,
    monitor: Option<Output>,
    want_primary: Option<String>,
    want_primary_name: Option<String>,
    want_monitor: Option<String>,
    want_monitor_name: Option<String>,
    primary_error: Option<String>,
    monitor_error: Option<String>,
    master: f32,
    monitor_volume: f32,
    max_voices: usize,
    voices: HashMap<u64, VoiceInfo>,
    sound_keys: HashMap<String, u64>,
    next_key: u64,
    next_voice: u64,
    next_generation: u64,
    cache: ClipCache,
    loader: Sender<LoadJob>,
    mix_events: Sender<MixEvent>,
    emit: Box<dyn Fn(EngineEvent) + Send>,
    last_status: AudioStatus,
    needs_rebuild: Vec<Role>,
}

pub fn spawn(
    master: f32,
    monitor_volume: f32,
    max_voices: u32,
    emit: impl Fn(EngineEvent) + Send + 'static,
) -> AudioHandle {
    let (tx, rx) = crossbeam_channel::unbounded::<EngineCmd>();
    let (mix_tx, mix_rx) = crossbeam_channel::bounded::<MixEvent>(1024);
    let (load_tx, load_rx) = crossbeam_channel::unbounded::<LoadJob>();
    let cache: ClipCache = Arc::new(RwLock::new(HashMap::new()));

    spawn_loader(load_rx, cache.clone(), tx.clone());

    let handle = AudioHandle {
        tx,
        cache: cache.clone(),
        loader: load_tx.clone(),
    };
    let emit: Box<dyn Fn(EngineEvent) + Send> = Box::new(emit);

    thread::Builder::new()
        .name("cueboard-audio".into())
        .spawn(move || {
            let mut engine = Engine {
                host: cpal::default_host(),
                primary: None,
                monitor: None,
                want_primary: None,
                want_primary_name: None,
                want_monitor: None,
                want_monitor_name: None,
                primary_error: None,
                monitor_error: None,
                master,
                monitor_volume,
                max_voices: max_voices.max(1) as usize,
                voices: HashMap::new(),
                sound_keys: HashMap::new(),
                next_key: 1,
                next_voice: 1,
                next_generation: 1,
                cache,
                loader: load_tx,
                mix_events: mix_tx,
                emit,
                last_status: AudioStatus::default(),
                needs_rebuild: Vec::new(),
            };
            engine.run(rx, mix_rx);
        })
        .expect("failed to spawn audio thread");
    handle
}

fn spawn_loader(rx: Receiver<LoadJob>, cache: ClipCache, engine: Sender<EngineCmd>) {
    thread::Builder::new()
        .name("cueboard-decoder".into())
        .spawn(move || {
            for job in rx {
                if job.then_play.is_none() && cache.read().contains_key(&job.id) {
                    continue;
                }
                let loaded = if let Some(clip) = cache.read().get(&job.id).cloned() {
                    Ok(clip)
                } else {
                    decode_file(&job.path).map(Arc::new)
                };
                match loaded {
                    Ok(clip) => {
                        cache.write().insert(job.id.clone(), clip);
                        if let Some(req) = job.then_play {
                            let _ = engine.send(EngineCmd::Play(req));
                        }
                    }
                    Err(message) => {
                        log::warn!("could not load {}: {message}", job.path.display());
                        if job.then_play.is_some() {
                            let _ = engine.send(EngineCmd::LoadFailed {
                                sound_id: job.id,
                                message,
                            });
                        }
                    }
                }
            }
        })
        .expect("failed to spawn decoder thread");
}

fn device_name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "Unknown device".into())
}

impl Engine {
    fn run(&mut self, rx: Receiver<EngineCmd>, mix_rx: Receiver<MixEvent>) {
        self.rebuild(Role::Primary);
        self.publish_status();
        let mut last_poll = Instant::now();
        loop {
            let timeout = if self.wants_polling() {
                Duration::from_secs(3)
            } else {
                Duration::from_secs(60)
            };
            select! {
                recv(rx) -> cmd => match cmd {
                    Ok(EngineCmd::Shutdown) | Err(_) => break,
                    Ok(cmd) => self.handle(cmd),
                },
                recv(mix_rx) -> ev => if let Ok(ev) = ev { self.handle_mix(ev) },
                default(timeout) => {},
            }
            if !self.needs_rebuild.is_empty() {
                let roles: Vec<Role> = std::mem::take(&mut self.needs_rebuild);
                for role in roles {
                    self.rebuild(role);
                }
                self.publish_status();
                self.publish_playback();
            }
            if self.wants_polling() && last_poll.elapsed() >= Duration::from_secs(3) {
                last_poll = Instant::now();
                self.poll_reconnect();
            }
        }
        self.primary = None;
        self.monitor = None;
    }

    /// Polling only happens while a chosen device is missing, to reattach it when it returns.
    fn wants_polling(&self) -> bool {
        let p = self.primary.as_ref().map_or(true, |o| o.status.fallback);
        let m = self.want_monitor.is_some()
            && self.monitor.as_ref().map_or(true, |o| o.status.fallback);
        p || m
    }

    fn poll_reconnect(&mut self) {
        let primary_missing = self.primary.is_none()
            || self.primary.as_ref().is_some_and(|o| o.status.fallback);
        if primary_missing && self.find_device(self.want_primary.as_deref()).is_some() {
            self.rebuild(Role::Primary);
            self.publish_status();
        }
        if self.want_monitor.is_some()
            && self.monitor.is_none()
            && self.find_device(self.want_monitor.as_deref()).is_some()
        {
            self.rebuild(Role::Monitor);
            self.publish_status();
        }
    }

    fn find_device(&self, id: Option<&str>) -> Option<cpal::Device> {
        match id {
            None => self.host.default_output_device(),
            Some(id) => {
                let parsed: cpal::DeviceId = id.parse().ok()?;
                let dev = self.host.device_by_id(&parsed)?;
                dev.supports_output().then_some(dev)
            }
        }
    }

    fn rebuild(&mut self, role: Role) {
        // Dropping the old stream stops it; any voices it was rendering are released.
        let old = match role {
            Role::Primary => self.primary.take(),
            Role::Monitor => self.monitor.take(),
        };
        if let Some(old) = old {
            self.forget_output(old.generation);
            drop(old);
        }

        let (want, want_name) = match role {
            Role::Primary => (self.want_primary.clone(), self.want_primary_name.clone()),
            Role::Monitor => match &self.want_monitor {
                Some(id) => (Some(id.clone()), self.want_monitor_name.clone()),
                None => {
                    self.monitor_error = None;
                    return;
                }
            },
        };

        let mut fallback = false;
        let mut error: Option<String> = None;
        let device = match self.find_device(want.as_deref()) {
            Some(d) => Some(d),
            None if role == Role::Primary && want.is_some() => {
                fallback = true;
                error = Some(format!(
                    "{} is not connected. Playing through the Windows default output until it returns.",
                    want_name.clone().unwrap_or_else(|| "The selected device".into())
                ));
                self.host.default_output_device()
            }
            None => {
                error = Some(match role {
                    Role::Primary => "No audio output device is available.".into(),
                    Role::Monitor => format!(
                        "{} is not connected. Monitoring resumes when it returns.",
                        want_name.clone().unwrap_or_else(|| "The monitor device".into())
                    ),
                });
                None
            }
        };

        let Some(device) = device else {
            match role {
                Role::Primary => self.primary_error = error,
                Role::Monitor => self.monitor_error = error,
            }
            return;
        };

        let generation = self.next_generation;
        self.next_generation += 1;
        let gain = self.gain_for(role);
        let opened = open_stream(&device, generation, gain, self.mix_events.clone())
            .and_then(|o| {
                o.stream
                    .play()
                    .map_err(|e| format!("Could not start the audio device: {e}"))?;
                Ok(o)
            });
        match opened {
            Ok(o) => {
                let status = OutputStatus {
                    requested: want.clone(),
                    requested_name: want_name,
                    active_name: Some(device_name(&device)),
                    sample_rate: o.rate,
                    channels: o.channels,
                    fallback,
                    error: error.clone(),
                };
                if let Some(msg) = &error {
                    (self.emit)(EngineEvent::Notice(msg.clone()));
                }
                let out = Output {
                    generation,
                    stream: o.stream,
                    tx: o.tx,
                    status,
                };
                match role {
                    Role::Primary => {
                        self.primary_error = None;
                        self.primary = Some(out);
                    }
                    Role::Monitor => {
                        self.monitor_error = None;
                        self.monitor = Some(out);
                    }
                }
            }
            Err(e) => {
                log::error!("output open failed: {e}");
                match role {
                    Role::Primary => self.primary_error = Some(e),
                    Role::Monitor => self.monitor_error = Some(e),
                }
            }
        }
    }

    fn gain_for(&self, role: Role) -> f32 {
        match role {
            Role::Primary => self.master,
            Role::Monitor => self.master * self.monitor_volume,
        }
    }

    fn forget_output(&mut self, generation: u64) {
        for v in self.voices.values_mut() {
            v.outputs.retain(|g| *g != generation);
        }
        self.voices.retain(|_, v| !v.outputs.is_empty());
    }

    fn sound_key(&mut self, id: &str) -> u64 {
        if let Some(k) = self.sound_keys.get(id) {
            return *k;
        }
        let k = self.next_key;
        self.next_key += 1;
        self.sound_keys.insert(id.to_string(), k);
        k
    }

    fn outputs(&self) -> impl Iterator<Item = &Output> {
        self.primary.iter().chain(self.monitor.iter())
    }

    fn broadcast(&self, make: impl Fn() -> MixCmd) {
        for o in self.outputs() {
            let _ = o.tx.try_send(make());
        }
    }

    fn handle(&mut self, cmd: EngineCmd) {
        match cmd {
            EngineCmd::Play(req) => self.play(req),
            EngineCmd::LoadFailed { sound_id, message } => {
                (self.emit)(EngineEvent::LoadError { sound_id, message });
            }
            EngineCmd::Stop(id) => {
                if let Some(&key) = self.sound_keys.get(&id) {
                    self.broadcast(|| MixCmd::StopSound(key));
                }
            }
            EngineCmd::StopAll => self.broadcast(|| MixCmd::StopAll),
            EngineCmd::SetSoundVolume(id, vol) => {
                if let Some(&key) = self.sound_keys.get(&id) {
                    self.broadcast(|| MixCmd::SetSoundGain(key, vol));
                }
            }
            EngineCmd::SetSoundLoop(id, looping) => {
                if let Some(&key) = self.sound_keys.get(&id) {
                    self.broadcast(|| MixCmd::SetSoundLoop(key, looping));
                }
                for v in self.voices.values_mut().filter(|v| v.sound_id == id) {
                    v.looping = looping;
                }
                self.publish_playback();
            }
            EngineCmd::SetMaster(m) => {
                self.master = m.clamp(0.0, 1.0);
                self.push_gains();
            }
            EngineCmd::SetMonitorVolume(m) => {
                self.monitor_volume = m.clamp(0.0, 1.0);
                self.push_gains();
            }
            EngineCmd::SetOutputs {
                primary,
                primary_name,
                monitor,
                monitor_name,
            } => {
                let primary_changed = primary != self.want_primary || self.primary.is_none();
                let monitor_changed = monitor != self.want_monitor;
                self.want_primary = primary;
                self.want_primary_name = primary_name;
                self.want_monitor = monitor;
                self.want_monitor_name = monitor_name;
                if primary_changed {
                    self.rebuild(Role::Primary);
                }
                if monitor_changed {
                    self.rebuild(Role::Monitor);
                }
                self.publish_status();
                self.publish_playback();
            }
            EngineCmd::SetMaxVoices(n) => self.max_voices = n.max(1) as usize,
            EngineCmd::ListDevices(reply) => {
                let default_id = self
                    .host
                    .default_output_device()
                    .and_then(|d| d.id().ok())
                    .map(|id| id.to_string());
                let mut list = Vec::new();
                if let Ok(devices) = self.host.output_devices() {
                    for d in devices {
                        let Ok(id) = d.id() else { continue };
                        let id = id.to_string();
                        list.push(DeviceInfo {
                            is_default: Some(&id) == default_id.as_ref(),
                            name: device_name(&d),
                            id,
                        });
                    }
                }
                list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                let _ = reply.send(list);
            }
            EngineCmd::GetStatus(reply) => {
                let _ = reply.send(self.build_status());
            }
            EngineCmd::Shutdown => {}
        }
    }

    fn push_gains(&self) {
        if let Some(p) = &self.primary {
            let _ = p.tx.try_send(MixCmd::SetMaster(self.gain_for(Role::Primary)));
        }
        if let Some(m) = &self.monitor {
            let _ = m.tx.try_send(MixCmd::SetMaster(self.gain_for(Role::Monitor)));
        }
    }

    fn play(&mut self, req: PlayRequest) {
        let clip = self.cache.read().get(&req.sound_id).cloned();
        let Some(clip) = clip else {
            let _ = self.loader.send(LoadJob {
                id: req.sound_id.clone(),
                path: req.path.clone(),
                then_play: Some(req),
            });
            return;
        };
        let key = self.sound_key(&req.sound_id);

        let already: Vec<u64> = self
            .voices
            .iter()
            .filter(|(_, v)| v.sound_id == req.sound_id)
            .map(|(id, _)| *id)
            .collect();
        match req.mode {
            RetriggerMode::Toggle if !already.is_empty() => {
                self.broadcast(|| MixCmd::StopSound(key));
                return;
            }
            RetriggerMode::Restart if !already.is_empty() => {
                self.broadcast(|| MixCmd::StopSound(key));
            }
            _ => {}
        }

        // Voice limit: release the oldest voices first.
        while self.voices.len() >= self.max_voices {
            let Some((&oldest, _)) = self.voices.iter().min_by_key(|(_, v)| v.started_at) else {
                break;
            };
            self.broadcast(|| MixCmd::StopVoice(oldest));
            self.voices.remove(&oldest);
        }

        let voice = self.next_voice;
        self.next_voice += 1;
        let mut generations = Vec::new();
        let targets: Vec<&Output> = if req.preview {
            match (&self.monitor, &self.primary) {
                (Some(m), _) => vec![m],
                (None, Some(p)) => vec![p],
                _ => vec![],
            }
        } else {
            self.outputs().collect()
        };
        for out in targets {
            let ok = out
                .tx
                .try_send(MixCmd::Play {
                    voice,
                    sound: key,
                    clip: clip.clone(),
                    gain: req.volume,
                    looping: req.looping,
                })
                .is_ok();
            if ok {
                generations.push(out.generation);
            }
        }
        if generations.is_empty() {
            (self.emit)(EngineEvent::Notice(
                "No audio output is available. Check the output device in Settings.".into(),
            ));
            return;
        }
        self.voices.insert(
            voice,
            VoiceInfo {
                sound_id: req.sound_id,
                started_at: now_ms(),
                duration_ms: clip.duration_ms(),
                looping: req.looping,
                preview: req.preview,
                outputs: generations,
            },
        );
        self.publish_playback();
    }

    fn handle_mix(&mut self, ev: MixEvent) {
        match ev {
            MixEvent::Ended {
                output,
                voice,
                clip,
            } => {
                drop(clip);
                if let Some(v) = self.voices.get_mut(&voice) {
                    v.outputs.retain(|g| *g != output);
                    if v.outputs.is_empty() {
                        self.voices.remove(&voice);
                        self.publish_playback();
                    }
                }
            }
            MixEvent::Error {
                output,
                kind,
                message,
            } => {
                let role = if self.primary.as_ref().is_some_and(|o| o.generation == output) {
                    Role::Primary
                } else if self.monitor.as_ref().is_some_and(|o| o.generation == output) {
                    Role::Monitor
                } else {
                    return;
                };
                match kind {
                    cpal::ErrorKind::Xrun | cpal::ErrorKind::RealtimeDenied => {}
                    cpal::ErrorKind::DeviceChanged => {}
                    _ => {
                        log::warn!("output error ({kind:?}): {message}");
                        if !self.needs_rebuild.contains(&role) {
                            self.needs_rebuild.push(role);
                        }
                    }
                }
            }
        }
    }

    fn build_status(&self) -> AudioStatus {
        let primary = match &self.primary {
            Some(o) => o.status.clone(),
            None => OutputStatus {
                requested: self.want_primary.clone(),
                requested_name: self.want_primary_name.clone(),
                error: self.primary_error.clone(),
                ..Default::default()
            },
        };
        let monitor = if self.want_monitor.is_some() {
            Some(match &self.monitor {
                Some(o) => o.status.clone(),
                None => OutputStatus {
                    requested: self.want_monitor.clone(),
                    requested_name: self.want_monitor_name.clone(),
                    error: self.monitor_error.clone(),
                    ..Default::default()
                },
            })
        } else {
            None
        };
        AudioStatus { primary, monitor }
    }

    fn publish_status(&mut self) {
        let status = self.build_status();
        if status != self.last_status {
            self.last_status = status.clone();
            (self.emit)(EngineEvent::Status(status));
        }
    }

    fn publish_playback(&self) {
        let mut list: Vec<PlayingVoice> = self
            .voices
            .iter()
            .map(|(id, v)| PlayingVoice {
                voice_id: *id,
                sound_id: v.sound_id.clone(),
                started_at: v.started_at,
                duration_ms: v.duration_ms,
                looping: v.looping,
                preview: v.preview,
            })
            .collect();
        list.sort_by_key(|v| v.voice_id);
        (self.emit)(EngineEvent::Playback(list));
    }
}
