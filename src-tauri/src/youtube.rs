use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use flate2::read::GzDecoder;
use parking_lot::Mutex;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use crate::{
    audio::decode::MAX_CLIP_SECONDS,
    models::Sound,
    state::AppState,
};

const YTDLP_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
const FFMPEG_GZ_URL: &str =
    "https://github.com/eugeneware/ffmpeg-static/releases/download/b6.1.1/ffmpeg-win32-x64.gz";
const USER_AGENT: &str = "Cueboard/1.0 (Windows; +https://cueboard.local)";

pub struct YoutubeCtl {
    cancel: AtomicBool,
    child: Mutex<Option<u32>>,
    busy: AtomicBool,
}

impl YoutubeCtl {
    pub fn new() -> Self {
        Self {
            cancel: AtomicBool::new(false),
            child: Mutex::new(None),
            busy: AtomicBool::new(false),
        }
    }

    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Some(pid) = *self.child.lock() {
            kill_pid(pid);
        }
    }

    fn begin(&self) {
        self.cancel.store(false, Ordering::SeqCst);
        *self.child.lock() = None;
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YoutubePreview {
    pub url: String,
    pub video_id: String,
    pub title: String,
    pub author: String,
    pub thumbnail: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YoutubeProgress {
    pub stage: String,
    pub percent: f32,
    pub message: String,
}

#[derive(Deserialize)]
struct OEmbed {
    title: Option<String>,
    author_name: Option<String>,
    thumbnail_url: Option<String>,
}

pub fn extract_video_id(input: &str) -> Option<String> {
    let text = input.trim();
    if text.is_empty() {
        return None;
    }
    let id_re = Regex::new(r"^[A-Za-z0-9_-]{11}$").ok()?;
    if id_re.is_match(text) {
        return Some(text.to_string());
    }
    let patterns = [
        r"(?:youtube\.com/watch\?(?:[^#]*&)?v=|youtu\.be/|youtube\.com/shorts/|youtube\.com/embed/|youtube\.com/live/)([A-Za-z0-9_-]{11})",
        r"youtube\.com/watch\?.*\bv=([A-Za-z0-9_-]{11})",
    ];
    for pat in patterns {
        if let Ok(re) = Regex::new(pat) {
            if let Some(cap) = re.captures(text) {
                return cap.get(1).map(|m| m.as_str().to_string());
            }
        }
    }
    None
}

pub fn canonical_url(input: &str) -> Result<String, String> {
    let id = extract_video_id(input).ok_or_else(|| {
        "Paste a YouTube link. Music, Shorts and youtu.be links all work.".to_string()
    })?;
    Ok(format!("https://www.youtube.com/watch?v={id}"))
}

fn http() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("Could not start a download: {e}"))
}

fn emit_progress(app: &AppHandle, stage: &str, percent: f32, message: &str) {
    let _ = app.emit(
        "youtube-progress",
        YoutubeProgress {
            stage: stage.into(),
            percent: percent.clamp(0.0, 100.0),
            message: message.into(),
        },
    );
}

fn check_cancel(app: &AppHandle) -> Result<(), String> {
    if app
        .try_state::<YoutubeCtl>()
        .is_some_and(|ctl| ctl.cancelled())
    {
        Err("Cancelled.".into())
    } else {
        Ok(())
    }
}

pub fn preview(input: &str) -> Result<YoutubePreview, String> {
    let url = canonical_url(input)?;
    let id = extract_video_id(&url).unwrap_or_default();
    let endpoint = format!(
        "https://www.youtube.com/oembed?url={}&format=json",
        urlencoding::encode(&url)
    );
    let client = http()?;
    let response = client
        .get(endpoint)
        .send()
        .map_err(|e| format!("Could not look up that video: {e}"))?;
    if !response.status().is_success() {
        return Err("YouTube did not recognise that link. Check it and try again.".into());
    }
    let body: OEmbed = response
        .json()
        .map_err(|_| "YouTube returned an unexpected preview.".to_string())?;
    Ok(YoutubePreview {
        url,
        video_id: id,
        title: body
            .title
            .unwrap_or_else(|| "YouTube audio".into())
            .chars()
            .take(80)
            .collect(),
        author: body.author_name.unwrap_or_default(),
        thumbnail: body.thumbnail_url,
    })
}

fn tools_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let state = app.try_state::<AppState>().ok_or("Cueboard is still starting.")?;
    let dir = state.paths.root.join("tools");
    fs::create_dir_all(&dir).map_err(|e| format!("Could not create the tools folder: {e}"))?;
    Ok(dir)
}

fn download_file(url: &str, dest: &Path, app: &AppHandle, label: &str) -> Result<(), String> {
    let client = http()?;
    let mut response = client
        .get(url)
        .send()
        .map_err(|e| format!("Could not download {label}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Could not download {label} (HTTP {}).",
            response.status()
        ));
    }
    let total = response.content_length().unwrap_or(0);
    let tmp = dest.with_extension("tmp");
    let mut file = fs::File::create(&tmp).map_err(|e| format!("Could not write {label}: {e}"))?;
    let mut copied: u64 = 0;
    let mut buf = [0u8; 64 * 1024];
    loop {
        check_cancel(app)?;
        let n = response
            .read(&mut buf)
            .map_err(|e| format!("Download stalled while getting {label}: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("Could not write {label}: {e}"))?;
        copied += n as u64;
        if total > 0 {
            let pct = 8.0 + (copied as f32 / total as f32) * 18.0;
            emit_progress(
                app,
                "preparing",
                pct,
                &format!("Getting {label}…"),
            );
        }
    }
    drop(file);
    fs::rename(&tmp, dest).map_err(|e| format!("Could not save {label}: {e}"))?;
    Ok(())
}

fn ensure_ytdlp(app: &AppHandle) -> Result<PathBuf, String> {
    let dest = tools_dir(app)?.join("yt-dlp.exe");
    if dest.exists() && dest.metadata().map(|m| m.len() > 1_000_000).unwrap_or(false) {
        return Ok(dest);
    }
    emit_progress(app, "preparing", 6.0, "Getting the YouTube engine…");
    download_file(YTDLP_URL, &dest, app, "yt-dlp")?;
    Ok(dest)
}

fn ensure_ffmpeg(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(output) = hidden_command("ffmpeg").arg("-version").output() {
        if output.status.success() {
            return Some(PathBuf::from("ffmpeg"));
        }
    }
    let dest = tools_dir(app).ok()?.join("ffmpeg.exe");
    if dest.exists() && dest.metadata().map(|m| m.len() > 1_000_000).unwrap_or(false) {
        return Some(dest);
    }
    emit_progress(app, "preparing", 22.0, "Getting the MP3 encoder…");
    let gz_path = dest.with_extension("gz");
    if download_file(FFMPEG_GZ_URL, &gz_path, app, "ffmpeg").is_err() {
        let _ = fs::remove_file(&gz_path);
        return None;
    }
    let gz = fs::File::open(&gz_path).ok()?;
    let mut decoder = GzDecoder::new(gz);
    let mut out = fs::File::create(&dest).ok()?;
    if std::io::copy(&mut decoder, &mut out).is_err() {
        let _ = fs::remove_file(&dest);
        let _ = fs::remove_file(&gz_path);
        return None;
    }
    let _ = fs::remove_file(&gz_path);
    Some(dest)
}

pub fn prepare(app: &AppHandle) -> Result<(), String> {
    if let Some(ctl) = app.try_state::<YoutubeCtl>() {
        ctl.begin();
    }
    emit_progress(app, "preparing", 2.0, "Getting ready…");
    ensure_ytdlp(app)?;
    let _ = ensure_ffmpeg(app);
    emit_progress(app, "preparing", 100.0, "Ready.");
    Ok(())
}

fn hidden_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new(program);
    cmd.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

fn kill_pid(pid: u32) {
    let _ = hidden_command("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .status();
}

fn track_child(app: &AppHandle, pid: u32) {
    if let Some(ctl) = app.try_state::<YoutubeCtl>() {
        *ctl.child.lock() = Some(pid);
    }
}

fn clear_child(app: &AppHandle) {
    if let Some(ctl) = app.try_state::<YoutubeCtl>() {
        *ctl.child.lock() = None;
    }
}

fn parse_percent(line: &str) -> Option<f32> {
    let re = Regex::new(r"(\d{1,3}(?:\.\d+)?)%").ok()?;
    re.captures(line)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<f32>().ok())
        .map(|v| v.clamp(0.0, 100.0))
}

fn run_ytdlp(
    app: &AppHandle,
    ytdlp: &Path,
    args: &[String],
    stage: &str,
    base: f32,
    span: f32,
) -> Result<String, String> {
    check_cancel(app)?;
    let mut child = hidden_command(ytdlp)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not start the YouTube engine: {e}"))?;
    track_child(app, child.id());
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let mut combined = String::new();
    if let Some(out) = stdout {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if check_cancel(app).is_err() {
                let _ = child.kill();
                clear_child(app);
                return Err("Cancelled.".into());
            }
            combined.push_str(&line);
            combined.push('\n');
            if let Some(pct) = parse_percent(&line) {
                emit_progress(
                    app,
                    stage,
                    base + (pct / 100.0) * span,
                    if stage == "downloading" {
                        "Downloading audio…"
                    } else {
                        "Converting to MP3…"
                    },
                );
            }
        }
    }
    if let Some(err) = stderr {
        for line in BufReader::new(err).lines().map_while(Result::ok) {
            combined.push_str(&line);
            combined.push('\n');
        }
    }
    let status = child.wait().map_err(|e| format!("YouTube engine stopped: {e}"))?;
    clear_child(app);
    check_cancel(app)?;
    if !status.success() {
        if combined.contains("Duration") && combined.to_lowercase().contains("filter") {
            return Err(format!(
                "That video is longer than {} minutes. Trim it, or pick a shorter clip.",
                MAX_CLIP_SECONDS / 60
            ));
        }
        if combined.to_lowercase().contains("private") {
            return Err("That video is private.".into());
        }
        if combined.to_lowercase().contains("unavailable") {
            return Err("That video is unavailable.".into());
        }
        return Err("Could not fetch audio from that link.".into());
    }
    Ok(combined)
}

fn newest_audio(dir: &Path) -> Option<PathBuf> {
    let mut files: Vec<_> = fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| {
                    matches!(
                        ext.to_ascii_lowercase().as_str(),
                        "mp3" | "m4a" | "webm" | "opus" | "ogg" | "wav" | "aac" | "flac"
                    )
                })
        })
        .collect();
    files.sort_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok());
    files.pop()
}

pub fn import_url(
    app: &AppHandle,
    input: &str,
    folder_id: Option<String>,
) -> Result<Sound, String> {
    if let Some(ctl) = app.try_state::<YoutubeCtl>() {
        if ctl
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("Already converting a video.".into());
        }
        ctl.begin();
    }
    let result = import_url_inner(app, input, folder_id);
    if let Some(ctl) = app.try_state::<YoutubeCtl>() {
        ctl.busy.store(false, Ordering::SeqCst);
    }
    result
}

fn import_url_inner(
    app: &AppHandle,
    input: &str,
    folder_id: Option<String>,
) -> Result<Sound, String> {
    let preview = preview(input)?;
    emit_progress(app, "fetching", 4.0, "Looking up the video…");
    let ytdlp = ensure_ytdlp(app)?;
    let ffmpeg = ensure_ffmpeg(app);
    check_cancel(app)?;

    let state = app
        .try_state::<AppState>()
        .ok_or("Cueboard is still starting.")?;
    let work = state
        .paths
        .root
        .join("tmp")
        .join(format!("yt-{}", Uuid::new_v4()));
    fs::create_dir_all(&work).map_err(|e| format!("Could not create a temp folder: {e}"))?;
    let template = work.join("%(id)s.%(ext)s");

    let args = vec![
        "--no-playlist".into(),
        "--no-warnings".into(),
        "--newline".into(),
        "--no-mtime".into(),
        "--restrict-filenames".into(),
        "-f".into(),
        "bestaudio[ext=m4a]/bestaudio/best".into(),
        "--match-filter".into(),
        format!("!is_live & duration <= {}", MAX_CLIP_SECONDS),
        "-o".into(),
        template.to_string_lossy().into_owned(),
        preview.url.clone(),
    ];

    emit_progress(app, "downloading", 30.0, "Downloading audio…");
    if let Err(err) = run_ytdlp(app, &ytdlp, &args, "downloading", 30.0, 40.0) {
        let _ = fs::remove_dir_all(&work);
        return Err(err);
    }

    let mut audio = newest_audio(&work).ok_or_else(|| {
        let _ = fs::remove_dir_all(&work);
        "The download finished without an audio file.".to_string()
    })?;

    if let Some(ffmpeg) = ffmpeg {
        if audio
            .extension()
            .and_then(|e| e.to_str())
            .is_none_or(|ext| !ext.eq_ignore_ascii_case("mp3"))
        {
            emit_progress(app, "converting", 74.0, "Converting to MP3…");
            let mp3 = work.join(format!("{}.mp3", preview.video_id));
            let conv = hidden_command(&ffmpeg)
                .args([
                    "-y",
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-i",
                    &audio.to_string_lossy(),
                    "-vn",
                    "-codec:a",
                    "libmp3lame",
                    "-qscale:a",
                    "5",
                    mp3.to_str().unwrap_or("out.mp3"),
                ])
                .output();
            if conv.map(|out| out.status.success()).unwrap_or(false) && mp3.exists() {
                audio = mp3;
            }
        }
    }

    emit_progress(app, "importing", 92.0, "Adding to your board…");
    check_cancel(app)?;
    let sound = match state.import_one(&audio, Some(preview.title.clone()), folder_id) {
        Ok(sound) => sound,
        Err(err) => {
            let _ = fs::remove_dir_all(&work);
            return Err(err);
        }
    };
    let _ = fs::remove_dir_all(&work);
    emit_progress(app, "done", 100.0, "Added to your board.");
    Ok(sound)
}

pub fn cancel(app: &AppHandle) {
    if let Some(ctl) = app.try_state::<YoutubeCtl>() {
        ctl.request_cancel();
    }
}
