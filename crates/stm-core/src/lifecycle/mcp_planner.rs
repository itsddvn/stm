use std::{
    collections::BTreeSet,
    path::{Component, Path},
    time::SystemTime,
};

use serde_json::json;

use crate::{
    adapters::{compute_sha256, FixtureWorkspace},
    domain::{
        lifecycle::{
            LifecycleExecution, LifecyclePlan, LifecyclePlanRequest, LifecyclePrivilege,
            LifecycleRevalidation, LifecycleRevalidationState,
        },
        mcp::{
            AuthReferenceState, McpBindingState, McpClientName, McpServerRecord, McpTransport,
            McpTrustState,
        },
        source::{SourceAnalysisStatus, SourceKind},
    },
    error::CoreError,
    mcp::{
        discover_mcp,
        lifecycle::{McpConfigTarget, McpMutationAction, PreparedMcpAction, PreparedMcpMutation},
    },
};

use super::{
    command::ExecutableIdentity,
    planner::{opaque_plan_id, plan_digest, prepare_review_only, PlannerContext, PreparedPlan},
    source_registry::SourceAnalysisBinding,
    time::{format_timestamp, PLAN_TTL},
};

pub(super) fn prepare_mcp(
    workspace: &FixtureWorkspace,
    context: PlannerContext<'_>,
    request: LifecyclePlanRequest,
    source: Option<&SourceAnalysisBinding>,
    sequence: u64,
    now: SystemTime,
) -> Result<PreparedPlan, CoreError> {
    let action = normalized_action(&request.action);
    if action == "rollback" {
        return prepare_rollback(context, request, sequence, now);
    }
    if action == "keep-partial" {
        let mut prepared = prepare_review_only(
            request,
            sequence,
            now,
            "Keep successful MCP client bindings and acknowledge the partial result.",
        )?;
        prepared.plan.privilege = LifecyclePrivilege::UserConfirmation;
        prepared.mcp_action = Some(PreparedMcpAction::KeepPartial);
        prepared.plan.execution = LifecycleExecution::ManagedConfigMutation {
            action: "keep_partial".into(),
        };
        prepared.plan.digest = plan_digest(&prepared.plan)?;
        return Ok(prepared);
    }

    let inventory = discover_mcp(workspace)?;
    let selected_clients = requested_clients(&request);
    let trailing_args = match requested_stdio_args(&request) {
        Ok(arguments) => arguments,
        Err(reason) => return prepare_review_only(request, sequence, now, &reason),
    };
    let server = if action == "add" {
        let candidate = if source.is_some() {
            if !trailing_args.is_empty() {
                Err(CoreError::CommandDenied(
                    "remote MCP add does not accept stdio resource-root arguments".into(),
                ))
            } else {
                server_from_reviewed_source(source, &selected_clients)
            }
        } else {
            crate::mcp::policy::approved_server(
                &request.resource_id,
                &trailing_args,
                &selected_clients,
            )
            .map_err(CoreError::CommandDenied)?
            .ok_or_else(|| {
                CoreError::CommandDenied(
                    "MCP add requires a reviewed approved-catalog mapping".into(),
                )
            })
        };
        match candidate {
            Ok(server) => server,
            Err(error) => {
                return prepare_review_only(request, sequence, now, &error.to_string());
            }
        }
    } else {
        let Some(current) = inventory
            .servers
            .iter()
            .find(|server| server.id == request.resource_id)
            .cloned()
        else {
            return prepare_review_only(
                request,
                sequence,
                now,
                "The MCP server is no longer present in supported global client configurations.",
            );
        };
        if matches!(action, "update" | "enable")
            && (current.trust == McpTrustState::Blocked
                || current.auth_state == AuthReferenceState::ReferenceMissing)
        {
            return prepare_review_only(
                request,
                sequence,
                now,
                "MCP configuration remains blocked until trust and credential-reference requirements are satisfied.",
            );
        }
        if action == "update" && source.is_some() {
            let mut reviewed = match server_from_reviewed_source(source, &selected_clients) {
                Ok(server) => server,
                Err(error) => {
                    return prepare_review_only(request, sequence, now, &error.to_string());
                }
            };
            reviewed.id = current.id;
            reviewed.name = current.name;
            reviewed.clients = current
                .clients
                .into_iter()
                .map(|binding| {
                    crate::domain::mcp::McpClientBindingRecord::from_server(
                        binding.client,
                        binding.state,
                        binding.scope,
                        binding.entry_name,
                        &reviewed,
                    )
                })
                .collect();
            reviewed
        } else {
            current
        }
    };
    let selected = selected_client_keys(&request);
    let execution_servers = server
        .clients
        .iter()
        .filter(|binding| binding.state != McpBindingState::Unsupported)
        .filter(|binding| {
            selected.is_empty()
                || selected.contains(&client_label(&binding.client).to_ascii_lowercase())
        })
        .map(|binding| binding.project_server(&server))
        .collect::<Vec<_>>();
    let mut executable_identities = Vec::new();
    if matches!(action, "add" | "update" | "enable") {
        for candidate in &execution_servers {
            if let Err(reason) = crate::mcp::policy::validate_lifecycle_server(candidate) {
                return prepare_review_only(request, sequence, now, &reason);
            }
            if candidate.transport == McpTransport::Stdio {
                match context
                    .mcp_lifecycle
                    .compile_stdio(&candidate.command_or_url, &candidate.args)
                {
                    Ok(Some(command)) => {
                        for identity in command.identities {
                            if !executable_identities.contains(&identity) {
                                executable_identities.push(identity);
                            }
                        }
                    }
                    Ok(None) => {
                        return prepare_review_only(
                            request,
                            sequence,
                            now,
                            "The approved MCP stdio runtime is unavailable.",
                        );
                    }
                    Err(error) => {
                        return prepare_review_only(request, sequence, now, &error.to_string());
                    }
                }
            }
        }
    }
    let action = match action {
        "add" => McpMutationAction::Add,
        "update" => McpMutationAction::Update,
        "enable" => McpMutationAction::Enable,
        "disable" => McpMutationAction::Disable,
        "remove" => McpMutationAction::Remove,
        _ => {
            return prepare_review_only(
                request,
                sequence,
                now,
                "Choose a concrete MCP lifecycle action before execution.",
            )
        }
    };
    build_mutation_plan(
        context,
        request,
        server,
        action,
        executable_identities,
        sequence,
        now,
    )
}

fn build_mutation_plan(
    context: PlannerContext<'_>,
    request: LifecyclePlanRequest,
    server: McpServerRecord,
    action: McpMutationAction,
    executable_identities: Vec<ExecutableIdentity>,
    sequence: u64,
    now: SystemTime,
) -> Result<PreparedPlan, CoreError> {
    let selected = selected_client_keys(&request);
    let bindings = server
        .clients
        .iter()
        .filter(|binding| binding.state != McpBindingState::Unsupported)
        .cloned()
        .collect::<Vec<_>>();
    let mut targets = Vec::new();
    for binding in bindings {
        if !selected.is_empty()
            && !selected.contains(&client_label(&binding.client).to_ascii_lowercase())
        {
            continue;
        }
        let config_path = context.mcp_lifecycle.client_config_path(&binding.client);
        targets.push(McpConfigTarget {
            client: binding.client,
            expected_sha256: context.mcp_lifecycle.config_digest(&config_path)?,
            config_path,
            entry_name: if binding.entry_name.is_empty() {
                server.name.clone()
            } else {
                binding.entry_name
            },
        });
    }
    if targets.is_empty() {
        return Err(CoreError::MalformedInput(
            "no supported MCP client targets were selected".into(),
        ));
    }

    let plan_id = opaque_plan_id(sequence, &request, now)?;
    let prepared = PreparedMcpMutation {
        operation_id: plan_id.clone(),
        server: server.clone(),
        action: action.clone(),
        targets,
    };
    let fingerprint = compute_sha256([serde_json::to_vec(&json!({
        "request": request,
        "server": server,
        "action": action,
        "targets": prepared.targets,
    }))?]);
    let issued_at = format_timestamp(now)?;
    let expires_at = format_timestamp(now + PLAN_TTL)?;
    let capability_summary = if server.capabilities.is_empty() {
        "none declared".to_string()
    } else {
        server.capabilities.join(", ")
    };
    let reference_summary = if server.auth_references.is_empty() {
        "none".to_string()
    } else {
        server
            .auth_references
            .iter()
            .map(|reference| reference.reference.clone())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let target_state = action_label(&action);
    let mut plan = LifecyclePlan {
        request: request.clone(),
        plan_id,
        canonical_id: server.id.clone(),
        mapping_id: format!("mcp-config:{}", server.id),
        resource_id: server.id.clone(),
        owner: "STM reviewed MCP configuration lifecycle".into(),
        source: server.command_or_url.clone(),
        current_version: current_binding_state(&server),
        target_version: target_state.to_string(),
        privilege: LifecyclePrivilege::UserConfirmation,
        affected_paths: prepared
            .targets
            .iter()
            .map(|target| target.config_path.display().to_string())
            .collect(),
        affected_records: prepared
            .targets
            .iter()
            .map(|target| format!("mcp-binding:{}:{}", server.id, client_label(&target.client)))
            .collect(),
        confidence: "Supported client schema and current configuration digest verified".into(),
        limitations: vec![
            format!("Transport bound to {}.", transport_label(&server.transport)),
            format!("Capabilities bound to {capability_summary}."),
            format!("Credential references bound to {reference_summary}; values are excluded."),
            "Every selected client configuration digest is revalidated before mutation.".into(),
            "Protocol health never invokes a domain tool and may remain unverified when authentication is required.".into(),
        ],
        digest: String::new(),
        expires_at: expires_at.clone(),
        revalidation: LifecycleRevalidation {
            state: LifecycleRevalidationState::Fresh,
            checked_at: issued_at,
            checks: vec![
                "Server identity, transport, endpoint or command, arguments, capabilities, and references are unchanged.".into(),
                "Selected client configuration digests and bounded global paths are unchanged.".into(),
            ],
        },
        execution: LifecycleExecution::ManagedConfigMutation {
            action: target_state.to_ascii_lowercase(),
        },
    };
    plan.digest = plan_digest(&plan)?;
    Ok(PreparedPlan {
        dependency_key: plan.resource_id.clone(),
        plan,
        evidence_fingerprint: fingerprint.clone(),
        recipe_fingerprint: fingerprint,
        executable_identities,
        children: Vec::new(),
        depends_on: Vec::new(),
        exact_mapping: false,
        preconditions: Vec::new(),
        precondition_executable_paths: Vec::new(),
        precondition_expected_version: None,
        postcondition_executable_paths: Vec::new(),
        staged: false,
        skill_action: None,
        mcp_action: Some(PreparedMcpAction::Mutate(Box::new(prepared))),
    })
}

fn prepare_rollback(
    context: PlannerContext<'_>,
    request: LifecyclePlanRequest,
    sequence: u64,
    now: SystemTime,
) -> Result<PreparedPlan, CoreError> {
    let backup_id = request
        .item_ids
        .as_ref()
        .and_then(|items| items.first())
        .cloned()
        .or_else(|| {
            context
                .mcp_lifecycle
                .load_available_backups(&request.resource_id)
                .ok()
                .and_then(|backups| backups.first().map(|backup| backup.backup_id.clone()))
        });
    let Some(backup_id) = backup_id else {
        return prepare_review_only(
            request,
            sequence,
            now,
            "No available receipt-backed MCP configuration backup was found.",
        );
    };
    let backup = context
        .mcp_lifecycle
        .load_backup(&backup_id)?
        .ok_or_else(|| CoreError::LifecycleEvidenceChanged("MCP backup is unavailable".into()))?;
    let plan_id = opaque_plan_id(sequence, &request, now)?;
    let config_path = context.mcp_lifecycle.client_config_path(&backup.client);
    let issued_at = format_timestamp(now)?;
    let expires_at = format_timestamp(now + PLAN_TTL)?;
    let mut plan = LifecyclePlan {
        request: request.clone(),
        plan_id,
        canonical_id: backup.server_id.clone(),
        mapping_id: format!("mcp-backup:{backup_id}"),
        resource_id: backup.server_id.clone(),
        owner: "STM reviewed MCP configuration lifecycle".into(),
        source: "Receipt-backed MCP client configuration backup".into(),
        current_version: "Current client configuration".into(),
        target_version: "Previous reviewed configuration".into(),
        privilege: LifecyclePrivilege::UserConfirmation,
        affected_paths: vec![config_path.display().to_string()],
        affected_records: vec![format!("mcp-backup:{backup_id}")],
        confidence: "Backup receipt and digest verified".into(),
        limitations: vec!["Only the selected client configuration is restored; unrelated entries are preserved from the receipt-backed backup.".into()],
        digest: String::new(),
        expires_at: expires_at.clone(),
        revalidation: LifecycleRevalidation {
            state: LifecycleRevalidationState::Fresh,
            checked_at: issued_at,
            checks: vec!["Selected backup remains available and digest-matched.".into()],
        },
        execution: LifecycleExecution::ManagedConfigMutation {
            action: "rollback".into(),
        },
    };
    plan.digest = plan_digest(&plan)?;
    let fingerprint = compute_sha256([serde_json::to_vec(&json!({
        "request": request,
        "backup": backup,
    }))?]);
    Ok(PreparedPlan {
        dependency_key: plan.resource_id.clone(),
        plan,
        evidence_fingerprint: fingerprint.clone(),
        recipe_fingerprint: fingerprint,
        executable_identities: Vec::new(),
        children: Vec::new(),
        depends_on: Vec::new(),
        exact_mapping: false,
        preconditions: Vec::new(),
        precondition_executable_paths: Vec::new(),
        precondition_expected_version: None,
        postcondition_executable_paths: Vec::new(),
        staged: false,
        skill_action: None,
        mcp_action: Some(PreparedMcpAction::RestoreBackup(backup_id)),
    })
}

fn server_from_reviewed_source(
    source: Option<&SourceAnalysisBinding>,
    clients: &[McpClientName],
) -> Result<McpServerRecord, CoreError> {
    let binding = source.ok_or_else(|| {
        CoreError::LifecycleEvidenceChanged("MCP source review is required before planning".into())
    })?;
    if binding.record.kind != SourceKind::Mcp
        || binding.record.status != SourceAnalysisStatus::ReviewReady
    {
        return Err(CoreError::LifecycleEvidenceChanged(
            "MCP source review is not ready".into(),
        ));
    }
    let endpoint = binding
        .record
        .normalized_url
        .as_deref()
        .ok_or_else(|| CoreError::MalformedInput("reviewed MCP endpoint is missing".into()))?;
    crate::mcp::policy::approved_server_for_endpoint(endpoint, clients)
        .map_err(CoreError::CommandDenied)?
        .ok_or_else(|| {
            CoreError::CommandDenied(
                "reviewed MCP endpoint has no approved capability and credential mapping".into(),
            )
        })
}

fn normalized_action(action: &str) -> &'static str {
    if action.contains("rollback") {
        "rollback"
    } else if action.contains("keep_partial") {
        "keep-partial"
    } else if action.contains("remove") {
        "remove"
    } else if action.contains("disable") {
        "disable"
    } else if action.contains("enable") {
        "enable"
    } else if action.contains("configuration")
        || action.contains("update")
        || action.contains("retry")
    {
        "update"
    } else if action.contains("add") || action.contains("install") {
        "add"
    } else {
        "review"
    }
}

fn current_binding_state(server: &McpServerRecord) -> String {
    let enabled = server
        .clients
        .iter()
        .filter(|binding| binding.state == McpBindingState::Enabled)
        .count();
    format!("{enabled}/{} clients enabled", server.clients.len())
}

fn action_label(action: &McpMutationAction) -> &'static str {
    match action {
        McpMutationAction::Add => "Add",
        McpMutationAction::Update => "Update",
        McpMutationAction::Enable => "Enable",
        McpMutationAction::Disable => "Disable",
        McpMutationAction::Remove => "Remove",
    }
}

fn transport_label(transport: &McpTransport) -> &'static str {
    match transport {
        McpTransport::Stdio => "stdio",
        McpTransport::StreamableHttp => "Streamable HTTP",
        McpTransport::Sse => "SSE",
    }
}

pub(super) fn client_label(client: &McpClientName) -> &'static str {
    match client {
        McpClientName::Codex => "Codex",
        McpClientName::ClaudeCode => "Claude Code",
        McpClientName::Cursor => "Cursor",
    }
}

fn requested_clients(request: &LifecyclePlanRequest) -> Vec<McpClientName> {
    let mut selected = Vec::new();
    for client in request
        .item_ids
        .as_ref()
        .into_iter()
        .flatten()
        .filter_map(|value| parse_client_selector(value))
    {
        if !selected.contains(&client) {
            selected.push(client);
        }
    }
    if selected.is_empty() {
        vec![
            McpClientName::Codex,
            McpClientName::ClaudeCode,
            McpClientName::Cursor,
        ]
    } else {
        selected
    }
}

fn selected_client_keys(request: &LifecyclePlanRequest) -> BTreeSet<String> {
    request
        .item_ids
        .as_ref()
        .into_iter()
        .flatten()
        .filter_map(|value| parse_client_selector(value))
        .map(|client| client_label(&client).to_ascii_lowercase())
        .collect()
}

fn parse_client_selector(value: &str) -> Option<McpClientName> {
    match value
        .trim()
        .to_ascii_lowercase()
        .replace(['-', '_'], " ")
        .as_str()
    {
        "codex" => Some(McpClientName::Codex),
        "claude code" | "claude" => Some(McpClientName::ClaudeCode),
        "cursor" => Some(McpClientName::Cursor),
        _ => None,
    }
}

fn requested_stdio_args(request: &LifecyclePlanRequest) -> Result<Vec<String>, String> {
    request
        .item_ids
        .as_ref()
        .into_iter()
        .flatten()
        .filter(|value| parse_client_selector(value).is_none())
        .map(|value| {
            let path = Path::new(value);
            if !path.is_absolute()
                || path
                    .components()
                    .any(|component| component == Component::ParentDir)
            {
                return Err(
                    "MCP stdio add arguments must be absolute resource roots without parent traversal"
                        .into(),
                );
            }
            Ok(value.clone())
        })
        .collect()
}
