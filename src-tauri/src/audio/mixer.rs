//! Real-time mixer that runs inside the cpal output callback. It never blocks or allocates
//! in the steady state: commands arrive over a bounded channel and finished clips are sent
//! back to the engine thread so their memory is released outside the audio callback.

use std::sync::Arc;

use cpal::{
    traits::DeviceTrait, BufferSize, Device, FromSample, SampleFormat, SizedSample, Stream,
    StreamConfig,
};
use crossbeam_channel::{Receiver, Sender};

use super::decode::Clip;

pub enum MixCmd {
    Play {
        voice: u64,
        sound: u64,
        clip: Arc<Clip>,
        gain: f32,
        looping: bool,
    },
    StopSound(u64),
    StopVoice(u64),
    StopAll,
    SetSoundGain(u64, f32),
    SetSoundLoop(u64, bool),
    SetMaster(f32),
}

pub enum MixEvent {
    Ended {
        output: u64,
        voice: u64,
        clip: Arc<Clip>,
    },
    Error {
        output: u64,
        kind: cpal::ErrorKind,
        message: String,
    },
}

struct Voice {
    id: u64,
    sound: u64,
    clip: Arc<Clip>,
    pos: f64,
    step: f64,
    gain: f32,
    cur_gain: f32,
    looping: bool,
    fade: Option<u32>,
}

struct MixerState {
    output: u64,
    rx: Receiver<MixCmd>,
    events: Sender<MixEvent>,
    voices: Vec<Voice>,
    mix: Vec<f32>,
    channels: usize,
    rate: u32,
    master: f32,
    cur_master: f32,
    smooth: f32,
    fade_frames: u32,
}

#[inline]
fn hermite(xm1: f32, x0: f32, x1: f32, x2: f32, t: f32) -> f32 {
    let c1 = 0.5 * (x1 - xm1);
    let c2 = xm1 - 2.5 * x0 + 2.0 * x1 - 0.5 * x2;
    let c3 = 0.5 * (x2 - xm1) + 1.5 * (x0 - x1);
    ((c3 * t + c2) * t + c1) * t + x0
}

#[inline]
fn soft_clip(x: f32) -> f32 {
    let a = x.abs();
    if a <= 0.9 {
        x
    } else {
        x.signum() * (0.9 + 0.1 * ((a - 0.9) / 0.1).tanh())
    }
}

impl MixerState {
    fn apply(&mut self, cmd: MixCmd) {
        match cmd {
            MixCmd::Play {
                voice,
                sound,
                clip,
                gain,
                looping,
            } => {
                let step = clip.rate as f64 / self.rate as f64;
                self.voices.push(Voice {
                    id: voice,
                    sound,
                    clip,
                    pos: 0.0,
                    step,
                    gain,
                    cur_gain: gain,
                    looping,
                    fade: None,
                });
            }
            MixCmd::StopSound(sound) => {
                for v in self.voices.iter_mut().filter(|v| v.sound == sound) {
                    v.fade.get_or_insert(self.fade_frames);
                }
            }
            MixCmd::StopVoice(id) => {
                for v in self.voices.iter_mut().filter(|v| v.id == id) {
                    v.fade.get_or_insert(self.fade_frames);
                }
            }
            MixCmd::StopAll => {
                for v in self.voices.iter_mut() {
                    v.fade.get_or_insert(self.fade_frames);
                }
            }
            MixCmd::SetSoundGain(sound, gain) => {
                for v in self.voices.iter_mut().filter(|v| v.sound == sound) {
                    v.gain = gain;
                }
            }
            MixCmd::SetSoundLoop(sound, looping) => {
                for v in self.voices.iter_mut().filter(|v| v.sound == sound) {
                    v.looping = looping;
                }
            }
            MixCmd::SetMaster(m) => self.master = m,
        }
    }

    fn render(&mut self, frames: usize) {
        let ch = self.channels;
        let needed = frames * ch;
        if self.mix.len() < needed {
            self.mix.resize(needed, 0.0);
        }
        let mix = &mut self.mix[..needed];
        mix.fill(0.0);

        let smooth = self.smooth;
        let fade_total = self.fade_frames.max(1) as f32;
        for v in self.voices.iter_mut() {
            let clip = &*v.clip;
            let cc = clip.channels as usize;
            let total = clip.frames;
            let data = &clip.data;
            let exact = (v.step - 1.0).abs() < 1e-9;
            let sample = |frame: isize, c: usize, looping: bool| -> f32 {
                let idx = if frame < 0 {
                    if looping {
                        (frame + total as isize) as usize
                    } else {
                        return 0.0;
                    }
                } else if frame as usize >= total {
                    if looping {
                        frame as usize % total
                    } else {
                        return 0.0;
                    }
                } else {
                    frame as usize
                };
                data[idx * cc + c] as f32 * (1.0 / 32768.0)
            };

            for f in 0..frames {
                if v.pos >= total as f64 {
                    if v.looping {
                        v.pos %= total as f64;
                    } else {
                        v.fade = Some(0);
                        break;
                    }
                }
                let mut env = 1.0f32;
                if let Some(remaining) = v.fade.as_mut() {
                    if *remaining == 0 {
                        break;
                    }
                    env = *remaining as f32 / fade_total;
                    *remaining -= 1;
                }
                v.cur_gain += (v.gain - v.cur_gain) * smooth;
                let g = v.cur_gain * env;

                let i0 = v.pos.floor() as isize;
                let t = (v.pos - i0 as f64) as f32;
                let read = |c: usize| -> f32 {
                    if exact {
                        sample(i0, c, v.looping)
                    } else {
                        hermite(
                            sample(i0 - 1, c, v.looping),
                            sample(i0, c, v.looping),
                            sample(i0 + 1, c, v.looping),
                            sample(i0 + 2, c, v.looping),
                            t,
                        )
                    }
                };

                let out = &mut mix[f * ch..f * ch + ch];
                if ch == 1 {
                    let mut acc = 0.0;
                    for c in 0..cc {
                        acc += read(c);
                    }
                    out[0] += acc / cc as f32 * g;
                } else if cc == 1 {
                    let s = read(0) * g;
                    out[0] += s;
                    out[1] += s;
                } else {
                    let n = cc.min(ch);
                    for (c, o) in out.iter_mut().enumerate().take(n) {
                        *o += read(c) * g;
                    }
                }
                v.pos += v.step;
            }
        }

        // Master gain with smoothing to avoid zipper noise on slider moves.
        for frame in mix.chunks_mut(ch) {
            self.cur_master += (self.master - self.cur_master) * smooth;
            let m = self.cur_master;
            for s in frame.iter_mut() {
                *s = soft_clip(*s * m);
            }
        }

        // Retire finished voices, handing their clips back to the engine thread.
        let mut i = 0;
        while i < self.voices.len() {
            if self.voices[i].fade == Some(0) {
                let v = self.voices.swap_remove(i);
                let _ = self.events.try_send(MixEvent::Ended {
                    output: self.output,
                    voice: v.id,
                    clip: v.clip,
                });
            } else {
                i += 1;
            }
        }
    }

    fn process<T: SizedSample + FromSample<f32>>(&mut self, data: &mut [T]) {
        while let Ok(cmd) = self.rx.try_recv() {
            self.apply(cmd);
        }
        let frames = data.len() / self.channels;
        if self.voices.is_empty() {
            for s in data.iter_mut() {
                *s = T::from_sample(0.0f32);
            }
            self.cur_master = self.master;
            return;
        }
        self.render(frames);
        for (d, s) in data.iter_mut().zip(self.mix.iter()) {
            *d = T::from_sample(*s);
        }
    }
}

pub struct OpenedStream {
    pub stream: Stream,
    pub tx: Sender<MixCmd>,
    pub rate: u32,
    pub channels: u16,
}

pub fn open_stream(
    device: &Device,
    output: u64,
    master: f32,
    events: Sender<MixEvent>,
) -> Result<OpenedStream, String> {
    let supported = device
        .default_output_config()
        .map_err(|e| format!("The device did not report an output format: {e}"))?;
    let config = StreamConfig {
        channels: supported.channels(),
        sample_rate: supported.sample_rate(),
        buffer_size: BufferSize::Default,
    };
    let (tx, rx) = crossbeam_channel::bounded::<MixCmd>(512);
    let rate = config.sample_rate;
    let channels = config.channels;
    let state = MixerState {
        output,
        rx,
        events: events.clone(),
        voices: Vec::with_capacity(64),
        mix: vec![0.0; 8192 * channels as usize],
        channels: channels as usize,
        rate,
        master,
        cur_master: master,
        smooth: 1.0 - (-1.0f32 / (0.008 * rate as f32)).exp(),
        fade_frames: (rate as f32 * 0.008) as u32,
    };

    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(device, config, state, events, output),
        SampleFormat::I16 => build::<i16>(device, config, state, events, output),
        SampleFormat::I32 => build::<i32>(device, config, state, events, output),
        SampleFormat::U16 => build::<u16>(device, config, state, events, output),
        SampleFormat::F64 => build::<f64>(device, config, state, events, output),
        other => {
            return Err(format!(
                "The device uses an unsupported sample format ({other})."
            ))
        }
    }?;
    Ok(OpenedStream {
        stream,
        tx,
        rate,
        channels,
    })
}

fn build<T>(
    device: &Device,
    config: StreamConfig,
    mut state: MixerState,
    events: Sender<MixEvent>,
    output: u64,
) -> Result<Stream, String>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    device
        .build_output_stream::<T, _, _>(
            config,
            move |data: &mut [T], _| state.process(data),
            move |err: cpal::Error| {
                let _ = events.try_send(MixEvent::Error {
                    output,
                    kind: err.kind(),
                    message: err.to_string(),
                });
            },
            None,
        )
        .map_err(|e| format!("Could not open the audio device: {e}"))
}
