use crate::{
    catalog::ToolCatalogSnapshot,
    domain::{
        inventory::InventoryState,
        lifecycle::{
            LifecycleChildIntent, LifecyclePlanRequest, LifecycleResourceKind, MAX_BATCH_ITEMS,
        },
        setup::SetupRow,
        tool::ToolRecord,
    },
    error::CoreError,
};

pub struct InstallerService;

impl InstallerService {
    pub fn normalize_setup_queue(
        request: LifecyclePlanRequest,
        tools: &[ToolRecord],
        catalog: &ToolCatalogSnapshot,
        resolved_rows: &[SetupRow],
    ) -> Result<LifecyclePlanRequest, CoreError> {
        if request.action != "setup-queue" {
            return Ok(request);
        }
        let intents = if request.children.is_empty() {
            request
                .item_ids
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|raw_id| LifecycleChildIntent {
                    resource_kind: LifecycleResourceKind::Tool,
                    resource_id: raw_id.trim_start_matches("update-").to_string(),
                    desired_action: String::new(),
                    mapping_id: None,
                    depends_on: Vec::new(),
                })
                .collect::<Vec<_>>()
        } else {
            request.children.clone()
        };
        if intents.is_empty() {
            return Err(CoreError::MalformedInput(
                "setup-queue requires children or itemIds".to_string(),
            ));
        }
        if intents.len() > MAX_BATCH_ITEMS {
            return Err(CoreError::MalformedInput(format!(
                "setup-queue exceeds {MAX_BATCH_ITEMS} items"
            )));
        }
        let unique = intents
            .iter()
            .map(|intent| (intent.resource_kind.clone(), intent.resource_id.clone()))
            .collect::<std::collections::HashSet<_>>();
        if unique.len() != intents.len() {
            return Err(CoreError::MalformedInput(
                "setup-queue contains duplicate items".to_string(),
            ));
        }
        let mut children = Vec::new();
        for intent in intents {
            if intent.resource_kind != LifecycleResourceKind::Tool {
                children.push(LifecycleChildIntent {
                    resource_kind: intent.resource_kind,
                    resource_id: intent.resource_id,
                    desired_action: "review".to_string(),
                    mapping_id: None,
                    depends_on: Vec::new(),
                });
                continue;
            }
            let tool_id = intent.resource_id.trim_start_matches("update-").to_string();
            let Some(tool) = tools.iter().find(|tool| tool.id == tool_id) else {
                if catalog.get(&tool_id).is_none() {
                    if intent.desired_action == "review" && intent.mapping_id.is_none() {
                        children.push(LifecycleChildIntent {
                            resource_kind: LifecycleResourceKind::Tool,
                            resource_id: tool_id,
                            desired_action: "review".to_string(),
                            mapping_id: None,
                            depends_on: Vec::new(),
                        });
                        continue;
                    }
                    return Err(CoreError::MalformedInput(format!(
                        "unknown setup-queue item: {}",
                        intent.resource_id
                    )));
                }
                children.push(LifecycleChildIntent {
                    resource_kind: LifecycleResourceKind::Tool,
                    resource_id: tool_id,
                    desired_action: "install".to_string(),
                    mapping_id: None,
                    depends_on: Vec::new(),
                });
                continue;
            };
            let desired_action = match tool.state {
                InventoryState::Missing => "install",
                InventoryState::ManagedUpdateAvailable => "update",
                _ if tool.execution_mode
                    == crate::domain::inventory::ExecutionMode::VendorHandoff =>
                {
                    "update"
                }
                _ if tool.execution_mode == crate::domain::inventory::ExecutionMode::DetectOnly => {
                    "review"
                }
                InventoryState::ManagedCurrent => continue,
                _ => "review",
            };
            let mapping_id = if tool.installed_version.is_some() {
                Some(format!(
                    "{}:{}",
                    tool.manager.to_ascii_lowercase(),
                    tool.package_id
                ))
            } else {
                resolved_rows
                    .iter()
                    .find(|row| row.id == tool.id)
                    .and_then(|row| row.mapping_id.clone())
            };
            children.push(LifecycleChildIntent {
                resource_kind: LifecycleResourceKind::Tool,
                resource_id: tool.id.clone(),
                desired_action: desired_action.to_string(),
                mapping_id,
                depends_on: Vec::new(),
            });
        }
        if children.is_empty() {
            return Err(CoreError::MalformedInput(
                "setup-queue has no installable, updatable, or review children".to_string(),
            ));
        }
        let tool_item_ids = children
            .iter()
            .filter(|child| child.resource_kind == LifecycleResourceKind::Tool)
            .map(|child| child.resource_id.clone())
            .collect();
        Ok(LifecyclePlanRequest {
            resource_kind: LifecycleResourceKind::Operation,
            action: "setup-queue".to_string(),
            resource_id: request.resource_id,
            source_analysis_handle: None,
            item_ids: Some(tool_item_ids),
            children,
            mapping_id: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::inventory::{
        CatalogStatus, ExecutionMode, MappingStatus, OwnershipKind, PrivilegeRequirement,
    };

    fn tool(id: &str, state: InventoryState, manager: &str, package: &str) -> ToolRecord {
        let installed_version = (state != InventoryState::Missing).then(|| "1.0.0".into());
        ToolRecord {
            id: id.into(),
            name: id.into(),
            summary: id.into(),
            kind: "CLI tool".into(),
            groups: vec![],
            recommended: true,
            catalog_status: CatalogStatus::Locked,
            mapping_status: MappingStatus::Supported,
            state,
            owner: manager.into(),
            ownership_kind: OwnershipKind::ManagerOwned,
            execution_mode: ExecutionMode::ManagedExecute,
            installed_version,
            available_version: None,
            manager: manager.into(),
            package_id: package.into(),
            platform: "macos_arm64".into(),
            privilege: PrivilegeRequirement::None,
            lifecycle_confidence: "high".into(),
            reason_code: None,
        }
    }

    #[test]
    fn replaces_client_mapping_with_server_recipe_for_missing_tools() {
        let request = LifecyclePlanRequest {
            resource_kind: LifecycleResourceKind::Operation,
            action: "setup-queue".into(),
            resource_id: "quick-setup".into(),
            source_analysis_handle: None,
            item_ids: Some(vec!["orbstack".into()]),
            children: vec![LifecycleChildIntent {
                resource_kind: LifecycleResourceKind::Tool,
                resource_id: "orbstack".into(),
                desired_action: "install".into(),
                mapping_id: Some("evil:payload".into()),
                depends_on: Vec::new(),
            }],
            mapping_id: None,
        };
        let catalog = ToolCatalogSnapshot {
            version: "test".into(),
            tools: vec![],
        };
        let normalized = InstallerService::normalize_setup_queue(
            request,
            &[tool(
                "orbstack",
                InventoryState::Missing,
                "homebrew",
                "orbstack",
            )],
            &catalog,
            &[SetupRow {
                id: "orbstack".into(),
                name: "OrbStack".into(),
                summary: "containers".into(),
                selected: true,
                optional: false,
                action: crate::domain::setup::SetupRowAction::Install,
                reason: None,
                owner: "Homebrew".into(),
                mapping_id: Some("homebrew:orbstack".into()),
            }],
        )
        .expect("normalized");
        assert_eq!(
            normalized.children[0].mapping_id.as_deref(),
            Some("homebrew:orbstack")
        );
        assert_eq!(normalized.children[0].desired_action, "install");
    }

    #[test]
    fn keeps_owner_mapping_for_installed_updates() {
        let request = LifecyclePlanRequest {
            resource_kind: LifecycleResourceKind::Operation,
            action: "setup-queue".into(),
            resource_id: "quick-setup".into(),
            source_analysis_handle: None,
            item_ids: Some(vec!["codex-cli".into()]),
            children: Vec::new(),
            mapping_id: None,
        };
        let catalog = ToolCatalogSnapshot {
            version: "test".into(),
            tools: vec![],
        };
        let normalized = InstallerService::normalize_setup_queue(
            request,
            &[tool(
                "codex-cli",
                InventoryState::ManagedUpdateAvailable,
                "npm",
                "@openai/codex",
            )],
            &catalog,
            &[],
        )
        .expect("normalized");
        assert_eq!(
            normalized.children[0].mapping_id.as_deref(),
            Some("npm:@openai/codex")
        );
        assert_eq!(normalized.children[0].desired_action, "update");
    }

    #[test]
    fn rejects_duplicate_and_oversized_setup_queues() {
        let request = |ids: Vec<String>| LifecyclePlanRequest {
            resource_kind: LifecycleResourceKind::Operation,
            action: "setup-queue".into(),
            resource_id: "quick-setup".into(),
            source_analysis_handle: None,
            item_ids: Some(ids),
            children: Vec::new(),
            mapping_id: None,
        };
        let catalog = ToolCatalogSnapshot {
            version: "test".into(),
            tools: vec![],
        };
        let duplicate = InstallerService::normalize_setup_queue(
            request(vec!["git".into(), "git".into()]),
            &[],
            &catalog,
            &[],
        )
        .expect_err("duplicate");
        assert!(duplicate.to_string().contains("duplicate"));
        let oversized = InstallerService::normalize_setup_queue(
            request(
                (0..=MAX_BATCH_ITEMS)
                    .map(|index| format!("tool-{index}"))
                    .collect(),
            ),
            &[],
            &catalog,
            &[],
        )
        .expect_err("oversized");
        assert!(oversized.to_string().contains("exceeds"));
    }

    #[test]
    fn preserves_imported_non_tool_resources_as_review_only() {
        let request = LifecyclePlanRequest {
            resource_kind: LifecycleResourceKind::Operation,
            action: "setup-queue".into(),
            resource_id: "quick-setup".into(),
            source_analysis_handle: None,
            item_ids: Some(Vec::new()),
            children: vec![LifecycleChildIntent {
                resource_kind: LifecycleResourceKind::Skill,
                resource_id: "custom-skill".into(),
                desired_action: "install".into(),
                mapping_id: Some("evil:mapping".into()),
                depends_on: Vec::new(),
            }],
            mapping_id: None,
        };
        let normalized = InstallerService::normalize_setup_queue(
            request,
            &[],
            &ToolCatalogSnapshot {
                version: "test".into(),
                tools: vec![],
            },
            &[],
        )
        .expect("normalized");
        assert_eq!(
            normalized.children[0].resource_kind,
            LifecycleResourceKind::Skill
        );
        assert_eq!(normalized.children[0].desired_action, "review");
        assert_eq!(normalized.children[0].mapping_id, None);
    }
}
