//! Mojang's version metadata: the manifest of all versions and the per-version
//! file that lists libraries, arguments, assets and the client jar.

use crate::download::{fetch_json, Checksum, DownloadJob};
use crate::paths::{maven_path, Paths};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Clone, Deserialize)]
pub struct VersionManifest {
    pub latest: Latest,
    pub versions: Vec<VersionSummary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionSummary {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    #[serde(rename = "releaseTime")]
    pub release_time: String,
}

/// A version file. Fabric and other loaders produce the same shape with
/// `inherits_from` set, and [`VersionJson::merge_into_parent`] combines them.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    #[serde(default)]
    pub inherits_from: Option<String>,
    #[serde(default)]
    pub main_class: String,
    #[serde(default)]
    pub arguments: Arguments,
    /// Very old versions use this instead of `arguments`.
    #[serde(default)]
    pub minecraft_arguments: Option<String>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default)]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(default)]
    pub downloads: Option<Downloads>,
    #[serde(default)]
    pub java_version: Option<JavaVersion>,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub logging: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

/// An argument is either a plain string or a rule-guarded value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Conditional {
        rules: Vec<Rule>,
        value: ArgumentValue,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgumentValue {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub action: String,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    /// Maven repository base for loaders that give no explicit download.
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<Artifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub path: Option<String>,
    pub url: String,
    pub sha1: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    pub url: String,
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default, rename = "totalSize")]
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Downloads {
    pub client: Option<Artifact>,
    pub server: Option<Artifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaVersion {
    pub component: String,
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
}

/// The current platform, for rule evaluation.
pub fn os_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "osx",
        other => other,
    }
}

pub fn os_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86" => "x86",
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => other,
    }
}

/// Evaluates Mojang's allow/disallow rules for this machine. `features` are
/// launcher features like `has_custom_resolution`; anything not listed is off.
pub fn rules_allow(rules: &[Rule], features: &[&str]) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        let mut matches = true;
        if let Some(os) = &rule.os {
            if let Some(name) = &os.name {
                matches &= name == os_name();
            }
            if let Some(arch) = &os.arch {
                matches &= arch == os_arch() || (arch == "x86" && os_arch() == "x86");
            }
        }
        if let Some(wanted) = &rule.features {
            for (feature, value) in wanted {
                let on = features.contains(&feature.as_str());
                matches &= value.as_bool().unwrap_or(false) == on;
            }
        }
        if matches {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

impl Library {
    pub fn applies(&self) -> bool {
        rules_allow(&self.rules, &[])
    }

    /// Where the jar lives under `libraries/` and where to get it.
    pub fn download(&self, paths: &Paths) -> Option<DownloadJob> {
        if let Some(artifact) = self.downloads.as_ref().and_then(|d| d.artifact.as_ref()) {
            let rel = artifact
                .path
                .clone()
                .map(PathBuf::from)
                .or_else(|| maven_path(&self.name))?;
            return Some(DownloadJob {
                url: artifact.url.clone(),
                target: paths.libraries().join(rel),
                checksum: Checksum::Sha1(artifact.sha1.clone()),
                size: Some(artifact.size),
            });
        }
        // Loader libraries: a Maven base URL plus the coordinates.
        let rel = maven_path(&self.name)?;
        let base = self.url.clone().unwrap_or_else(|| "https://libraries.minecraft.net/".into());
        let url = format!("{}{}", base.trim_end_matches('/').to_owned() + "/", rel.to_string_lossy().replace('\\', "/"));
        Some(DownloadJob {
            url,
            target: paths.libraries().join(rel),
            checksum: match &self.sha1 {
                Some(h) => Checksum::Sha1(h.clone()),
                None => Checksum::None,
            },
            size: self.size,
        })
    }
}

impl VersionJson {
    /// Applies a loader profile (`self`, with `inherits_from`) on top of the
    /// vanilla version it inherits from. Loader libraries go first so the
    /// loader's copies of shared libraries win on the classpath.
    pub fn merge_into_parent(self, parent: VersionJson) -> VersionJson {
        let mut merged = parent;
        merged.id = self.id;
        if !self.main_class.is_empty() {
            merged.main_class = self.main_class;
        }
        let mut libraries = self.libraries;
        libraries.extend(merged.libraries);
        merged.libraries = libraries;
        let mut game = merged.arguments.game;
        game.extend(self.arguments.game);
        let mut jvm = merged.arguments.jvm;
        jvm.extend(self.arguments.jvm);
        merged.arguments = Arguments { game, jvm };
        merged.inherits_from = None;
        merged
    }
}

pub async fn manifest(client: &reqwest::Client, paths: &Paths) -> Result<VersionManifest> {
    fetch_json(client, MANIFEST_URL, Some(&paths.versions().join("version_manifest_v2.json"))).await
}

/// Fetches (and caches) the version file for `id`.
pub async fn version(client: &reqwest::Client, paths: &Paths, id: &str) -> Result<VersionJson> {
    let cache = paths.version_json(id);
    if let Ok(text) = std::fs::read_to_string(&cache) {
        if let Ok(v) = serde_json::from_str::<VersionJson>(&text) {
            return Ok(v);
        }
    }
    let manifest = manifest(client, paths).await?;
    let summary = manifest
        .versions
        .iter()
        .find(|v| v.id == id)
        .with_context(|| format!("Minecraft version {id} does not exist"))?;
    fetch_json(client, &summary.url, Some(&cache)).await
}

/// Resolves a version id that may have `inherits_from` (a loader profile
/// saved under `versions/`) into a fully merged version file.
pub async fn resolve(client: &reqwest::Client, paths: &Paths, id: &str) -> Result<VersionJson> {
    let json = version(client, paths, id).await?;
    match json.inherits_from.clone() {
        Some(parent_id) => {
            let parent = Box::pin(resolve(client, paths, &parent_id)).await?;
            Ok(json.merge_into_parent(parent))
        }
        None => Ok(json),
    }
}
