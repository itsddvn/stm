use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::{
    application::adapters::{
        ensure_https_url, json_array, json_object, json_string, FixtureWorkspace,
    },
    domain::{
        inventory::{
            CatalogStatus, ExecutionMode, MappingStatus, OwnershipKind, PrivilegeRequirement,
        },
        profile::PlatformProfileDocument,
        recipe::RecipeStepType,
    },
    error::CoreError,
};

const KNOWN_ADAPTERS: &[&str] = &[
    "winget_package",
    "homebrew_formula",
    "homebrew_cask",
    "npm_package",
    "bun_package",
    "apt_package",
    "dnf_package",
    "pacman_package",
    "vendor_receipt",
    "system_bundle",
    "external_probe",
];

const KNOWN_UPDATE_AUTHORITIES: &[&str] = &["manager", "vendor", "pinned_git", "none"];
const KNOWN_PROBE_KEYS: &[&str] = &[
    "git",
    "orca",
    "cmux",
    "docker_desktop",
    "orbstack",
    "agentkit",
    "oh_my_pi",
    "codex",
    "grok",
    "cloudflared",
    "unavailable",
];

#[derive(Debug, Clone)]
pub struct ToolCatalogSnapshot {
    pub version: String,
    pub tools: Vec<ToolCatalogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolCatalogDocument {
    pub version: String,
    pub tools: Vec<ToolCatalogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolCatalogEntry {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub kind: String,
    pub groups: Vec<String>,
    pub tags: Vec<String>,
    pub recommended: bool,
    pub catalog_status: CatalogStatus,
    pub license: String,
    pub homepage: String,
    pub source_url: String,
    pub aliases: Vec<String>,
    pub probe_key: String,
    pub mappings: Vec<ToolCatalogMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolCatalogMapping {
    pub platform: String,
    pub manager: String,
    pub package_id: String,
    pub adapter: String,
    pub step: RecipeStepType,
    pub mapping_status: MappingStatus,
    pub execution_mode: ExecutionMode,
    pub ownership_kind: OwnershipKind,
    pub privilege: PrivilegeRequirement,
    pub update_authority: String,
}

impl ToolCatalogSnapshot {
    pub fn get(&self, id: &str) -> Option<&ToolCatalogEntry> {
        self.tools.iter().find(|entry| entry.id == id)
    }
}

impl ToolCatalogEntry {
    pub fn primary_mapping(&self, platform: &str) -> Option<&ToolCatalogMapping> {
        self.mappings
            .iter()
            .find(|mapping| mapping.platform == platform)
    }
}

pub fn load_tool_catalog(_workspace: &FixtureWorkspace) -> Result<ToolCatalogSnapshot, CoreError> {
    let recommended_raw: JsonValue =
        serde_json::from_str(include_str!("../../../../catalog/tools/recommended.json"))?;
    let candidates_raw: JsonValue =
        serde_json::from_str(include_str!("../../../../catalog/tools/candidates.json"))?;
    reject_catalog_commands(&recommended_raw, "catalog/tools/recommended.json")?;
    reject_catalog_commands(&candidates_raw, "catalog/tools/candidates.json")?;

    validate_tool_catalog_shape(&recommended_raw, "catalog/tools/recommended.json")?;
    validate_tool_catalog_shape(&candidates_raw, "catalog/tools/candidates.json")?;

    let recommended: ToolCatalogDocument = serde_json::from_value(recommended_raw)?;
    let candidates: ToolCatalogDocument = serde_json::from_value(candidates_raw)?;

    validate_tool_catalog_semantics(&recommended, &candidates)?;

    Ok(ToolCatalogSnapshot {
        version: recommended.version.clone(),
        tools: recommended
            .tools
            .into_iter()
            .chain(candidates.tools)
            .collect(),
    })
}

pub fn load_platform_profiles() -> Result<PlatformProfileDocument, CoreError> {
    let document: PlatformProfileDocument =
        serde_json::from_str(include_str!("../../../../catalog/tools/profiles.json"))?;
    if document.profiles.is_empty() {
        return Err(CoreError::MalformedInput(
            "platform profile catalog must declare at least one target".to_string(),
        ));
    }
    Ok(document)
}

fn validate_tool_catalog_shape(value: &JsonValue, context: &str) -> Result<(), CoreError> {
    let object = json_object(value, context)?;
    let version = object
        .get("version")
        .ok_or_else(|| CoreError::MalformedInput(format!("{context} missing version")))?;
    let tools = object
        .get("tools")
        .ok_or_else(|| CoreError::MalformedInput(format!("{context} missing tools")))?;
    let _ = json_string(version, &format!("{context}.version"))?;
    let tools = json_array(tools, &format!("{context}.tools"))?;

    for (index, item) in tools.iter().enumerate() {
        let tool = json_object(item, &format!("{context}.tools[{index}]"))?;
        for key in [
            "id",
            "name",
            "summary",
            "kind",
            "groups",
            "recommended",
            "catalogStatus",
            "license",
            "homepage",
            "sourceUrl",
            "aliases",
            "probeKey",
            "mappings",
        ] {
            if !tool.contains_key(key) {
                return Err(CoreError::MalformedInput(format!(
                    "{context}.tools[{index}] missing {key}"
                )));
            }
        }
    }

    Ok(())
}

fn validate_tool_catalog_semantics(
    recommended: &ToolCatalogDocument,
    candidates: &ToolCatalogDocument,
) -> Result<(), CoreError> {
    if recommended.tools.is_empty() {
        return Err(CoreError::MalformedInput(
            "recommended catalog must contain at least one tool".to_string(),
        ));
    }
    if candidates.tools.is_empty() {
        return Err(CoreError::MalformedInput(
            "candidate catalog must retain non-recommended tools".to_string(),
        ));
    }

    let mut ids = BTreeSet::new();
    let mut aliases = BTreeMap::<String, String>::new();
    let mut mappings = BTreeSet::new();

    for (bucket, entries) in [
        ("recommended", &recommended.tools),
        ("candidates", &candidates.tools),
    ] {
        for entry in entries {
            if !ids.insert(entry.id.clone()) {
                return Err(CoreError::MalformedInput(format!(
                    "duplicate tool id: {}",
                    entry.id
                )));
            }

            if entry.groups.is_empty() {
                return Err(CoreError::MalformedInput(format!(
                    "tool {} must declare at least one group",
                    entry.id
                )));
            }
            ensure_https_url(&entry.homepage, &format!("{}.homepage", entry.id))?;
            ensure_https_url(&entry.source_url, &format!("{}.sourceUrl", entry.id))?;

            if !KNOWN_PROBE_KEYS.contains(&entry.probe_key.as_str()) {
                return Err(CoreError::UnsupportedSchema(format!(
                    "tool {} references unknown probe key {}",
                    entry.id, entry.probe_key
                )));
            }

            match bucket {
                "recommended" => {
                    if !entry.recommended || entry.catalog_status == CatalogStatus::Candidate {
                        return Err(CoreError::MalformedInput(format!(
                            "recommended entry {} must remain recommended and non-candidate",
                            entry.id
                        )));
                    }
                }
                "candidates" => {
                    if entry.recommended || entry.catalog_status != CatalogStatus::Candidate {
                        return Err(CoreError::MalformedInput(format!(
                            "candidate entry {} must remain non-recommended candidate",
                            entry.id
                        )));
                    }
                }
                _ => {}
            }

            for alias in &entry.aliases {
                let key = alias.to_ascii_lowercase();
                if let Some(existing) = aliases.insert(key.clone(), entry.id.clone()) {
                    return Err(CoreError::MalformedInput(format!(
                        "tool alias collision: {key} shared by {existing} and {}",
                        entry.id
                    )));
                }
            }

            for mapping in &entry.mappings {
                if !KNOWN_ADAPTERS.contains(&mapping.adapter.as_str()) {
                    return Err(CoreError::UnsupportedSchema(format!(
                        "tool {} references unknown adapter {}",
                        entry.id, mapping.adapter
                    )));
                }
                if !KNOWN_UPDATE_AUTHORITIES.contains(&mapping.update_authority.as_str()) {
                    return Err(CoreError::UnsupportedSchema(format!(
                        "tool {} references unknown update authority {}",
                        entry.id, mapping.update_authority
                    )));
                }
                let expected_step = recipe_step_for_mapping(mapping);
                if mapping.step != expected_step {
                    return Err(CoreError::UnsupportedSchema(format!(
                        "tool {} mapping {}:{} declares incompatible recipe step {:?}",
                        entry.id, mapping.manager, mapping.package_id, mapping.step
                    )));
                }
                let mapping_key = format!("{}|{}|{}", entry.id, mapping.platform, mapping.manager);
                if !mappings.insert(mapping_key.clone()) {
                    return Err(CoreError::MalformedInput(format!(
                        "tool {} has overlapping mapping {}",
                        entry.id, mapping_key
                    )));
                }
            }
        }
    }

    Ok(())
}

fn recipe_step_for_mapping(mapping: &ToolCatalogMapping) -> RecipeStepType {
    if mapping.execution_mode == ExecutionMode::VendorHandoff {
        return RecipeStepType::VendorHandoff;
    }
    match mapping.adapter.as_str() {
        "winget_package" | "homebrew_formula" | "homebrew_cask" | "npm_package" | "bun_package"
        | "apt_package" | "dnf_package" | "pacman_package" => RecipeStepType::ManagerPackage,
        "vendor_receipt" => RecipeStepType::SignedArtifact,
        "system_bundle" | "external_probe" => RecipeStepType::VerifyPostcondition,
        _ => RecipeStepType::VerifyPostcondition,
    }
}

fn reject_catalog_commands(value: &JsonValue, context: &str) -> Result<(), CoreError> {
    fn walk(value: &JsonValue, path: &str) -> Result<(), CoreError> {
        match value {
            JsonValue::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    walk(item, &format!("{path}[{index}]"))?;
                }
            }
            JsonValue::Object(map) => {
                for (key, item) in map {
                    let next = format!("{path}.{key}");
                    if matches!(
                        key.to_ascii_lowercase().as_str(),
                        "command" | "args" | "executable" | "shell" | "script"
                    ) {
                        return Err(CoreError::MalformedInput(format!(
                            "catalog data may not provide shell or executable content: {next}"
                        )));
                    }
                    walk(item, &next)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    walk(value, context)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn workspace() -> FixtureWorkspace {
        FixtureWorkspace::new(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
    }

    #[test]
    fn loads_phase_three_catalog() {
        let snapshot = load_tool_catalog(&workspace()).expect("catalog");
        assert!(snapshot.tools.iter().any(|tool| tool.recommended));
        assert!(load_platform_profiles()
            .expect("profiles")
            .for_target("macos_arm64")
            .is_some());
        assert!(snapshot.tools.iter().any(|tool| tool.id == "cursor"));
    }

    #[test]
    fn rejects_catalog_commands_and_duplicate_aliases() {
        let invalid = json!({
            "version": "2026-08-20",
            "tools": [{
                "id": "git",
                "name": "Git",
                "summary": "summary",
                "kind": "cli_tool",
                "groups": ["source_control"],
                "tags": [],
                "recommended": true,
                "catalogStatus": "locked",
                "license": "GPL-2.0-only",
                "homepage": "https://git-scm.com/",
                "sourceUrl": "https://github.com/git/git",
                "aliases": ["git"],
                "probeKey": "git",
                "command": "git --version",
                "mappings": [{
                    "platform": "macos_arm64",
                    "manager": "homebrew",
                    "packageId": "git",
                    "adapter": "homebrew_formula",
                    "mappingStatus": "supported",
                    "executionMode": "managed_execute",
                    "ownershipKind": "manager_owned",
                    "privilege": "none",
                    "updateAuthority": "manager"
                }]
            }]
        });
        let error = reject_catalog_commands(&invalid, "invalid").expect_err("command rejected");
        assert!(error
            .to_string()
            .contains("catalog data may not provide shell or executable content"));
    }

    #[test]
    fn rejects_unknown_and_incompatible_recipe_steps() {
        let invalid_step = json!({
            "version": "2026-08-20",
            "tools": [{
                "id": "git",
                "name": "Git",
                "summary": "summary",
                "kind": "cli_tool",
                "groups": ["source_control"],
                "tags": [],
                "recommended": true,
                "catalogStatus": "locked",
                "license": "GPL-2.0-only",
                "homepage": "https://git-scm.com/",
                "sourceUrl": "https://github.com/git/git",
                "aliases": ["git"],
                "probeKey": "git",
                "mappings": [{
                    "platform": "macos_arm64",
                    "manager": "homebrew",
                    "packageId": "git",
                    "adapter": "homebrew_formula",
                    "step": "shell_command",
                    "mappingStatus": "supported",
                    "executionMode": "managed_execute",
                    "ownershipKind": "manager_owned",
                    "privilege": "none",
                    "updateAuthority": "manager"
                }]
            }]
        });
        assert!(serde_json::from_value::<ToolCatalogDocument>(invalid_step).is_err());

        let workspace = workspace();
        let mut recommended: ToolCatalogDocument = workspace
            .read_json("catalog/tools/recommended.json")
            .expect("recommended catalog");
        let candidates: ToolCatalogDocument = workspace
            .read_json("catalog/tools/candidates.json")
            .expect("candidate catalog");
        recommended.tools[0].mappings[0].step = RecipeStepType::SignedArtifact;
        let error = validate_tool_catalog_semantics(&recommended, &candidates)
            .expect_err("incompatible recipe step");
        assert!(error.to_string().contains("incompatible recipe step"));
    }
}
