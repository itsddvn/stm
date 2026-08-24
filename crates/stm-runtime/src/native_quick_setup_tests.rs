use std::{path::PathBuf, sync::Arc};

use stm_core::{
    adapters::FixtureWorkspace,
    application::service::PhaseThreeApplicationService,
    domain::{
        lifecycle::{
            LifecycleChildIntent, LifecycleExecution, LifecyclePlanRequest, LifecycleResourceKind,
        },
        provider::MemoryPreferencesStore,
        setup::SetupRowAction,
    },
    lifecycle::LifecycleService,
    ports::SnapshotStore,
};
use tempfile::TempDir;

use crate::{
    detect_provider_inventory, BoundedHttpsSourceProbe, NativeProcessLiveness,
    RealHostExecutableResolver, RealLifecycleExecutor, RealManagerEvidence, SqliteSnapshotStore,
};

#[test]
#[ignore = "queries the current host package managers and registries without mutating them"]
fn native_quick_setup_uses_live_host_evidence() {
    let temp = TempDir::new().expect("temporary runtime data");
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let workspace =
        FixtureWorkspace::new(project_root.clone()).with_db_path(temp.path().join("stm.sqlite"));
    let (sqlite, _) = SqliteSnapshotStore::open(workspace.db_path()).expect("snapshot store");
    let storage: Arc<dyn SnapshotStore> = Arc::new(sqlite);
    let host = Arc::new(RealHostExecutableResolver);
    let lifecycle = LifecycleService::with_dependencies(
        workspace,
        Arc::new(RealLifecycleExecutor),
        Arc::new(BoundedHttpsSourceProbe),
        Arc::new(RealManagerEvidence::new(host.clone())),
        host,
        storage.clone(),
        Arc::new(NativeProcessLiveness),
    );
    let service = PhaseThreeApplicationService::with_services(
        project_root,
        lifecycle,
        storage,
        Arc::new(MemoryPreferencesStore::new()),
    );
    let scan = service.headless_scan().expect("live headless scan");
    println!(
        "live warnings: {}",
        serde_json::to_string_pretty(&scan.diagnostics.warnings).expect("warning JSON")
    );
    let providers = detect_provider_inventory();
    let view = service
        .quick_setup(providers.clone())
        .expect("live Quick Setup");
    let selected = view
        .tools
        .iter()
        .chain(&view.optional_mcp)
        .filter(|row| row.selected)
        .collect::<Vec<_>>();

    println!(
        "{}",
        serde_json::to_string_pretty(&view).expect("Quick Setup JSON")
    );
    assert!(
        !selected.is_empty(),
        "live Quick Setup must offer actionable defaults"
    );
    assert!(selected.iter().all(|row| matches!(
        row.action,
        SetupRowAction::Install | SetupRowAction::Update | SetupRowAction::Handoff
    )));
    assert!(selected.iter().all(|row| {
        row.mapping_id.is_some()
            || matches!(row.action, SetupRowAction::Handoff | SetupRowAction::Update)
    }));

    for action in [SetupRowAction::Install, SetupRowAction::Update] {
        let Some(row) = selected.iter().find(|row| row.action == action) else {
            continue;
        };
        let plan = service
            .prepare_lifecycle_with_providers(
                LifecyclePlanRequest {
                    resource_kind: LifecycleResourceKind::Operation,
                    action: "setup-queue".to_string(),
                    resource_id: format!("native-smoke-{}", row.id),
                    source_analysis_handle: None,
                    item_ids: Some(vec![row.id.clone()]),
                    children: vec![LifecycleChildIntent {
                        resource_kind: LifecycleResourceKind::Tool,
                        resource_id: row.id.clone(),
                        desired_action: if action == SetupRowAction::Install {
                            "install"
                        } else {
                            "update"
                        }
                        .to_string(),
                        mapping_id: row.mapping_id.clone(),
                        depends_on: Vec::new(),
                    }],
                    mapping_id: None,
                },
                providers.clone(),
            )
            .expect("native lifecycle plan");
        let LifecycleExecution::Batch { items } = plan.execution else {
            panic!("native setup must prepare a batch");
        };
        assert!(
            items.iter().any(|item| {
                item.resource_id == row.id
                    && matches!(item.execution, LifecycleExecution::ManagedExecute { .. })
            }),
            "{} must compile to managed execution",
            row.id
        );
    }
}
