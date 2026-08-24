use std::{collections::BTreeSet, path::Path};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};
use url::Url;

use crate::domain::skill::SkillClientName;

pub const CATALOG_KEY_ID: &str = "stm-skill-catalog-2026-7445f242";
pub const CATALOG_MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/itsddvn/stm-skill-catalog/main/stable/manifest.json";
pub const CATALOG_SIGNATURE_URL: &str =
    "https://raw.githubusercontent.com/itsddvn/stm-skill-catalog/main/stable/manifest.sig.json";
pub const CATALOG_PAYLOAD_URL: &str =
    "https://raw.githubusercontent.com/itsddvn/stm-skill-catalog/main/stable/catalog.json";
pub const MAX_CATALOG_BYTES: usize = 1_048_576;
pub const MAX_MANIFEST_BYTES: usize = 16_384;
pub const MAX_SIGNATURE_BYTES: usize = 4_096;
const SCHEMA_VERSION: u32 = 1;
const CHANNEL: &str = "stable";
const PUBLIC_KEY_BYTES: [u8; 32] = [
    0xa9, 0xcb, 0x19, 0xa7, 0x25, 0x90, 0x2e, 0x22, 0xb6, 0x49, 0xdb, 0x72, 0x4e, 0x0a, 0xf5, 0x03,
    0xa4, 0x28, 0xce, 0x27, 0x4f, 0x18, 0xcc, 0x35, 0x99, 0xa8, 0x52, 0x51, 0x04, 0x1e, 0x7d, 0xdf,
];
const BUNDLED_CATALOG: &[u8] = include_bytes!("../../../../catalog/skills/stable/catalog.json");
const BUNDLED_MANIFEST: &[u8] = include_bytes!("../../../../catalog/skills/stable/manifest.json");
const BUNDLED_SIGNATURE: &[u8] =
    include_bytes!("../../../../catalog/skills/stable/manifest.sig.json");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthenticatedSkillCatalog {
    pub schema_version: u32,
    pub catalog_version: u64,
    pub channel: String,
    pub skills: Vec<TrustedSkillEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrustedSkillEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub publisher: String,
    pub purposes: Vec<String>,
    pub risk_flags: Vec<String>,
    pub source: TrustedSkillSource,
    pub targets: Vec<TrustedSkillTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrustedSkillSource {
    pub repository: String,
    pub subpath: String,
    pub commit: String,
    pub tree_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrustedSkillTarget {
    pub client: SkillClientName,
    pub relative_path: String,
}

impl AuthenticatedSkillCatalog {
    pub fn find_by_id(&self, id: &str) -> Option<&TrustedSkillEntry> {
        self.skills.iter().find(|entry| entry.id == id)
    }

    pub fn find_by_source(&self, repository: &str, subpath: &str) -> Option<&TrustedSkillEntry> {
        self.skills
            .iter()
            .find(|entry| entry.source.repository == repository && entry.source.subpath == subpath)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillCatalogManifest {
    pub schema_version: u32,
    pub catalog_version: u64,
    pub channel: String,
    pub created_at: String,
    pub expires_at: String,
    pub payload_sha256: String,
    pub payload_length: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillCatalogOrigin {
    Remote,
    Persisted,
    Bundled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedSkillCatalog {
    pub catalog: AuthenticatedSkillCatalog,
    pub manifest: SkillCatalogManifest,
    pub payload_sha256: String,
    pub origin: SkillCatalogOrigin,
    authenticated_bytes: SnapshotParts,
}

impl VerifiedSkillCatalog {
    pub fn authenticated_parts(&self) -> &SnapshotParts {
        &self.authenticated_bytes
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedCatalogIdentity {
    pub catalog_version: u64,
    pub payload_sha256: String,
}

impl From<&VerifiedSkillCatalog> for AcceptedCatalogIdentity {
    fn from(snapshot: &VerifiedSkillCatalog) -> Self {
        Self {
            catalog_version: snapshot.catalog.catalog_version,
            payload_sha256: snapshot.payload_sha256.clone(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SkillCatalogError {
    #[error("catalog transport failed: {0}")]
    Transport(String),
    #[error("catalog response for {resource} exceeded {limit} bytes")]
    ResponseTooLarge {
        resource: &'static str,
        limit: usize,
    },
    #[error("catalog document is malformed: {0}")]
    Malformed(String),
    #[error("catalog signing key is unknown: {0}")]
    UnknownKey(String),
    #[error("catalog manifest signature is invalid")]
    BadSignature,
    #[error("catalog payload hash or length does not match the signed manifest")]
    PayloadMismatch,
    #[error("catalog manifest is not yet valid")]
    NotYetValid,
    #[error("catalog manifest expired")]
    Expired,
    #[error("catalog version {received} is below accepted version {accepted}")]
    Downgrade { received: u64, accepted: u64 },
    #[error("catalog content changed without a version increment")]
    SameVersionDrift,
    #[error("catalog persistence failed: {0}")]
    Persistence(String),
    #[error("no authenticated catalog snapshot is available; remote={remote}; persisted={persisted}; bundled={bundled}")]
    Unavailable {
        remote: String,
        persisted: String,
        bundled: String,
    },
}

pub trait CatalogRemote: Send + Sync {
    fn fetch(&self, url: &'static str, maximum_bytes: usize) -> Result<Vec<u8>, SkillCatalogError>;
}

pub trait CatalogSnapshotStore: Send + Sync {
    fn read_state(&self) -> Result<SnapshotParts, SkillCatalogError>;
    fn persist_state(&self, snapshot: &VerifiedSkillCatalog) -> Result<(), SkillCatalogError>;
}

pub trait CatalogClock: Send + Sync {
    fn now_utc(&self) -> OffsetDateTime;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemCatalogClock;

impl CatalogClock for SystemCatalogClock {
    fn now_utc(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

pub struct SkillCatalogService<R, C> {
    remote: R,
    clock: C,
}

impl<R, C> SkillCatalogService<R, C>
where
    R: CatalogRemote,
    C: CatalogClock,
{
    pub fn new(remote: R, clock: C) -> Self {
        Self { remote, clock }
    }

    pub fn refresh_remote(
        &self,
        store: &dyn CatalogSnapshotStore,
        accepted: Option<&AcceptedCatalogIdentity>,
    ) -> Result<VerifiedSkillCatalog, SkillCatalogError> {
        let now = self.clock.now_utc();
        let persisted = store.read_state().ok().and_then(|parts| {
            verify_snapshot(
                &parts,
                now,
                None,
                Freshness::AllowExpired,
                SkillCatalogOrigin::Persisted,
            )
            .ok()
        });
        let floor = strongest_identity(accepted, persisted.as_ref());
        let manifest = self
            .remote
            .fetch(CATALOG_MANIFEST_URL, MAX_MANIFEST_BYTES)?;
        ensure_bound(&manifest, MAX_MANIFEST_BYTES, "manifest")?;
        let signature = self
            .remote
            .fetch(CATALOG_SIGNATURE_URL, MAX_SIGNATURE_BYTES)?;
        ensure_bound(&signature, MAX_SIGNATURE_BYTES, "signature")?;
        let catalog = self.remote.fetch(CATALOG_PAYLOAD_URL, MAX_CATALOG_BYTES)?;
        ensure_bound(&catalog, MAX_CATALOG_BYTES, "catalog")?;
        let parts = SnapshotParts {
            catalog,
            manifest,
            signature,
        };
        let snapshot = verify_snapshot(
            &parts,
            now,
            floor.as_ref(),
            Freshness::RequireFresh,
            SkillCatalogOrigin::Remote,
        )?;
        store.persist_state(&snapshot)?;
        Ok(snapshot)
    }

    pub fn load_last_good_or_bundled(
        &self,
        store: &dyn CatalogSnapshotStore,
        accepted: Option<&AcceptedCatalogIdentity>,
    ) -> Result<VerifiedSkillCatalog, SkillCatalogError> {
        let now = self.clock.now_utc();
        let persisted_parts = store.read_state();
        let authenticated_persisted = persisted_parts.as_ref().ok().and_then(|parts| {
            verify_snapshot(
                parts,
                now,
                None,
                Freshness::AllowExpired,
                SkillCatalogOrigin::Persisted,
            )
            .ok()
        });
        let floor = strongest_identity(accepted, authenticated_persisted.as_ref());
        let persisted = persisted_parts.and_then(|parts| {
            verify_snapshot(
                &parts,
                now,
                accepted,
                Freshness::RequireFresh,
                SkillCatalogOrigin::Persisted,
            )
        });
        if let Ok(snapshot) = persisted {
            return Ok(snapshot);
        }
        let persisted_error = persisted
            .expect_err("persisted success returned above")
            .to_string();
        let bundled = verify_snapshot(
            &SnapshotParts {
                catalog: BUNDLED_CATALOG.to_vec(),
                manifest: BUNDLED_MANIFEST.to_vec(),
                signature: BUNDLED_SIGNATURE.to_vec(),
            },
            now,
            floor.as_ref(),
            Freshness::RequireFresh,
            SkillCatalogOrigin::Bundled,
        );
        if let Ok(snapshot) = bundled {
            return Ok(snapshot);
        }
        let bundled_error = bundled
            .expect_err("bundled success returned above")
            .to_string();
        Err(SkillCatalogError::Unavailable {
            remote: "not attempted".to_string(),
            persisted: persisted_error,
            bundled: bundled_error,
        })
    }

    pub fn load(
        &self,
        store: &dyn CatalogSnapshotStore,
        accepted: Option<&AcceptedCatalogIdentity>,
    ) -> Result<VerifiedSkillCatalog, SkillCatalogError> {
        match self.refresh_remote(store, accepted) {
            Ok(snapshot) => Ok(snapshot),
            Err(remote_error) => {
                self.load_last_good_or_bundled(store, accepted)
                    .map_err(|fallback_error| SkillCatalogError::Unavailable {
                        remote: remote_error.to_string(),
                        persisted: fallback_error.to_string(),
                        bundled: fallback_error.to_string(),
                    })
            }
        }
    }
}
#[derive(Debug, Clone, Copy)]
struct OfflineCatalogRemote;

impl CatalogRemote for OfflineCatalogRemote {
    fn fetch(
        &self,
        _url: &'static str,
        _maximum_bytes: usize,
    ) -> Result<Vec<u8>, SkillCatalogError> {
        Err(SkillCatalogError::Transport(
            "remote catalog refresh was not requested".to_string(),
        ))
    }
}

struct MissingCatalogStore;

impl CatalogSnapshotStore for MissingCatalogStore {
    fn read_state(&self) -> Result<SnapshotParts, SkillCatalogError> {
        Err(SkillCatalogError::Persistence(
            "persisted catalog access requires a runtime adapter".to_string(),
        ))
    }

    fn persist_state(&self, _snapshot: &VerifiedSkillCatalog) -> Result<(), SkillCatalogError> {
        Err(SkillCatalogError::Persistence(
            "persisted catalog access requires a runtime adapter".to_string(),
        ))
    }
}

pub fn load_current_authenticated_catalog(
    _workspace_db_path: &Path,
) -> Result<VerifiedSkillCatalog, SkillCatalogError> {
    SkillCatalogService::new(OfflineCatalogRemote, SystemCatalogClock)
        .load_last_good_or_bundled(&MissingCatalogStore, None)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotParts {
    pub catalog: Vec<u8>,
    pub manifest: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DetachedSignature {
    schema_version: u32,
    algorithm: String,
    key_id: String,
    signature: String,
}

#[derive(Clone, Copy)]
enum Freshness {
    RequireFresh,
    AllowExpired,
}

fn strongest_identity(
    accepted: Option<&AcceptedCatalogIdentity>,
    persisted: Option<&VerifiedSkillCatalog>,
) -> Option<AcceptedCatalogIdentity> {
    let persisted = persisted.map(AcceptedCatalogIdentity::from);
    match (accepted, persisted) {
        (Some(left), Some(right)) if left.catalog_version >= right.catalog_version => {
            Some(left.clone())
        }
        (Some(_), Some(right)) => Some(right),
        (Some(left), None) => Some(left.clone()),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn ensure_bound(
    bytes: &[u8],
    limit: usize,
    resource: &'static str,
) -> Result<(), SkillCatalogError> {
    if bytes.is_empty() || bytes.len() > limit {
        return Err(SkillCatalogError::ResponseTooLarge { resource, limit });
    }
    Ok(())
}

fn verify_snapshot(
    parts: &SnapshotParts,
    now: OffsetDateTime,
    accepted: Option<&AcceptedCatalogIdentity>,
    freshness: Freshness,
    origin: SkillCatalogOrigin,
) -> Result<VerifiedSkillCatalog, SkillCatalogError> {
    ensure_bound(&parts.catalog, MAX_CATALOG_BYTES, "catalog")?;
    ensure_bound(&parts.manifest, MAX_MANIFEST_BYTES, "manifest")?;
    ensure_bound(&parts.signature, MAX_SIGNATURE_BYTES, "signature")?;

    let detached: DetachedSignature = serde_json::from_slice(&parts.signature)
        .map_err(|error| SkillCatalogError::Malformed(format!("signature: {error}")))?;
    if detached.schema_version != SCHEMA_VERSION || detached.algorithm != "Ed25519" {
        return Err(SkillCatalogError::Malformed(
            "unsupported signature schema or algorithm".to_string(),
        ));
    }
    if detached.key_id != CATALOG_KEY_ID {
        return Err(SkillCatalogError::UnknownKey(detached.key_id));
    }
    let signature_bytes = decode_canonical_base64(&detached.signature, "signature")?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| SkillCatalogError::Malformed("signature must encode 64 bytes".to_string()))?;
    let verifying_key = VerifyingKey::from_bytes(&PUBLIC_KEY_BYTES)
        .map_err(|_| SkillCatalogError::Malformed("compiled trust root is invalid".to_string()))?;
    verifying_key
        .verify_strict(&parts.manifest, &signature)
        .map_err(|_| SkillCatalogError::BadSignature)?;

    let manifest: SkillCatalogManifest = serde_json::from_slice(&parts.manifest)
        .map_err(|error| SkillCatalogError::Malformed(format!("manifest: {error}")))?;
    validate_manifest(&manifest, now, freshness)?;
    if manifest.payload_length != parts.catalog.len() as u64 {
        return Err(SkillCatalogError::PayloadMismatch);
    }
    let payload_sha256 = hex_sha256(&parts.catalog);
    if payload_sha256 != manifest.payload_sha256 {
        return Err(SkillCatalogError::PayloadMismatch);
    }

    let catalog: AuthenticatedSkillCatalog = serde_json::from_slice(&parts.catalog)
        .map_err(|error| SkillCatalogError::Malformed(format!("catalog: {error}")))?;
    validate_catalog(&catalog)?;
    if catalog.catalog_version != manifest.catalog_version || catalog.channel != manifest.channel {
        return Err(SkillCatalogError::Malformed(
            "catalog identity does not match its signed manifest".to_string(),
        ));
    }
    if let Some(accepted) = accepted {
        validate_hex(&accepted.payload_sha256, 64, "accepted payload SHA-256")?;
        if catalog.catalog_version < accepted.catalog_version {
            return Err(SkillCatalogError::Downgrade {
                received: catalog.catalog_version,
                accepted: accepted.catalog_version,
            });
        }
        if catalog.catalog_version == accepted.catalog_version
            && payload_sha256 != accepted.payload_sha256
        {
            return Err(SkillCatalogError::SameVersionDrift);
        }
    }
    Ok(VerifiedSkillCatalog {
        catalog,
        manifest,
        payload_sha256,
        origin,
        authenticated_bytes: parts.clone(),
    })
}

fn validate_manifest(
    manifest: &SkillCatalogManifest,
    now: OffsetDateTime,
    freshness: Freshness,
) -> Result<(), SkillCatalogError> {
    if manifest.schema_version != SCHEMA_VERSION || manifest.channel != CHANNEL {
        return Err(SkillCatalogError::Malformed(
            "unsupported manifest schema or channel".to_string(),
        ));
    }
    if manifest.catalog_version == 0
        || manifest.payload_length == 0
        || manifest.payload_length > MAX_CATALOG_BYTES as u64
    {
        return Err(SkillCatalogError::Malformed(
            "manifest version or payload length is out of bounds".to_string(),
        ));
    }
    validate_hex(&manifest.payload_sha256, 64, "manifest payload SHA-256")?;
    let created = parse_utc_timestamp(&manifest.created_at, "manifest createdAt")?;
    let expires = parse_utc_timestamp(&manifest.expires_at, "manifest expiresAt")?;
    if expires <= created || expires - created > Duration::days(366) {
        return Err(SkillCatalogError::Malformed(
            "manifest validity window is invalid".to_string(),
        ));
    }
    if created > now + Duration::minutes(5) {
        return Err(SkillCatalogError::NotYetValid);
    }
    if matches!(freshness, Freshness::RequireFresh) && expires <= now {
        return Err(SkillCatalogError::Expired);
    }
    Ok(())
}

fn validate_catalog(catalog: &AuthenticatedSkillCatalog) -> Result<(), SkillCatalogError> {
    if catalog.schema_version != SCHEMA_VERSION || catalog.channel != CHANNEL {
        return Err(SkillCatalogError::Malformed(
            "unsupported catalog schema or channel".to_string(),
        ));
    }
    if catalog.catalog_version == 0 || catalog.skills.is_empty() || catalog.skills.len() > 1_000 {
        return Err(SkillCatalogError::Malformed(
            "catalog version or entry count is out of bounds".to_string(),
        ));
    }
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut sources = BTreeSet::new();
    let mut targets = BTreeSet::new();
    let mut previous_id: Option<&str> = None;
    for entry in &catalog.skills {
        validate_segment(&entry.id, "skill id")?;
        validate_text(&entry.name, 100, "skill name")?;
        validate_text(&entry.description, 1_000, "skill description")?;
        validate_segment(&entry.publisher, "skill publisher")?;
        validate_string_set(&entry.purposes, 1, "skill purposes")?;
        validate_string_set(&entry.risk_flags, 0, "skill risk flags")?;
        if !ids.insert(entry.id.as_str()) {
            return Err(SkillCatalogError::Malformed(format!(
                "duplicate skill id {}",
                entry.id
            )));
        }
        let folded_name = entry.name.to_lowercase();
        if !names.insert(folded_name) {
            return Err(SkillCatalogError::Malformed(format!(
                "duplicate skill name {}",
                entry.name
            )));
        }
        if previous_id.is_some_and(|previous| previous.as_bytes() >= entry.id.as_bytes()) {
            return Err(SkillCatalogError::Malformed(
                "catalog entries are not strictly sorted by id".to_string(),
            ));
        }
        previous_id = Some(&entry.id);

        let repository = validate_repository(&entry.source.repository)?;
        validate_relative_path(&entry.source.subpath, "source subpath")?;
        validate_hex(&entry.source.commit, 40, "source commit")?;
        validate_hex(&entry.source.tree_sha256, 64, "source tree SHA-256")?;
        if entry.source.subpath.rsplit('/').next().unwrap_or_default() != entry.id {
            return Err(SkillCatalogError::Malformed(format!(
                "skill {} does not match its source basename",
                entry.id
            )));
        }
        let owner = repository
            .path_segments()
            .and_then(|mut segments| segments.next())
            .unwrap_or_default();
        if !owner.eq_ignore_ascii_case(&entry.publisher) {
            return Err(SkillCatalogError::Malformed(format!(
                "skill {} publisher does not match repository owner",
                entry.id
            )));
        }
        let source_key = format!(
            "{}\0{}",
            entry.source.repository.to_ascii_lowercase(),
            entry.source.subpath
        );
        if !sources.insert(source_key) {
            return Err(SkillCatalogError::Malformed(format!(
                "duplicate source identity for {}",
                entry.id
            )));
        }

        if entry.targets.is_empty() || entry.targets.len() > 3 {
            return Err(SkillCatalogError::Malformed(format!(
                "skill {} target count is out of bounds",
                entry.id
            )));
        }
        let mut clients = BTreeSet::new();
        let mut previous_order: Option<u8> = None;
        for target in &entry.targets {
            validate_relative_path(&target.relative_path, "target relative path")?;
            if target.relative_path != entry.id {
                return Err(SkillCatalogError::Malformed(format!(
                    "skill {} target path must equal its id",
                    entry.id
                )));
            }
            let (client, order) = client_identity(&target.client);
            if !clients.insert(client) {
                return Err(SkillCatalogError::Malformed(format!(
                    "skill {} repeats target client {client}",
                    entry.id
                )));
            }
            if previous_order.is_some_and(|previous| previous >= order) {
                return Err(SkillCatalogError::Malformed(format!(
                    "skill {} targets are not in canonical order",
                    entry.id
                )));
            }
            previous_order = Some(order);
            if !targets.insert(format!("{client}\0{}", target.relative_path)) {
                return Err(SkillCatalogError::Malformed(format!(
                    "duplicate target identity {client}:{}",
                    target.relative_path
                )));
            }
        }
    }
    Ok(())
}

fn validate_repository(value: &str) -> Result<Url, SkillCatalogError> {
    let url = Url::parse(value)
        .map_err(|error| SkillCatalogError::Malformed(format!("source repository: {error}")))?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(SkillCatalogError::Malformed(
            "source repository must be credential-free canonical GitHub HTTPS".to_string(),
        ));
    }
    let segments = url
        .path_segments()
        .map(|segments| segments.collect::<Vec<_>>())
        .unwrap_or_default();
    if segments.len() != 2
        || !segments[1].ends_with(".git")
        || segments[1].len() <= 4
        || !segments.iter().all(|segment| {
            valid_segment_bytes(segment.as_bytes()) && segment != &"." && segment != &".."
        })
    {
        return Err(SkillCatalogError::Malformed(
            "source repository must use /owner/repository.git".to_string(),
        ));
    }
    if format!("https://github.com/{}/{}", segments[0], segments[1]) != value {
        return Err(SkillCatalogError::Malformed(
            "source repository is not canonically spelled".to_string(),
        ));
    }
    Ok(url)
}

fn validate_relative_path(value: &str, context: &str) -> Result<(), SkillCatalogError> {
    if value.is_empty()
        || value.len() > 512
        || value.starts_with('/')
        || value.ends_with('/')
        || value.contains('\\')
        || value.contains('\0')
        || value.split('/').count() > 16
        || !value.split('/').all(|segment| {
            segment != "." && segment != ".." && valid_segment_bytes(segment.as_bytes())
        })
    {
        return Err(SkillCatalogError::Malformed(format!(
            "{context} is not a safe relative path"
        )));
    }
    Ok(())
}

fn validate_segment(value: &str, context: &str) -> Result<(), SkillCatalogError> {
    if value.len() > 100 || value == "." || value == ".." || !valid_segment_bytes(value.as_bytes())
    {
        return Err(SkillCatalogError::Malformed(format!(
            "{context} is not a safe segment"
        )));
    }
    Ok(())
}

fn valid_segment_bytes(value: &[u8]) -> bool {
    !value.is_empty()
        && value[0].is_ascii_alphanumeric()
        && value
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn validate_text(value: &str, maximum: usize, context: &str) -> Result<(), SkillCatalogError> {
    if value.is_empty() || value.chars().count() > maximum || value.contains('\0') {
        return Err(SkillCatalogError::Malformed(format!(
            "{context} is empty or too long"
        )));
    }
    Ok(())
}

fn validate_string_set(
    values: &[String],
    minimum: usize,
    context: &str,
) -> Result<(), SkillCatalogError> {
    if values.len() < minimum || values.len() > 32 {
        return Err(SkillCatalogError::Malformed(format!(
            "{context} count is out of bounds"
        )));
    }
    let mut unique = BTreeSet::new();
    for value in values {
        validate_segment(value, context)?;
        if !unique.insert(value) {
            return Err(SkillCatalogError::Malformed(format!(
                "{context} contains duplicates"
            )));
        }
    }
    Ok(())
}

fn validate_hex(value: &str, length: usize, context: &str) -> Result<(), SkillCatalogError> {
    if value.len() != length
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(SkillCatalogError::Malformed(format!(
            "{context} must be lowercase hexadecimal"
        )));
    }
    Ok(())
}

fn parse_utc_timestamp(value: &str, context: &str) -> Result<OffsetDateTime, SkillCatalogError> {
    let bytes = value.as_bytes();
    let canonical = bytes.len() == 20
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes[19] == b'Z'
        && bytes.iter().enumerate().all(|(index, byte)| {
            matches!(index, 4 | 7 | 10 | 13 | 16 | 19) || byte.is_ascii_digit()
        });
    if !canonical {
        return Err(SkillCatalogError::Malformed(format!(
            "{context} must use UTC RFC 3339 second precision"
        )));
    }
    OffsetDateTime::parse(value, &Rfc3339)
        .map_err(|error| SkillCatalogError::Malformed(format!("{context}: {error}")))
}

fn client_identity(client: &SkillClientName) -> (&'static str, u8) {
    match client {
        SkillClientName::Codex => ("Codex", 0),
        SkillClientName::ClaudeCode => ("Claude Code", 1),
        SkillClientName::AgentKit => ("AgentKit", 2),
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn decode_canonical_base64(value: &str, context: &str) -> Result<Vec<u8>, SkillCatalogError> {
    let decoded = BASE64
        .decode(value)
        .map_err(|error| SkillCatalogError::Malformed(format!("{context}: {error}")))?;
    if BASE64.encode(&decoded) != value {
        return Err(SkillCatalogError::Malformed(format!(
            "{context} is not canonical padded base64"
        )));
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests;
