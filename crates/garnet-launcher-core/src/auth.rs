//! Microsoft account login, the only kind Minecraft supports.
//!
//! The flow is the standard one every third-party launcher uses:
//! Microsoft device code -> Xbox Live -> XSTS -> Minecraft services ->
//! profile. Tokens are cached in `accounts.json` and refreshed silently.
//!
//! The launcher needs its own Azure application id (`client_id`) that
//! Mojang has approved for the Minecraft API; see the README for how to get
//! one. There is deliberately no "offline account" option.

use crate::paths::Paths;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;
use uuid::Uuid;

const SCOPE: &str = "XboxLive.signin offline_access";
const DEVICE_CODE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub uuid: Uuid,
    pub name: String,
    pub xuid: String,
    pub access_token: String,
    pub expires_at: DateTime<Utc>,
    pub refresh_token: String,
    /// Client id the tokens were issued for.
    pub client_id: String,
}

impl Account {
    pub fn is_expired(&self) -> bool {
        self.expires_at - Duration::minutes(5) < Utc::now()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AccountStore {
    pub accounts: Vec<Account>,
    pub active: Option<Uuid>,
}

impl AccountStore {
    pub fn load(paths: &Paths) -> Self {
        std::fs::read_to_string(paths.accounts())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        let path = paths.accounts();
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn active(&self) -> Option<&Account> {
        let id = self.active?;
        self.accounts.iter().find(|a| a.uuid == id)
    }

    pub fn upsert(&mut self, account: Account) {
        self.accounts.retain(|a| a.uuid != account.uuid);
        self.active = Some(account.uuid);
        self.accounts.push(account);
    }

    pub fn remove(&mut self, uuid: Uuid) {
        self.accounts.retain(|a| a.uuid != uuid);
        if self.active == Some(uuid) {
            self.active = self.accounts.first().map(|a| a.uuid);
        }
    }
}

/// Step one of a device-code login: show the user a code and a URL.
#[derive(Debug, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
    pub message: String,
}

pub async fn start_device_login(client: &reqwest::Client, client_id: &str) -> Result<DeviceCode> {
    if client_id.is_empty() {
        bail!("no Microsoft client id configured; set `client_id` in settings.toml (see the README)");
    }
    let response = client
        .post(DEVICE_CODE_URL)
        .form(&[("client_id", client_id), ("scope", SCOPE)])
        .send()
        .await?
        .error_for_status()
        .context("requesting a device code")?;
    Ok(response.json().await?)
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    #[serde(default)]
    error: Option<String>,
}

/// Polls until the user finishes signing in in their browser, then runs the
/// rest of the chain. Returns the logged-in account.
pub async fn finish_device_login(client: &reqwest::Client, client_id: &str, code: &DeviceCode) -> Result<Account> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(code.expires_in);
    let interval = std::time::Duration::from_secs(code.interval.max(1));
    loop {
        tokio::time::sleep(interval).await;
        if std::time::Instant::now() > deadline {
            bail!("the sign-in code expired; try again");
        }
        let response = client
            .post(TOKEN_URL)
            .form(&[
                ("client_id", client_id),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", &code.device_code),
            ])
            .send()
            .await?;
        let body: serde_json::Value = response.json().await?;
        match body.get("error").and_then(|e| e.as_str()) {
            Some("authorization_pending") | Some("slow_down") => continue,
            Some("authorization_declined") => bail!("sign-in was declined"),
            Some("expired_token") => bail!("the sign-in code expired; try again"),
            Some(other) => bail!("sign-in failed: {other}"),
            None => {
                let tokens: TokenResponse = serde_json::from_value(body)?;
                return xbox_chain(client, client_id, tokens).await;
            }
        }
    }
}

/// Refreshes an account's Minecraft token using the Microsoft refresh token.
pub async fn refresh(client: &reqwest::Client, account: &Account) -> Result<Account> {
    let response = client
        .post(TOKEN_URL)
        .form(&[
            ("client_id", account.client_id.as_str()),
            ("grant_type", "refresh_token"),
            ("refresh_token", account.refresh_token.as_str()),
            ("scope", SCOPE),
        ])
        .send()
        .await?;
    let body: serde_json::Value = response.json().await?;
    if let Some(err) = body.get("error").and_then(|e| e.as_str()) {
        bail!("could not refresh the sign-in ({err}); please log in again");
    }
    let tokens: TokenResponse = serde_json::from_value(body)?;
    xbox_chain(client, &account.client_id, tokens).await
}

/// Microsoft token -> Xbox Live -> XSTS -> Minecraft -> profile.
async fn xbox_chain(client: &reqwest::Client, client_id: &str, tokens: TokenResponse) -> Result<Account> {
    if let Some(err) = tokens.error {
        bail!("sign-in failed: {err}");
    }
    let xbl: serde_json::Value = client
        .post(XBL_URL)
        .json(&json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={}", tokens.access_token),
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
        }))
        .send()
        .await?
        .error_for_status()
        .context("Xbox Live sign-in")?
        .json()
        .await?;
    let xbl_token = xbl["Token"].as_str().context("Xbox Live response without token")?.to_owned();
    let uhs = xbl["DisplayClaims"]["xui"][0]["uhs"]
        .as_str()
        .context("Xbox Live response without user hash")?
        .to_owned();

    let xsts_response = client
        .post(XSTS_URL)
        .json(&json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT",
        }))
        .send()
        .await?;
    if xsts_response.status() == reqwest::StatusCode::UNAUTHORIZED {
        let body: serde_json::Value = xsts_response.json().await.unwrap_or_default();
        let reason = match body["XErr"].as_u64() {
            Some(2148916233) => "this Microsoft account has no Xbox profile; create one at xbox.com first",
            Some(2148916238) => "this account belongs to a child and needs to be added to a family by an adult",
            _ => "Xbox denied the sign-in",
        };
        bail!("{reason}");
    }
    let xsts: serde_json::Value = xsts_response.error_for_status().context("XSTS")?.json().await?;
    let xsts_token = xsts["Token"].as_str().context("XSTS response without token")?.to_owned();

    let mc: serde_json::Value = client
        .post(MC_LOGIN_URL)
        .json(&json!({ "identityToken": format!("XBL3.0 x={uhs};{xsts_token}") }))
        .send()
        .await?
        .error_for_status()
        .context("Minecraft services sign-in")?
        .json()
        .await?;
    let access_token = mc["access_token"].as_str().context("no Minecraft token")?.to_owned();
    let expires_in = mc["expires_in"].as_i64().unwrap_or(86400);

    let profile_response = client.get(MC_PROFILE_URL).bearer_auth(&access_token).send().await?;
    if profile_response.status() == reqwest::StatusCode::NOT_FOUND {
        bail!("this Microsoft account does not own Minecraft: Java Edition");
    }
    let profile: serde_json::Value = profile_response.error_for_status().context("fetching the profile")?.json().await?;
    let uuid = Uuid::parse_str(profile["id"].as_str().context("profile without id")?)?;
    let name = profile["name"].as_str().context("profile without name")?.to_owned();

    // The XUID is inside the Minecraft JWT; the launcher passes it to the game.
    let xuid = xuid_from_token(&access_token).unwrap_or_default();

    Ok(Account {
        uuid,
        name,
        xuid,
        access_token,
        expires_at: Utc::now() + Duration::seconds(expires_in),
        refresh_token: tokens.refresh_token,
        client_id: client_id.to_owned(),
    })
}

fn xuid_from_token(jwt: &str) -> Option<String> {
    let payload = jwt.split('.').nth(1)?;
    let bytes = base64_url_decode(payload)?;
    let claims: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    claims["xuid"].as_str().map(str::to_owned)
}

fn base64_url_decode(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;
    for c in input.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            b'=' => break,
            _ => return None,
        } as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

/// Returns a usable account, refreshing the token if it is about to expire.
pub async fn ensure_fresh(client: &reqwest::Client, paths: &Paths, store: &mut AccountStore) -> Result<Account> {
    let account = store.active().cloned().context("no account is logged in; run `login` first")?;
    if !account.is_expired() {
        return Ok(account);
    }
    let refreshed = refresh(client, &account).await?;
    store.upsert(refreshed.clone());
    store.save(paths)?;
    Ok(refreshed)
}

/// Launcher settings that live next to the accounts file.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Azure application id used for Microsoft sign-in.
    pub client_id: String,
    /// Default memory for new instances, in MB.
    pub default_memory_mb: u32,
    /// Extra JVM arguments for every instance.
    pub jvm_args: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            default_memory_mb: 4096,
            jvm_args: Vec::new(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}
