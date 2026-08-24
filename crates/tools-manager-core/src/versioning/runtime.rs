use std::collections::BTreeMap;

use crate::{
    application::adapters::FixtureWorkspace, error::CoreError,
    skill_catalog::load_current_authenticated_catalog, storage::SqliteSnapshotStore,
};

use super::{SkillUpdateEvidence, VersionCatalog};

pub(super) fn load_runtime_version_catalog(
    workspace: &FixtureWorkspace,
) -> Result<VersionCatalog, CoreError> {
    let (store, _) = SqliteSnapshotStore::open(workspace.db_path())?;
    let receipts = store.load_managed_skill_receipts()?;
    let trusted = load_current_authenticated_catalog(&workspace.db_path()).map_err(|_| {
        CoreError::LifecycleEvidenceChanged(
            "authenticated skill catalog is unavailable for update detection".to_string(),
        )
    })?;
    let mut skill_updates = BTreeMap::new();
    for entry in &trusted.catalog.skills {
        let current = receipts
            .iter()
            .find(|(_, receipt)| receipt.skill_id == entry.id)
            .map(|(_, receipt)| receipt.source.commit.clone());
        if let Some(current) = current {
            skill_updates.insert(
                entry.id.clone(),
                SkillUpdateEvidence {
                    skill_id: entry.id.clone(),
                    current_revision: format!("Git {}", short_commit(&current)),
                    target_revision: format!("Git {}", short_commit(&entry.source.commit)),
                    update_available: current != entry.source.commit,
                },
            );
        }
    }

    Ok(VersionCatalog {
        tool_updates: BTreeMap::new(),
        skill_updates,
        product_update: None,
    })
}

fn short_commit(value: &str) -> &str {
    &value[..value.len().min(12)]
}
