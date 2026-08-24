use std::{
    fs,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

use minisign_verify::{PublicKey, Signature};
use reqwest::Url;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};

const MANIFEST_URL: &str = "https://github.com/itsddvn/stm/releases/latest/download/latest.json";
const SIGNATURE_URL: &str =
    "https://github.com/itsddvn/stm/releases/latest/download/latest.json.sig";
const MAX_MANIFEST_BYTES: usize = 256 * 1024;
const MAX_SIGNATURE_BYTES: usize = 64 * 1024;

#[derive(Debug, Deserialize)]
struct UpdaterManifest {
    version: String,
    platforms: std::collections::BTreeMap<String, UpdaterEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct UpdaterEntry {
    url: String,
    signature: String,
}

#[derive(Debug, Clone)]
pub(super) struct VerifiedReleaseMetadata {
    pub version: String,
    pub digest: String,
    platforms: std::collections::BTreeMap<String, UpdaterEntry>,
}

impl VerifiedReleaseMetadata {
    pub fn matches_update(&self, update: &tauri_plugin_updater::Update) -> bool {
        self.version == update.version
            && self.platforms.values().any(|entry| {
                entry.url == update.download_url.as_str()
                    && entry.signature.trim() == update.signature.trim()
            })
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AcceptedMetadata {
    schema_version: u64,
    version: String,
    sha256: String,
}

pub(super) async fn verify_release_metadata(
    app: &AppHandle,
) -> Result<VerifiedReleaseMetadata, String> {
    let public_key = updater_public_key(app)?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() < 5 && approved_metadata_url(attempt.url()) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|_| "Signed updater metadata client failed".to_string())?;
    let manifest_bytes = fetch_bounded(&client, MANIFEST_URL, MAX_MANIFEST_BYTES).await?;
    let signature_bytes = fetch_bounded(&client, SIGNATURE_URL, MAX_SIGNATURE_BYTES).await?;
    let signature_text = std::str::from_utf8(&signature_bytes)
        .map_err(|_| "Signed updater metadata signature is not UTF-8".to_string())?;
    let signature = Signature::decode(signature_text.trim())
        .map_err(|_| "Signed updater metadata signature is malformed".to_string())?;
    public_key
        .verify(&manifest_bytes[..], &signature, false)
        .map_err(|_| "Signed updater metadata authentication failed".to_string())?;
    let manifest: UpdaterManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| "Signed updater metadata is malformed".to_string())?;
    Version::parse(manifest.version.trim_start_matches('v'))
        .map_err(|_| "Signed updater metadata version is invalid".to_string())?;
    if manifest.platforms.is_empty() {
        return Err("Signed updater metadata has no platform artifacts".into());
    }
    for entry in manifest.platforms.values() {
        let url = Url::parse(&entry.url)
            .map_err(|_| "Signed updater artifact URL is malformed".to_string())?;
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || entry.signature.trim().len() < 40
        {
            return Err("Signed updater artifact metadata is unsafe".into());
        }
    }
    let digest = format!("sha256:{:x}", Sha256::digest(&manifest_bytes));
    persist_monotonic_metadata(app, &manifest.version, &digest)?;
    Ok(VerifiedReleaseMetadata {
        version: manifest.version,
        digest,
        platforms: manifest.platforms,
    })
}

async fn fetch_bounded(
    client: &reqwest::Client,
    url: &str,
    maximum: usize,
) -> Result<Vec<u8>, String> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| "Signed updater metadata request failed".to_string())?;
    if !response.status().is_success()
        || !approved_metadata_url(response.url())
        || response
            .content_length()
            .is_some_and(|length| length as usize > maximum)
    {
        return Err("Signed updater metadata response was rejected".into());
    }
    let mut output = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Signed updater metadata body failed".to_string())?
    {
        if output.len() + chunk.len() > maximum {
            return Err("Signed updater metadata exceeds its size bound".into());
        }
        output.extend_from_slice(&chunk);
    }
    Ok(output)
}

fn approved_metadata_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
        && matches!(
            url.host_str(),
            Some("github.com")
                | Some("objects.githubusercontent.com")
                | Some("release-assets.githubusercontent.com")
        )
}

fn updater_public_key(app: &AppHandle) -> Result<PublicKey, String> {
    let value = app
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|config| config.get("pubkey"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Signed updater public key is unavailable".to_string())?;
    let encoded = value
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("RW"))
        .ok_or_else(|| "Signed updater public key payload is unavailable".to_string())?;
    PublicKey::from_base64(encoded).map_err(|_| "Signed updater public key is malformed".into())
}

fn persist_monotonic_metadata(app: &AppHandle, version: &str, digest: &str) -> Result<(), String> {
    let incoming = Version::parse(version.trim_start_matches('v'))
        .map_err(|_| "Signed updater metadata version is invalid".to_string())?;
    let root = app
        .path()
        .app_data_dir()
        .map_err(|_| "Signed updater metadata directory is unavailable".to_string())?
        .join("product-updates")
        .join("metadata");
    fs::create_dir_all(&root)
        .map_err(|_| "Signed updater metadata directory could not be created".to_string())?;
    if fs::symlink_metadata(&root).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err("Signed updater metadata directory symlink rejected".into());
    }
    for entry in fs::read_dir(&root)
        .map_err(|_| "Signed updater metadata history could not be read".to_string())?
    {
        let path = entry
            .map_err(|_| "Signed updater metadata history entry failed".to_string())?
            .path();
        let accepted = read_accepted_metadata(&path)?;
        let accepted_version = Version::parse(accepted.version.trim_start_matches('v'))
            .map_err(|_| "Stored updater metadata version is invalid".to_string())?;
        if accepted_version > incoming
            || (accepted_version == incoming && accepted.sha256 != digest)
        {
            return Err("Signed updater metadata downgrade or same-version drift rejected".into());
        }
        if accepted_version == incoming && accepted.sha256 == digest {
            return Ok(());
        }
    }
    let file_name = format!("{}-{}.json", incoming, digest.trim_start_matches("sha256:"));
    let path = root.join(file_name);
    write_private_json(
        &path,
        &AcceptedMetadata {
            schema_version: 1,
            version: version.to_string(),
            sha256: digest.to_string(),
        },
    )
}

fn read_accepted_metadata(path: &Path) -> Result<AcceptedMetadata, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| "Stored updater metadata is unavailable".to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 64 * 1024 {
        return Err("Stored updater metadata file rejected".into());
    }
    let bytes = fs::read(path).map_err(|_| "Stored updater metadata read failed".to_string())?;
    let value: AcceptedMetadata = serde_json::from_slice(&bytes)
        .map_err(|_| "Stored updater metadata is malformed".to_string())?;
    if value.schema_version != 1 {
        return Err("Stored updater metadata schema is unsupported".into());
    }
    Ok(value)
}

fn write_private_json(path: &PathBuf, value: &AcceptedMetadata) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|_| "Signed updater metadata serialization failed".to_string())?;
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|_| "Signed updater metadata record could not be created".to_string())?;
    file.write_all(&bytes)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|_| "Signed updater metadata record could not be persisted".to_string())
}
