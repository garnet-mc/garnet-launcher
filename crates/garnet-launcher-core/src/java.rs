//! Java runtimes. Mojang publishes the exact runtime each Minecraft version
//! needs; we download it into `java/<component>/` and never touch the
//! system's Java.

use crate::download::{fetch_all, fetch_json, Checksum, DownloadJob};
use crate::paths::Paths;
use crate::{report, Progress, ProgressSender};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const RUNTIME_MANIFEST_URL: &str =
    "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

#[derive(Debug, Deserialize)]
struct RuntimeComponent {
    manifest: ManifestRef,
    version: RuntimeVersion,
}

#[derive(Debug, Deserialize)]
struct ManifestRef {
    url: String,
}

#[derive(Debug, Deserialize)]
struct RuntimeVersion {
    name: String,
}

#[derive(Debug, Deserialize)]
struct RuntimeManifest {
    files: BTreeMap<String, RuntimeFile>,
}

#[derive(Debug, Deserialize)]
struct RuntimeFile {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    executable: bool,
    downloads: Option<RuntimeDownloads>,
    target: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RuntimeDownloads {
    raw: RawDownload,
}

#[derive(Debug, Deserialize)]
struct RawDownload {
    url: String,
    sha1: String,
    size: u64,
}

pub fn platform_key() -> Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "windows-x64",
        ("windows", "aarch64") => "windows-arm64",
        ("windows", "x86") => "windows-x86",
        ("linux", "x86_64") => "linux",
        ("linux", "x86") => "linux-i386",
        ("macos", "x86_64") => "mac-os",
        ("macos", "aarch64") => "mac-os-arm64",
        (os, arch) => bail!("Mojang has no Java runtime for {os}/{arch}"),
    })
}

/// Path of the `java` executable inside a downloaded runtime.
pub fn executable(paths: &Paths, component: &str) -> PathBuf {
    let dir = paths.java().join(component);
    if cfg!(target_os = "macos") {
        dir.join("jre.bundle/Contents/Home/bin/java")
    } else if cfg!(windows) {
        dir.join("bin").join("javaw.exe")
    } else {
        dir.join("bin").join("java")
    }
}

/// Runs `java -version` and returns the major version, if it runs.
pub fn probe(exe: &Path) -> Option<u32> {
    let exe = if cfg!(windows) && exe.ends_with("javaw.exe") {
        exe.with_file_name("java.exe")
    } else {
        exe.to_owned()
    };
    let output = std::process::Command::new(exe).arg("-version").output().ok()?;
    let text = String::from_utf8_lossy(&output.stderr).to_string() + &String::from_utf8_lossy(&output.stdout);
    let quoted = text.split('"').nth(1)?;
    let mut parts = quoted.split('.');
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

/// Makes sure the runtime `component` (e.g. `java-runtime-epsilon`) is
/// installed and returns its executable.
pub async fn ensure(client: &reqwest::Client, paths: &Paths, component: &str, progress: &Option<ProgressSender>) -> Result<PathBuf> {
    let exe = executable(paths, component);
    if probe(&exe).is_some() {
        return Ok(exe);
    }
    report(progress, Progress::Phase(format!("Downloading Java ({component})")));
    let all: BTreeMap<String, BTreeMap<String, Vec<RuntimeComponent>>> =
        fetch_json(client, RUNTIME_MANIFEST_URL, Some(&paths.java().join("all.json"))).await?;
    let platform = platform_key()?;
    let entry = all
        .get(platform)
        .and_then(|p| p.get(component))
        .and_then(|list| list.first())
        .with_context(|| format!("no Java runtime '{component}' for {platform}"))?;
    tracing::info!("installing Java {} ({component})", entry.version.name);
    let manifest: RuntimeManifest = fetch_json(client, &entry.manifest.url, None).await?;

    let root = paths.java().join(component);
    let mut jobs = Vec::new();
    let mut executables = Vec::new();
    let mut links = Vec::new();
    for (name, file) in &manifest.files {
        let path = root.join(name);
        match file.kind.as_str() {
            "directory" => std::fs::create_dir_all(&path)?,
            "file" => {
                let raw = &file.downloads.as_ref().context("runtime file without download")?.raw;
                jobs.push(DownloadJob {
                    url: raw.url.clone(),
                    target: path.clone(),
                    checksum: Checksum::Sha1(raw.sha1.clone()),
                    size: Some(raw.size),
                });
                if file.executable {
                    executables.push(path);
                }
            }
            "link" => {
                if let Some(target) = &file.target {
                    links.push((path, target.clone()));
                }
            }
            _ => {}
        }
    }
    fetch_all(client, jobs, progress).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in &executables {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
        }
        for (path, target) in &links {
            let _ = std::fs::remove_file(path);
            std::os::unix::fs::symlink(target, path)?;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (&executables, &links);
    }
    probe(&exe).context("the downloaded Java runtime does not start")?;
    Ok(exe)
}
