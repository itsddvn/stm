use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use reqwest::{blocking::Client, redirect::Policy, Url};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const HOMEBREW_PKG_VERSION: &str = "6.0.18";
pub const HOMEBREW_PKG_URL: &str =
    "https://github.com/Homebrew/brew/releases/download/6.0.18/Homebrew.pkg";
pub const HOMEBREW_PKG_SHA256: &str =
    "dc892c034bf7c5567489bd02c34301e9cc63faf246c69372639c943cf5006d12";
pub const HOMEBREW_TEAM_ID: &str = "927JGANW46";
pub const HOMEBREW_PACKAGE_ID: &str = "sh.brew.homebrew";
pub const MAX_HOMEBREW_PKG_BYTES: u64 = 200 * 1024 * 1024;

const ALLOWED_DOWNLOAD_HOSTS: &[&str] = &[
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

struct PartialFileGuard {
    path: PathBuf,
    armed: bool,
}

impl PartialFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for PartialFileGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedHomebrewPkg {
    pub path: PathBuf,
    pub version: String,
    pub sha256: String,
    pub signer_team_id: String,
    pub package_id: String,
    pub previous_receipt_install_time: Option<u64>,
}

impl VerifiedHomebrewPkg {
    pub fn into_core(self) -> stm_core::domain::recipe::VerifiedInstallerArtifact {
        stm_core::domain::recipe::VerifiedInstallerArtifact {
            provider_id: "homebrew".to_string(),
            path: self.path.display().to_string(),
            version: self.version,
            source_url: HOMEBREW_PKG_URL.to_string(),
            sha256: self.sha256,
            signer_team_id: self.signer_team_id,
            package_id: self.package_id,
            previous_receipt_install_time: self.previous_receipt_install_time,
            expected_executable_paths: vec![
                "/opt/homebrew/bin/brew".to_string(),
                "/usr/local/bin/brew".to_string(),
            ],
        }
    }
}

#[derive(Debug, Error)]
pub enum HomebrewBootstrapError {
    #[error("Homebrew bootstrap is supported only on macOS")]
    UnsupportedPlatform,
    #[error("invalid Homebrew bootstrap URL")]
    InvalidUrl,
    #[error("unapproved Homebrew download host: {0}")]
    UnapprovedHost(String),
    #[error("Homebrew package exceeds the 200 MiB bound")]
    Oversized,
    #[error("Homebrew package digest mismatch")]
    DigestMismatch,
    #[error("Homebrew package signature or identifier mismatch")]
    IdentityMismatch,
    #[error("Homebrew bootstrap I/O failed: {0}")]
    Io(String),
    #[error("Homebrew bootstrap download failed: {0}")]
    Download(String),
}

pub fn download_and_verify_homebrew_pkg(
    cache_dir: &Path,
) -> Result<VerifiedHomebrewPkg, HomebrewBootstrapError> {
    if !cfg!(target_os = "macos") {
        return Err(HomebrewBootstrapError::UnsupportedPlatform);
    }
    let url = Url::parse(HOMEBREW_PKG_URL).map_err(|_| HomebrewBootstrapError::InvalidUrl)?;
    if url.scheme() != "https" || url.host_str() != Some("github.com") {
        return Err(HomebrewBootstrapError::InvalidUrl);
    }
    create_private_dir(cache_dir)?;
    let final_path = cache_dir.join(format!("Homebrew-{HOMEBREW_PKG_VERSION}.pkg"));
    if final_path.exists() {
        let safe_file = fs::symlink_metadata(&final_path)
            .map(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
            .unwrap_or(false);
        if safe_file && verify_file_sha256(&final_path, HOMEBREW_PKG_SHA256).unwrap_or(false) {
            if let Ok(verified) = verify_homebrew_pkg_identity(&final_path) {
                return Ok(verified);
            }
        }
        remove_hostile_path(&final_path)?;
    }

    let client = Client::builder()
        .redirect(Policy::custom(|attempt| {
            if allowed_download_url(attempt.url()) && attempt.previous().len() < 5 {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|error| HomebrewBootstrapError::Download(error.to_string()))?;
    let mut response = client
        .get(url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| HomebrewBootstrapError::Download(error.to_string()))?;
    let host = response.url().host_str().unwrap_or_default();
    if !allowed_download_url(response.url()) {
        return Err(HomebrewBootstrapError::UnapprovedHost(host.to_string()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_HOMEBREW_PKG_BYTES)
    {
        return Err(HomebrewBootstrapError::Oversized);
    }
    let partial = cache_dir.join(format!("Homebrew-{HOMEBREW_PKG_VERSION}.pkg.partial"));
    let _ = remove_hostile_path(&partial);
    let mut guard = PartialFileGuard::new(partial.clone());
    let mut file = create_private_file(&partial)?;
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = response
            .read(&mut buffer)
            .map_err(|error| HomebrewBootstrapError::Download(error.to_string()))?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > MAX_HOMEBREW_PKG_BYTES {
            return Err(HomebrewBootstrapError::Oversized);
        }
        digest.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .map_err(|error| HomebrewBootstrapError::Io(error.to_string()))?;
    }
    file.sync_all()
        .map_err(|error| HomebrewBootstrapError::Io(error.to_string()))?;
    let actual = format!("{:x}", digest.finalize());
    if actual != HOMEBREW_PKG_SHA256 {
        return Err(HomebrewBootstrapError::DigestMismatch);
    }
    fs::rename(&partial, &final_path)
        .map_err(|error| HomebrewBootstrapError::Io(error.to_string()))?;
    guard.disarm();
    match verify_homebrew_pkg_identity(&final_path) {
        Ok(verified) => Ok(verified),
        Err(error) => {
            let _ = fs::remove_file(&final_path);
            Err(error)
        }
    }
}

pub fn verify_homebrew_pkg_identity(
    path: &Path,
) -> Result<VerifiedHomebrewPkg, HomebrewBootstrapError> {
    if !verify_file_sha256(path, HOMEBREW_PKG_SHA256)? {
        return Err(HomebrewBootstrapError::DigestMismatch);
    }
    if !cfg!(target_os = "macos") {
        return Err(HomebrewBootstrapError::UnsupportedPlatform);
    }
    let signature = Command::new("/usr/sbin/pkgutil")
        .arg("--check-signature")
        .arg(path)
        .output()
        .map_err(|error| HomebrewBootstrapError::Io(error.to_string()))?;
    let signature_text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&signature.stdout),
        String::from_utf8_lossy(&signature.stderr)
    );
    if !signature.status.success()
        || !signature_text.contains("Developer ID Installer: Homebrew")
        || !signature_text.contains(HOMEBREW_TEAM_ID)
    {
        return Err(HomebrewBootstrapError::IdentityMismatch);
    }
    let package_info = Command::new("/usr/sbin/installer")
        .args(["-pkginfo", "-pkg"])
        .arg(path)
        .output()
        .map_err(|error| HomebrewBootstrapError::Io(error.to_string()))?;
    let package_text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&package_info.stdout),
        String::from_utf8_lossy(&package_info.stderr)
    );
    if !package_info.status.success() || !package_text.contains(HOMEBREW_PACKAGE_ID) {
        return Err(HomebrewBootstrapError::IdentityMismatch);
    }
    Ok(VerifiedHomebrewPkg {
        path: path.to_path_buf(),
        version: HOMEBREW_PKG_VERSION.to_string(),
        sha256: HOMEBREW_PKG_SHA256.to_string(),
        signer_team_id: HOMEBREW_TEAM_ID.to_string(),
        package_id: HOMEBREW_PACKAGE_ID.to_string(),
        previous_receipt_install_time: current_receipt_install_time(),
    })
}

fn current_receipt_install_time() -> Option<u64> {
    let output = Command::new("/usr/sbin/pkgutil")
        .args(["--pkg-info", HOMEBREW_PACKAGE_ID])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("install-time: "))
        .and_then(|value| value.trim().parse().ok())
}

fn allowed_download_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|host| ALLOWED_DOWNLOAD_HOSTS.contains(&host))
}

fn verify_file_sha256(path: &Path, expected: &str) -> Result<bool, HomebrewBootstrapError> {
    let mut file =
        File::open(path).map_err(|error| HomebrewBootstrapError::Io(error.to_string()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| HomebrewBootstrapError::Io(error.to_string()))?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > MAX_HOMEBREW_PKG_BYTES {
            return Err(HomebrewBootstrapError::Oversized);
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()) == expected)
}

fn remove_hostile_path(path: &Path) -> Result<(), HomebrewBootstrapError> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path).map_err(|error| HomebrewBootstrapError::Io(error.to_string()))
    } else {
        fs::remove_file(path).map_err(|error| HomebrewBootstrapError::Io(error.to_string()))
    }
}

fn create_private_dir(path: &Path) -> Result<(), HomebrewBootstrapError> {
    if path.exists() {
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| HomebrewBootstrapError::Io(error.to_string()))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(HomebrewBootstrapError::Io(
                "bootstrap cache path is not a private directory".to_string(),
            ));
        }
    } else {
        fs::create_dir_all(path).map_err(|error| HomebrewBootstrapError::Io(error.to_string()))?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| HomebrewBootstrapError::Io(error.to_string()))?;
    }
    Ok(())
}

fn create_private_file(path: &Path) -> Result<File, HomebrewBootstrapError> {
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| HomebrewBootstrapError::Io(error.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|error| HomebrewBootstrapError::Io(error.to_string()))?;
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_is_https_pinned_and_bounded() {
        let url = Url::parse(HOMEBREW_PKG_URL).expect("url");
        assert!(allowed_download_url(&url));
        assert_eq!(HOMEBREW_PKG_SHA256.len(), 64);
        assert_eq!(HOMEBREW_TEAM_ID, "927JGANW46");
    }

    #[test]
    fn rejects_wrong_digest_without_running_installer_tools() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("Homebrew.pkg");
        fs::write(&path, b"not homebrew").expect("fixture");
        let error = verify_homebrew_pkg_identity(&path).expect_err("digest");
        assert!(matches!(error, HomebrewBootstrapError::DigestMismatch));
    }

    #[test]
    fn rejects_unapproved_redirect_targets() {
        let url = Url::parse("http://attacker.invalid/Homebrew.pkg").expect("url");
        assert!(!allowed_download_url(&url));
        let url = Url::parse("https://attacker.invalid/Homebrew.pkg").expect("url");
        assert!(!allowed_download_url(&url));
    }
}
