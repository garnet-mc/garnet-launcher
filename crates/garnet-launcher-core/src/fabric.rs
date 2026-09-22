//! Installing the Fabric loader. Fabric's meta service hands out a ready
//! version profile that inherits from the vanilla version; we save it under
//! `versions/` and the normal launch path takes it from there.
//!
//! Garnet Loader is Fabric plus Garnet's own client mod, so every Fabric mod
//! keeps working.

use crate::download::fetch_json;
use crate::meta::VersionJson;
use crate::paths::Paths;
use anyhow::{bail, Context, Result};
use serde::Deserialize;

const META: &str = "https://meta.fabricmc.net/v2";

#[derive(Debug, Deserialize)]
pub struct LoaderEntry {
    pub loader: LoaderVersion,
}

#[derive(Debug, Deserialize)]
pub struct LoaderVersion {
    pub version: String,
    pub stable: bool,
}

/// Loader versions available for a Minecraft version, newest first.
pub async fn loader_versions(client: &reqwest::Client, minecraft: &str) -> Result<Vec<LoaderVersion>> {
    let entries: Vec<LoaderEntry> = fetch_json(client, &format!("{META}/versions/loader/{minecraft}"), None)
        .await
        .with_context(|| format!("Fabric does not support Minecraft {minecraft} yet"))?;
    if entries.is_empty() {
        bail!("Fabric does not support Minecraft {minecraft} yet");
    }
    Ok(entries.into_iter().map(|e| e.loader).collect())
}

/// Installs the profile for `minecraft` + `loader` (empty = newest stable)
/// and returns the version id to launch.
pub async fn install(client: &reqwest::Client, paths: &Paths, minecraft: &str, loader: &str) -> Result<String> {
    let loader = if loader.is_empty() {
        let versions = loader_versions(client, minecraft).await?;
        versions
            .iter()
            .find(|v| v.stable)
            .or(versions.first())
            .map(|v| v.version.clone())
            .context("no Fabric loader versions")?
    } else {
        loader.to_owned()
    };
    let url = format!("{META}/versions/loader/{minecraft}/{loader}/profile/json");
    let profile: VersionJson = fetch_json(client, &url, None)
        .await
        .with_context(|| format!("fetching the Fabric profile for {minecraft} / {loader}"))?;
    let id = profile.id.clone();
    let path = paths.version_json(&id);
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, serde_json::to_string_pretty(&profile)?)?;
    tracing::info!("installed Fabric loader {loader} for Minecraft {minecraft} as {id}");
    Ok(id)
}
