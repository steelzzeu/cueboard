//! Windows Media Foundation decoder, used for formats Symphonia cannot read (WMA in
//! particular) and as a second opinion for files Symphonia rejects.

use std::path::Path;

use windows::{
    core::HSTRING,
    Win32::{
        Media::MediaFoundation::*,
        System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
    },
};

use super::decode::{max_frames_for_rate, too_long_message};

struct ComGuard(bool);
impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe {
            if self.0 {
                CoUninitialize();
            }
        }
    }
}

struct MfGuard;
impl Drop for MfGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
        }
    }
}

pub fn decode_media_foundation(path: &Path) -> Result<(Vec<i16>, u16, u32), String> {
    unsafe {
        let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
        let _com = ComGuard(hr.is_ok());
        MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET).map_err(|e| e.message())?;
        let _mf = MfGuard;

        let url = HSTRING::from(path.as_os_str());
        let reader: IMFSourceReader =
            MFCreateSourceReaderFromURL(&url, None).map_err(|e| e.message())?;
        let audio = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
        reader
            .SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)
            .map_err(|e| e.message())?;
        reader
            .SetStreamSelection(audio, true)
            .map_err(|_| "No audio stream found.".to_string())?;

        let wanted: IMFMediaType = MFCreateMediaType().map_err(|e| e.message())?;
        wanted
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)
            .map_err(|e| e.message())?;
        wanted
            .SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_Float)
            .map_err(|e| e.message())?;
        reader
            .SetCurrentMediaType(audio, None, &wanted)
            .map_err(|e| e.message())?;
        let actual = reader.GetCurrentMediaType(audio).map_err(|e| e.message())?;
        let channels = actual
            .GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)
            .map_err(|e| e.message())? as u16;
        let rate = actual
            .GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)
            .map_err(|e| e.message())?;
        if channels == 0 || rate == 0 {
            return Err("Invalid audio format.".into());
        }
        let max_samples = max_frames_for_rate(rate) as usize * channels as usize;

        let mut out: Vec<i16> = Vec::new();
        loop {
            let mut flags = 0u32;
            let mut sample: Option<IMFSample> = None;
            reader
                .ReadSample(
                    audio,
                    0,
                    None,
                    Some(&mut flags),
                    None,
                    Some(&mut sample),
                )
                .map_err(|e| e.message())?;
            if let Some(sample) = sample {
                let buffer = sample
                    .ConvertToContiguousBuffer()
                    .map_err(|e| e.message())?;
                let mut ptr: *mut u8 = std::ptr::null_mut();
                let mut len = 0u32;
                buffer
                    .Lock(&mut ptr, None, Some(&mut len))
                    .map_err(|e| e.message())?;
                if !ptr.is_null() && len >= 4 {
                    let floats = std::slice::from_raw_parts(ptr as *const f32, len as usize / 4);
                    out.extend(
                        floats
                            .iter()
                            .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16),
                    );
                }
                let _ = buffer.Unlock();
                if out.len() > max_samples {
                    return Err(too_long_message());
                }
            }
            if flags & (MF_SOURCE_READERF_ENDOFSTREAM.0 as u32) != 0 {
                break;
            }
            if flags & (MF_SOURCE_READERF_ERROR.0 as u32) != 0 {
                return Err("Media Foundation reported a stream error.".into());
            }
        }
        if out.is_empty() {
            return Err("No audio decoded.".into());
        }
        Ok((out, channels, rate))
    }
}
