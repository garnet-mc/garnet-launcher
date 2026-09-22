//! Instances: separate game folders with their own mods, saves and options,
//! all sharing the downloaded versions, libraries and assets.

use crate::paths::Paths;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Loader {
    Vanilla,
    /// Fabric, with Garnet's client mod on top when `garnet_loader` is set.
    Fabric,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct InstanceConfig {
    pub name: String,
    /// Minecraft version, e.g. `26.3`.
    pub minecraft: String,
    pub loader: Loader,
    /// Loader version, e.g. the Fabric loader version. Empty = latest stable.
    pub loader_version: String,
    /// The merged version id under `versions/` to launch (set on install).
    pub version_id: String,
    pub memory_mb: u32,
    pub jvm_args: Vec<String>,
    /// Install Garnet's client mod (voice, server mod sync, shaders).
    pub garnet_loader: bool,
    pub created: String,
    pub last_played: Option<String>,
    /// Server this instance was created for, if any (`host:port`).
    pub server: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl Default for InstanceConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            minecraft: String::new(),
            loader: Loader::Vanilla,
            loader_version: String::new(),
            version_id: String::new(),
            memory_mb: 4096,
            jvm_args: Vec::new(),
            garnet_loader: true,
            created: chrono::Utc::now().to_rfc3339(),
            last_played: None,
            server: None,
            width: None,
            height: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Instance {
    pub dir: PathBuf,
    pub config: InstanceConfig,
}

impl Instance {
    pub fn config_path(dir: &std::path::Path) -> PathBuf {
        dir.join("instance.toml")
    }

    pub fn game_dir(&self) -> PathBuf {
        self.dir.join("minecraft")
    }

    pub fn mods_dir(&self) -> PathBuf {
        self.game_dir().join("mods")
    }

    pub fn shaderpacks_dir(&self) -> PathBuf {
        self.game_dir().join("shaderpacks")
    }

    pub fn load(dir: PathBuf) -> Result<Self> {
        let text = std::fs::read_to_string(Self::config_path(&dir))
            .with_context(|| format!("{} is not an instance", dir.display()))?;
        let config = toml::from_str(&text).context("parsing instance.toml")?;
        Ok(Self { dir, config })
    }

    pub fn save(&self) -> Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        std::fs::create_dir_all(self.mods_dir())?;
        std::fs::write(Self::config_path(&self.dir), toml::to_string_pretty(&self.config)?)?;
        Ok(())
    }

    /// Jars in the mods folder with the id/version Garnet can infer from the
    /// filename, used to answer a server's mod check.
    pub fn installed_mods(&self) -> Vec<InstalledMod> {
        let Ok(entries) = std::fs::read_dir(self.mods_dir()) else {
            return Vec::new();
        };
        entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "jar").unwrap_or(false))
            .map(|p| {
                let file = p.file_name().unwrap().to_string_lossy().to_string();
                let sidecar = p.with_extension("garnet.json");
                let meta: Option<InstalledMod> = std::fs::read_to_string(&sidecar)
                    .ok()
                    .and_then(|t| serde_json::from_str(&t).ok());
                meta.unwrap_or(InstalledMod {
                    id: file.trim_end_matches(".jar").to_owned(),
                    version: String::new(),
                    file,
                    source: "manual".into(),
                })
            })
            .collect()
    }
}

/// Recorded next to each mod we install, so we know what it is later.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstalledMod {
    pub id: String,
    pub version: String,
    pub file: String,
    pub source: String,
}

pub fn list(paths: &Paths) -> Result<Vec<Instance>> {
    let mut out = Vec::new();
    if !paths.instances().exists() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(paths.instances())? {
        let dir = entry?.path();
        if Instance::config_path(&dir).exists() {
            match Instance::load(dir) {
                Ok(i) => out.push(i),
                Err(err) => tracing::warn!("{err:#}"),
            }
        }
    }
    out.sort_by(|a, b| a.config.name.to_lowercase().cmp(&b.config.name.to_lowercase()));
    Ok(out)
}

pub fn get(paths: &Paths, name: &str) -> Result<Instance> {
    Instance::load(paths.instance(&safe_name(name)))
}

pub fn create(paths: &Paths, config: InstanceConfig) -> Result<Instance> {
    if config.name.trim().is_empty() {
        bail!("an instance needs a name");
    }
    let dir = paths.instance(&safe_name(&config.name));
    if Instance::config_path(&dir).exists() {
        bail!("an instance called '{}' already exists", config.name);
    }
    let instance = Instance { dir, config };
    instance.save()?;
    Ok(instance)
}

pub fn delete(paths: &Paths, name: &str) -> Result<()> {
    let dir = paths.instance(&safe_name(name));
    if !Instance::config_path(&dir).exists() {
        bail!("no instance called '{name}'");
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

/// Folder name for an instance: the display name with anything unsafe removed.
pub fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == ' ' { c } else { '_' })
        .collect();
    cleaned.trim().to_owned()
}
