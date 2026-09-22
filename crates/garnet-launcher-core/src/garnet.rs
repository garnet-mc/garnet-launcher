//! Talking to Garnet servers.
//!
//! A Garnet server's list ping carries a `garnet` block naming the client
//! mods it wants. `join` pings the server, prepares an instance with those
//! mods, and launches straight into the server. No modpacks.

use crate::download::{fetch, Checksum, DownloadJob};
use crate::instance::{self, Instance, InstanceConfig, InstalledMod, Loader};
use crate::launch::{self, LaunchOptions};
use crate::paths::Paths;
use crate::{auth::Account, fabric, modrinth, report, Progress, ProgressSender};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// A mod as advertised by the server.
#[derive(Clone, Debug, Deserialize)]
pub struct ServerMod {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default = "modrinth_source")]
    pub source: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha512: String,
    #[serde(default = "fabric_loader")]
    pub loader: String,
}

fn modrinth_source() -> String {
    "modrinth".into()
}

fn fabric_loader() -> String {
    "fabric".into()
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct GarnetManifest {
    #[serde(default)]
    pub server: String,
    #[serde(default)]
    pub minecraft: String,
    #[serde(default)]
    pub launcher_url: String,
    #[serde(default)]
    pub enforce: bool,
    #[serde(default)]
    pub required_mods: Vec<ServerMod>,
    #[serde(default)]
    pub optional_mods: Vec<ServerMod>,
    #[serde(default)]
    pub shader_pack: Option<ServerMod>,
    #[serde(default)]
    pub voice: bool,
}

#[derive(Clone, Debug)]
pub struct ServerStatus {
    pub description: String,
    pub online: i64,
    pub max: i64,
    pub version_name: String,
    pub protocol: i64,
    /// Present for Garnet servers.
    pub garnet: Option<GarnetManifest>,
}

fn write_varint(buf: &mut Vec<u8>, mut v: u32) {
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            buf.push(b);
            return;
        }
        buf.push(b | 0x80);
    }
}

async fn read_varint(stream: &mut TcpStream) -> Result<u32> {
    let mut value = 0u32;
    for i in 0..5 {
        let byte = stream.read_u8().await?;
        value |= ((byte & 0x7F) as u32) << (7 * i);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    bail!("bad varint from server");
}

/// Server list ping, the same one the game's multiplayer screen does.
pub async fn ping(host: &str, port: u16) -> Result<ServerStatus> {
    let mut stream = tokio::time::timeout(std::time::Duration::from_secs(5), TcpStream::connect((host, port)))
        .await
        .context("connection timed out")?
        .with_context(|| format!("connecting to {host}:{port}"))?;

    // Handshake with intent 1 (status), then an empty status request.
    let mut body = Vec::new();
    write_varint(&mut body, 0); // packet id
    write_varint(&mut body, 0x7FFF_FFFF); // any protocol; we only want the status
    write_varint(&mut body, host.len() as u32);
    body.extend_from_slice(host.as_bytes());
    body.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut body, 1);
    let mut frame = Vec::new();
    write_varint(&mut frame, body.len() as u32);
    frame.extend_from_slice(&body);
    frame.extend_from_slice(&[1, 0]);
    stream.write_all(&frame).await?;

    let _length = read_varint(&mut stream).await?;
    let _id = read_varint(&mut stream).await?;
    let json_len = read_varint(&mut stream).await? as usize;
    if json_len > 1 << 20 {
        bail!("status response too large");
    }
    let mut json = vec![0u8; json_len];
    stream.read_exact(&mut json).await?;
    let value: serde_json::Value = serde_json::from_slice(&json).context("parsing the status response")?;

    Ok(ServerStatus {
        description: plain_text(&value["description"]),
        online: value["players"]["online"].as_i64().unwrap_or(0),
        max: value["players"]["max"].as_i64().unwrap_or(0),
        version_name: value["version"]["name"].as_str().unwrap_or("").to_owned(),
        protocol: value["version"]["protocol"].as_i64().unwrap_or(0),
        garnet: value.get("garnet").and_then(|g| serde_json::from_value(g.clone()).ok()),
    })
}

/// Flattens a chat component into plain text for display.
fn plain_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(map) => {
            let mut out = map.get("text").and_then(|t| t.as_str()).unwrap_or("").to_owned();
            if let Some(extra) = map.get("extra").and_then(|e| e.as_array()) {
                for part in extra {
                    out.push_str(&plain_text(part));
                }
            }
            out
        }
        serde_json::Value::Array(parts) => parts.iter().map(plain_text).collect(),
        _ => String::new(),
    }
}

pub fn parse_address(address: &str) -> (String, u16) {
    let address = address.trim().trim_start_matches("garnet://").trim_end_matches('/');
    match address.rsplit_once(':') {
        Some((host, port)) if port.parse::<u16>().is_ok() => (host.to_owned(), port.parse().unwrap()),
        _ => (address.to_owned(), 25565),
    }
}

/// Installs one advertised mod into the instance unless it is already there
/// in the wanted version.
async fn install_mod(client: &reqwest::Client, instance: &Instance, m: &ServerMod, project_type: &str) -> Result<bool> {
    let already = instance
        .installed_mods()
        .into_iter()
        .any(|have| (have.id == m.id || have.id == m.name) && (m.version.is_empty() || have.version == m.version));
    if already {
        return Ok(false);
    }
    match m.source.as_str() {
        "url" => {
            let file = m
                .url
                .rsplit('/')
                .next()
                .filter(|f| !f.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{}.jar", m.id));
            let dir = if project_type == "shader" { instance.shaderpacks_dir() } else { instance.mods_dir() };
            std::fs::create_dir_all(&dir)?;
            let target = dir.join(&file);
            fetch(
                client,
                &DownloadJob {
                    url: m.url.clone(),
                    target: target.clone(),
                    checksum: if m.sha512.is_empty() { Checksum::None } else { Checksum::Sha512(m.sha512.clone()) },
                    size: None,
                },
            )
            .await?;
            let record = InstalledMod {
                id: m.id.clone(),
                version: m.version.clone(),
                file,
                source: "url".into(),
            };
            std::fs::write(target.with_extension("garnet.json"), serde_json::to_string_pretty(&record)?)?;
        }
        _ => {
            let loader = if project_type == "shader" { None } else { Some(m.loader.as_str()) };
            let version = modrinth::pick_version(client, &m.id, &instance.config.minecraft, loader, &m.version).await?;
            let target = modrinth::install(client, instance, &version, project_type).await?;
            // Record the server's id (a slug) too so the check above matches next time.
            let record = InstalledMod {
                id: m.id.clone(),
                version: version.version_number.clone(),
                file: target.file_name().unwrap().to_string_lossy().to_string(),
                source: "modrinth".into(),
            };
            std::fs::write(target.with_extension("garnet.json"), serde_json::to_string_pretty(&record)?)?;
        }
    }
    Ok(true)
}

/// Brings an instance up to date with a server's manifest. Returns the names
/// of mods that were installed.
pub async fn sync_mods(client: &reqwest::Client, instance: &Instance, manifest: &GarnetManifest, include_optional: bool, progress: &Option<ProgressSender>) -> Result<Vec<String>> {
    let mut installed = Vec::new();
    let mut wanted: Vec<(&ServerMod, &str)> = manifest.required_mods.iter().map(|m| (m, "mod")).collect();
    if include_optional {
        wanted.extend(manifest.optional_mods.iter().map(|m| (m, "mod")));
        if let Some(pack) = &manifest.shader_pack {
            wanted.push((pack, "shader"));
        }
    }
    let total = wanted.len();
    for (i, (m, kind)) in wanted.iter().enumerate() {
        report(progress, Progress::Items { done: i, total });
        let label = if m.name.is_empty() { m.id.clone() } else { m.name.clone() };
        report(progress, Progress::Message(format!("Checking {label}")));
        match install_mod(client, instance, m, kind).await {
            Ok(true) => installed.push(label),
            Ok(false) => {}
            Err(err) => {
                if manifest.required_mods.iter().any(|r| r.id == m.id) {
                    return Err(err.context(format!("installing required mod {label}")));
                }
                tracing::warn!("optional {label} skipped: {err:#}");
            }
        }
    }
    report(progress, Progress::Items { done: total, total });
    Ok(installed)
}

/// Where Garnet's own client mod is published, by Minecraft version.
pub fn client_mod_url(minecraft: &str) -> String {
    format!("https://github.com/garnet-mc/garnet-launcher/releases/latest/download/garnet-client-{minecraft}.jar")
}

/// Installs Garnet's client mod (voice chat, server mod sync, shader
/// helper) into the instance if a build exists for its Minecraft version.
pub async fn ensure_client_mod(client: &reqwest::Client, instance: &Instance) -> Result<bool> {
    if !instance.config.garnet_loader || !matches!(instance.config.loader, Loader::Fabric) {
        return Ok(false);
    }
    let url = client_mod_url(&instance.config.minecraft);
    let target = instance.mods_dir().join(format!("garnet-client-{}.jar", instance.config.minecraft));
    if target.exists() {
        return Ok(false);
    }
    let head = client.head(&url).send().await;
    match head {
        Ok(r) if r.status().is_success() => {
            fetch(
                client,
                &DownloadJob {
                    url,
                    target: target.clone(),
                    checksum: Checksum::None,
                    size: None,
                },
            )
            .await?;
            let record = InstalledMod {
                id: "garnet-client".into(),
                version: instance.config.minecraft.clone(),
                file: target.file_name().unwrap().to_string_lossy().to_string(),
                source: "garnet".into(),
            };
            std::fs::write(target.with_extension("garnet.json"), serde_json::to_string_pretty(&record)?)?;
            Ok(true)
        }
        _ => {
            tracing::warn!("no Garnet client mod build for Minecraft {} yet", instance.config.minecraft);
            Ok(false)
        }
    }
}

/// The instance used for a server: one per server address, created with
/// the server's Minecraft version and Fabric.
pub async fn instance_for_server(client: &reqwest::Client, paths: &Paths, host: &str, port: u16, manifest: &GarnetManifest, progress: &Option<ProgressSender>) -> Result<Instance> {
    let address = format!("{host}:{port}");
    if let Ok(existing) = instance::list(paths) {
        if let Some(found) = existing.into_iter().find(|i| i.config.server.as_deref() == Some(address.as_str())) {
            return Ok(found);
        }
    }
    let minecraft = if manifest.minecraft.is_empty() {
        crate::meta::manifest(client, paths).await?.latest.release
    } else {
        manifest.minecraft.clone()
    };
    report(progress, Progress::Phase(format!("Creating an instance for {address}")));
    let needs_loader = !manifest.required_mods.is_empty() || !manifest.optional_mods.is_empty() || manifest.voice;
    let mut config = InstanceConfig {
        name: host.to_owned(),
        minecraft: minecraft.clone(),
        loader: if needs_loader { Loader::Fabric } else { Loader::Vanilla },
        server: Some(address),
        ..Default::default()
    };
    if needs_loader {
        config.version_id = fabric::install(client, paths, &minecraft, "").await?;
    }
    instance::create(paths, config)
}

/// The whole "join this server" flow.
pub async fn join(client: &reqwest::Client, paths: &Paths, account: &Account, address: &str, include_optional: bool, progress: &Option<ProgressSender>) -> Result<(Instance, tokio::process::Child)> {
    let (host, port) = parse_address(address);
    report(progress, Progress::Phase(format!("Pinging {host}:{port}")));
    let status = ping(&host, port).await?;
    let manifest = status.garnet.clone().unwrap_or_default();
    let mut instance = instance_for_server(client, paths, &host, port, &manifest, progress).await?;
    report(progress, Progress::Phase("Syncing mods".into()));
    let installed = sync_mods(client, &instance, &manifest, include_optional, progress).await?;
    if !installed.is_empty() {
        report(progress, Progress::Message(format!("Installed {}", installed.join(", "))));
    }
    if ensure_client_mod(client, &instance).await? {
        report(progress, Progress::Message("Installed the Garnet client mod".into()));
    }
    let prepared = launch::prepare(client, paths, &instance, progress).await?;
    report(progress, Progress::Phase(format!("Starting Minecraft {}", instance.config.minecraft)));
    let child = launch::start(
        paths,
        &mut instance,
        &prepared,
        &LaunchOptions {
            account,
            server: Some(format!("{host}:{port}")),
        },
    )
    .await?;
    Ok((instance, child))
}
