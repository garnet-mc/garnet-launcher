//! Preparing an instance (downloads) and starting the game.

use crate::auth::Account;
use crate::download::{fetch, fetch_all, Checksum, DownloadJob};
use crate::instance::Instance;
use crate::meta::{self, rules_allow, Argument, ArgumentValue, VersionJson};
use crate::paths::Paths;
use crate::{assets, java, report, Progress, ProgressSender};
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Stdio;

pub const LAUNCHER_NAME: &str = "garnet";

/// Everything needed to start the game, produced by [`prepare`].
pub struct Prepared {
    pub version: VersionJson,
    pub java: PathBuf,
    pub classpath: Vec<PathBuf>,
}

/// Downloads whatever the instance is missing: version file, client jar,
/// libraries, assets and Java. Safe to run every time; it is fast when
/// nothing changed.
pub async fn prepare(client: &reqwest::Client, paths: &Paths, instance: &Instance, progress: &Option<ProgressSender>) -> Result<Prepared> {
    paths.ensure()?;
    let version_id = if instance.config.version_id.is_empty() {
        instance.config.minecraft.clone()
    } else {
        instance.config.version_id.clone()
    };
    report(progress, Progress::Phase(format!("Resolving {version_id}")));
    let version = meta::resolve(client, paths, &version_id).await?;

    // The client jar belongs to the vanilla version even for loader profiles.
    let vanilla_id = instance.config.minecraft.clone();
    let client_jar = paths.version_jar(&vanilla_id);
    if let Some(artifact) = version.downloads.as_ref().and_then(|d| d.client.as_ref()) {
        report(progress, Progress::Phase("Downloading the game".into()));
        fetch(
            client,
            &DownloadJob {
                url: artifact.url.clone(),
                target: client_jar.clone(),
                checksum: Checksum::Sha1(artifact.sha1.clone()),
                size: Some(artifact.size),
            },
        )
        .await?;
    }

    report(progress, Progress::Phase("Downloading libraries".into()));
    let mut classpath = Vec::new();
    let mut jobs = Vec::new();
    for library in version.libraries.iter().filter(|l| l.applies()) {
        if let Some(job) = library.download(paths) {
            classpath.push(job.target.clone());
            jobs.push(job);
        }
    }
    fetch_all(client, jobs, progress).await?;
    classpath.push(client_jar);

    assets::ensure(client, paths, &version, progress).await?;

    let component = version
        .java_version
        .as_ref()
        .map(|j| j.component.clone())
        .unwrap_or_else(|| "java-runtime-gamma".into());
    let java = java::ensure(client, paths, &component, progress).await?;

    Ok(Prepared {
        version,
        java,
        classpath,
    })
}

/// What to fill into the argument placeholders.
pub struct LaunchOptions<'a> {
    pub account: &'a Account,
    /// `host:port` to join straight away.
    pub server: Option<String>,
}

fn classpath_separator() -> &'static str {
    if cfg!(windows) {
        ";"
    } else {
        ":"
    }
}

/// Expands Mojang's `${placeholder}` arguments.
fn substitute(arg: &str, vars: &[(&str, String)]) -> String {
    let mut out = arg.to_owned();
    for (key, value) in vars {
        out = out.replace(&format!("${{{key}}}"), value);
    }
    out
}

fn collect_args(args: &[Argument], features: &[&str], vars: &[(&str, String)]) -> Vec<String> {
    let mut out = Vec::new();
    for arg in args {
        match arg {
            Argument::Plain(s) => out.push(substitute(s, vars)),
            Argument::Conditional { rules, value } => {
                if rules_allow(rules, features) {
                    match value {
                        ArgumentValue::One(s) => out.push(substitute(s, vars)),
                        ArgumentValue::Many(list) => out.extend(list.iter().map(|s| substitute(s, vars))),
                    }
                }
            }
        }
    }
    out
}

/// Builds the full command line for the game.
pub fn command(paths: &Paths, instance: &Instance, prepared: &Prepared, options: &LaunchOptions<'_>) -> Result<tokio::process::Command> {
    let game_dir = instance.game_dir();
    std::fs::create_dir_all(&game_dir)?;
    let natives = instance.dir.join("natives");
    std::fs::create_dir_all(&natives)?;

    let classpath = prepared
        .classpath
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(classpath_separator());
    let asset_index = prepared
        .version
        .asset_index
        .as_ref()
        .map(|a| a.id.clone())
        .or_else(|| prepared.version.assets.clone())
        .unwrap_or_default();

    let mut features: Vec<&str> = Vec::new();
    if instance.config.width.is_some() && instance.config.height.is_some() {
        features.push("has_custom_resolution");
    }
    if options.server.is_some() {
        features.push("is_quick_play_multiplayer");
    }

    let vars: Vec<(&str, String)> = vec![
        ("auth_player_name", options.account.name.clone()),
        ("version_name", prepared.version.id.clone()),
        ("game_directory", game_dir.to_string_lossy().to_string()),
        ("assets_root", paths.assets().to_string_lossy().to_string()),
        ("assets_index_name", asset_index),
        ("auth_uuid", options.account.uuid.simple().to_string()),
        ("auth_access_token", options.account.access_token.clone()),
        ("clientid", String::new()),
        ("auth_xuid", options.account.xuid.clone()),
        ("user_type", "msa".into()),
        ("version_type", prepared.version.kind.clone().unwrap_or_else(|| "release".into())),
        ("resolution_width", instance.config.width.map(|w| w.to_string()).unwrap_or_default()),
        ("resolution_height", instance.config.height.map(|h| h.to_string()).unwrap_or_default()),
        ("natives_directory", natives.to_string_lossy().to_string()),
        ("launcher_name", LAUNCHER_NAME.into()),
        ("launcher_version", env!("CARGO_PKG_VERSION").into()),
        ("classpath", classpath.clone()),
        ("classpath_separator", classpath_separator().into()),
        ("library_directory", paths.libraries().to_string_lossy().to_string()),
        ("quickPlayMultiplayer", options.server.clone().unwrap_or_default()),
        ("quickPlayPath", String::new()),
    ];

    let mut cmd = tokio::process::Command::new(&prepared.java);
    cmd.current_dir(&game_dir);
    cmd.arg(format!("-Xmx{}M", instance.config.memory_mb.max(512)));
    cmd.arg("-XX:+UseG1GC");
    cmd.arg("-Dfile.encoding=UTF-8");
    for arg in &instance.config.jvm_args {
        cmd.arg(arg);
    }
    let jvm_args = collect_args(&prepared.version.arguments.jvm, &features, &vars);
    if jvm_args.is_empty() {
        // Very old versions: no jvm arguments in the file.
        cmd.arg(format!("-Djava.library.path={}", natives.display()));
        cmd.arg("-cp").arg(&classpath);
    } else {
        cmd.args(jvm_args);
    }
    cmd.arg(&prepared.version.main_class);
    match &prepared.version.minecraft_arguments {
        Some(legacy) => {
            for arg in legacy.split_whitespace() {
                cmd.arg(substitute(arg, &vars));
            }
        }
        None => {
            cmd.args(collect_args(&prepared.version.arguments.game, &features, &vars));
        }
    }
    Ok(cmd)
}

/// Starts the game. Output is piped so the UI can show it; the CLI prints it.
pub async fn start(paths: &Paths, instance: &mut Instance, prepared: &Prepared, options: &LaunchOptions<'_>) -> Result<tokio::process::Child> {
    let mut cmd = command(paths, instance, prepared, options)?;
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());
    #[cfg(windows)]
    {
        // Do not pop up a console window for the game process.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let child = cmd.spawn().with_context(|| format!("starting Java at {}", prepared.java.display()))?;
    instance.config.last_played = Some(chrono::Utc::now().to_rfc3339());
    instance.save()?;
    Ok(child)
}
