//! The Garnet launcher, minus the user interface.
//!
//! - `paths`     – where everything lives on disk
//! - `download`  – verified, parallel downloads with progress
//! - `meta`      – Mojang's version manifest and version files
//! - `assets`    – game assets (sounds, languages, textures index)
//! - `java`      – Mojang's Java runtimes
//! - `auth`      – Microsoft account login
//! - `instance`  – isolated game installs with their own mods and settings
//! - `fabric`    – installing the Fabric loader (the base of Garnet Loader)
//! - `modrinth`  – finding and downloading mods and shader packs
//! - `launch`    – building the command line and starting the game
//! - `garnet`    – talking to Garnet servers: required mods, quick join

pub mod assets;
pub mod auth;
pub mod download;
pub mod fabric;
pub mod garnet;
pub mod instance;
pub mod java;
pub mod launch;
pub mod meta;
pub mod modrinth;
pub mod paths;

pub use paths::Paths;

/// User agent for every HTTP request; Modrinth requires one that names the app.
pub const USER_AGENT: &str = concat!("garnet-launcher/", env!("CARGO_PKG_VERSION"), " (github.com/garnet-mc/garnet-launcher)");

pub fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .expect("http client")
}

/// Progress messages for a long-running job, for a progress bar or log.
#[derive(Clone, Debug)]
pub enum Progress {
    /// A new phase started (e.g. "Downloading assets").
    Phase(String),
    /// `done` of `total` items in the current phase.
    Items { done: usize, total: usize },
    /// Free text worth showing.
    Message(String),
}

pub type ProgressSender = tokio::sync::mpsc::UnboundedSender<Progress>;

/// Report progress if anyone is listening.
pub fn report(tx: &Option<ProgressSender>, p: Progress) {
    if let Some(tx) = tx {
        let _ = tx.send(p);
    }
}
