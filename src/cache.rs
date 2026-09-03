// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Matteo842
// See LICENSE in the project root for the full terms.

//! Opt-in, bounded local history cache. Files are replaced atomically.
use std::{
    fs,
    io::Read,
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

const SCHEMA: u32 = 1;
const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
const MAX_CACHE_BYTES: u64 = 50 * 1024 * 1024;
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub struct RetryAfter {
    pub at: DateTime<Utc>,
    pub message: String,
}
impl std::fmt::Display for RetryAfter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}
impl std::error::Error for RetryAfter {}

#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheState {
    #[default]
    Network,
    Fresh,
    Stale,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub cache_enabled: bool,
    #[serde(default)]
    pub recent_games: Vec<crate::Game>,
}

pub fn data_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("STEAMCOUNTER_DATA_DIR") {
        return Ok(PathBuf::from(path));
    }
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let root = std::env::var_os("HOME")
        .map(|path| PathBuf::from(path).join("Library/Application Support"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let root = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|path| PathBuf::from(path).join(".local/share")));
    Ok(root
        .context("Could not locate the local application data folder")?
        .join("SteamCounter"))
}

impl Settings {
    pub fn load() -> Result<Self> {
        let path = data_dir()?.join("settings.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        read_json(&path).context("Could not read settings; local caching is disabled")
    }
    pub fn save(&self) -> Result<()> {
        atomic_json(&data_dir()?.join("settings.json"), self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryBody {
    pub html: String,
    pub chart: Option<String>,
    pub chart_error: Option<String>,
    #[serde(default)]
    pub retry_after: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize)]
struct Record {
    schema: u32,
    appid: NonZeroU32,
    fetched_at: DateTime<Utc>,
    body: Option<HistoryBody>,
    retry_at: Option<DateTime<Utc>>,
    last_error: Option<String>,
}

pub struct CachedHistory {
    pub body: HistoryBody,
    pub fetched_at: DateTime<Utc>,
    pub state: CacheState,
    pub warnings: Vec<String>,
}

#[derive(Clone)]
pub struct HistoryCache {
    directory: Option<PathBuf>,
}

impl HistoryCache {
    pub fn disabled() -> Self {
        Self { directory: None }
    }
    pub fn new(enabled: bool) -> Result<Self> {
        Ok(Self {
            directory: if enabled {
                Some(data_dir()?.join("history-v1"))
            } else {
                None
            },
        })
    }
    pub fn clear(&self) -> Result<()> {
        if let Some(directory) = &self.directory {
            for entry in cache_files(directory)? {
                fs::remove_file(entry.path())?;
            }
        }
        Ok(())
    }
    pub fn size_bytes(&self) -> Result<u64> {
        match &self.directory {
            Some(directory) => Ok(cache_files(directory)?
                .iter()
                .filter_map(|e| e.metadata().ok())
                .map(|m| m.len())
                .sum()),
            None => Ok(0),
        }
    }
    pub fn get_or_fetch(
        &self,
        appid: NonZeroU32,
        validate: impl Fn(&HistoryBody, DateTime<Utc>) -> Result<()>,
        fetch: impl FnOnce() -> Result<HistoryBody>,
    ) -> Result<CachedHistory> {
        self.at(appid, Utc::now(), validate, fetch)
    }

    fn at(
        &self,
        appid: NonZeroU32,
        now: DateTime<Utc>,
        validate: impl Fn(&HistoryBody, DateTime<Utc>) -> Result<()>,
        fetch: impl FnOnce() -> Result<HistoryBody>,
    ) -> Result<CachedHistory> {
        let path = self
            .directory
            .as_ref()
            .map(|dir| dir.join(format!("{appid}.json")));
        let mut warnings = Vec::new();
        let mut cached = path.as_ref().and_then(|path| {
            if !path.exists() {
                return None;
            }
            let record = read_json::<Record>(path).and_then(|record| {
                ensure!(
                    record.schema == SCHEMA && record.appid == appid,
                    "Invalid cache version or AppID"
                );
                ensure!(
                    record.body.is_some() || record.retry_at.is_some(),
                    "Empty cache entry"
                );
                ensure!(
                    record.fetched_at <= now + Duration::minutes(5),
                    "Cache timestamp is in the future"
                );
                ensure!(
                    record
                        .retry_at
                        .is_none_or(|at| at <= now + Duration::days(366)),
                    "Invalid retry timestamp"
                );
                if let Some(body) = &record.body {
                    validate(body, record.fetched_at)?;
                }
                Ok(record)
            });
            match record {
                Ok(record) => Some(record),
                Err(error) => {
                    warnings.push(format!("Ignored unreadable cache: {error:#}"));
                    None
                }
            }
        });
        if let Some(record) = &cached {
            let fresh = now.signed_duration_since(record.fetched_at) < Duration::hours(1)
                || record
                    .body
                    .as_ref()
                    .and_then(|body| body.retry_after)
                    .is_some_and(|at| now < at);
            let cooling = record.retry_at.is_some_and(|at| now < at);
            if fresh && record.retry_at.is_none() || cooling {
                if let Some(body) = &record.body {
                    if cooling {
                        warnings.push(stale_message(record));
                    }
                    return Ok(CachedHistory {
                        body: body.clone(),
                        fetched_at: record.fetched_at,
                        state: if cooling {
                            CacheState::Stale
                        } else {
                            CacheState::Fresh
                        },
                        warnings,
                    });
                }
                bail!(
                    "{} Retry after {} UTC.",
                    record
                        .last_error
                        .as_deref()
                        .unwrap_or("History temporarily unavailable."),
                    record.retry_at.unwrap().format("%Y-%m-%d %H:%M")
                );
            }
        }
        match fetch() {
            Ok(body) => {
                let record = Record {
                    schema: SCHEMA,
                    appid,
                    fetched_at: now,
                    body: Some(body.clone()),
                    retry_at: None,
                    last_error: None,
                };
                self.write(path.as_deref(), &record, &mut warnings);
                Ok(CachedHistory {
                    body,
                    fetched_at: now,
                    state: CacheState::Network,
                    warnings,
                })
            }
            Err(error) => {
                let mut record = cached.take().unwrap_or(Record {
                    schema: SCHEMA,
                    appid,
                    fetched_at: now,
                    body: None,
                    retry_at: None,
                    last_error: None,
                });
                record.retry_at = Some(
                    error
                        .downcast_ref::<RetryAfter>()
                        .map_or(now + Duration::minutes(15), |error| {
                            error.at.max(now + Duration::minutes(15))
                        }),
                );
                record.last_error = Some(format!("{error:#}"));
                self.write(path.as_deref(), &record, &mut warnings);
                if let Some(body) = &record.body {
                    warnings.push(stale_message(&record));
                    Ok(CachedHistory {
                        body: body.clone(),
                        fetched_at: record.fetched_at,
                        state: CacheState::Stale,
                        warnings,
                    })
                } else {
                    Err(error)
                }
            }
        }
    }

    fn write(&self, path: Option<&Path>, record: &Record, warnings: &mut Vec<String>) {
        if let Some(path) = path
            && let Err(error) =
                atomic_json(path, record).and_then(|()| prune(path.parent().unwrap()))
        {
            warnings.push(format!("Could not save local history: {error:#}"));
        }
    }
}

fn stale_message(record: &Record) -> String {
    format!(
        "Using saved history from {} UTC. Refresh failed: {}. Next attempt after {} UTC.",
        record.fetched_at.format("%Y-%m-%d %H:%M"),
        record.last_error.as_deref().unwrap_or("source unavailable"),
        record.retry_at.unwrap_or(record.fetched_at).format("%H:%M")
    )
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let file = fs::File::open(path)?;
    ensure!(
        file.metadata()?.len() <= MAX_FILE_BYTES,
        "Cache file is too large"
    );
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_FILE_BYTES,
        "Cache file is too large"
    );
    serde_json::from_slice(&bytes).context("Invalid local JSON")
}

fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    use std::io::Write;
    let parent = path.parent().context("Invalid storage path")?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(
        ".write-{}-{}.tmp",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        let bytes = serde_json::to_vec(value)?;
        ensure!(
            bytes.len() as u64 <= MAX_FILE_BYTES,
            "Local data exceeds the file limit"
        );
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

fn cache_files(directory: &Path) -> Result<Vec<fs::DirEntry>> {
    if !directory.exists() {
        return Ok(Vec::new());
    }
    Ok(fs::read_dir(directory)?
        .filter_map(Result::ok)
        .filter(|entry| {
            let path = entry.path();
            path.extension()
                .is_some_and(|extension| extension == "json")
                && path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.parse::<NonZeroU32>().is_ok())
                && entry.file_type().is_ok_and(|kind| kind.is_file())
        })
        .collect())
}

fn prune(directory: &Path) -> Result<()> {
    let mut entries: Vec<_> = cache_files(directory)?
        .into_iter()
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            Some((entry.path(), metadata.len(), metadata.modified().ok()?))
        })
        .collect();
    entries.sort_by_key(|entry| entry.2);
    let mut total: u64 = entries.iter().map(|entry| entry.1).sum();
    for (path, len, _) in entries {
        if total <= MAX_CACHE_BYTES {
            break;
        }
        fs::remove_file(path)?;
        total -= len;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
