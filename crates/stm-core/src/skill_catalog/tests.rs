use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::*;

#[derive(Clone)]
struct FixedClock(OffsetDateTime);

impl FixedClock {
    fn at(value: &str) -> Self {
        Self(OffsetDateTime::parse(value, &Rfc3339).expect("valid test time"))
    }
}

impl CatalogClock for FixedClock {
    fn now_utc(&self) -> OffsetDateTime {
        self.0
    }
}

#[derive(Clone, Default)]
struct FixtureRemote {
    responses: Arc<BTreeMap<&'static str, Vec<u8>>>,
}

impl FixtureRemote {
    fn bundled() -> Self {
        Self {
            responses: Arc::new(BTreeMap::from([
                (CATALOG_MANIFEST_URL, BUNDLED_MANIFEST.to_vec()),
                (CATALOG_SIGNATURE_URL, BUNDLED_SIGNATURE.to_vec()),
                (CATALOG_PAYLOAD_URL, BUNDLED_CATALOG.to_vec()),
            ])),
        }
    }
}

impl CatalogRemote for FixtureRemote {
    fn fetch(
        &self,
        url: &'static str,
        _maximum_bytes: usize,
    ) -> Result<Vec<u8>, SkillCatalogError> {
        self.responses
            .get(url)
            .cloned()
            .ok_or_else(|| SkillCatalogError::Transport("fixture offline".to_string()))
    }
}

#[derive(Default)]
struct FixtureStore(Mutex<Option<SnapshotParts>>);

impl CatalogSnapshotStore for FixtureStore {
    fn read_state(&self) -> Result<SnapshotParts, SkillCatalogError> {
        self.0
            .lock()
            .map_err(|_| SkillCatalogError::Persistence("fixture catalog lock poisoned".into()))?
            .clone()
            .ok_or_else(|| SkillCatalogError::Persistence("fixture catalog is empty".into()))
    }

    fn persist_state(&self, snapshot: &VerifiedSkillCatalog) -> Result<(), SkillCatalogError> {
        *self.0.lock().map_err(|_| {
            SkillCatalogError::Persistence("fixture catalog lock poisoned".into())
        })? = Some(snapshot.authenticated_parts().clone());
        Ok(())
    }
}

#[test]
fn bundled_catalog_is_authenticated_and_matches_pinned_source() {
    let store = FixtureStore::default();
    let service = SkillCatalogService::new(
        FixtureRemote::default(),
        FixedClock::at("2026-08-21T03:08:00Z"),
    );

    let verified = service
        .load_last_good_or_bundled(&store, None)
        .expect("bundled catalog verifies");

    assert_eq!(verified.origin, SkillCatalogOrigin::Bundled);
    let skill = verified
        .catalog
        .find_by_id("frontend-design")
        .expect("trusted skill exists");
    assert_eq!(skill.source.commit.len(), 40);
    assert_eq!(skill.source.tree_sha256.len(), 64);
    assert_eq!(
        verified.catalog.find_by_source(
            "https://github.com/anthropics/skills.git",
            "skills/frontend-design",
        ),
        Some(skill),
    );
}

#[test]
fn remote_snapshot_persists_as_last_known_good() {
    let store = FixtureStore::default();
    let clock = FixedClock::at("2026-08-21T03:08:00Z");
    let online = SkillCatalogService::new(FixtureRemote::bundled(), clock.clone());

    let remote = online
        .refresh_remote(&store, None)
        .expect("remote fixture verifies");
    assert_eq!(remote.origin, SkillCatalogOrigin::Remote);

    let offline = SkillCatalogService::new(FixtureRemote::default(), clock);
    let persisted = offline
        .load_last_good_or_bundled(&store, Some(&AcceptedCatalogIdentity::from(&remote)))
        .expect("last known good remains usable offline");
    assert_eq!(persisted.origin, SkillCatalogOrigin::Persisted);
    assert_eq!(persisted.payload_sha256, remote.payload_sha256);
}

#[test]
fn invalid_signature_expiry_and_downgrade_are_rejected() {
    let now = FixedClock::at("2026-08-21T03:08:00Z").now_utc();
    let mut signature: serde_json::Value =
        serde_json::from_slice(BUNDLED_SIGNATURE).expect("fixture signature");
    let original_signature = signature["signature"]
        .as_str()
        .expect("fixture signature text");
    let replacement = if original_signature.starts_with('A') {
        "B"
    } else {
        "A"
    };
    signature["signature"] =
        serde_json::Value::String(format!("{replacement}{}", &original_signature[1..]));
    let invalid = SnapshotParts {
        catalog: BUNDLED_CATALOG.to_vec(),
        manifest: BUNDLED_MANIFEST.to_vec(),
        signature: serde_json::to_vec(&signature).expect("mutated signature"),
    };
    assert!(matches!(
        verify_snapshot(
            &invalid,
            now,
            None,
            Freshness::RequireFresh,
            SkillCatalogOrigin::Remote,
        ),
        Err(SkillCatalogError::BadSignature)
    ));

    let store = FixtureStore::default();
    let expired = SkillCatalogService::new(
        FixtureRemote::default(),
        FixedClock::at("2028-01-01T00:00:00Z"),
    )
    .load_last_good_or_bundled(&store, None);
    assert!(matches!(
        expired,
        Err(SkillCatalogError::Unavailable { .. })
    ));

    let downgrade = SkillCatalogService::new(
        FixtureRemote::default(),
        FixedClock::at("2026-08-21T03:08:00Z"),
    )
    .load_last_good_or_bundled(
        &store,
        Some(&AcceptedCatalogIdentity {
            catalog_version: 2,
            payload_sha256: "0".repeat(64),
        }),
    );
    assert!(matches!(
        downgrade,
        Err(SkillCatalogError::Unavailable { .. })
    ));
}

#[test]
fn catalog_rejects_traversal_and_duplicate_logical_targets() {
    let mut catalog: AuthenticatedSkillCatalog =
        serde_json::from_slice(BUNDLED_CATALOG).expect("bundled catalog JSON");
    catalog.skills[0].targets[0].relative_path = "../frontend-design".to_string();
    assert!(matches!(
        validate_catalog(&catalog),
        Err(SkillCatalogError::Malformed(_))
    ));

    let mut duplicate: AuthenticatedSkillCatalog =
        serde_json::from_slice(BUNDLED_CATALOG).expect("bundled catalog JSON");
    let repeated_target = duplicate.skills[0].targets[0].clone();
    duplicate.skills[0].targets.push(repeated_target);
    assert!(matches!(
        validate_catalog(&duplicate),
        Err(SkillCatalogError::Malformed(_))
    ));
}
