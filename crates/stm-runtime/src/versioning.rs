use std::collections::BTreeMap;

use crate::{
    skill_catalog::load_current_authenticated_catalog, skill_inventory::scan_runtime_skills,
    storage::SqliteSnapshotStore,
};
use stm_core::{
    adapters::FixtureWorkspace,
    mcp::{discover_mcp, McpInventorySnapshot},
    ports::LiveInventoryPort,
    skills::SkillInventorySnapshot,
    versioning::{ProductUpdateEvidence, SkillUpdateEvidence, VersionCatalog},
    CoreError,
};

#[derive(Debug, Default)]
pub struct RuntimeLiveInventory;

impl LiveInventoryPort for RuntimeLiveInventory {
    fn load_version_catalog(
        &self,
        workspace: &FixtureWorkspace,
    ) -> Result<VersionCatalog, CoreError> {
        load_runtime_version_catalog(workspace)
    }

    fn scan_skills(
        &self,
        workspace: &FixtureWorkspace,
        versions: &VersionCatalog,
    ) -> Result<SkillInventorySnapshot, CoreError> {
        scan_runtime_skills(workspace, versions)
    }

    fn discover_mcp(
        &self,
        workspace: &FixtureWorkspace,
    ) -> Result<McpInventorySnapshot, CoreError> {
        discover_mcp(workspace)
    }
}

pub fn load_runtime_version_catalog(
    workspace: &FixtureWorkspace,
) -> Result<VersionCatalog, CoreError> {
    let (store, _) = SqliteSnapshotStore::open(workspace.db_path())?;
    let receipts = store.load_managed_skill_receipts()?;
    let Ok(trusted) = load_current_authenticated_catalog(&workspace.db_path()) else {
        return Ok(VersionCatalog {
            product_update: Some(internal_product_update()),
            ..VersionCatalog::default()
        });
    };
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
        product_update: Some(internal_product_update()),
    })
}

fn internal_product_update() -> ProductUpdateEvidence {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    ProductUpdateEvidence {
        target_version: current_version.clone(),
        current_version,
        available: false,
    }
}

fn short_commit(value: &str) -> &str {
    &value[..value.len().min(12)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_skill_catalog_does_not_hide_internal_product_state() {
        let root = tempfile::tempdir().expect("tempdir");
        let workspace =
            FixtureWorkspace::new(root.path()).with_db_path(root.path().join("inventory.sqlite"));

        let versions = load_runtime_version_catalog(&workspace).expect("version catalog");

        let product = versions.product_update.expect("product evidence");
        assert!(!product.available);
        assert!(versions.skill_updates.is_empty());
        assert!(versions.tool_updates.is_empty());
    }
    #[test]
    fn internal_inventory_exposes_unavailable_product_evidence() {
        let product = internal_product_update();

        assert!(!product.available);
        assert_eq!(product.current_version, product.target_version);
    }
}
