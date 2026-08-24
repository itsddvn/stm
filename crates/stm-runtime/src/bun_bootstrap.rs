use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use reqwest::{blocking::Client, redirect::Policy, Url};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zip::ZipArchive;

#[cfg(test)]
use stm_core::domain::recipe::PINNED_BUN_ARCHIVES;
use stm_core::domain::recipe::{
    pinned_bun_archive, pinned_bun_source_url, PinnedBunArchive, PINNED_BUN_VERSION,
};

pub const BUN_VERSION: &str = PINNED_BUN_VERSION;
const MAX_ARCHIVE_BYTES: u64 = 80 * 1024 * 1024;
const MAX_BINARY_BYTES: u64 = 200 * 1024 * 1024;
const MAX_ZIP_ENTRIES: usize = 8;
const ALLOWED_HOSTS: &[&str] = &[
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

#[cfg(test)]
const SPECS: &[PinnedBunArchive] = PINNED_BUN_ARCHIVES;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedBunBinary {
    pub version: String,
    pub source_url: String,
    pub archive_sha256: String,
    pub binary_sha256: String,
    pub staged_binary_path: PathBuf,
    pub target_binary_path: PathBuf,
}

impl VerifiedBunBinary {
    pub fn into_core(self) -> stm_core::domain::recipe::VerifiedArchiveBinary {
        stm_core::domain::recipe::VerifiedArchiveBinary {
            provider_id: "bun".to_string(),
            version: self.version,
            source_url: self.source_url,
            archive_sha256: self.archive_sha256,
            binary_sha256: self.binary_sha256,
            staged_binary_path: self.staged_binary_path.display().to_string(),
            target_binary_path: self.target_binary_path.display().to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub enum BunBootstrapError {
    #[error("unsupported Bun bootstrap target")]
    UnsupportedTarget,
    #[error("invalid or unapproved Bun download URL")]
    InvalidUrl,
    #[error("Bun archive exceeds the download bound")]
    Oversized,
    #[error("Bun archive digest mismatch")]
    DigestMismatch,
    #[error("Bun archive layout is not bounded and symlink-free")]
    UnsafeArchive,
    #[error("Bun binary format is invalid")]
    InvalidBinary,
    #[error("Bun bootstrap I/O failed: {0}")]
    Io(String),
    #[error("Bun bootstrap download failed: {0}")]
    Download(String),
}

pub fn prepare_bun_binary(
    cache_dir: &Path,
    install_root: &Path,
) -> Result<VerifiedBunBinary, BunBootstrapError> {
    let spec = current_target()
        .and_then(pinned_bun_archive)
        .ok_or(BunBootstrapError::UnsupportedTarget)?;
    create_private_dir(cache_dir)?;
    let url_text = pinned_bun_source_url(spec);
    let url = Url::parse(&url_text).map_err(|_| BunBootstrapError::InvalidUrl)?;
    if !allowed_url(&url) || url.host_str() != Some("github.com") {
        return Err(BunBootstrapError::InvalidUrl);
    }
    let archive_path = cache_dir.join(format!("{}-{}", BUN_VERSION, spec.asset));
    if !archive_path.is_file() || !verify_sha256(&archive_path, spec.sha256, MAX_ARCHIVE_BYTES)? {
        remove_path(&archive_path)?;
        download_archive(&url, &archive_path, spec.sha256)?;
    }
    let staged = cache_dir.join(format!("bun-{BUN_VERSION}-{}.staged", spec.target));
    remove_path(&staged)?;
    let binary_sha256 = extract_exact_binary(&archive_path, &staged, spec)?;
    let target_binary_path =
        install_root
            .join(BUN_VERSION)
            .join("bin")
            .join(if cfg!(target_os = "windows") {
                "bun.exe"
            } else {
                "bun"
            });
    Ok(VerifiedBunBinary {
        version: BUN_VERSION.to_string(),
        source_url: url_text,
        archive_sha256: spec.sha256.to_string(),
        binary_sha256,
        staged_binary_path: staged,
        target_binary_path,
    })
}

fn download_archive(
    url: &Url,
    destination: &Path,
    expected_sha256: &str,
) -> Result<(), BunBootstrapError> {
    let client = Client::builder()
        .redirect(Policy::custom(|attempt| {
            if allowed_url(attempt.url()) && attempt.previous().len() < 5 {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| BunBootstrapError::Download(error.to_string()))?;
    let mut response = client
        .get(url.clone())
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| BunBootstrapError::Download(error.to_string()))?;
    if !allowed_url(response.url())
        || response
            .content_length()
            .is_some_and(|length| length > MAX_ARCHIVE_BYTES)
    {
        return Err(BunBootstrapError::Oversized);
    }
    let partial = destination.with_extension("zip.partial");
    remove_path(&partial)?;
    let mut file = create_private_file(&partial, 0o600)?;
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    let result = (|| {
        loop {
            let read = response
                .read(&mut buffer)
                .map_err(|error| BunBootstrapError::Download(error.to_string()))?;
            if read == 0 {
                break;
            }
            total = total.saturating_add(read as u64);
            if total > MAX_ARCHIVE_BYTES {
                return Err(BunBootstrapError::Oversized);
            }
            digest.update(&buffer[..read]);
            file.write_all(&buffer[..read])
                .map_err(|error| BunBootstrapError::Io(error.to_string()))?;
        }
        file.sync_all()
            .map_err(|error| BunBootstrapError::Io(error.to_string()))?;
        if format!("{:x}", digest.finalize()) != expected_sha256 {
            return Err(BunBootstrapError::DigestMismatch);
        }
        fs::rename(&partial, destination).map_err(|error| BunBootstrapError::Io(error.to_string()))
    })();
    if result.is_err() {
        let _ = remove_path(&partial);
    }
    result
}

fn extract_exact_binary(
    archive_path: &Path,
    staged_path: &Path,
    spec: PinnedBunArchive,
) -> Result<String, BunBootstrapError> {
    let file =
        File::open(archive_path).map_err(|error| BunBootstrapError::Io(error.to_string()))?;
    let mut archive = ZipArchive::new(file).map_err(|_| BunBootstrapError::UnsafeArchive)?;
    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(BunBootstrapError::UnsafeArchive);
    }
    let mut binary_index = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|_| BunBootstrapError::UnsafeArchive)?;
        let name = entry.name();
        let is_directory = name.ends_with('/');
        let is_binary = name == spec.entry;
        let is_symlink = entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000);
        if is_symlink
            || (!is_directory && !is_binary)
            || name.contains("..")
            || name.starts_with('/')
        {
            return Err(BunBootstrapError::UnsafeArchive);
        }
        if is_binary && (entry.size() > MAX_BINARY_BYTES || binary_index.replace(index).is_some()) {
            return Err(BunBootstrapError::UnsafeArchive);
        }
    }
    let index = binary_index.ok_or(BunBootstrapError::UnsafeArchive)?;
    let mut entry = archive
        .by_index(index)
        .map_err(|_| BunBootstrapError::UnsafeArchive)?;
    let mut target = create_private_file(staged_path, 0o500)?;
    let mut digest = Sha256::new();
    let mut header = [0_u8; 4];
    let mut header_length = 0_usize;
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = entry
            .read(&mut buffer)
            .map_err(|error| BunBootstrapError::Io(error.to_string()))?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > MAX_BINARY_BYTES {
            let _ = remove_path(staged_path);
            return Err(BunBootstrapError::Oversized);
        }
        if header_length < header.len() {
            let copy = (header.len() - header_length).min(read);
            header[header_length..header_length + copy].copy_from_slice(&buffer[..copy]);
            header_length += copy;
        }
        digest.update(&buffer[..read]);
        target
            .write_all(&buffer[..read])
            .map_err(|error| BunBootstrapError::Io(error.to_string()))?;
    }
    target
        .sync_all()
        .map_err(|error| BunBootstrapError::Io(error.to_string()))?;
    if header_length < 2 || !valid_binary_magic(&header) {
        let _ = remove_path(staged_path);
        return Err(BunBootstrapError::InvalidBinary);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn valid_binary_magic(header: &[u8; 4]) -> bool {
    matches!(header, [0xcf, 0xfa, 0xed, 0xfe] | [0xfe, 0xed, 0xfa, 0xcf])
        || header == b"\x7fELF"
        || header[..2] == *b"MZ"
}

fn verify_sha256(path: &Path, expected: &str, max: u64) -> Result<bool, BunBootstrapError> {
    let mut file = File::open(path).map_err(|error| BunBootstrapError::Io(error.to_string()))?;
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| BunBootstrapError::Io(error.to_string()))?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > max {
            return Err(BunBootstrapError::Oversized);
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()) == expected)
}

fn current_target() -> Option<&'static str> {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("macos_arm64")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some("macos_x64")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("linux_x64")
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some("windows_x64")
    } else {
        None
    }
}

fn allowed_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|host| ALLOWED_HOSTS.contains(&host))
}

fn create_private_dir(path: &Path) -> Result<(), BunBootstrapError> {
    if path.exists() {
        let metadata =
            fs::symlink_metadata(path).map_err(|error| BunBootstrapError::Io(error.to_string()))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(BunBootstrapError::UnsafeArchive);
        }
    } else {
        fs::create_dir_all(path).map_err(|error| BunBootstrapError::Io(error.to_string()))?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| BunBootstrapError::Io(error.to_string()))?;
    }
    Ok(())
}

fn create_private_file(path: &Path, unix_mode: u32) -> Result<File, BunBootstrapError> {
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| BunBootstrapError::Io(error.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(unix_mode))
            .map_err(|error| BunBootstrapError::Io(error.to_string()))?;
    }
    Ok(file)
}

fn remove_path(path: &Path) -> Result<(), BunBootstrapError> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path).map_err(|error| BunBootstrapError::Io(error.to_string()))
    } else {
        fs::remove_file(path).map_err(|error| BunBootstrapError::Io(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use zip::{write::SimpleFileOptions, ZipWriter};

    fn fixture_spec() -> PinnedBunArchive {
        PinnedBunArchive {
            target: "test",
            asset: "bun-test.zip",
            sha256: "",
            entry: "bun-test/bun",
        }
    }

    fn write_zip(path: &Path, entries: &[(&str, Option<&[u8]>)]) {
        let file = File::create(path).expect("zip fixture");
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        for (name, contents) in entries {
            match contents {
                Some(contents) => {
                    archive
                        .start_file(*name, options)
                        .expect("start zip fixture file");
                    archive.write_all(contents).expect("write zip fixture file");
                }
                None => archive
                    .add_directory(*name, options)
                    .expect("add zip fixture directory"),
            }
        }
        archive.finish().expect("finish zip fixture");
    }

    #[test]
    fn specs_are_pinned_and_target_specific() {
        assert_eq!(SPECS.len(), 4);
        assert!(SPECS.iter().all(|spec| spec.sha256.len() == 64));
        assert!(SPECS.iter().all(|spec| !spec.entry.contains("..")));
    }

    #[test]
    fn rejects_unapproved_download_hosts() {
        let url = Url::parse("https://attacker.invalid/bun.zip").expect("url");
        assert!(!allowed_url(&url));
    }

    #[test]
    fn extracts_only_the_exact_bun_archive_entry() {
        let temp = TempDir::new().expect("tempdir");
        let archive = temp.path().join("bun.zip");
        let staged = temp.path().join("bun.staged");
        let binary = b"\x7fELFfixture Bun binary";
        write_zip(
            &archive,
            &[
                ("bun-test/", None),
                ("bun-test/bun", Some(binary.as_slice())),
            ],
        );

        let digest =
            extract_exact_binary(&archive, &staged, fixture_spec()).expect("extract exact binary");

        assert_eq!(fs::read(&staged).expect("staged binary"), binary);
        assert_eq!(digest, format!("{:x}", Sha256::digest(binary)));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&staged)
                    .expect("staged metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o500
            );
        }
    }

    #[test]
    fn rejects_unexpected_and_traversing_archive_entries() {
        let temp = TempDir::new().expect("tempdir");
        let binary = b"\x7fELFfixture Bun binary";
        let cases = [
            vec![
                ("bun-test/", None),
                ("bun-test/not-bun", Some(binary.as_slice())),
            ],
            vec![("../", None), ("bun-test/bun", Some(binary.as_slice()))],
        ];

        for (index, entries) in cases.iter().enumerate() {
            let archive = temp.path().join(format!("unsafe-{index}.zip"));
            let staged = temp.path().join(format!("unsafe-{index}.staged"));
            write_zip(&archive, entries);
            let error = extract_exact_binary(&archive, &staged, fixture_spec())
                .expect_err("unsafe archive must be rejected");
            assert!(matches!(error, BunBootstrapError::UnsafeArchive));
            assert!(!staged.exists());
        }
    }

    #[test]
    #[ignore = "downloads and extracts the pinned Bun release archive from GitHub"]
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "x86_64")
    ))]
    fn downloads_and_extracts_pinned_bun_release_archive_smoke() {
        let temp = TempDir::new().expect("tempdir");
        let install_root = temp.path().join("install");
        let prepared =
            prepare_bun_binary(&temp.path().join("cache"), &install_root).expect("prepare Bun");

        assert_eq!(prepared.version, BUN_VERSION);
        assert!(prepared.staged_binary_path.is_file());
        assert!(!install_root.exists());
        assert!(verify_sha256(
            &prepared.staged_binary_path,
            &prepared.binary_sha256,
            MAX_BINARY_BYTES
        )
        .expect("verify extracted Bun"));
    }
}
