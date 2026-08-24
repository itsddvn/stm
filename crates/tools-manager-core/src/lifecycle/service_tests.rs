#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        thread,
        time::{Duration, SystemTime},
    };

    use tempfile::TempDir;

    use super::*;
    use crate::{
        catalog::ToolCatalogMapping,
        domain::lifecycle::LifecyclePrivilege,
        lifecycle::{
            command::ExecutableIdentity,
            evidence::{ManagerEvidencePort, ManagerStateEvidence},
            executor::ManagedExecutionResult,
            skill_source::fixture_skill_source_resolver,
            source_probe::SourceProbeEvidence,
        },
    };

    #[derive(Default)]
    struct SuccessfulExecutor {
        managed_calls: AtomicUsize,
        handoff_calls: AtomicUsize,
    }

    impl LifecycleExecutionPort for SuccessfulExecutor {
        fn execute_managed(
            &self,
            _: &str,
            _: &[String],
            _: &[ExecutableIdentity],
            on_spawn: &(dyn Fn(u32) -> Result<(), CoreError> + Send + Sync),
            cancel: &CancelSignal,
        ) -> Result<ManagedExecutionResult, CoreError> {
            on_spawn(std::process::id())?;
            self.managed_calls.fetch_add(1, Ordering::SeqCst);
            for _ in 0..20 {
                if cancel.is_cancelled() {
                    return Ok(ManagedExecutionResult {
                        success: false,
                        cancelled: true,
                        redacted_detail: "cancelled by fixture".to_string(),
                    });
                }
                thread::sleep(Duration::from_millis(2));
            }
            Ok(ManagedExecutionResult {
                success: true,
                cancelled: false,
                redacted_detail: "fixture manager success".to_string(),
            })
        }

        fn open_vendor_handoff(&self, _: &str) -> Result<(), CoreError> {
            self.handoff_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FixtureManagerEvidence {
        inspections: AtomicUsize,
    }

    impl ManagerEvidencePort for FixtureManagerEvidence {
        fn inspect(
            &self,
            _: &ToolCatalogMapping,
            _: &str,
        ) -> Result<ManagerStateEvidence, CoreError> {
            let inspection = self.inspections.fetch_add(1, Ordering::SeqCst);
            Ok(ManagerStateEvidence {
                installed: true,
                current_version: Some(if inspection >= 2 { "1.0.0" } else { "0.1.0" }.to_string()),
                target_version: "1.0.0".to_string(),
                update_available: inspection < 2,
                source: "Injected test manager evidence".to_string(),
            })
        }
    }

    #[cfg(target_os = "macos")]
    #[derive(Default)]
    struct ConvergingPerPackageEvidence {
        inspections: Mutex<std::collections::BTreeMap<String, usize>>,
    }

    #[cfg(target_os = "macos")]
    impl ManagerEvidencePort for ConvergingPerPackageEvidence {
        fn inspect(
            &self,
            mapping: &ToolCatalogMapping,
            _: &str,
        ) -> Result<ManagerStateEvidence, CoreError> {
            let mut inspections = self.inspections.lock().expect("inspection counts");
            let inspection = inspections.entry(mapping.package_id.clone()).or_default();
            let current_version = if *inspection >= 2 { "1.0.0" } else { "0.1.0" };
            *inspection += 1;
            Ok(ManagerStateEvidence {
                installed: true,
                current_version: Some(current_version.to_string()),
                target_version: "1.0.0".to_string(),
                update_available: current_version != "1.0.0",
                source: "Injected package-scoped manager evidence".to_string(),
            })
        }
    }

    struct AlwaysUpdateManagerEvidence;

    impl ManagerEvidencePort for AlwaysUpdateManagerEvidence {
        fn inspect(
            &self,
            _: &ToolCatalogMapping,
            _: &str,
        ) -> Result<ManagerStateEvidence, CoreError> {
            Ok(ManagerStateEvidence {
                installed: true,
                current_version: Some("0.1.0".to_string()),
                target_version: "1.0.0".to_string(),
                update_available: true,
                source: "Injected stable manager evidence".to_string(),
            })
        }
    }

    #[cfg(target_os = "macos")]
    #[derive(Default)]
    struct ChangesAfterFirstInspection {
        inspections: AtomicUsize,
    }

    #[cfg(target_os = "macos")]
    impl ManagerEvidencePort for ChangesAfterFirstInspection {
        fn inspect(
            &self,
            _: &ToolCatalogMapping,
            _: &str,
        ) -> Result<ManagerStateEvidence, CoreError> {
            let inspection = self.inspections.fetch_add(1, Ordering::SeqCst);
            Ok(ManagerStateEvidence {
                installed: true,
                current_version: Some(if inspection == 0 { "0.1.0" } else { "1.0.0" }.to_string()),
                target_version: "1.0.0".to_string(),
                update_available: inspection == 0,
                source: "Injected changing manager evidence".to_string(),
            })
        }
    }

    #[cfg(target_os = "macos")]
    #[derive(Default)]
    struct FailsPostconditionInspection {
        inspections: AtomicUsize,
    }

    #[cfg(target_os = "macos")]
    impl ManagerEvidencePort for FailsPostconditionInspection {
        fn inspect(
            &self,
            _: &ToolCatalogMapping,
            _: &str,
        ) -> Result<ManagerStateEvidence, CoreError> {
            let inspection = self.inspections.fetch_add(1, Ordering::SeqCst);
            if inspection >= 2 {
                return Err(CoreError::ProcessExecution(
                    "post-operation manager evidence unavailable".to_string(),
                ));
            }
            Ok(ManagerStateEvidence {
                installed: true,
                current_version: Some("0.1.0".to_string()),
                target_version: "1.0.0".to_string(),
                update_available: true,
                source: "Injected stable manager evidence".to_string(),
            })
        }
    }

    struct FixtureProbe;

    impl SourceProbe for FixtureProbe {
        fn probe(&self, url: &str) -> Result<SourceProbeEvidence, CoreError> {
            Ok(SourceProbeEvidence {
                final_url: url.to_string(),
                status: 200,
                content_length: Some(512),
                sampled_bytes: 512,
            })
        }
    }

    #[derive(Default)]
    struct RedirectingProbe {
        probes: AtomicUsize,
    }

    impl SourceProbe for RedirectingProbe {
        fn probe(&self, url: &str) -> Result<SourceProbeEvidence, CoreError> {
            let probe = self.probes.fetch_add(1, Ordering::SeqCst);
            Ok(SourceProbeEvidence {
                final_url: if probe == 0 {
                    url.to_string()
                } else {
                    "https://github.com/attacker/replacement".to_string()
                },
                status: 200,
                content_length: Some(512),
                sampled_bytes: 512,
            })
        }
    }

    struct PanicProbe;

    impl SourceProbe for PanicProbe {
        fn probe(&self, _: &str) -> Result<SourceProbeEvidence, CoreError> {
            panic!("unmatched sources must never reach the network probe")
        }
    }

    fn service(temp: &TempDir) -> (LifecycleService, Arc<SuccessfulExecutor>) {
        let workspace =
            FixtureWorkspace::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
                .with_db_path(temp.path().join("stm.sqlite"));
        let executor = Arc::new(SuccessfulExecutor::default());
        let service = LifecycleService::with_ports(
            workspace,
            executor.clone(),
            Arc::new(FixtureProbe),
            Arc::new(FixtureManagerEvidence::default()),
        );
        (service, executor)
    }

    fn skill_service(temp: &TempDir) -> (LifecycleService, PathBuf) {
        let home = temp.path().join("home");
        fs::create_dir_all(&home).expect("skill home");
        let workspace =
            FixtureWorkspace::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
                .with_db_path(temp.path().join("state/stm.sqlite"))
                .with_skill_home(&home);
        let service = LifecycleService::with_ports_and_skill_resolver(
            workspace,
            Arc::new(SuccessfulExecutor::default()),
            Arc::new(FixtureProbe),
            Arc::new(FixtureManagerEvidence::default()),
            fixture_skill_source_resolver(),
        );
        (service, home)
    }

    fn request(resource_id: &str) -> LifecyclePlanRequest {
        LifecyclePlanRequest {
            resource_kind: LifecycleResourceKind::Tool,
            action: "update".to_string(),
            resource_id: resource_id.to_string(),
            source_analysis_handle: None,
            item_ids: None,
        }
    }

    fn authorize(plan: &LifecyclePlan) -> LifecycleConsentAuthorization {
        LifecycleConsentAuthorization {
            plan_digest: plan.digest.clone(),
            plan_expires_at: plan.expires_at.clone(),
            granted_at: plan.revalidation.checked_at.clone(),
        }
    }

    fn wait_for_completion(
        service: &LifecycleService,
        operation_id: &str,
    ) -> LifecycleExecutionResult {
        for _ in 0..6_000 {
            let result = service.status(operation_id).expect("status");
            if result.status != LifecycleExecutionStatus::InProgress {
                return result;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("lifecycle operation did not complete")
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn consent_binds_exact_plan_and_persists_redacted_receipt() {
        let temp = TempDir::new().expect("tempdir");
        let (service, executor) = service(&temp);
        let plan = service.prepare(request("codex-cli")).expect("plan");
        let LifecycleExecution::ManagedExecute { executable, argv } = &plan.execution else {
            panic!("expected managed plan: {:?}", plan.execution);
        };
        assert!(executable.starts_with('/'));
        assert!(argv[0].ends_with("/npm-cli.js"));
        assert_eq!(
            &argv[1..4],
            ["install", "--global", "@openai/codex@1.0.0"]
        );
        assert!(argv[4..]
            .iter()
            .any(|argument| argument == "--registry=https://registry.npmjs.org/"));
        assert!(argv[4..]
            .iter()
            .any(|argument| argument.starts_with("--userconfig=")));
        assert!(argv[4..]
            .iter()
            .any(|argument| argument.starts_with("--globalconfig=")));
        assert!(plan.digest.starts_with("sha256:"));

        let initial = service
            .start(&plan.plan_id, authorize(&plan))
            .expect("start lifecycle");
        assert_eq!(initial.status, LifecycleExecutionStatus::InProgress);
        let operation_suffix = initial
            .operation_id
            .strip_prefix("lifecycle-operation-")
            .expect("opaque operation prefix");
        assert_eq!(operation_suffix.len(), 64);
        assert!(operation_suffix.bytes().all(|byte| byte.is_ascii_hexdigit()));
        let (journal_store, _) =
            SqliteSnapshotStore::open(temp.path().join("stm.sqlite")).expect("journal store");
        let journal = (0..6_000)
            .find_map(|_| {
                let entries = journal_store
                    .load_lifecycle_receipts()
                    .expect("in-progress journal");
                let entry = entries.into_iter().next()?;
                if entry.child_process_id.is_some() {
                    Some(entry)
                } else {
                    thread::sleep(Duration::from_millis(2));
                    None
                }
            })
            .expect("durable child process journal");
        assert_eq!(
            journal
                .lifecycle_result
                .as_ref()
                .map(|entry| &entry.status),
            Some(&LifecycleExecutionStatus::InProgress)
        );
        assert_eq!(journal.owner_process_id, Some(std::process::id()));
        assert_eq!(journal.child_process_id, Some(std::process::id()));
        let result = wait_for_completion(&service, &initial.operation_id);
        assert_eq!(
            result.status,
            LifecycleExecutionStatus::Success,
            "{result:?}"
        );
        assert_eq!(executor.managed_calls.load(Ordering::SeqCst), 1);
        assert!(!result.redacted_detail.contains("@openai/codex"));

        let (store, _) = SqliteSnapshotStore::open(temp.path().join("stm.sqlite")).expect("store");
        let receipts = store.load_lifecycle_receipts().expect("receipts");
        assert_eq!(receipts.len(), 1);
        assert_eq!(
            receipts[0].lifecycle_request,
            Some(LifecyclePlanRequest {
                resource_kind: LifecycleResourceKind::Operation,
                action: "inspect-receipt".to_string(),
                resource_id: result.operation_id.clone(),
                source_analysis_handle: None,
                item_ids: None,
            })
        );
        let restored_result = receipts[0]
            .lifecycle_result
            .as_ref()
            .expect("persisted lifecycle result");
        assert_eq!(restored_result.plan_digest, plan.digest);
        assert_eq!(restored_result.operation_id, result.operation_id);
        assert!(service.start(&plan.plan_id, authorize(&plan)).is_err());
        let history_plan = service
            .prepare(
                receipts[0]
                    .lifecycle_request
                    .clone()
                    .expect("persisted inspect request"),
            )
            .expect("inspect receipt plan");
        assert_eq!(
            history_plan.request.resource_kind,
            LifecycleResourceKind::Operation
        );
        assert_eq!(history_plan.request.action, "inspect-receipt");
        assert!(matches!(
            history_plan.execution,
            LifecycleExecution::DetectOnly { .. }
        ));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn startup_reconciles_in_progress_source_operation_without_reusing_opaque_handle() {
        let temp = TempDir::new().expect("tempdir");
        let (original_service, _) = service(&temp);
        let mut plan = original_service
            .prepare(request("codex-cli"))
            .expect("plan");
        plan.request.source_analysis_handle = Some("source-analysis-opaque".to_string());
        let authorization = authorize(&plan);
        let result = initial_result(&plan, "lifecycle-operation-interrupted");
        let mut operation = operation_log_entry(&plan, &result, &authorization, None);
        operation.owner_process_id = Some(u32::MAX);
        let (store, _) =
            SqliteSnapshotStore::open(temp.path().join("stm.sqlite")).expect("store");
        store
            .persist_lifecycle_receipt(
                &operation,
                &result,
                &authorization,
                &authorization.granted_at,
            )
            .expect("persist in-progress operation");
        drop(original_service);

        let (_restarted, _) = service(&temp);
        let receipts = store.load_lifecycle_receipts().expect("reconciled receipts");
        let recovered = receipts
            .iter()
            .find(|entry| entry.receipt.operation_id == "lifecycle-operation-interrupted")
            .expect("recovered operation");
        assert_eq!(recovered.receipt.status, OperationStatus::Recoverable);
        let restart_request = recovered
            .lifecycle_request
            .as_ref()
            .expect("restart request");
        assert_eq!(restart_request.action, "reanalyze-source");
        assert_eq!(restart_request.resource_kind, LifecycleResourceKind::Tool);
        assert!(restart_request.source_analysis_handle.is_none());
        let recovered_result = recovered
            .lifecycle_result
            .as_ref()
            .expect("recovered result");
        assert_eq!(
            recovered_result.status,
            LifecycleExecutionStatus::Recoverable
        );
        assert_eq!(recovered_result.recovery_actions.len(), 1);
        assert!(recovered_result.recovery_actions[0]
            .plan_request
            .source_analysis_handle
            .is_none());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn second_service_blocks_while_managed_operation_owner_is_live() {
        let temp = TempDir::new().expect("tempdir");
        let (original_service, _) = service(&temp);
        let plan = original_service
            .prepare(request("codex-cli"))
            .expect("plan");
        let authorization = authorize(&plan);
        let result = initial_result(&plan, "lifecycle-operation-live-owner");
        let operation = operation_log_entry(&plan, &result, &authorization, None);
        let (store, _) =
            SqliteSnapshotStore::open(temp.path().join("stm.sqlite")).expect("store");
        store
            .persist_lifecycle_receipt(
                &operation,
                &result,
                &authorization,
                &authorization.granted_at,
            )
            .expect("persist live operation");

        let (second_service, _) = service(&temp);
        let error = second_service
            .prepare(request("codex-cli"))
            .expect_err("live owner must block competing execution");
        assert!(error
            .to_string()
            .contains("managed lifecycle process is still active"));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn completed_mutation_with_unavailable_postcondition_is_recoverable() {
        let temp = TempDir::new().expect("tempdir");
        let workspace =
            FixtureWorkspace::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
                .with_db_path(temp.path().join("stm.sqlite"));
        let executor = Arc::new(SuccessfulExecutor::default());
        let service = LifecycleService::with_ports(
            workspace,
            executor.clone(),
            Arc::new(FixtureProbe),
            Arc::new(FailsPostconditionInspection::default()),
        );
        let plan = service.prepare(request("codex-cli")).expect("plan");
        let initial = service
            .start(&plan.plan_id, authorize(&plan))
            .expect("start lifecycle");
        let result = wait_for_completion(&service, &initial.operation_id);

        assert_eq!(executor.managed_calls.load(Ordering::SeqCst), 1);
        assert_eq!(result.status, LifecycleExecutionStatus::Recoverable);
        assert!(result.receipt.is_some());
        assert_eq!(result.items[0].status, LifecycleItemStatus::Success);
        assert_eq!(result.recovery_actions.len(), 1);
        assert!(result.redacted_detail.contains("requires recovery"));
    }

    #[test]
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    fn platform_contract_manages_supported_git_or_fails_closed_without_broker() {
        let temp = TempDir::new().expect("tempdir");
        let (service, executor) = service(&temp);
        let plan = service.prepare(request("git")).expect("plan");
        match &plan.execution {
            LifecycleExecution::ManagedExecute { executable, argv } => {
                assert!(!argv.is_empty());
                #[cfg(target_os = "windows")]
                assert!(executable.to_ascii_lowercase().ends_with("winget.exe"));
                #[cfg(target_os = "linux")]
                assert!(executable.ends_with("pkexec"));
                let initial = service
                    .start(&plan.plan_id, authorize(&plan))
                    .expect("start lifecycle");
                let result = wait_for_completion(&service, &initial.operation_id);
                assert_eq!(result.status, LifecycleExecutionStatus::Success);
                assert_eq!(executor.managed_calls.load(Ordering::SeqCst), 1);
            }
            LifecycleExecution::DetectOnly { guidance } => {
                assert!(!guidance.trim().is_empty());
                assert_eq!(executor.managed_calls.load(Ordering::SeqCst), 0);
            }
            execution => panic!("unexpected platform contract: {execution:?}"),
        }
    }

    #[test]
    fn rejects_changed_consent_digest_before_execution() {
        let temp = TempDir::new().expect("tempdir");
        let (service, executor) = service(&temp);
        let plan = service.prepare(request("codex-cli")).expect("plan");
        let mut authorization = authorize(&plan);
        authorization.plan_digest = "sha256:changed".to_string();
        let error = service
            .start(&plan.plan_id, authorization)
            .expect_err("changed digest must fail");
        assert!(matches!(error, CoreError::LifecycleConsentDenied(_)));
        assert_eq!(executor.managed_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn batch_keeps_supported_tool_and_deferred_skill_as_independent_results() {
        let temp = TempDir::new().expect("tempdir");
        let (service, executor) = service(&temp);
        let plan = service
            .prepare(LifecyclePlanRequest {
                resource_kind: LifecycleResourceKind::Operation,
                action: "update-queue".to_string(),
                resource_id: "selected-update-queue".to_string(),
                source_analysis_handle: None,
                item_ids: Some(vec![
                    "update-codex-cli".to_string(),
                    "update-frontend-design".to_string(),
                ]),
            })
            .expect("batch plan");
        let LifecycleExecution::Batch { items } = &plan.execution else {
            panic!("expected batch plan");
        };
        assert_eq!(items.len(), 2);
        assert_ne!(items[0].plan_id, items[1].plan_id);
        assert_ne!(items[0].digest, items[1].digest);

        let initial = service
            .start(&plan.plan_id, authorize(&plan))
            .expect("start");
        let result = wait_for_completion(&service, &initial.operation_id);
        assert_eq!(
            result.status,
            LifecycleExecutionStatus::Partial,
            "{result:?}"
        );
        assert_eq!(result.items[0].status, LifecycleItemStatus::Success);
        assert_eq!(result.items[1].status, LifecycleItemStatus::Skipped);
        assert_eq!(executor.managed_calls.load(Ordering::SeqCst), 1);
        let mut multi_failure = result.clone();
        multi_failure.status = LifecycleExecutionStatus::Failed;
        multi_failure.retry_actions = vec![LifecycleFollowUpAction {
            id: "retry:codex-cli".to_string(),
            label: "Review fresh retry plan".to_string(),
            plan_request: LifecyclePlanRequest {
                resource_kind: LifecycleResourceKind::Tool,
                action: "update".to_string(),
                resource_id: "codex-cli".to_string(),
                source_analysis_handle: None,
                item_ids: None,
            },
        }];
        multi_failure.recovery_actions = vec![LifecycleFollowUpAction {
            id: "recover:frontend-design".to_string(),
            label: "Inspect state and review recovery".to_string(),
            plan_request: LifecyclePlanRequest {
                resource_kind: LifecycleResourceKind::Skill,
                action: "update".to_string(),
                resource_id: "frontend-design".to_string(),
                source_analysis_handle: None,
                item_ids: None,
            },
        }];
        let restored_request = persisted_lifecycle_request(&plan, &multi_failure);
        assert_eq!(
            restored_request.item_ids,
            Some(vec![
                "update-codex-cli".to_string(),
                "update-frontend-design".to_string()
            ])
        );
        let restored_plan = service
            .prepare(restored_request)
            .expect("restart-safe filtered plan");
        assert!(matches!(
            restored_plan.execution,
            LifecycleExecution::Batch { .. }
        ));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn batch_revalidation_failure_does_not_hide_unaffected_child_result() {
        let temp = TempDir::new().expect("tempdir");
        let workspace =
            FixtureWorkspace::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
                .with_db_path(temp.path().join("stm.sqlite"));
        let executor = Arc::new(SuccessfulExecutor::default());
        let service = LifecycleService::with_ports(
            workspace,
            executor.clone(),
            Arc::new(FixtureProbe),
            Arc::new(ChangesAfterFirstInspection::default()),
        );
        let plan = service
            .prepare(LifecyclePlanRequest {
                resource_kind: LifecycleResourceKind::Operation,
                action: "update-queue".to_string(),
                resource_id: "selected-update-queue".to_string(),
                source_analysis_handle: None,
                item_ids: Some(vec![
                    "update-codex-cli".to_string(),
                    "update-frontend-design".to_string(),
                ]),
            })
            .expect("batch plan");
        let initial = service
            .start(&plan.plan_id, authorize(&plan))
            .expect("start");
        let result = wait_for_completion(&service, &initial.operation_id);
        assert_eq!(result.items.len(), 2, "{result:?}");
        assert_eq!(result.items[0].status, LifecycleItemStatus::Failed);
        assert_eq!(result.items[1].status, LifecycleItemStatus::Skipped);
        assert_eq!(executor.managed_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn same_manager_operations_cannot_overlap() {
        let temp = TempDir::new().expect("tempdir");
        let workspace =
            FixtureWorkspace::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
                .with_db_path(temp.path().join("stm.sqlite"));
        let executor = Arc::new(SuccessfulExecutor::default());
        let service = LifecycleService::with_ports(
            workspace,
            executor.clone(),
            Arc::new(FixtureProbe),
            Arc::new(AlwaysUpdateManagerEvidence),
        );
        let first = service.prepare(request("codex-cli")).expect("first plan");
        let second = service.prepare(request("codex-cli")).expect("second plan");
        service
            .start(&first.plan_id, authorize(&first))
            .expect("first start");
        for _ in 0..50 {
            if executor.managed_calls.load(Ordering::SeqCst) == 1 {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        let error = service
            .start(&second.plan_id, authorize(&second))
            .expect_err("manager overlap must be rejected");
        assert!(matches!(error, CoreError::LifecycleConsentDenied(_)));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn independent_managers_persist_concurrent_receipts_without_busy_failures() {
        let temp = TempDir::new().expect("tempdir");
        let workspace =
            FixtureWorkspace::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
                .with_db_path(temp.path().join("stm.sqlite"));
        let executor = Arc::new(SuccessfulExecutor::default());
        let service = LifecycleService::with_ports(
            workspace,
            executor,
            Arc::new(FixtureProbe),
            Arc::new(ConvergingPerPackageEvidence::default()),
        );
        let npm_plan = service.prepare(request("codex-cli")).expect("npm plan");
        let brew_plan = service.prepare(request("cloudflared")).expect("brew plan");
        let npm_initial = service
            .start(&npm_plan.plan_id, authorize(&npm_plan))
            .expect("start npm");
        let brew_initial = service
            .start(&brew_plan.plan_id, authorize(&brew_plan))
            .expect("start Homebrew");

        let npm_result = wait_for_completion(&service, &npm_initial.operation_id);
        let brew_result = wait_for_completion(&service, &brew_initial.operation_id);
        assert_eq!(npm_result.status, LifecycleExecutionStatus::Success);
        assert_eq!(brew_result.status, LifecycleExecutionStatus::Success);
        let (store, _) = SqliteSnapshotStore::open(temp.path().join("stm.sqlite")).expect("store");
        let receipts = store.load_lifecycle_receipts().expect("receipts");
        assert_eq!(receipts.len(), 2);
        assert!(receipts
            .iter()
            .all(|receipt| receipt.lifecycle_result.is_some()));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn cancellation_reaches_running_managed_boundary() {
        let temp = TempDir::new().expect("tempdir");
        let (service, _) = service(&temp);
        let plan = service.prepare(request("codex-cli")).expect("plan");
        let initial = service
            .start(&plan.plan_id, authorize(&plan))
            .expect("start");
        service.cancel(&initial.operation_id).expect("cancel");
        let result = wait_for_completion(&service, &initial.operation_id);
        assert_eq!(
            result.status,
            LifecycleExecutionStatus::Cancelled,
            "{result:?}"
        );
        assert!(!result.can_cancel);
    }

    #[test]
    fn exact_catalog_source_issues_opaque_handle_without_accepting_commands() {
        let temp = TempDir::new().expect("tempdir");
        let (service, _) = service(&temp);
        let (analysis, request) = service
            .analyze_source(SourceKind::Tool, "https://github.com/openai/codex")
            .expect("analysis");
        assert_eq!(analysis.trust, SourceTrust::CatalogMatch);
        assert_eq!(request.resource_id, "codex-cli");
        assert!(request
            .source_analysis_handle
            .as_deref()
            .is_some_and(|handle| handle.starts_with("source-analysis-")));
        let plan = service.prepare(request).expect("source-bound plan");
        assert!(matches!(
            plan.execution,
            LifecycleExecution::DetectOnly { .. }
        ));
    }

    #[test]
    fn source_bound_plan_reprobes_and_rejects_changed_identity() {
        let temp = TempDir::new().expect("tempdir");
        let workspace =
            FixtureWorkspace::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
                .with_db_path(temp.path().join("stm.sqlite"));
        let service = LifecycleService::with_ports(
            workspace,
            Arc::new(SuccessfulExecutor::default()),
            Arc::new(RedirectingProbe::default()),
            Arc::new(AlwaysUpdateManagerEvidence),
        );
        let (_, request) = service
            .analyze_source(SourceKind::Tool, "https://github.com/openai/codex")
            .expect("analysis");
        let error = service
            .prepare(request)
            .expect_err("redirected source identity must be rejected");
        assert!(matches!(error, CoreError::LifecycleEvidenceChanged(_)));
    }

    #[test]
    fn expired_source_handle_cannot_prepare_a_plan() {
        let temp = TempDir::new().expect("tempdir");
        let (service, _) = service(&temp);
        let (_, request) = service
            .analyze_source(SourceKind::Tool, "https://github.com/openai/codex")
            .expect("analysis");
        let handle = request
            .source_analysis_handle
            .as_deref()
            .expect("source handle")
            .to_string();
        service
            .state
            .lock()
            .expect("lifecycle state")
            .sources
            .get_mut(&handle)
            .expect("source binding")
            .expires_at = SystemTime::UNIX_EPOCH;
        let error = service
            .prepare(request)
            .expect_err("expired source evidence must be rejected");
        assert!(matches!(error, CoreError::LifecycleEvidenceChanged(_)));
    }

    #[test]
    fn successful_source_reprobe_renews_binding_and_cache_expiry() {
        let temp = TempDir::new().expect("tempdir");
        let (service, _) = service(&temp);
        let (_, request) = service
            .analyze_source(SourceKind::Tool, "https://github.com/openai/codex")
            .expect("analysis");
        let handle = request
            .source_analysis_handle
            .as_deref()
            .expect("source handle")
            .to_string();
        let near_expiry = SystemTime::now() + Duration::from_secs(1);
        {
            let mut state = service.state.lock().expect("lifecycle state");
            state
                .sources
                .get_mut(&handle)
                .expect("source binding")
                .expires_at = near_expiry;
            state
                .source_cache
                .values_mut()
                .find(|cached| cached.handle == handle)
                .expect("source cache")
                .expires_at = near_expiry;
        }
        service.prepare(request).expect("source-bound plan");
        let state = service.state.lock().expect("lifecycle state");
        let renewed = state.sources.get(&handle).expect("source binding");
        let renewed_cache = state
            .source_cache
            .values()
            .find(|cached| cached.handle == handle)
            .expect("source cache");
        assert!(renewed.expires_at > near_expiry);
        assert_eq!(renewed_cache.expires_at, renewed.expires_at);
    }

    #[test]
    fn unmatched_source_stays_inspect_only_without_network_access() {
        let temp = TempDir::new().expect("tempdir");
        let workspace =
            FixtureWorkspace::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
                .with_db_path(temp.path().join("stm.sqlite"));
        let service = LifecycleService::with_ports(
            workspace,
            Arc::new(SuccessfulExecutor::default()),
            Arc::new(PanicProbe),
            Arc::new(AlwaysUpdateManagerEvidence),
        );
        let (analysis, request) = service
            .analyze_source(SourceKind::Tool, "https://example.com/unreviewed-tool")
            .expect("inspect-only analysis");
        assert_eq!(analysis.trust, SourceTrust::ReviewRequired);
        assert!(analysis
            .notes
            .iter()
            .any(|note| note.contains("network probing was skipped")));
        let (case_variant, _) = service
            .analyze_source(SourceKind::Tool, "https://github.com/OpenAI/codex")
            .expect("case-variant analysis");
        assert_eq!(case_variant.trust, SourceTrust::ReviewRequired);
        assert!(case_variant
            .notes
            .iter()
            .any(|note| note.contains("network probing was skipped")));
        let plan = service.prepare(request).expect("inspect-only plan");
        assert!(matches!(
            plan.execution,
            LifecycleExecution::DetectOnly { .. }
        ));
    }

    #[test]
    #[cfg(target_os = "linux")]
    #[ignore = "requires a disposable non-root Ubuntu runner without pkexec"]
    fn missing_non_root_privilege_broker_degrades_to_detect_only() {
        assert_eq!(
            std::env::var("STM_DISPOSABLE_LIFECYCLE").as_deref(),
            Ok("1"),
            "refusing to probe privilege behavior outside a disposable runner"
        );
        let effective_uid = std::fs::read_to_string("/proc/self/status")
            .expect("process status")
            .lines()
            .find_map(|line| line.strip_prefix("Uid:"))
            .and_then(|uids| uids.split_whitespace().nth(1))
            .and_then(|uid| uid.parse::<u32>().ok())
            .expect("effective uid");
        assert_ne!(effective_uid, 0, "test must run without root privileges");
        assert!(
            !std::path::Path::new("/usr/bin/pkexec").exists(),
            "test requires pkexec to be unavailable"
        );
        let temp = TempDir::new().expect("tempdir");
        let workspace =
            FixtureWorkspace::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
                .with_db_path(temp.path().join("stm.sqlite"));
        let executor = Arc::new(SuccessfulExecutor::default());
        let service = LifecycleService::with_ports(
            workspace,
            executor.clone(),
            Arc::new(FixtureProbe),
            Arc::new(AlwaysUpdateManagerEvidence),
        );
        let plan = service
            .prepare(LifecyclePlanRequest {
                resource_kind: LifecycleResourceKind::Tool,
                action: "uninstall".to_string(),
                resource_id: "git".to_string(),
                source_analysis_handle: None,
                item_ids: None,
            })
            .expect("detect-only plan");
        assert!(matches!(
            plan.execution,
            LifecycleExecution::DetectOnly { .. }
        ));
        assert!(plan
            .limitations
            .iter()
            .any(|limitation| limitation.contains("privilege broker")));
        assert_eq!(executor.managed_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn trusted_skill_install_conflict_restore_and_rollback_are_receipt_backed() {
        let temp = TempDir::new().expect("tempdir");
        let (service, home) = skill_service(&temp);
        let skill_request = |action: &str| LifecyclePlanRequest {
            resource_kind: LifecycleResourceKind::Skill,
            action: action.to_string(),
            resource_id: "frontend-design".to_string(),
            source_analysis_handle: None,
            item_ids: None,
        };

        let install = service
            .prepare(skill_request("skill.review_install"))
            .expect("trusted install plan");
        assert_eq!(install.privilege, LifecyclePrivilege::UserConfirmation);
        assert!(install
            .limitations
            .iter()
            .any(|line| line == "ADD SKILL.md"));
        let started = service
            .start(&install.plan_id, authorize(&install))
            .expect("start install");
        let installed = wait_for_completion(&service, &started.operation_id);
        assert_eq!(installed.status, LifecycleExecutionStatus::Success);

        let codex = home.join(".codex/skills/frontend-design");
        let claude = home.join(".claude/skills/frontend-design");
        let agentkit = home.join(".agents/skills/frontend-design");
        for target in [&codex, &claude, &agentkit] {
            assert!(target.join("SKILL.md").is_file());
            assert!(target.join("LICENSE.txt").is_file());
        }
        let (store, _) =
            SqliteSnapshotStore::open(temp.path().join("state/stm.sqlite")).expect("store");
        assert_eq!(store.load_managed_skill_receipts().expect("receipts").len(), 3);

        let marker = "\n# local managed override\n";
        let mut local = fs::read_to_string(codex.join("SKILL.md")).expect("managed manifest");
        local.push_str(marker);
        fs::write(codex.join("SKILL.md"), local).expect("local modification");
        let update = service
            .prepare(skill_request("skill.review_update"))
            .expect("update plan");
        let started = service
            .start(&update.plan_id, authorize(&update))
            .expect("start blocked update");
        let blocked = wait_for_completion(&service, &started.operation_id);
        assert!(matches!(
            blocked.status,
            LifecycleExecutionStatus::Failed | LifecycleExecutionStatus::Partial
        ));
        assert!(fs::read_to_string(codex.join("SKILL.md"))
            .expect("preserved local manifest")
            .contains(marker.trim()));

        let restore = service
            .prepare(skill_request("skill.restore_managed"))
            .expect("restore plan");
        let started = service
            .start(&restore.plan_id, authorize(&restore))
            .expect("start restore");
        let restored = wait_for_completion(&service, &started.operation_id);
        assert_eq!(restored.status, LifecycleExecutionStatus::Success);
        assert!(!fs::read_to_string(codex.join("SKILL.md"))
            .expect("restored managed manifest")
            .contains(marker.trim()));
        assert!(!store
            .load_available_skill_backups("frontend-design")
            .expect("available backup")
            .is_empty());

        let rollback = service
            .prepare(skill_request("skill.rollback_completed_target"))
            .expect("rollback plan");
        let started = service
            .start(&rollback.plan_id, authorize(&rollback))
            .expect("start rollback");
        let rolled_back = wait_for_completion(&service, &started.operation_id);
        assert_eq!(
            rolled_back.status,
            LifecycleExecutionStatus::Success,
            "{rolled_back:?}"
        );
        assert!(fs::read_to_string(codex.join("SKILL.md"))
            .expect("rolled back local manifest")
            .contains(marker.trim()));
    }
    #[test]
    fn mcp_disable_and_rollback_use_immutable_consent_and_receipts() {
        let temp = TempDir::new().expect("tempdir");
        let (service, home) = skill_service(&temp);
        let config_path = home.join(".claude.json");
        let original = br#"{
  "theme": "dark",
  "mcpServers": {
    "Filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
      "capabilities": ["resources", "tools"]
    }
  }
}"#;
        fs::write(&config_path, original).expect("MCP config");
        let request = LifecyclePlanRequest {
            resource_kind: LifecycleResourceKind::Mcp,
            action: "mcp.review_disable".to_string(),
            resource_id: "filesystem".to_string(),
            source_analysis_handle: None,
            item_ids: None,
        };
        let plan = service.prepare(request).expect("MCP disable plan");
        assert_eq!(
            plan.privilege,
            LifecyclePrivilege::UserConfirmation,
            "{plan:?}"
        );
        assert_eq!(plan.affected_paths, vec![config_path.display().to_string()]);
        let started = service
            .start(&plan.plan_id, authorize(&plan))
            .expect("start MCP disable");
        let disabled = wait_for_completion(&service, &started.operation_id);
        assert_eq!(
            disabled.status,
            LifecycleExecutionStatus::Success,
            "{disabled:?}"
        );
        assert_eq!(disabled.items.len(), 1);
        assert_eq!(disabled.items[0].status, LifecycleItemStatus::Success);
        assert!(disabled.items[0].receipt.is_some());
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(&config_path).expect("disabled config"))
                .expect("disabled JSON");
        assert_eq!(
            value["mcpServers"]["Filesystem"]["enabled"].as_bool(),
            Some(false)
        );

        let rollback_request = disabled
            .recovery_actions
            .first()
            .expect("rollback action")
            .plan_request
            .clone();
        let rollback = service.prepare(rollback_request).expect("rollback plan");
        let started = service
            .start(&rollback.plan_id, authorize(&rollback))
            .expect("start MCP rollback");
        let restored = wait_for_completion(&service, &started.operation_id);
        assert_eq!(
            restored.status,
            LifecycleExecutionStatus::Success,
            "{restored:?}"
        );
        assert_eq!(fs::read(&config_path).expect("restored config"), original);
    }
    #[test]
    fn mcp_lifecycle_mutates_the_client_specific_entry_name() {
        let temp = TempDir::new().expect("tempdir");
        let (service, home) = skill_service(&temp);
        let config_path = home.join(".claude.json");
        fs::write(
            &config_path,
            br#"{"mcpServers":{"server-filesystem":{"command":"npx","args":["-y","@modelcontextprotocol/server-filesystem","/tmp"],"capabilities":["resources","tools"]}}}"#,
        )
        .expect("MCP config");
        let plan = service
            .prepare(LifecyclePlanRequest {
                resource_kind: LifecycleResourceKind::Mcp,
                action: "mcp.review_disable".into(),
                resource_id: "filesystem".into(),
                source_analysis_handle: None,
                item_ids: None,
            })
            .expect("disable alias plan");
        let started = service
            .start(&plan.plan_id, authorize(&plan))
            .expect("disable alias");
        let result = wait_for_completion(&service, &started.operation_id);
        assert_eq!(result.status, LifecycleExecutionStatus::Success);
        let config: serde_json::Value =
            serde_json::from_slice(&fs::read(config_path).expect("updated config")).expect("json");
        assert_eq!(
            config["mcpServers"]["server-filesystem"]["enabled"].as_bool(),
            Some(false)
        );
        assert!(config["mcpServers"].get("Filesystem").is_none());
    }

    #[test]
    fn approved_stdio_mapping_produces_an_immutable_add_plan() {
        let temp = TempDir::new().expect("tempdir");
        let (service, home) = skill_service(&temp);
        let resource_root = home.join("shared-mcp-root");
        fs::create_dir_all(&resource_root).expect("resource root");
        let plan = service
            .prepare(LifecyclePlanRequest {
                resource_kind: LifecycleResourceKind::Mcp,
                action: "mcp.add".into(),
                resource_id: "filesystem".into(),
                source_analysis_handle: None,
                item_ids: Some(vec![
                    "Codex".into(),
                    resource_root.display().to_string(),
                ]),
            })
            .expect("stdio add plan");

        assert_eq!(plan.privilege, LifecyclePrivilege::UserConfirmation);
        assert!(matches!(
            plan.execution,
            LifecycleExecution::ManagedConfigMutation { ref action } if action == "add"
        ));
        assert_eq!(plan.affected_paths, vec![home.join(".codex/config.toml").display().to_string()]);
        assert!(plan.limitations.iter().any(|line| line.contains("stdio")));
        assert_eq!(
            mcp_retry_item_ids(&plan, &McpClientName::ClaudeCode),
            vec![
                resource_root.display().to_string(),
                "Claude Code".to_string(),
            ]
        );
    }

    #[test]
    fn keep_partial_plan_dispatches_as_an_mcp_lifecycle_action() {
        let temp = TempDir::new().expect("tempdir");
        let (service, _) = skill_service(&temp);
        let plan = service
            .prepare(LifecyclePlanRequest {
                resource_kind: LifecycleResourceKind::Mcp,
                action: "mcp.keep_partial".into(),
                resource_id: "github".into(),
                source_analysis_handle: None,
                item_ids: None,
            })
            .expect("keep partial plan");
        let started = service
            .start(&plan.plan_id, authorize(&plan))
            .expect("start keep partial");
        let result = wait_for_completion(&service, &started.operation_id);

        assert_eq!(result.status, LifecycleExecutionStatus::Success);
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].label, "Keep partial MCP result");
    }

    #[test]
    fn reviewed_remote_mcp_adds_supported_clients_without_persisting_credentials() {
        let temp = TempDir::new().expect("tempdir");
        std::env::set_var("GITHUB_COPILOT_TOKEN", "fixture-copilot-token");
        let (service, home) = skill_service(&temp);
        let (analysis, request) = service
            .analyze_source(SourceKind::Mcp, "https://api.githubcopilot.com/mcp/")
            .expect("MCP source analysis");
        assert_eq!(analysis.status, SourceAnalysisStatus::ReviewReady);
        let plan = service.prepare(request).expect("MCP add plan");
        assert_eq!(plan.privilege, LifecyclePrivilege::UserConfirmation);
        assert!(matches!(
            &plan.execution,
            LifecycleExecution::ManagedConfigMutation { action } if action == "add"
        ));
        assert_eq!(plan.affected_paths.len(), 3);
        assert!(plan
            .limitations
            .iter()
            .any(|line| line.contains("Credential references bound to GITHUB_COPILOT_TOKEN")));
        let started = service
            .start(&plan.plan_id, authorize(&plan))
            .expect("start MCP add");
        let added = wait_for_completion(&service, &started.operation_id);
        assert_eq!(
            added.status,
            LifecycleExecutionStatus::Success,
            "{added:?}"
        );
        assert_eq!(added.items.len(), 3);
        for path in [
            home.join(".codex/config.toml"),
            home.join(".claude.json"),
            home.join(".cursor/mcp.json"),
        ] {
            let content = fs::read_to_string(path).expect("created MCP config");
            assert!(content.contains("https://api.githubcopilot.com/mcp/"));
            assert!(content.contains("${GITHUB_COPILOT_TOKEN}"));
            assert!(!content.contains("fixture-copilot-token"));
        }
        let database = temp.path().join("state/stm.sqlite");
        let bytes = fs::read(database).expect("SQLite state");
        let database_text = String::from_utf8_lossy(&bytes);
        assert!(!database_text.contains("access_token"));
        assert!(!database_text.contains("password"));
    }
    #[test]
    fn mcp_partial_client_failure_exposes_retry_keep_and_rollback_choices() {
        let temp = TempDir::new().expect("tempdir");
        std::env::set_var("GITHUB_COPILOT_TOKEN", "fixture-copilot-token");
        let (service, home) = skill_service(&temp);
        fs::write(home.join(".claude.json"), b"{ malformed").expect("malformed config");
        let (_, request) = service
            .analyze_source(SourceKind::Mcp, "https://api.githubcopilot.com/mcp/")
            .expect("MCP source analysis");
        let plan = service.prepare(request).expect("MCP add plan");
        let started = service
            .start(&plan.plan_id, authorize(&plan))
            .expect("start partial MCP add");
        let partial = wait_for_completion(&service, &started.operation_id);
        assert_eq!(
            partial.status,
            LifecycleExecutionStatus::Partial,
            "{partial:?}"
        );
        assert_eq!(partial.completed_steps, 2);
        assert_eq!(partial.total_steps, 3);
        assert_eq!(partial.retry_actions.len(), 1);
        assert!(partial
            .recovery_actions
            .iter()
            .any(|action| action.plan_request.action == "mcp.keep_partial"));
        assert_eq!(
            partial
                .recovery_actions
                .iter()
                .filter(|action| action.plan_request.action == "mcp.rollback_completed_target")
                .count(),
            2
        );
        assert_eq!(
            fs::read(home.join(".claude.json")).expect("malformed config preserved"),
            b"{ malformed"
        );
    }
}
