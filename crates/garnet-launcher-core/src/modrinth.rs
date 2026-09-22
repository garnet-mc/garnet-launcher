//! Modrinth: searching for mods and shader packs and downloading the right
//! version for an instance.

use crate::download::{fetch, Checksum, DownloadJob};
use crate::instance::{Instance, InstalledMod};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const API: &str = "https://api.modrinth.com/v2";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub project_type: String,
    pub downloads: u64,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    hits: Vec<SearchHit>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Version {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub version_type: String,
    pub files: Vec<VersionFile>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionFile {
    pub url: String,
    pub filename: String,
    pub primary: bool,
    pub size: u64,
    pub hashes: Hashes,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Hashes {
    pub sha512: String,
    #[serde(default)]
    pub sha1: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Dependency {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub version_id: Option<String>,
    pub dependency_type: String,
}

/// `project_type` is `mod`, `shader`, `resourcepack` or `modpack`.
pub async fn search(client: &reqwest::Client, query: &str, project_type: &str, minecraft: &str, loader: &str) -> Result<Vec<SearchHit>> {
    let mut facets = vec![vec![format!("project_type:{project_type}")], vec![format!("versions:{minecraft}")]];
    if project_type == "mod" && !loader.is_empty() {
        facets.push(vec![format!("categories:{loader}")]);
    }
    let response: SearchResponse = client
        .get(format!("{API}/search"))
        .query(&[("query", query), ("limit", "20"), ("facets", &serde_json::to_string(&facets)?)])
        .send()
        .await?
        .error_for_status()
        .context("Modrinth search")?
        .json()
        .await?;
    Ok(response.hits)
}

/// Versions of a project that fit the instance, newest first.
pub async fn versions(client: &reqwest::Client, project: &str, minecraft: &str, loader: Option<&str>) -> Result<Vec<Version>> {
    let mut request = client
        .get(format!("{API}/project/{project}/version"))
        .query(&[("game_versions", serde_json::to_string(&[minecraft])?)]);
    if let Some(loader) = loader {
        request = request.query(&[("loaders", serde_json::to_string(&[loader])?)]);
    }
    let versions: Vec<Version> = request
        .send()
        .await?
        .error_for_status()
        .with_context(|| format!("looking up {project} on Modrinth"))?
        .json()
        .await?;
    Ok(versions)
}

/// Picks a version: an exact `version_number` when given, otherwise the
/// newest release (falling back to any newest).
pub async fn pick_version(client: &reqwest::Client, project: &str, minecraft: &str, loader: Option<&str>, wanted: &str) -> Result<Version> {
    let versions = versions(client, project, minecraft, loader).await?;
    if versions.is_empty() {
        bail!("{project} has no version for Minecraft {minecraft}{}", loader.map(|l| format!(" ({l})")).unwrap_or_default());
    }
    if !wanted.is_empty() {
        return versions
            .into_iter()
            .find(|v| v.version_number == wanted || v.id == wanted)
            .with_context(|| format!("{project} has no version {wanted} for Minecraft {minecraft}"));
    }
    Ok(versions
        .iter()
        .find(|v| v.version_type == "release")
        .cloned()
        .unwrap_or_else(|| versions[0].clone()))
}

/// Downloads a version's primary file into the right instance folder and
/// records what it is. Returns the file path.
pub async fn install(client: &reqwest::Client, instance: &Instance, version: &Version, project_type: &str) -> Result<PathBuf> {
    let file = version
        .files
        .iter()
        .find(|f| f.primary)
        .or_else(|| version.files.first())
        .context("version has no files")?;
    let dir = match project_type {
        "shader" => instance.shaderpacks_dir(),
        "resourcepack" => instance.game_dir().join("resourcepacks"),
        _ => instance.mods_dir(),
    };
    std::fs::create_dir_all(&dir)?;
    let target = dir.join(&file.filename);
    fetch(
        client,
        &DownloadJob {
            url: file.url.clone(),
            target: target.clone(),
            checksum: Checksum::Sha512(file.hashes.sha512.clone()),
            size: Some(file.size),
        },
    )
    .await?;
    let record = InstalledMod {
        id: version.project_id.clone(),
        version: version.version_number.clone(),
        file: file.filename.clone(),
        source: "modrinth".into(),
    };
    std::fs::write(target.with_extension("garnet.json"), serde_json::to_string_pretty(&record)?)?;
    Ok(target)
}

/// Resolves a slug to a project id (ids are what versions reference).
pub async fn project_id(client: &reqwest::Client, slug_or_id: &str) -> Result<String> {
    #[derive(Deserialize)]
    struct Project {
        id: String,
    }
    let project: Project = client
        .get(format!("{API}/project/{slug_or_id}"))
        .send()
        .await?
        .error_for_status()
        .with_context(|| format!("no Modrinth project called {slug_or_id}"))?
        .json()
        .await?;
    Ok(project.id)
}
