use std::path::Path;

#[cfg(not(test))]
use std::{fs, fs::OpenOptions};

use chacha20poly1305::{
    aead::{rand_core::RngCore, Aead, KeyInit, OsRng, Payload},
    XChaCha20Poly1305, XNonce,
};
#[cfg(not(test))]
use fs2::FileExt;

use stm_core::CoreError;

const BACKUP_MAGIC: &[u8; 8] = b"STMMCP01";

#[cfg(test)]
pub(crate) fn load_backup_key(_database_path: &Path) -> Result<[u8; 32], CoreError> {
    Ok([0x5a; 32])
}

#[cfg(not(test))]
pub(crate) fn load_backup_key(database_path: &Path) -> Result<[u8; 32], CoreError> {
    let parent = database_path
        .parent()
        .ok_or_else(|| CoreError::InvalidPath("MCP database has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let lock_path = parent.join(".stm-mcp-backup-key.lock");
    if lock_path.exists() && fs::symlink_metadata(&lock_path)?.file_type().is_symlink() {
        return Err(CoreError::PathEscape(lock_path));
    }
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let lock = options.open(&lock_path)?;
    lock.lock_exclusive()?;
    let result = load_or_create_os_backup_key();
    let _ = FileExt::unlock(&lock);
    result
}

#[cfg(not(test))]
fn load_or_create_os_backup_key() -> Result<[u8; 32], CoreError> {
    let entry =
        keyring::v1::Entry::new("com.stm.tools-manager.mcp-backup", "default").map_err(|_| {
            CoreError::CommandDenied("OS credential store is unavailable for MCP backups".into())
        })?;
    let secret = match entry.get_secret() {
        Ok(secret) => secret,
        Err(keyring::v1::Error::NoEntry) => {
            let mut generated = [0_u8; 32];
            OsRng.fill_bytes(&mut generated);
            entry.set_secret(&generated).map_err(|_| {
                CoreError::CommandDenied("OS credential store rejected the MCP backup key".into())
            })?;
            entry.get_secret().map_err(|_| {
                CoreError::CommandDenied(
                    "OS credential store could not verify the MCP backup key".into(),
                )
            })?
        }
        Err(_) => {
            return Err(CoreError::CommandDenied(
                "OS credential store could not read the MCP backup key".into(),
            ));
        }
    };
    secret.try_into().map_err(|_| {
        CoreError::CommandDenied("OS credential store returned an invalid MCP backup key".into())
    })
}

pub(crate) fn encrypt_backup_bytes(
    key: &[u8; 32],
    backup_id: &str,
    plaintext: &[u8],
) -> Result<Vec<u8>, CoreError> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce = [0_u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: backup_id.as_bytes(),
            },
        )
        .map_err(|_| CoreError::CommandDenied("MCP backup encryption failed".into()))?;
    let mut output = Vec::with_capacity(BACKUP_MAGIC.len() + nonce.len() + ciphertext.len());
    output.extend_from_slice(BACKUP_MAGIC);
    output.extend_from_slice(&nonce);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

pub(crate) fn decrypt_backup_bytes(
    key: &[u8; 32],
    backup_id: &str,
    encrypted: &[u8],
) -> Result<Vec<u8>, CoreError> {
    let payload = encrypted
        .strip_prefix(BACKUP_MAGIC)
        .ok_or_else(|| CoreError::LifecycleEvidenceChanged("MCP backup is not encrypted".into()))?;
    if payload.len() < 24 {
        return Err(CoreError::LifecycleEvidenceChanged(
            "MCP backup envelope is truncated".into(),
        ));
    }
    let (nonce, ciphertext) = payload.split_at(24);
    XChaCha20Poly1305::new(key.into())
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: backup_id.as_bytes(),
            },
        )
        .map_err(|_| CoreError::LifecycleEvidenceChanged("MCP backup authentication failed".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_backups_hide_plaintext_and_reject_tampering() {
        let key = [0x5a; 32];
        let plaintext = b"raw-fixture-credential";
        let mut encrypted =
            encrypt_backup_bytes(&key, "backup-1", plaintext).expect("encrypt backup");
        assert!(encrypted.starts_with(BACKUP_MAGIC));
        assert!(!encrypted
            .windows(plaintext.len())
            .any(|window| window == plaintext));
        assert_eq!(
            decrypt_backup_bytes(&key, "backup-1", &encrypted).expect("decrypt backup"),
            plaintext
        );

        let last = encrypted.last_mut().expect("ciphertext");
        *last ^= 1;
        assert!(decrypt_backup_bytes(&key, "backup-1", &encrypted).is_err());
    }
}
