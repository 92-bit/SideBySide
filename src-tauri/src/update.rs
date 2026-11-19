//! Checking GitHub for a newer SideBySide, and installing it.
//!
//! Each release on GitHub has a small "latest.json" that says the newest version and where to
//! download it. Every download is signed with the 92-bit update key; SideBySide checks that
//! signature before using the file, so nobody can slip in a fake update.

use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use base64::Engine;
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};

const LATEST_URL: &str = "https://github.com/92-bit/SideBySide/releases/latest/download/latest.json";
/// The public half of the update key. Safe to share; only the private half can sign.
const PUBLIC_KEY: &str = include_str!("../update-key.pub");
const MAX_DOWNLOAD: u64 = 200 << 20;

#[derive(Deserialize)]
struct Latest {
    version: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    pub_date: String,
    #[serde(default)]
    platforms: HashMap<String, Platform>,
}

#[derive(Deserialize)]
struct Platform {
    url: String,
    signature: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub available: bool,
    pub current: String,
    pub latest: String,
    pub notes: String,
    pub date: String,
    /// SideBySide can replace itself on this computer (otherwise: open the download page).
    pub can_install: bool,
}

/// Where to look for updates. Can be changed for testing with SIDEBYSIDE_UPDATE_URL.
fn source_url() -> String {
    std::env::var("SIDEBYSIDE_UPDATE_URL").unwrap_or_else(|_| LATEST_URL.into())
}

fn platform_key() -> &'static str {
    if cfg!(windows) {
        "windows-x86_64"
    } else if cfg!(target_os = "linux") {
        "linux-x86_64"
    } else {
        "other"
    }
}

/// "v1.2.3" → (1, 2, 3). Anything after "-" or "+" is ignored.
pub fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.trim().trim_start_matches('v').split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let major = parts.next()??;
    let minor = parts.next().flatten().unwrap_or(0);
    let patch = parts.next().flatten().unwrap_or(0);
    Some((major, minor, patch))
}

fn is_newer(latest: &str, current: &str) -> bool {
    matches!((parse_version(latest), parse_version(current)), (Some(a), Some(b)) if a > b)
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .user_agent("SideBySide-updater")
        .build()
}

/// None when nothing has been published yet.
fn fetch_latest() -> Result<Option<Latest>, String> {
    match agent().get(&source_url()).call() {
        Ok(resp) => {
            let body = resp.into_string().map_err(|e| format!("Couldn't read the update information: {e}"))?;
            serde_json::from_str(&body).map(Some).map_err(|e| format!("The update information looks wrong: {e}"))
        }
        Err(ureq::Error::Status(404, _)) => Ok(None),
        Err(e) => Err(format!("Couldn't reach GitHub to check for updates. Are you online? ({e})")),
    }
}

pub fn check(current: &str) -> Result<UpdateInfo, String> {
    let latest = fetch_latest()?;
    Ok(match latest {
        Some(l) => UpdateInfo {
            available: is_newer(&l.version, current),
            current: current.into(),
            can_install: cfg!(windows) && l.platforms.contains_key(platform_key()),
            latest: l.version,
            notes: l.notes,
            date: l.pub_date,
        },
        None => UpdateInfo {
            available: false,
            current: current.into(),
            latest: current.into(),
            notes: String::new(),
            date: String::new(),
            can_install: false,
        },
    })
}

/// Check that `data` was signed with the 92-bit update key.
pub fn verify(data: &[u8], signature_b64: &str) -> Result<(), String> {
    let b64 = base64::engine::general_purpose::STANDARD;
    fn bad<E>(_: E) -> String {
        "The update's signature is broken, so it was not installed.".to_string()
    }
    let pk_text = String::from_utf8(b64.decode(PUBLIC_KEY.trim()).map_err(|_| "Built-in update key is broken".to_string())?)
        .map_err(|_| "Built-in update key is broken".to_string())?;
    let pk = PublicKey::decode(&pk_text).map_err(|_| "Built-in update key is broken".to_string())?;
    let sig_text = String::from_utf8(b64.decode(signature_b64.trim()).map_err(bad)?).map_err(bad)?;
    let sig = Signature::decode(&sig_text).map_err(bad)?;
    pk.verify(data, &sig, true)
        .map_err(|_| "The download didn't match its signature, so it was not installed. Please try again later.".to_string())
}

/// Download the newest version, check its signature, and put it in place of this program.
/// Returns the path to start afterwards. The old copy is kept as "SideBySide.old.exe"
/// until the next start (a running program can't delete itself).
pub fn download_and_replace(current: &str) -> Result<PathBuf, String> {
    let latest = fetch_latest()?.ok_or("There is no published version yet.")?;
    if !is_newer(&latest.version, current) {
        return Err("You already have the newest version.".into());
    }
    let p = latest
        .platforms
        .get(platform_key())
        .ok_or("There's no automatic update for this computer. Please download it from the website.")?;

    let mut bytes = Vec::new();
    agent()
        .get(&p.url)
        .call()
        .map_err(|e| format!("The download failed: {e}"))?
        .into_reader()
        .take(MAX_DOWNLOAD)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("The download was interrupted: {e}"))?;
    verify(&bytes, &p.signature)?;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("Couldn't find SideBySide's folder")?;
    let fresh = dir.join("SideBySide.update.exe");
    let old = dir.join("SideBySide.old.exe");
    std::fs::write(&fresh, &bytes).map_err(|e| {
        format!("Couldn't save the update next to SideBySide ({e}). If it's in a protected folder, move it to your Documents and try again.")
    })?;
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old).map_err(|e| format!("Couldn't replace SideBySide: {e}"))?;
    if let Err(e) = std::fs::rename(&fresh, &exe) {
        let _ = std::fs::rename(&old, &exe); // put the old one back
        return Err(format!("Couldn't replace SideBySide: {e}"));
    }
    Ok(exe)
}

/// Remove the copy left behind by the last update.
pub fn clean_up_old() {
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
        let _ = std::fs::remove_file(dir.join("SideBySide.old.exe"));
        let _ = std::fs::remove_file(dir.join("SideBySide.update.exe"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = include_bytes!("../test-data/signed-sample.txt");
    const SAMPLE_SIG: &str = include_str!("../test-data/signed-sample.txt.sig");

    #[test]
    fn versions() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("0.10"), Some((0, 10, 0)));
        assert_eq!(parse_version("2.0.0-beta.1"), Some((2, 0, 0)));
        assert_eq!(parse_version("hello"), None);
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
    }

    #[test]
    fn genuine_signature_is_accepted() {
        assert_eq!(verify(SAMPLE, SAMPLE_SIG), Ok(()));
    }

    #[test]
    fn tampered_file_is_refused() {
        let mut changed = SAMPLE.to_vec();
        changed[0] ^= 1;
        assert!(verify(&changed, SAMPLE_SIG).is_err());
        assert!(verify(SAMPLE, "not a signature").is_err());
    }
}
