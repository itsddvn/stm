use crate::{
    catalog::{load_platform_profiles, load_tool_catalog, ToolCatalogSnapshot},
    domain::{
        inventory::InventoryState,
        provider::{InstallProviderPreference, ProviderInventory},
        setup::{QuickSetupView, SetupRow, SetupRowAction},
        tool::ToolRecord,
    },
};

use super::validator::{adapter_supported, ValidatorService};

pub fn resolve_setup(
    catalog: &ToolCatalogSnapshot,
    tools: &[ToolRecord],
    target: &str,
    preference: InstallProviderPreference,
    providers: ProviderInventory,
    dismissed: bool,
) -> Result<QuickSetupView, crate::error::CoreError> {
    let profiles = load_platform_profiles()?;
    let profile = profiles.for_target(target);
    let defaults = profile
        .map(|profile| profile.defaults.as_slice())
        .unwrap_or(&[]);
    let optional = profile
        .map(|profile| profile.optional.as_slice())
        .unwrap_or(&[]);

    let mut rows = Vec::new();
    for tool in tools {
        let Some(entry) = catalog.get(&tool.id) else {
            continue;
        };
        let is_default = defaults.iter().any(|id| id == &tool.id);
        let is_optional = optional.iter().any(|id| id == &tool.id);
        if !is_default && !is_optional && !tool.recommended {
            continue;
        }
        let mut action = ValidatorService::row_action(tool);
        let mut reason = tool.reason_code.clone();
        let mut mapping_id = None;
        if matches!(action, SetupRowAction::Install) {
            if let Some(recipe) = adapter_supported(entry, target, preference, &providers) {
                mapping_id = Some(recipe.mapping_id);
            } else if tool.state == InventoryState::Missing {
                action = SetupRowAction::Blocked;
                reason = Some("No supported provider recipe for this machine".to_string());
            }
        }
        if tool.execution_mode == crate::domain::inventory::ExecutionMode::DetectOnly
            && tool.state != InventoryState::ManagedCurrent
        {
            action = SetupRowAction::Guidance;
        }
        rows.push(SetupRow {
            id: tool.id.clone(),
            name: tool.name.clone(),
            summary: tool.summary.clone(),
            selected: is_default
                && action != SetupRowAction::Installed
                && action != SetupRowAction::Blocked,
            optional: is_optional && !is_default,
            action,
            reason,
            owner: tool.owner.clone(),
            mapping_id,
        });
    }

    let tools = rows.iter().filter(|row| !row.optional).cloned().collect();
    let optional_tools = rows.into_iter().filter(|row| row.optional).collect();

    Ok(QuickSetupView {
        target: target.to_string(),
        preference,
        dismissed,
        providers,
        tools,
        optional_skills: Vec::new(),
        optional_mcp: optional_tools,
    })
}

pub fn current_target() -> String {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "macos_arm64".to_string()
    } else if cfg!(target_os = "macos") {
        "macos_x64".to_string()
    } else if cfg!(target_os = "windows") {
        "windows_x64".to_string()
    } else {
        "linux_x64".to_string()
    }
}

pub fn load_catalog_for_setup() -> Result<ToolCatalogSnapshot, crate::error::CoreError> {
    let workspace =
        crate::adapters::FixtureWorkspace::new(std::env::current_dir().unwrap_or_default());
    load_tool_catalog(&workspace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::FixtureWorkspace;
    use crate::domain::inventory::{
        CatalogStatus, ExecutionMode, InventoryState, MappingStatus, OwnershipKind,
        PrivilegeRequirement,
    };
    use crate::domain::provider::{DetectedProvider, ProviderKind, ProviderTrust};

    fn record(id: &str, state: InventoryState) -> ToolRecord {
        ToolRecord {
            id: id.into(),
            name: id.into(),
            summary: id.into(),
            kind: "CLI tool".into(),
            groups: vec!["source_control".into()],
            recommended: true,
            catalog_status: CatalogStatus::Locked,
            mapping_status: MappingStatus::Supported,
            state,
            owner: "Homebrew".into(),
            ownership_kind: OwnershipKind::ManagerOwned,
            execution_mode: ExecutionMode::ManagedExecute,
            installed_version: None,
            available_version: None,
            manager: "homebrew".into(),
            package_id: id.into(),
            platform: "macos_arm64".into(),
            privilege: PrivilegeRequirement::None,
            lifecycle_confidence: "high".into(),
            reason_code: None,
        }
    }

    #[test]
    fn selects_macos_defaults_except_installed() {
        let workspace = FixtureWorkspace::new(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
        );
        let catalog = load_tool_catalog(&workspace).expect("catalog");
        let providers = ProviderInventory {
            generation: "test".into(),
            homebrew: Some(DetectedProvider {
                kind: ProviderKind::Homebrew,
                version: Some("4.0".into()),
                path: "/opt/homebrew/bin/brew".into(),
                trust: ProviderTrust::ApprovedRoot,
            }),
            bun: None,
            node: None,
            npm: None,
        };
        let view = resolve_setup(
            &catalog,
            &[
                record("git", InventoryState::ManagedCurrent),
                record("orbstack", InventoryState::Missing),
            ],
            "macos_arm64",
            InstallProviderPreference::Automatic,
            providers,
            false,
        )
        .expect("setup");
        let git = view.tools.iter().find(|row| row.id == "git").expect("git");
        let orb = view
            .tools
            .iter()
            .find(|row| row.id == "orbstack")
            .expect("orb");
        assert_eq!(git.action, SetupRowAction::Installed);
        assert!(!git.selected);
        assert_eq!(orb.action, SetupRowAction::Install);
        assert!(orb.selected);
    }
}
