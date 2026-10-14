use std::{fs, path::Path, time::Duration};

use serde::{Deserialize, Serialize};

const OPENVERSE: &str = "https://api.openverse.org/v1/audio/";
const USER_AGENT: &str = "Cueboard/1.0 (Windows desktop soundboard)";
const PAGE_SIZE: u32 = 20;
const MAX_DURATION_MS: u64 = 30_000;
const MIN_DURATION_MS: u64 = 80;
const MAX_DOWNLOAD_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogItem {
    pub id: String,
    pub name: String,
    pub url: String,
    pub creator: String,
    pub license: String,
    pub license_url: String,
    pub attribution: String,
    pub duration_ms: u64,
    pub source_url: String,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogPage {
    pub query: String,
    pub page: u32,
    pub page_count: u32,
    pub result_count: u64,
    pub results: Vec<CatalogItem>,
}

#[derive(Deserialize)]
struct OvPage {
    result_count: u64,
    page_count: u32,
    page: u32,
    results: Vec<OvHit>,
}

#[derive(Deserialize)]
struct OvHit {
    id: String,
    title: Option<String>,
    url: Option<String>,
    creator: Option<String>,
    license: Option<String>,
    license_url: Option<String>,
    attribution: Option<String>,
    duration: Option<u64>,
    foreign_landing_url: Option<String>,
    mature: Option<bool>,
    tags: Option<Vec<OvTag>>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OvTag {
    Name(String),
    Obj { name: String },
}

impl OvTag {
    fn name(self) -> String {
        match self {
            OvTag::Name(name) => name,
            OvTag::Obj { name } => name,
        }
    }
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::limited(4))
        .build()
        .map_err(|e| format!("Could not start the catalog client: {e}"))
}

pub fn play_id(openverse_id: &str) -> String {
    format!("cat:{openverse_id}")
}

pub fn original_name(openverse_id: &str) -> String {
    format!("openverse:{openverse_id}")
}

pub fn cache_name(openverse_id: &str) -> String {
    let safe: String = openverse_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    format!("{safe}.mp3")
}

pub fn search(query: &str, page: u32) -> Result<CatalogPage, String> {
    let q = query.trim();
    if q.chars().count() < 2 {
        return Err("Type at least two characters.".into());
    }
    if q.chars().count() > 80 {
        return Err("Search is too long.".into());
    }
    let page = page.max(1);
    let url = format!(
        "{OPENVERSE}?q={}&page={page}&page_size={PAGE_SIZE}&source=freesound&mature=false&license_type=commercial",
        urlencoding::encode(q)
    );
    let client = client()?;
    let response = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .map_err(|e| format!("Catalog search failed: {e}"))?;
    if !response.status().is_success() {
        let status = response.status();
        if status.as_u16() == 429 {
            return Err("The catalog is rate-limited. Wait a moment and search again.".into());
        }
        return Err(format!("Catalog search returned {status}."));
    }
    let page_data: OvPage = response
        .json()
        .map_err(|e| format!("Catalog response was not valid: {e}"))?;

    let results = page_data
        .results
        .into_iter()
        .filter_map(|hit| {
            if hit.mature.unwrap_or(false) {
                return None;
            }
            let url = hit.url.filter(|u| u.starts_with("https://"))?;
            let duration = hit.duration.unwrap_or(0);
            if duration < MIN_DURATION_MS || duration > MAX_DURATION_MS {
                return None;
            }
            let name = hit
                .title
                .unwrap_or_else(|| "Untitled".into())
                .chars()
                .take(80)
                .collect::<String>();
            Some(CatalogItem {
                id: hit.id,
                name,
                url,
                creator: hit.creator.unwrap_or_default(),
                license: hit.license.unwrap_or_else(|| "unknown".into()),
                license_url: hit.license_url.unwrap_or_default(),
                attribution: hit.attribution.unwrap_or_default(),
                duration_ms: duration,
                source_url: hit.foreign_landing_url.unwrap_or_default(),
                tags: hit
                    .tags
                    .unwrap_or_default()
                    .into_iter()
                    .map(OvTag::name)
                    .take(8)
                    .collect(),
            })
        })
        .collect();

    Ok(CatalogPage {
        query: q.to_string(),
        page: page_data.page,
        page_count: page_data.page_count.max(1),
        result_count: page_data.result_count,
        results,
    })
}

pub fn download(url: &str, dest: &Path) -> Result<(), String> {
    if !(url.starts_with("https://cdn.freesound.org/")
        || url.starts_with("https://freesound.org/")
        || url.starts_with("https://upload.wikimedia.org/"))
    {
        return Err("That audio host is not allowed.".into());
    }
    if dest.exists() {
        if let Ok(meta) = fs::metadata(dest) {
            if meta.len() > 256 {
                return Ok(());
            }
        }
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Could not create cache: {e}"))?;
    }
    let client = client()?;
    let mut response = client
        .get(url)
        .send()
        .map_err(|e| format!("Download failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("Download returned {}.", response.status()));
    }
    if let Some(len) = response.content_length() {
        if len > MAX_DOWNLOAD_BYTES {
            return Err("That clip is too large to import.".into());
        }
    }
    let bytes = response
        .bytes()
        .map_err(|e| format!("Download failed: {e}"))?;
    if bytes.len() as u64 > MAX_DOWNLOAD_BYTES {
        return Err("That clip is too large to import.".into());
    }
    if bytes.len() < 256 {
        return Err("The downloaded file was empty.".into());
    }
    let tmp = dest.with_extension("tmp");
    fs::write(&tmp, &bytes).map_err(|e| format!("Could not save the clip: {e}"))?;
    fs::rename(&tmp, dest).map_err(|e| format!("Could not save the clip: {e}"))?;
    Ok(())
}
