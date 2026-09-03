// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Matteo842
// See LICENSE in the project root for the full terms.

use super::*;
use std::cell::Cell;

struct Fixture {
    cache: HistoryCache,
    directory: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/cache-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT_FILE.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&directory).unwrap();
        Self {
            cache: HistoryCache {
                directory: Some(directory.clone()),
            },
            directory,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(
            self.directory
                .starts_with(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/cache-tests"))
        );
        let _ = fs::remove_dir_all(&self.directory);
    }
}
fn app() -> NonZeroU32 {
    NonZeroU32::new(730).unwrap()
}
fn body() -> HistoryBody {
    HistoryBody {
        html: "valid page".to_owned(),
        chart: Some("[]".to_owned()),
        chart_error: None,
        retry_after: None,
    }
}
fn validate(body: &HistoryBody, _: DateTime<Utc>) -> Result<()> {
    ensure!(body.html == "valid page", "invalid page");
    Ok(())
}

#[test]
fn repeated_reads_use_disk_until_expiry_then_refresh() {
    let f = Fixture::new();
    let calls = Cell::new(0);
    let fetch = || {
        calls.set(calls.get() + 1);
        Ok(body())
    };
    let now = Utc::now();
    assert_eq!(
        f.cache.at(app(), now, validate, fetch).unwrap().state,
        CacheState::Network
    );
    // A new cache instance represents an application restart.
    let restarted = HistoryCache {
        directory: Some(f.directory.clone()),
    };
    let read = restarted
        .at(app(), now + Duration::minutes(59), validate, fetch)
        .unwrap();
    assert_eq!(read.state, CacheState::Fresh);
    assert_eq!(read.fetched_at, now);
    assert_eq!(calls.get(), 1);
    assert_eq!(
        restarted
            .at(app(), now + Duration::hours(1), validate, fetch)
            .unwrap()
            .state,
        CacheState::Network
    );
    assert_eq!(calls.get(), 2);
}

#[test]
fn failed_refresh_preserves_old_data_and_pauses_requests() {
    let f = Fixture::new();
    let now = Utc::now();
    f.cache.at(app(), now, validate, || Ok(body())).unwrap();
    let later = now + Duration::hours(2);
    let read = f
        .cache
        .at(app(), later, validate, || bail!("HTTP 429"))
        .unwrap();
    assert_eq!(read.state, CacheState::Stale);
    assert_eq!(read.fetched_at, now);
    assert!(read.warnings.join(" ").contains("429"));
    let paused = f
        .cache
        .at(app(), later + Duration::minutes(14), validate, || {
            panic!("must not retry during cooldown")
        })
        .unwrap();
    assert_eq!(paused.state, CacheState::Stale);
    let refreshed = f
        .cache
        .at(
            app(),
            later + Duration::minutes(16),
            validate,
            || Ok(body()),
        )
        .unwrap();
    assert_eq!(refreshed.state, CacheState::Network);
}

#[test]
fn first_failure_is_not_converted_to_empty_history_and_is_throttled() {
    let f = Fixture::new();
    let now = Utc::now();
    assert!(
        f.cache
            .at(app(), now, validate, || bail!("offline"))
            .is_err()
    );
    assert!(
        f.cache
            .at(app(), now + Duration::minutes(1), validate, || panic!(
                "must not retry"
            ))
            .is_err()
    );
}

#[test]
fn corrupted_or_incompatible_cache_is_replaced_without_losing_live_result() {
    let f = Fixture::new();
    let path = f.directory.join("730.json");
    fs::write(&path, "broken JSON").unwrap();
    let read = f
        .cache
        .at(app(), Utc::now(), validate, || Ok(body()))
        .unwrap();
    assert_eq!(read.state, CacheState::Network);
    assert_eq!(read.warnings.len(), 1);
    let mut record: Record = read_json(&path).unwrap();
    record.body.as_mut().unwrap().html = "incompatible table".to_owned();
    atomic_json(&path, &record).unwrap();
    assert_eq!(
        f.cache
            .at(app(), Utc::now(), validate, || Ok(body()))
            .unwrap()
            .state,
        CacheState::Network
    );
}

#[test]
fn disabled_cache_never_reads_or_writes_and_clear_only_removes_app_entries() {
    let f = Fixture::new();
    let now = Utc::now();
    f.cache.at(app(), now, validate, || Ok(body())).unwrap();
    fs::write(f.directory.join("unrelated.json"), "keep").unwrap();
    assert!(
        HistoryCache::disabled()
            .at(app(), now, validate, || bail!("network required"))
            .is_err()
    );
    assert!(f.cache.size_bytes().unwrap() > 0);
    f.cache.clear().unwrap();
    assert_eq!(f.cache.size_bytes().unwrap(), 0);
    assert!(f.directory.join("unrelated.json").exists());
}

#[test]
fn pruning_keeps_storage_bounded_and_preserves_other_files() {
    let f = Fixture::new();
    for id in 1..=12 {
        let file = fs::File::create(f.directory.join(format!("{id}.json"))).unwrap();
        file.set_len(5 * 1024 * 1024).unwrap();
    }
    prune(&f.directory).unwrap();
    assert!(f.cache.size_bytes().unwrap() <= MAX_CACHE_BYTES);
}
