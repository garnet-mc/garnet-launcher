//! Where the launcher keeps things.
//!
//! ```text
//! <root>/
//!   versions/<id>/<id>.json, <id>.jar      Mojang version files and client jars
//!   libraries/...                          shared library jars (Maven layout)
//!   assets/indexes/, assets/objects/       shared game assets
//!   java/<component>/                      Mojang Java runtimes
//!   instances/<name>/                      one folder per instance (mods, saves, options)
//!   accounts.json                          logged-in accounts (tokens)
//!   settings.toml                          launcher settings
//! ```
//!
//! The root defaults to the platform's data directory (`%APPDATA%\garnet`,
//! `~/Library/Application Support/garnet`, `~/.local/share/garnet`) and can
//! be overridden with `GARNET_HOME`.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn default_root() -> PathBuf {
        if let Ok(home) = std::env::var("GARNET_HOME") {
            return PathBuf::from(home);
        }
        dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("garnet")
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn versions(&self) -> PathBuf {
        self.root.join("versions")
    }

    pub fn version_dir(&self, id: &str) -> PathBuf {
        self.versions().join(id)
    }

    pub fn version_json(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.json"))
    }

    pub fn version_jar(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.jar"))
    }

    pub fn libraries(&self) -> PathBuf {
        self.root.join("libraries")
    }

    pub fn assets(&self) -> PathBuf {
        self.root.join("assets")
    }

    pub fn java(&self) -> PathBuf {
        self.root.join("java")
    }

    pub fn instances(&self) -> PathBuf {
        self.root.join("instances")
    }

    pub fn instance(&self, name: &str) -> PathBuf {
        self.instances().join(name)
    }

    pub fn accounts(&self) -> PathBuf {
        self.root.join("accounts.json")
    }

    pub fn settings(&self) -> PathBuf {
        self.root.join("settings.toml")
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        for dir in [self.versions(), self.libraries(), self.assets(), self.java(), self.instances()] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

/// Turns Maven coordinates (`group:artifact:version[:classifier][@ext]`)
/// into the relative path libraries are stored under.
pub fn maven_path(name: &str) -> Option<PathBuf> {
    let (coords, ext) = match name.split_once('@') {
        Some((c, e)) => (c, e),
        None => (name, "jar"),
    };
    let parts: Vec<&str> = coords.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let (group, artifact, version) = (parts[0], parts[1], parts[2]);
    let file = match parts.get(3) {
        Some(classifier) => format!("{artifact}-{version}-{classifier}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    let mut path = PathBuf::new();
    for segment in group.split('.') {
        path.push(segment);
    }
    path.push(artifact);
    path.push(version);
    path.push(file);
    Some(path)
}

pub fn file_name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maven_paths() {
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3").unwrap(),
            PathBuf::from("org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar")
        );
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3:natives-windows").unwrap(),
            PathBuf::from("org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar")
        );
        assert_eq!(
            maven_path("net.fabricmc:fabric-loader:0.16.0").unwrap(),
            PathBuf::from("net/fabricmc/fabric-loader/0.16.0/fabric-loader-0.16.0.jar")
        );
    }
}
