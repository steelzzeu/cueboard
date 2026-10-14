use std::{fs::File, path::Path};

use symphonia::core::{
    audio::SampleBuffer,
    codecs::{DecoderOptions, CODEC_TYPE_NULL},
    errors::Error as SymError,
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
};

/// Longest clip accepted into the library. Clips are held in memory for instant triggering,
/// so this keeps RAM use predictable.
pub const MAX_CLIP_SECONDS: u64 = 10 * 60;

pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "mp3", "wav", "ogg", "oga", "flac", "aac", "m4a", "mp4", "wma", "opus", "aif", "aiff", "caf",
    "mka", "webm",
];

pub fn is_supported_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| {
            SUPPORTED_EXTENSIONS
                .iter()
                .any(|ok| ext.eq_ignore_ascii_case(ok))
        })
}

/// Decoded PCM held as interleaved 16-bit samples at the file's native rate. Resampling and
/// channel mapping happen on the fly in the mixer so one copy serves every output device.
pub struct Clip {
    pub data: Box<[i16]>,
    pub channels: u16,
    pub rate: u32,
    pub frames: usize,
}

impl Clip {
    pub fn duration_ms(&self) -> u64 {
        if self.rate == 0 {
            0
        } else {
            (self.frames as u64 * 1000) / self.rate as u64
        }
    }
}

struct Pcm {
    samples: Vec<i16>,
    channels: u16,
    rate: u32,
}

#[inline]
fn to_i16(s: f32) -> i16 {
    (s.clamp(-1.0, 1.0) * 32767.0) as i16
}

pub fn decode_file(path: &Path) -> Result<Clip, String> {
    if !path.exists() {
        return Err("The audio file no longer exists on disk.".into());
    }
    let pcm = match decode_symphonia(path) {
        Ok(pcm) => pcm,
        Err(sym_err) => {
            #[cfg(windows)]
            {
                match super::mf::decode_media_foundation(path) {
                    Ok((samples, channels, rate)) => Pcm {
                        samples,
                        channels,
                        rate,
                    },
                    Err(mf_err) => {
                        log::warn!(
                            "decode failed for {}: symphonia: {sym_err}; media foundation: {mf_err}",
                            path.display()
                        );
                        return Err(sym_err);
                    }
                }
            }
            #[cfg(not(windows))]
            {
                return Err(sym_err);
            }
        }
    };

    if pcm.channels == 0 || pcm.rate == 0 {
        return Err("The file does not describe a valid audio stream.".into());
    }
    let frames = pcm.samples.len() / pcm.channels as usize;
    if frames == 0 {
        return Err("The file contains no audio.".into());
    }
    let mut data = pcm.samples;
    data.truncate(frames * pcm.channels as usize);
    Ok(Clip {
        data: data.into_boxed_slice(),
        channels: pcm.channels,
        rate: pcm.rate,
        frames,
    })
}

fn too_long() -> String {
    format!(
        "The file is longer than {} minutes. Trim it before importing.",
        MAX_CLIP_SECONDS / 60
    )
}

fn decode_symphonia(path: &Path) -> Result<Pcm, String> {
    let file = File::open(path).map_err(|e| format!("Could not open the file: {e}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions {
                enable_gapless: true,
                ..Default::default()
            },
            &MetadataOptions::default(),
        )
        .map_err(|e| match e {
            SymError::Unsupported(_) => "This format is not supported.".to_string(),
            _ => "The file could not be read. It may be corrupted or not an audio file.".to_string(),
        })?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| "The file contains no audio track.".to_string())?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|_| "The audio codec in this file is not supported.".to_string())?;

    let mut channels = track
        .codec_params
        .channels
        .map(|c| c.count() as u16)
        .unwrap_or(0);
    let mut rate = track.codec_params.sample_rate.unwrap_or(0);
    let mut samples: Vec<i16> = Vec::new();
    if let (Some(n), true) = (track.codec_params.n_frames, channels > 0) {
        samples.reserve((n as usize).saturating_mul(channels as usize).min(64 << 20));
    }
    let mut sbuf: Option<SampleBuffer<f32>> = None;
    let mut decode_errors = 0usize;
    let mut decoded_any = false;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(SymError::IoError(_)) if decoded_any => break,
            Err(_) => {
                return Err(
                    "The file could not be read. It may be corrupted or truncated.".into(),
                )
            }
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buf) => {
                let spec = *buf.spec();
                let ch = spec.channels.count();
                if channels != 0 && channels as usize != ch && decoded_any {
                    // Channel layout changed mid-stream; keep the first layout.
                    continue;
                }
                channels = ch as u16;
                rate = spec.rate;
                let needed = buf.capacity() * ch;
                if sbuf.as_ref().map_or(true, |b| b.capacity() < needed) {
                    sbuf = Some(SampleBuffer::new(buf.capacity() as u64, spec));
                }
                let b = sbuf.as_mut().expect("sample buffer");
                b.copy_interleaved_ref(buf);
                samples.extend(b.samples().iter().map(|&s| to_i16(s)));
                decoded_any = true;
                if rate > 0
                    && (samples.len() / ch.max(1)) as u64 > MAX_CLIP_SECONDS * rate as u64
                {
                    return Err(too_long());
                }
            }
            Err(SymError::DecodeError(_)) => {
                decode_errors += 1;
                if decode_errors > 64 && !decoded_any {
                    return Err("The audio data is corrupted.".into());
                }
            }
            Err(SymError::IoError(_)) => break,
            Err(_) => {
                if decoded_any {
                    break;
                }
                return Err("The audio data could not be decoded.".into());
            }
        }
    }
    if !decoded_any {
        return Err("The file contains no decodable audio.".into());
    }
    Ok(Pcm {
        samples,
        channels,
        rate,
    })
}

pub fn max_frames_for_rate(rate: u32) -> u64 {
    MAX_CLIP_SECONDS * rate as u64
}

pub fn too_long_message() -> String {
    too_long()
}
