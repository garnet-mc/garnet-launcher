//! Command-line front end for the Garnet launcher. Everything the desktop
//! app can do, scriptable.

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use garnet_launcher_core::auth::{self, AccountStore, Settings};
use garnet_launcher_core::instance::{self, InstanceConfig, Loader};
use garnet_launcher_core::{fabric, garnet, launch, meta, modrinth, Paths, Progress};
use tokio::io::{AsyncBufReadExt, BufReader};

#[derive(Parser)]
#[command(name = "garnet-launcher", version, about = "Launch Minecraft, join Garnet servers, manage mods")]
struct Cli {
    /// Launcher data directory (default: the platform data dir, or $GARNET_HOME).
    #[arg(long)]
    home: Option<std::path::PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Sign in with a Microsoft account.
    Login,
    /// Forget the active account.
    Logout,
    /// Show signed-in accounts.
    Accounts,
    /// Store the Microsoft client id used for sign-in.
    SetClientId { client_id: String },
    /// List Minecraft versions.
    Versions {
        /// Include snapshots.
        #[arg(long)]
        all: bool,
    },
    /// Manage instances.
    #[command(subcommand)]
    Instance(InstanceCommand),
    /// Search and install mods, shaders and resource packs.
    #[command(subcommand)]
    Mod(ModCommand),
    /// Ping a server and show what it wants from clients.
    Ping { address: String },
    /// Prepare an instance for a Garnet server (install its mods) without launching.
    Sync {
        address: String,
        #[arg(long)]
        optional: bool,
    },
    /// Join a Garnet server: installs its mods and launches straight in.
    Join {
        address: String,
        /// Also install the server's optional mods and shader pack.
        #[arg(long)]
        optional: bool,
    },
}

#[derive(Subcommand)]
enum InstanceCommand {
    List,
    Create {
        name: String,
        /// Minecraft version (default: latest release).
        #[arg(long)]
        minecraft: Option<String>,
        /// Install the Fabric loader (needed for mods).
        #[arg(long)]
        fabric: bool,
        #[arg(long, default_value_t = 4096)]
        memory: u32,
    },
    Delete { name: String },
    /// Download everything the instance needs without starting it.
    Prepare { name: String },
    Launch {
        name: String,
        /// Join this server right away (host or host:port).
        #[arg(long)]
        server: Option<String>,
    },
}

#[derive(Subcommand)]
enum ModCommand {
    Search {
        instance: String,
        query: String,
        /// mod, shader or resourcepack
        #[arg(long, default_value = "mod")]
        kind: String,
    },
    Install {
        instance: String,
        /// Modrinth project slug or id.
        project: String,
        #[arg(long, default_value = "")]
        version: String,
        #[arg(long, default_value = "mod")]
        kind: String,
    },
    List { instance: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,hyper=warn,reqwest=warn".into()))
        .with_target(false)
        .without_time()
        .init();
    let cli = Cli::parse();
    let paths = Paths::new(cli.home.unwrap_or_else(Paths::default_root));
    paths.ensure()?;
    let client = garnet_launcher_core::http();
    let settings = Settings::load(&paths.settings());
    let mut accounts = AccountStore::load(&paths);

    match cli.command {
        Command::Login => {
            let code = auth::start_device_login(&client, &settings.client_id).await?;
            println!("{}", code.message);
            println!("\n  Open {}  and enter the code  {}\n", code.verification_uri, code.user_code);
            let account = auth::finish_device_login(&client, &settings.client_id, &code).await?;
            println!("Signed in as {} ({})", account.name, account.uuid);
            accounts.upsert(account);
            accounts.save(&paths)?;
        }
        Command::Logout => {
            if let Some(active) = accounts.active {
                accounts.remove(active);
                accounts.save(&paths)?;
                println!("Signed out.");
            } else {
                println!("No account is signed in.");
            }
        }
        Command::Accounts => {
            if accounts.accounts.is_empty() {
                println!("No accounts. Run `garnet-launcher login`.");
            }
            for a in &accounts.accounts {
                let marker = if accounts.active == Some(a.uuid) { "*" } else { " " };
                println!("{marker} {} ({})", a.name, a.uuid);
            }
        }
        Command::SetClientId { client_id } => {
            let mut settings = settings;
            settings.client_id = client_id;
            settings.save(&paths.settings())?;
            println!("Saved.");
        }
        Command::Versions { all } => {
            let manifest = meta::manifest(&client, &paths).await?;
            println!("latest release: {}   latest snapshot: {}", manifest.latest.release, manifest.latest.snapshot);
            for v in manifest.versions.iter().filter(|v| all || v.kind == "release").take(if all { 40 } else { 20 }) {
                println!("{:<14} {:<9} {}", v.id, v.kind, &v.release_time[..10]);
            }
        }
        Command::Instance(cmd) => instance_command(cmd, &client, &paths, &settings, &mut accounts).await?,
        Command::Mod(cmd) => mod_command(cmd, &client, &paths).await?,
        Command::Ping { address } => {
            let (host, port) = garnet::parse_address(&address);
            let status = garnet::ping(&host, port).await?;
            println!("{host}:{port}  {}  {}/{} players  (protocol {})", status.version_name, status.online, status.max, status.protocol);
            println!("  {}", status.description);
            match status.garnet {
                Some(g) => {
                    println!("  Garnet server {} for Minecraft {}{}", g.server, g.minecraft, if g.voice { ", voice chat" } else { "" });
                    for m in &g.required_mods {
                        println!("  required: {} {} ({})", m.name, m.version, m.source);
                    }
                    for m in &g.optional_mods {
                        println!("  optional: {} {} ({})", m.name, m.version, m.source);
                    }
                    if let Some(s) = &g.shader_pack {
                        println!("  shaders:  {} {}", s.name, s.version);
                    }
                }
                None => println!("  (not a Garnet server)"),
            }
        }
        Command::Sync { address, optional } => {
            let (host, port) = garnet::parse_address(&address);
            let status = garnet::ping(&host, port).await?;
            let manifest = status.garnet.unwrap_or_default();
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            tokio::spawn(print_progress(rx));
            let progress = Some(tx);
            let instance = garnet::instance_for_server(&client, &paths, &host, port, &manifest, &progress).await?;
            let installed = garnet::sync_mods(&client, &instance, &manifest, optional, &progress).await?;
            println!("Instance '{}' ready for {host}:{port}; installed: {}", instance.config.name, if installed.is_empty() { "nothing new".into() } else { installed.join(", ") });
            garnet::ensure_client_mod(&client, &instance).await?;
            launch::prepare(&client, &paths, &instance, &progress).await?;
            println!("Everything downloaded. `garnet-launcher join {host}:{port}` will start the game.");
        }
        Command::Join { address, optional } => {
            let account = auth::ensure_fresh(&client, &paths, &mut accounts).await?;
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            tokio::spawn(print_progress(rx));
            let (instance, child) = garnet::join(&client, &paths, &account, &address, optional, &Some(tx)).await?;
            println!("Game started (instance '{}'); output follows.", instance.config.name);
            pump(child).await?;
        }
    }
    Ok(())
}

async fn instance_command(cmd: InstanceCommand, client: &reqwest::Client, paths: &Paths, settings: &Settings, accounts: &mut AccountStore) -> Result<()> {
    match cmd {
        InstanceCommand::List => {
            let list = instance::list(paths)?;
            if list.is_empty() {
                println!("No instances. Create one with `garnet-launcher instance create <name>`.");
            }
            for i in list {
                println!(
                    "{:<24} {:<8} {:<8} {}",
                    i.config.name,
                    i.config.minecraft,
                    match i.config.loader {
                        Loader::Vanilla => "vanilla",
                        Loader::Fabric => "fabric",
                    },
                    i.config.server.clone().unwrap_or_default()
                );
            }
        }
        InstanceCommand::Create { name, minecraft, fabric: use_fabric, memory } => {
            let minecraft = match minecraft {
                Some(v) => v,
                None => meta::manifest(client, paths).await?.latest.release,
            };
            let mut config = InstanceConfig {
                name: name.clone(),
                minecraft: minecraft.clone(),
                loader: if use_fabric { Loader::Fabric } else { Loader::Vanilla },
                memory_mb: memory,
                jvm_args: settings.jvm_args.clone(),
                ..Default::default()
            };
            if use_fabric {
                config.version_id = fabric::install(client, paths, &minecraft, "").await?;
            }
            let instance = instance::create(paths, config)?;
            println!("Created '{}' for Minecraft {} at {}", instance.config.name, minecraft, instance.dir.display());
        }
        InstanceCommand::Delete { name } => {
            instance::delete(paths, &name)?;
            println!("Deleted '{name}'.");
        }
        InstanceCommand::Prepare { name } => {
            let instance = instance::get(paths, &name)?;
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            tokio::spawn(print_progress(rx));
            launch::prepare(client, paths, &instance, &Some(tx)).await?;
            println!("'{name}' is ready.");
        }
        InstanceCommand::Launch { name, server } => {
            let mut instance = instance::get(paths, &name)?;
            let account = auth::ensure_fresh(client, paths, accounts).await?;
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            tokio::spawn(print_progress(rx));
            garnet::ensure_client_mod(client, &instance).await?;
            let prepared = launch::prepare(client, paths, &instance, &Some(tx)).await?;
            let server = server.map(|s| {
                let (h, p) = garnet::parse_address(&s);
                format!("{h}:{p}")
            });
            let child = launch::start(paths, &mut instance, &prepared, &launch::LaunchOptions { account: &account, server }).await?;
            println!("Game started; output follows.");
            pump(child).await?;
        }
    }
    Ok(())
}

async fn mod_command(cmd: ModCommand, client: &reqwest::Client, paths: &Paths) -> Result<()> {
    match cmd {
        ModCommand::Search { instance, query, kind } => {
            let instance = instance::get(paths, &instance)?;
            let loader = match instance.config.loader {
                Loader::Fabric => "fabric",
                Loader::Vanilla => "",
            };
            let hits = modrinth::search(client, &query, &kind, &instance.config.minecraft, loader).await?;
            for hit in hits {
                println!("{:<28} {:>9} downloads  {}", hit.slug, hit.downloads, hit.description.chars().take(70).collect::<String>());
            }
        }
        ModCommand::Install { instance, project, version, kind } => {
            let instance = instance::get(paths, &instance)?;
            if kind == "mod" && !matches!(instance.config.loader, Loader::Fabric) {
                bail!("'{}' is a vanilla instance; create it with --fabric to use mods", instance.config.name);
            }
            let loader = if kind == "mod" { Some("fabric") } else { None };
            let picked = modrinth::pick_version(client, &project, &instance.config.minecraft, loader, &version).await?;
            let path = modrinth::install(client, &instance, &picked, &kind).await?;
            println!("Installed {} {} -> {}", picked.name, picked.version_number, path.display());
            for dep in picked.dependencies.iter().filter(|d| d.dependency_type == "required") {
                if let Some(id) = &dep.project_id {
                    println!("  requires {id}; installing");
                    let dep_version = modrinth::pick_version(client, id, &instance.config.minecraft, loader, "").await?;
                    modrinth::install(client, &instance, &dep_version, &kind).await?;
                }
            }
        }
        ModCommand::List { instance } => {
            let instance = instance::get(paths, &instance)?;
            for m in instance.installed_mods() {
                println!("{:<28} {:<14} {:<9} {}", m.id, m.version, m.source, m.file);
            }
        }
    }
    Ok(())
}

async fn print_progress(mut rx: tokio::sync::mpsc::UnboundedReceiver<Progress>) {
    let mut last_percent = 101;
    while let Some(p) = rx.recv().await {
        match p {
            Progress::Phase(name) => {
                println!("== {name}");
                last_percent = 101;
            }
            Progress::Items { done, total } => {
                let percent = if total == 0 { 100 } else { done * 100 / total };
                if percent / 10 != last_percent / 10 || done == total {
                    println!("   {done}/{total} ({percent}%)");
                    last_percent = percent;
                }
            }
            Progress::Message(m) => println!("   {m}"),
        }
    }
}

/// Copies the game's output to ours until it exits.
async fn pump(mut child: tokio::process::Child) -> Result<()> {
    let stdout = child.stdout.take().context("no stdout")?;
    let stderr = child.stderr.take().context("no stderr")?;
    let out = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            println!("{line}");
        }
    });
    let err = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            eprintln!("{line}");
        }
    });
    let status = child.wait().await?;
    let _ = out.await;
    let _ = err.await;
    println!("Game exited with {status}");
    Ok(())
}
