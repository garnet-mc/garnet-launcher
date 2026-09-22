//! Verified downloads. A file is only fetched when it is missing or its hash
//! does not match, so re-running any step is cheap.

use crate::{report, Progress, ProgressSender};
use anyhow::{bail, Context, Result};
use futures::stream::{self, StreamExt};
use sha1::{Digest, Sha1};
use sha2::Sha512;
use std::io::Read;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

/// How many files to download at once.
const PARALLEL: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Checksum {
    Sha1(String),
    Sha512(String),
    None,
}

#[derive(Clone, Debug)]
pub struct DownloadJob {
    pub url: String,
    pub target: PathBuf,
    pub checksum: Checksum,
    pub size: Option<u64>,
}

pub fn sha1_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn sha512_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha512::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Whether the file on disk already satisfies the job.
pub fn is_current(job: &DownloadJob) -> bool {
    if !job.target.exists() {
        return false;
    }
    match &job.checksum {
        Checksum::Sha1(want) => sha1_file(&job.target).map(|h| h.eq_ignore_ascii_case(want)).unwrap_or(false),
        Checksum::Sha512(want) => sha512_file(&job.target).map(|h| h.eq_ignore_ascii_case(want)).unwrap_or(false),
        Checksum::None => match job.size {
            Some(size) => std::fs::metadata(&job.target).map(|m| m.len() == size).unwrap_or(false),
            None => true,
        },
    }
}

/// Downloads one file to a temporary path, verifies it, then moves it into
/// place, so a crash never leaves a half-written file behind.
pub async fn fetch(client: &reqwest::Client, job: &DownloadJob) -> Result<()> {
    if is_current(job) {
        return Ok(());
    }
    if let Some(parent) = job.target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    // Append rather than replace the extension: `java.exe` and `java.dll`
    // must not share a temp file.
    let tmp = job.target.with_file_name(format!("{}.part", crate::paths::file_name_of(&job.target)));
    let mut response = client
        .get(&job.url)
        .send()
        .await
        .with_context(|| format!("requesting {}", job.url))?
        .error_for_status()
        .with_context(|| format!("downloading {}", job.url))?;
    let mut file = tokio::fs::File::create(&tmp).await?;
    let mut sha1 = Sha1::new();
    let mut sha512 = Sha512::new();
    while let Some(chunk) = response.chunk().await? {
        sha1.update(&chunk);
        sha512.update(&chunk);
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);
    let ok = match &job.checksum {
        Checksum::Sha1(want) => hex::encode(sha1.finalize()).eq_ignore_ascii_case(want),
        Checksum::Sha512(want) => hex::encode(sha512.finalize()).eq_ignore_ascii_case(want),
        Checksum::None => true,
    };
    if !ok {
        let _ = tokio::fs::remove_file(&tmp).await;
        bail!("checksum mismatch for {}", job.url);
    }
    tokio::fs::rename(&tmp, &job.target).await?;
    Ok(())
}

/// Downloads many files in parallel, reporting progress. Fails on the first
/// error after letting in-flight downloads finish.
pub async fn fetch_all(client: &reqwest::Client, jobs: Vec<DownloadJob>, progress: &Option<ProgressSender>) -> Result<()> {
    let pending: Vec<DownloadJob> = jobs.into_iter().filter(|j| !is_current(j)).collect();
    let total = pending.len();
    if total == 0 {
        return Ok(());
    }
    report(progress, Progress::Items { done: 0, total });
    let mut done = 0usize;
    let mut results = stream::iter(pending.into_iter().map(|job| {
        let client = client.clone();
        async move { fetch(&client, &job).await.map_err(|e| (job, e)) }
    }))
    .buffer_unordered(PARALLEL);
    let mut first_error = None;
    while let Some(result) = results.next().await {
        match result {
            Ok(()) => {
                done += 1;
                report(progress, Progress::Items { done, total });
            }
            Err((job, err)) => {
                if first_error.is_none() {
                    first_error = Some(err.context(format!("{}", crate::paths::file_name_of(&job.target))));
                }
            }
        }
    }
    match first_error {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

/// Fetches JSON, with a cache file used when offline.
pub async fn fetch_json<T: serde::de::DeserializeOwned>(client: &reqwest::Client, url: &str, cache: Option<&Path>) -> Result<T> {
    match client.get(url).send().await.and_then(|r| r.error_for_status()) {
        Ok(response) => {
            let text = response.text().await?;
            if let Some(cache) = cache {
                if let Some(parent) = cache.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(cache, &text);
            }
            serde_json::from_str(&text).with_context(|| format!("parsing {url}"))
        }
        Err(err) => {
            if let Some(cache) = cache {
                if let Ok(text) = std::fs::read_to_string(cache) {
                    tracing::warn!("using cached copy of {url}: {err}");
                    return serde_json::from_str(&text).with_context(|| format!("parsing cached {url}"));
                }
            }
            Err(err).with_context(|| format!("fetching {url}"))
        }
    }
}
