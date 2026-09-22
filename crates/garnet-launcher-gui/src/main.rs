//! The desktop app: a thin Tauri shell around `garnet-launcher-core`. The UI
//! in `ui/` calls the commands below and listens for `progress`, `game-log`
//! and `login` events.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use garnet_launcher_core::auth::{self, AccountStore, DeviceCode, Settings};
use garnet_launcher_core::instance::{self, InstanceConfig, Loader};
use garnet_launcher_core::{fabric, garnet, launch, meta, modrinth, Paths, Progress};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncBufReadExt, BufReader};

struct App {
    paths: Paths,
    client: reqwest::Client,
    accounts: Mutex<AccountStore>,
    settings: Mutex<Settings>,
    pending_login: Mutex<Option<DeviceCode>>,
    /// Set while a game is running so the UI can show it.
    running: Mutex<Option<String>>,
}

type Shared<'a> = State<'a, Arc<App>>;

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

#[derive(Serialize)]
struct AccountView {
    name: String,
    uuid: String,
}

#[derive(Serialize)]
struct InstanceView {
    name: String,
    minecraft: String,
    loader: String,
    server: Option<String>,
    memory_mb: u32,
    last_played: Option<String>,
    mods: usize,
    dir: String,
}

fn instance_view(i: &instance::Instance) -> InstanceView {
    InstanceView {
        name: i.config.name.clone(),
        minecraft: i.config.minecraft.clone(),
        loader: match i.config.loader {
            Loader::Vanilla => "vanilla".into(),
            Loader::Fabric => "fabric".into(),
        },
        server: i.config.server.clone(),
        memory_mb: i.config.memory_mb,
        last_played: i.config.last_played.clone(),
        mods: i.installed_mods().len(),
        dir: i.dir.to_string_lossy().to_string(),
    }
}

#[derive(Serialize)]
struct StateView {
    account: Option<AccountView>,
    accounts: Vec<AccountView>,
    settings: Settings,
    instances: Vec<InstanceView>,
    running: Option<String>,
    home: String,
    version: String,
}

#[tauri::command]
fn get_state(app: Shared<'_>) -> Result<StateView, String> {
    let accounts = app.accounts.lock().unwrap();
    Ok(StateView {
        account: accounts.active().map(|a| AccountView {
            name: a.name.clone(),
            uuid: a.uuid.to_string(),
        }),
        accounts: accounts
            .accounts
            .iter()
            .map(|a| AccountView {
                name: a.name.clone(),
                uuid: a.uuid.to_string(),
            })
            .collect(),
        settings: app.settings.lock().unwrap().clone(),
        instances: instance::list(&app.paths).map_err(err)?.iter().map(instance_view).collect(),
        running: app.running.lock().unwrap().clone(),
        home: app.paths.root.to_string_lossy().to_string(),
        version: env!("CARGO_PKG_VERSION").into(),
    })
}

#[tauri::command]
fn save_settings(app: Shared<'_>, settings: Settings) -> Result<(), String> {
    settings.save(&app.paths.settings()).map_err(err)?;
    *app.settings.lock().unwrap() = settings;
    Ok(())
}

#[derive(Serialize)]
struct VersionView {
    id: String,
    kind: String,
    date: String,
}

#[tauri::command]
async fn list_versions(app: Shared<'_>) -> Result<Vec<VersionView>, String> {
    let manifest = meta::manifest(&app.client, &app.paths).await.map_err(err)?;
    Ok(manifest
        .versions
        .iter()
        .filter(|v| v.kind == "release")
        .take(40)
        .map(|v| VersionView {
            id: v.id.clone(),
            kind: v.kind.clone(),
            date: v.release_time[..10].to_owned(),
        })
        .collect())
}

#[derive(Deserialize)]
struct NewInstance {
    name: String,
    minecraft: String,
    fabric: bool,
    memory_mb: u32,
}

#[tauri::command]
async fn create_instance(app: Shared<'_>, new: NewInstance) -> Result<InstanceView, String> {
    let mut config = InstanceConfig {
        name: new.name,
        minecraft: new.minecraft.clone(),
        loader: if new.fabric { Loader::Fabric } else { Loader::Vanilla },
        memory_mb: new.memory_mb,
        jvm_args: app.settings.lock().unwrap().jvm_args.clone(),
        ..Default::default()
    };
    if new.fabric {
        config.version_id = fabric::install(&app.client, &app.paths, &new.minecraft, "").await.map_err(err)?;
    }
    let created = instance::create(&app.paths, config).map_err(err)?;
    Ok(instance_view(&created))
}

#[tauri::command]
fn delete_instance(app: Shared<'_>, name: String) -> Result<(), String> {
    instance::delete(&app.paths, &name).map_err(err)
}

#[tauri::command]
fn open_instance_folder(app: Shared<'_>, handle: AppHandle, name: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let inst = instance::get(&app.paths, &name).map_err(err)?;
    handle.opener().open_path(inst.game_dir().to_string_lossy().to_string(), None::<&str>).map_err(err)
}

/// Forwards core progress to the UI as `progress` events.
fn progress_channel(handle: AppHandle) -> garnet_launcher_core::ProgressSender {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Progress>();
    tauri::async_runtime::spawn(async move {
        let mut phase = String::new();
        while let Some(p) = rx.recv().await {
            let payload = match p {
                Progress::Phase(name) => {
                    phase = name.clone();
                    serde_json::json!({ "phase": name, "done": 0, "total": 0 })
                }
                Progress::Items { done, total } => serde_json::json!({ "phase": phase, "done": done, "total": total }),
                Progress::Message(m) => serde_json::json!({ "phase": phase, "message": m }),
            };
            let _ = handle.emit("progress", payload);
        }
    });
    tx
}

/// Streams the game's output to the UI and reports when it exits.
fn watch_game(app: Arc<App>, handle: AppHandle, name: String, mut child: tokio::process::Child) {
    *app.running.lock().unwrap() = Some(name.clone());
    tauri::async_runtime::spawn(async move {
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let h1 = handle.clone();
        let out = tauri::async_runtime::spawn(async move {
            if let Some(stdout) = stdout {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let _ = h1.emit("game-log", line);
                }
            }
        });
        let h2 = handle.clone();
        let errs = tauri::async_runtime::spawn(async move {
            if let Some(stderr) = stderr {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let _ = h2.emit("game-log", line);
                }
            }
        });
        let status = child.wait().await.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
        let _ = out.await;
        let _ = errs.await;
        *app.running.lock().unwrap() = None;
        let _ = handle.emit("game-exit", serde_json::json!({ "instance": name, "code": status }));
    });
}

async fn fresh_account(app: &App) -> Result<auth::Account, String> {
    let mut store = app.accounts.lock().unwrap().clone();
    let account = auth::ensure_fresh(&app.client, &app.paths, &mut store).await.map_err(err)?;
    *app.accounts.lock().unwrap() = store;
    Ok(account)
}

#[tauri::command]
async fn launch_instance(app: Shared<'_>, handle: AppHandle, name: String, server: Option<String>) -> Result<(), String> {
    let account = fresh_account(&app).await?;
    let mut inst = instance::get(&app.paths, &name).map_err(err)?;
    let progress = Some(progress_channel(handle.clone()));
    garnet::ensure_client_mod(&app.client, &inst).await.map_err(err)?;
    let prepared = launch::prepare(&app.client, &app.paths, &inst, &progress).await.map_err(err)?;
    let _ = handle.emit("progress", serde_json::json!({ "phase": "Starting the game", "done": 0, "total": 0 }));
    let server = server.filter(|s| !s.trim().is_empty()).map(|s| {
        let (h, p) = garnet::parse_address(&s);
        format!("{h}:{p}")
    });
    let child = launch::start(&app.paths, &mut inst, &prepared, &launch::LaunchOptions { account: &account, server })
        .await
        .map_err(err)?;
    watch_game(Arc::clone(&app), handle, name, child);
    Ok(())
}

#[derive(Serialize)]
struct PingView {
    host: String,
    port: u16,
    description: String,
    online: i64,
    max: i64,
    version: String,
    garnet: Option<serde_json::Value>,
}

#[tauri::command]
async fn ping_server(address: String) -> Result<PingView, String> {
    let (host, port) = garnet::parse_address(&address);
    let status = garnet::ping(&host, port).await.map_err(err)?;
    Ok(PingView {
        host,
        port,
        description: status.description,
        online: status.online,
        max: status.max,
        version: status.version_name,
        garnet: status.garnet.map(|g| {
            serde_json::json!({
                "minecraft": g.minecraft,
                "voice": g.voice,
                "enforce": g.enforce,
                "required": g.required_mods.iter().map(|m| serde_json::json!({"id": m.id, "name": m.name, "version": m.version, "source": m.source})).collect::<Vec<_>>(),
                "optional": g.optional_mods.iter().map(|m| serde_json::json!({"id": m.id, "name": m.name, "version": m.version, "source": m.source})).collect::<Vec<_>>(),
                "shader_pack": g.shader_pack.as_ref().map(|s| serde_json::json!({"id": s.id, "name": s.name})),
            })
        }),
    })
}

#[tauri::command]
async fn join_server(app: Shared<'_>, handle: AppHandle, address: String, optional: bool) -> Result<String, String> {
    let account = fresh_account(&app).await?;
    let progress = Some(progress_channel(handle.clone()));
    let (inst, child) = garnet::join(&app.client, &app.paths, &account, &address, optional, &progress)
        .await
        .map_err(err)?;
    let name = inst.config.name.clone();
    watch_game(Arc::clone(&app), handle, name.clone(), child);
    Ok(name)
}

#[tauri::command]
async fn search_mods(app: Shared<'_>, instance: String, query: String, kind: String) -> Result<Vec<modrinth::SearchHit>, String> {
    let inst = instance::get(&app.paths, &instance).map_err(err)?;
    let loader = match inst.config.loader {
        Loader::Fabric => "fabric",
        Loader::Vanilla => "",
    };
    modrinth::search(&app.client, &query, &kind, &inst.config.minecraft, loader).await.map_err(err)
}

#[tauri::command]
async fn install_mod(app: Shared<'_>, instance: String, project: String, kind: String) -> Result<String, String> {
    let inst = instance::get(&app.paths, &instance).map_err(err)?;
    let loader = if kind == "mod" { Some("fabric") } else { None };
    let version = modrinth::pick_version(&app.client, &project, &inst.config.minecraft, loader, "").await.map_err(err)?;
    let path = modrinth::install(&app.client, &inst, &version, &kind).await.map_err(err)?;
    for dep in version.dependencies.iter().filter(|d| d.dependency_type == "required") {
        if let Some(id) = &dep.project_id {
            if let Ok(v) = modrinth::pick_version(&app.client, id, &inst.config.minecraft, loader, "").await {
                let _ = modrinth::install(&app.client, &inst, &v, &kind).await;
            }
        }
    }
    Ok(path.file_name().unwrap().to_string_lossy().to_string())
}

#[tauri::command]
fn list_mods(app: Shared<'_>, instance: String) -> Result<Vec<instance::InstalledMod>, String> {
    let inst = instance::get(&app.paths, &instance).map_err(err)?;
    Ok(inst.installed_mods())
}

#[tauri::command]
fn remove_mod(app: Shared<'_>, instance: String, file: String) -> Result<(), String> {
    let inst = instance::get(&app.paths, &instance).map_err(err)?;
    let path = inst.mods_dir().join(&file);
    if path.parent() != Some(inst.mods_dir().as_path()) {
        return Err("bad file name".into());
    }
    std::fs::remove_file(&path).map_err(err)?;
    let _ = std::fs::remove_file(path.with_extension("garnet.json"));
    Ok(())
}

#[derive(Serialize)]
struct LoginStartView {
    user_code: String,
    verification_uri: String,
}

/// Starts the Microsoft sign-in and keeps polling in the background; the
/// UI hears `login` events with `{ok, name}` or `{error}`.
#[tauri::command]
async fn login_start(app: Shared<'_>, handle: AppHandle) -> Result<LoginStartView, String> {
    let client_id = app.settings.lock().unwrap().client_id.clone();
    let code = auth::start_device_login(&app.client, &client_id).await.map_err(err)?;
    let view = LoginStartView {
        user_code: code.user_code.clone(),
        verification_uri: code.verification_uri.clone(),
    };
    *app.pending_login.lock().unwrap() = Some(DeviceCode {
        device_code: code.device_code.clone(),
        user_code: code.user_code.clone(),
        verification_uri: code.verification_uri.clone(),
        expires_in: code.expires_in,
        interval: code.interval,
        message: code.message.clone(),
    });
    let app = Arc::clone(&app);
    tauri::async_runtime::spawn(async move {
        match auth::finish_device_login(&app.client, &client_id, &code).await {
            Ok(account) => {
                let name = account.name.clone();
                let mut store = app.accounts.lock().unwrap();
                store.upsert(account);
                let _ = store.save(&app.paths);
                let _ = handle.emit("login", serde_json::json!({ "ok": true, "name": name }));
            }
            Err(e) => {
                let _ = handle.emit("login", serde_json::json!({ "ok": false, "error": format!("{e:#}") }));
            }
        }
        *app.pending_login.lock().unwrap() = None;
    });
    Ok(view)
}

#[tauri::command]
fn logout(app: Shared<'_>) -> Result<(), String> {
    let mut store = app.accounts.lock().unwrap();
    if let Some(active) = store.active {
        store.remove(active);
        store.save(&app.paths).map_err(err)?;
    }
    Ok(())
}

#[tauri::command]
fn switch_account(app: Shared<'_>, uuid: String) -> Result<(), String> {
    let mut store = app.accounts.lock().unwrap();
    let id = uuid::Uuid::parse_str(&uuid).map_err(err)?;
    if store.accounts.iter().any(|a| a.uuid == id) {
        store.active = Some(id);
        store.save(&app.paths).map_err(err)?;
    }
    Ok(())
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,hyper=warn,reqwest=warn".into()))
        .init();
    let paths = Paths::new(Paths::default_root());
    let _ = paths.ensure();
    let state = Arc::new(App {
        client: garnet_launcher_core::http(),
        accounts: Mutex::new(AccountStore::load(&paths)),
        settings: Mutex::new(Settings::load(&paths.settings())),
        pending_login: Mutex::new(None),
        running: Mutex::new(None),
        paths,
    });
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            get_state,
            save_settings,
            list_versions,
            create_instance,
            delete_instance,
            open_instance_folder,
            launch_instance,
            ping_server,
            join_server,
            search_mods,
            install_mod,
            list_mods,
            remove_mod,
            login_start,
            logout,
            switch_account,
        ])
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title("Garnet");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the launcher");
}
