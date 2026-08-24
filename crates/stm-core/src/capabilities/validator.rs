use crate::{
    catalog::ToolCatalogEntry,
    domain::{
        inventory::InventoryState,
        provider::{InstallProviderPreference, ProviderInventory, ProviderKind, ProviderTrust},
        recipe::ResolvedRecipe,
        setup::SetupRowAction,
        tool::ToolRecord,
    },
};

pub struct ValidatorService;

impl ValidatorService {
    pub fn row_action(tool: &ToolRecord) -> SetupRowAction {
        match tool.state {
            InventoryState::ManagedCurrent => SetupRowAction::Installed,
            _ if tool.execution_mode == crate::domain::inventory::ExecutionMode::VendorHandoff => {
                SetupRowAction::Handoff
            }
            InventoryState::ManagedUpdateAvailable => SetupRowAction::Update,
            InventoryState::Missing => SetupRowAction::Install,
            InventoryState::Unsupported | InventoryState::Blocked => SetupRowAction::Blocked,
            _ if tool.execution_mode == crate::domain::inventory::ExecutionMode::DetectOnly => {
                SetupRowAction::Guidance
            }
            _ => SetupRowAction::Blocked,
        }
    }
}

pub fn adapter_supported(
    entry: &ToolCatalogEntry,
    target: &str,
    preference: InstallProviderPreference,
    providers: &ProviderInventory,
) -> Option<ResolvedRecipe> {
    let mut adapters: Vec<&str> = Vec::new();
    match preference {
        InstallProviderPreference::PreferBun => {
            adapters.extend([
                "bun_package",
                "npm_package",
                "homebrew_formula",
                "homebrew_cask",
            ]);
        }
        InstallProviderPreference::PreferHomebrew | InstallProviderPreference::Automatic => {
            adapters.extend([
                "homebrew_formula",
                "homebrew_cask",
                "bun_package",
                "npm_package",
            ]);
        }
    }
    adapters.extend([
        "winget_package",
        "apt_package",
        "dnf_package",
        "vendor_receipt",
    ]);

    for adapter in adapters {
        if let Some(mapping) = entry
            .mappings
            .iter()
            .find(|mapping| mapping.platform == target && mapping.adapter == adapter)
        {
            if mapping.mapping_status == crate::domain::inventory::MappingStatus::DetectOnly {
                continue;
            }
            if adapter.starts_with("homebrew")
                && providers
                    .homebrew
                    .as_ref()
                    .is_none_or(|provider| provider.trust != ProviderTrust::ApprovedRoot)
            {
                continue;
            }
            if adapter == "bun_package" && providers.trusted(ProviderKind::Bun).is_none() {
                continue;
            }
            if adapter == "npm_package"
                && (providers.trusted(ProviderKind::Npm).is_none()
                    || providers.trusted(ProviderKind::Node).is_none())
            {
                continue;
            }
            return Some(ResolvedRecipe {
                resource_id: entry.id.clone(),
                desired_action: "install".to_string(),
                adapter: mapping.adapter.clone(),
                package_id: mapping.package_id.clone(),
                mapping_id: format!("{}:{}", mapping.manager, mapping.package_id),
                step: mapping.step.clone(),
                blocked_reason: None,
                depends_on: Vec::new(),
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::inventory::{
        CatalogStatus, ExecutionMode, MappingStatus, OwnershipKind, PrivilegeRequirement,
    };

    fn tool(state: InventoryState, mode: ExecutionMode) -> ToolRecord {
        ToolRecord {
            id: "git".into(),
            name: "Git".into(),
            summary: "source".into(),
            kind: "CLI tool".into(),
            groups: vec!["source_control".into()],
            recommended: true,
            catalog_status: CatalogStatus::Locked,
            mapping_status: MappingStatus::Supported,
            state,
            owner: "Homebrew".into(),
            ownership_kind: OwnershipKind::ManagerOwned,
            execution_mode: mode,
            installed_version: None,
            available_version: None,
            manager: "homebrew".into(),
            package_id: "git".into(),
            platform: "macos_arm64".into(),
            privilege: PrivilegeRequirement::None,
            lifecycle_confidence: "high".into(),
            reason_code: None,
        }
    }

    #[test]
    fn current_tools_are_installed_not_update() {
        assert_eq!(
            ValidatorService::row_action(&tool(
                InventoryState::ManagedCurrent,
                ExecutionMode::ManagedExecute
            )),
            SetupRowAction::Installed
        );
        assert_eq!(
            ValidatorService::row_action(&tool(
                InventoryState::ManagedUpdateAvailable,
                ExecutionMode::ManagedExecute
            )),
            SetupRowAction::Update
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn npm_recipe_requires_both_npm_and_node_providers() {
        let workspace = crate::adapters::FixtureWorkspace::new(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
        );
        let catalog = crate::catalog::load_tool_catalog(&workspace).expect("catalog");
        let codex = catalog
            .tools
            .iter()
            .find(|entry| entry.id == "codex-cli")
            .expect("Codex catalog entry");
        let provider = |kind| crate::domain::provider::DetectedProvider {
            kind,
            path: "/reviewed/provider".to_string(),
            version: Some("1.0.0".to_string()),
            trust: ProviderTrust::ApprovedRoot,
        };
        let mut providers = ProviderInventory {
            generation: "test".to_string(),
            homebrew: None,
            bun: None,
            node: None,
            npm: Some(provider(ProviderKind::Npm)),
        };
        assert!(adapter_supported(
            codex,
            crate::inventory::current_platform_slug(),
            InstallProviderPreference::Automatic,
            &providers,
        )
        .is_none());

        providers.node = Some(provider(ProviderKind::Node));
        assert_eq!(
            adapter_supported(
                codex,
                crate::inventory::current_platform_slug(),
                InstallProviderPreference::Automatic,
                &providers,
            )
            .map(|recipe| recipe.adapter),
            Some("npm_package".to_string())
        );
    }
}
