use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

use crate::models::{Folder, Sound};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackManifest {
    pub version: u32,
    pub sounds: Vec<Sound>,
    pub folders: Vec<Folder>,
}

pub fn write_pack(path: &Path, sounds_dir: &Path, sounds: &[Sound], folders: &[Folder]) -> Result<(), String> {
    let file = File::create(path).map_err(|e| format!("Could not create the pack file: {e}"))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let manifest = PackManifest {
        version: 1,
        sounds: sounds.to_vec(),
        folders: folders.to_vec(),
    };
    let json = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    zip.start_file("manifest.json", options)
        .map_err(|e| e.to_string())?;
    zip.write_all(&json).map_err(|e| e.to_string())?;

    for sound in sounds {
        let src = sounds_dir.join(&sound.file);
        if !src.exists() {
            continue;
        }
        let mut buf = Vec::new();
        File::open(&src)
            .and_then(|mut f| f.read_to_end(&mut buf))
            .map_err(|e| format!("Could not read {}: {e}", sound.name))?;
        let name = format!("files/{}", sound.file);
        zip.start_file(&name, options).map_err(|e| e.to_string())?;
        zip.write_all(&buf).map_err(|e| e.to_string())?;
    }

    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

pub struct ExtractedPack {
    pub manifest: Option<PackManifest>,
    pub loose_audio: Vec<PathBuf>,
    pub staging: PathBuf,
}

pub fn extract_pack(path: &Path, staging: PathBuf) -> Result<ExtractedPack, String> {
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let file = File::open(path).map_err(|e| format!("Could not open the pack: {e}"))?;
    let mut zip = ZipArchive::new(file).map_err(|e| format!("This is not a valid zip pack: {e}"))?;

    let mut manifest: Option<PackManifest> = None;
    let mut loose_audio = Vec::new();

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        if name.contains("..") {
            continue;
        }
        let dest = staging.join(&name);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = File::create(&dest).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        drop(out);

        if name == "manifest.json" || name.ends_with("/manifest.json") {
            let data = fs::read(&dest).map_err(|e| e.to_string())?;
            manifest = serde_json::from_slice(&data).ok();
        } else if crate::audio::decode::is_supported_path(&dest) {
            loose_audio.push(dest);
        }
    }

    Ok(ExtractedPack {
        manifest,
        loose_audio,
        staging,
    })
}

pub fn is_zip(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("zip") || e.eq_ignore_ascii_case("cueboard"))
}
