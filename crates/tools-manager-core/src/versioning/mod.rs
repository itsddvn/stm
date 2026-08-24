use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    application::adapters::FixtureWorkspace,
    domain::{
        application_update::{ApplicationUpdateKind, ApplicationUpdateRecord, UpdateExecutionMode},
        inventory::{Freshness, InventoryState},
        skill::SkillRecord,
        tool::ToolRecord,
    },
    error::CoreError,
};

mod runtime;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolUpdateEvidence {
    pub tool_id: String,
    pub authority: String,
    pub current_version: String,
    pub target_version: String,
    pub update_available: bool,
    pub freshness: Freshness,
    pub source_authority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillUpdateEvidence {
    pub skill_id: String,
    pub current_revision: String,
    pub target_revision: String,
    pub update_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProductUpdateEvidence {
    pub current_version: String,
    pub target_version: String,
    pub available: bool,
}

#[derive(Debug, Clone, Default)]
pub struct VersionCatalog {
    pub tool_updates: BTreeMap<String, ToolUpdateEvidence>,
    pub skill_updates: BTreeMap<String, SkillUpdateEvidence>,
    pub product_update: Option<ProductUpdateEvidence>,
}

pub fn load_version_catalog(workspace: &FixtureWorkspace) -> Result<VersionCatalog, CoreError> {
    if workspace.has_skill_home_override() {
        return runtime::load_runtime_version_catalog(workspace);
    }
    let tool_updates: Vec<ToolUpdateEvidence> =
        workspace.read_json("tests/fixtures/tools/update-metadata.json")?;
    let skill_updates: Vec<SkillUpdateEvidence> =
        workspace.read_json("tests/fixtures/skills/update-metadata.json")?;
    let product_update: ProductUpdateEvidence =
        workspace.read_json("tests/fixtures/catalog/product-update.json")?;

    Ok(VersionCatalog {
        tool_updates: tool_updates
            .into_iter()
            .map(|update| (update.tool_id.clone(), update))
            .collect(),
        skill_updates: skill_updates
            .into_iter()
            .map(|update| (update.skill_id.clone(), update))
            .collect(),
        product_update: Some(product_update),
    })
}

pub fn build_application_updates(
    tools: &[ToolRecord],
    skills: &[SkillRecord],
    versions: &VersionCatalog,
) -> Vec<ApplicationUpdateRecord> {
    let mut updates = Vec::new();

    for tool in tools {
        let Some(update) = versions.tool_updates.get(&tool.id) else {
            continue;
        };
        if !update.update_available
            || tool.installed_version.as_deref() == Some(update.target_version.as_str())
        {
            continue;
        }
        if !matches!(
            tool.state,
            InventoryState::ManagedCurrent | InventoryState::ManagedUpdateAvailable
        ) {
            continue;
        }

        updates.push(ApplicationUpdateRecord {
            id: format!("update-{}", tool.id),
            resource_type: ApplicationUpdateKind::Tool,
            name: tool.name.clone(),
            current: tool
                .installed_version
                .clone()
                .unwrap_or_else(|| update.current_version.clone()),
            target: update.target_version.clone(),
            execution_mode: UpdateExecutionMode::from(tool.execution_mode.clone()),
            selected: false,
            risk: match update.authority.as_str() {
                "manager" => format!("Managed by {}", tool.manager),
                "vendor" => "Vendor channel controls execution".to_string(),
                "pinned_git" => {
                    "Pinned Git metadata requires reviewed source authority".to_string()
                }
                _ => "Update authority is informational only in the read-only phase".to_string(),
            },
        });
    }

    for skill in skills {
        let Some(update) = versions.skill_updates.get(&skill.id) else {
            continue;
        };
        if !update.update_available {
            continue;
        }

        updates.push(ApplicationUpdateRecord {
            id: format!("update-{}", skill.id),
            resource_type: ApplicationUpdateKind::Skill,
            name: skill.name.clone(),
            current: update.current_revision.clone(),
            target: update.target_revision.clone(),
            execution_mode: UpdateExecutionMode::ManagedExecute,
            selected: false,
            risk: if skill.state == InventoryState::Modified {
                "Blocked by local modification".to_string()
            } else {
                format!("{} target files changed", skill.diff.len())
            },
        });
    }

    if let Some(product) = &versions.product_update {
        if product.available {
            updates.push(ApplicationUpdateRecord {
                id: "update-product".to_string(),
                resource_type: ApplicationUpdateKind::Product,
                name: "STM".to_string(),
                current: product.current_version.clone(),
                target: product.target_version.clone(),
                execution_mode: UpdateExecutionMode::SignedProductUpdate,
                selected: false,
                risk: "Separate signed product channel".to_string(),
            });
        }
    }

    updates
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn workspace() -> FixtureWorkspace {
        FixtureWorkspace::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
    }

    #[test]
    fn loads_update_catalog() {
        let versions = load_version_catalog(&workspace()).expect("versions");
        assert!(versions.tool_updates.contains_key("codex-cli"));
        assert!(versions.skill_updates.contains_key("frontend-design"));
        assert!(versions.product_update.is_some());
    }
}
