//! Game assets: the index lists thousands of small files stored by hash.

use crate::download::{fetch_all, Checksum, DownloadJob};
use crate::meta::VersionJson;
use crate::paths::Paths;
use crate::{report, Progress, ProgressSender};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;

const RESOURCES_URL: &str = "https://resources.download.minecraft.net";

#[derive(Debug, Deserialize)]
struct AssetIndex {
    objects: BTreeMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

/// Makes sure every asset of the version is present under `assets/objects`.
pub async fn ensure(client: &reqwest::Client, paths: &Paths, version: &VersionJson, progress: &Option<ProgressSender>) -> Result<()> {
    let index_ref = version.asset_index.as_ref().context("version has no asset index")?;
    let index_path = paths.assets().join("indexes").join(format!("{}.json", index_ref.id));
    crate::download::fetch(
        client,
        &DownloadJob {
            url: index_ref.url.clone(),
            target: index_path.clone(),
            checksum: Checksum::Sha1(index_ref.sha1.clone()),
            size: Some(index_ref.size),
        },
    )
    .await?;
    let index: AssetIndex = {
        let text = std::fs::read_to_string(&index_path)?;
        serde_json::from_str(&text).context("parsing the asset index")?
    };
    report(progress, Progress::Phase(format!("Checking {} assets", index.objects.len())));
    let jobs: Vec<DownloadJob> = index
        .objects
        .values()
        .map(|o| DownloadJob {
            url: format!("{RESOURCES_URL}/{}/{}", &o.hash[..2], o.hash),
            target: paths.assets().join("objects").join(&o.hash[..2]).join(&o.hash),
            checksum: Checksum::Sha1(o.hash.clone()),
            size: Some(o.size),
        })
        .collect();
    report(progress, Progress::Phase("Downloading assets".into()));
    fetch_all(client, jobs, progress).await
}
