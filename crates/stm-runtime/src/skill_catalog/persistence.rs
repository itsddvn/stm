use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};

use sha2::{Digest, Sha256};
use stm_core::skill_catalog::{SkillCatalogError, SnapshotParts, VerifiedSkillCatalog};

const SCHEMA_VERSION: u32 = 1;

const MAX_STATE_BYTES: u64 = 1_500_000;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PersistedState {
    schema_version: u32,
    catalog_version: u64,
    payload_sha256: String,
    catalog_base64: String,
    manifest_base64: String,
    signature_base64: String,
}

pub(super) fn read_state(path: &Path) -> Result<SnapshotParts, SkillCatalogError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| SkillCatalogError::Persistence(error.to_string()))?;
    if !metadata.file_type().is_file() || metadata.len() == 0 || metadata.len() > MAX_STATE_BYTES {
        return Err(SkillCatalogError::Persistence(
            "last-known-good state is not a bounded regular file".to_string(),
        ));
    }
    let bytes =
        fs::read(path).map_err(|error| SkillCatalogError::Persistence(error.to_string()))?;
    if bytes.len() as u64 > MAX_STATE_BYTES {
        return Err(SkillCatalogError::Persistence(
            "last-known-good state changed while being read".to_string(),
        ));
    }
    let state: PersistedState = serde_json::from_slice(&bytes)
        .map_err(|error| SkillCatalogError::Persistence(error.to_string()))?;
    if state.schema_version != SCHEMA_VERSION
        || state.catalog_version == 0
        || state.payload_sha256.len() != 64
    {
        return Err(SkillCatalogError::Persistence(
            "last-known-good state envelope is invalid".to_string(),
        ));
    }
    let catalog = decode_canonical_base64(&state.catalog_base64, "persisted catalog")?;
    let manifest = decode_canonical_base64(&state.manifest_base64, "persisted manifest")?;
    let signature = decode_canonical_base64(&state.signature_base64, "persisted signature")?;
    if hex_sha256(&catalog) != state.payload_sha256 {
        return Err(SkillCatalogError::Persistence(
            "last-known-good envelope hash is invalid".to_string(),
        ));
    }
    Ok(SnapshotParts {
        catalog,
        manifest,
        signature,
    })
}

pub(super) fn persist_state(
    path: &Path,
    snapshot: &VerifiedSkillCatalog,
) -> Result<(), SkillCatalogError> {
    let parts = snapshot.authenticated_parts();
    let state = PersistedState {
        schema_version: SCHEMA_VERSION,
        catalog_version: snapshot.catalog.catalog_version,
        payload_sha256: snapshot.payload_sha256.clone(),
        catalog_base64: BASE64.encode(&parts.catalog),
        manifest_base64: BASE64.encode(&parts.manifest),
        signature_base64: BASE64.encode(&parts.signature),
    };
    let mut bytes = serde_json::to_vec_pretty(&state)
        .map_err(|error| SkillCatalogError::Persistence(error.to_string()))?;
    bytes.push(b'\n');
    if bytes.len() as u64 > MAX_STATE_BYTES {
        return Err(SkillCatalogError::Persistence(
            "last-known-good state exceeds its size limit".to_string(),
        ));
    }
    atomic_replace(path, &bytes)
}

fn decode_canonical_base64(value: &str, context: &str) -> Result<Vec<u8>, SkillCatalogError> {
    let decoded = BASE64
        .decode(value)
        .map_err(|error| SkillCatalogError::Persistence(format!("{context}: {error}")))?;
    if BASE64.encode(&decoded) != value {
        return Err(SkillCatalogError::Persistence(format!(
            "{context}: encoding is not canonical"
        )));
    }
    Ok(decoded)
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<(), SkillCatalogError> {
    let parent = path.parent().ok_or_else(|| {
        SkillCatalogError::Persistence("state path has no parent directory".to_string())
    })?;
    fs::create_dir_all(parent)
        .map_err(|error| SkillCatalogError::Persistence(error.to_string()))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("catalog-state");
    let temporary = parent.join(format!(".{file_name}.tmp-{}-{nonce}", std::process::id()));
    let write_result = (|| -> Result<(), SkillCatalogError> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| SkillCatalogError::Persistence(error.to_string()))?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| SkillCatalogError::Persistence(error.to_string()))?;
        replace_path(&temporary, path)?;
        sync_parent(parent)?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> Result<(), SkillCatalogError> {
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| SkillCatalogError::Persistence(error.to_string()))
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> Result<(), SkillCatalogError> {
    Ok(())
}

#[cfg(not(windows))]
fn replace_path(source: &Path, destination: &Path) -> Result<(), SkillCatalogError> {
    fs::rename(source, destination)
        .map_err(|error| SkillCatalogError::Persistence(error.to_string()))
}

#[cfg(windows)]
fn replace_path(source: &Path, destination: &Path) -> Result<(), SkillCatalogError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source_wide = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination_wide = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(SkillCatalogError::Persistence(
            std::io::Error::last_os_error().to_string(),
        ));
    }
    Ok(())
}
