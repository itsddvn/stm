#[cfg(test)]
mod tests {
    use std::{
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
        lifecycle::{
            command::ExecutableIdentity,
            evidence::{ManagerEvidencePort, ManagerStateEvidence},
            executor::ManagedExecutionResult,
            source_probe::SourceProbeEvidence,
        },
        domain::lifecycle::LifecycleChildIntent,
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
    struct FailsNativeInstallerExecutor {
        managed_calls: AtomicUsize,
    }

    impl LifecycleExecutionPort for FailsNativeInstallerExecutor {
        fn execute_managed(
            &self,
            executable: &str,
            _: &[String],
            _: &[ExecutableIdentity],
            on_spawn: &(dyn Fn(u32) -> Result<(), CoreError> + Send + Sync),
            _: &CancelSignal,
        ) -> Result<ManagedExecutionResult, CoreError> {
            on_spawn(std::process::id())?;
            self.managed_calls.fetch_add(1, Ordering::SeqCst);
            Ok(ManagedExecutionResult {
                success: false,
                cancelled: false,
                redacted_detail: if executable == "/usr/bin/open" {
                    "native installer failed"
                } else {
                    "dependent should not execute"
                }
                .to_string(),
            })
        }

        fn open_vendor_handoff(&self, _: &str) -> Result<(), CoreError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct FailsBunArchiveExecutor {
        archive_calls: AtomicUsize,
    }

    impl LifecycleExecutionPort for FailsBunArchiveExecutor {
        fn execute_managed(
            &self,
            _: &str,
            _: &[String],
            _: &[ExecutableIdentity],
            on_spawn: &(dyn Fn(u32) -> Result<(), CoreError> + Send + Sync),
            _: &CancelSignal,
        ) -> Result<ManagedExecutionResult, CoreError> {
            on_spawn(std::process::id())?;
            Ok(ManagedExecutionResult {
                success: true,
                cancelled: false,
                redacted_detail: "fixture manager success".to_string(),
            })
        }

        fn install_archive_binary(
            &self,
            _: &str,
            _: &str,
            _: &[ExecutableIdentity],
            _: &CancelSignal,
        ) -> Result<ManagedExecutionResult, CoreError> {
            self.archive_calls.fetch_add(1, Ordering::SeqCst);
            Ok(ManagedExecutionResult {
                success: false,
                cancelled: false,
                redacted_detail: "fixture Bun bootstrap failed".to_string(),
            })
        }

        fn open_vendor_handoff(&self, _: &str) -> Result<(), CoreError> {
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
            let converged = inspection >= 4;
            Ok(ManagerStateEvidence {
                installed: true,
                current_version: Some(if converged { "1.0.0" } else { "0.1.0" }.to_string()),
                target_version: "1.0.0".to_string(),
                update_available: !converged,
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
            let current_version = if *inspection >= 4 { "1.0.0" } else { "0.1.0" };
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

    #[cfg(target_os = "macos")]
    struct HomebrewCodexOwnerEvidence;

    #[cfg(target_os = "macos")]
    impl ManagerEvidencePort for HomebrewCodexOwnerEvidence {
        fn inspect(
            &self,
            mapping: &ToolCatalogMapping,
            _: &str,
        ) -> Result<ManagerStateEvidence, CoreError> {
            let installed = mapping.manager == "homebrew" && mapping.package_id == "codex";
            Ok(ManagerStateEvidence {
                installed,
                current_version: installed.then(|| "0.32.0".to_string()),
                target_version: "1.0.0".to_string(),
                update_available: installed,
                source: "Injected exact owner evidence".to_string(),
            })
        }
    }

    #[cfg(target_os = "macos")]
    struct HomebrewCodexOwnerAfterNpmFailureEvidence;

    #[cfg(target_os = "macos")]
    impl ManagerEvidencePort for HomebrewCodexOwnerAfterNpmFailureEvidence {
        fn inspect(
            &self,
            mapping: &ToolCatalogMapping,
            _: &str,
        ) -> Result<ManagerStateEvidence, CoreError> {
            if mapping.manager == "npm" {
                return Err(CoreError::MalformedInput(
                    "npm registry unavailable".to_string(),
                ));
            }
            let installed = mapping.manager == "homebrew" && mapping.package_id == "codex";
            Ok(ManagerStateEvidence {
                installed,
                current_version: installed.then(|| "0.32.0".to_string()),
                target_version: "1.0.0".to_string(),
                update_available: installed,
                source: "Injected exact owner evidence after alternate failure".to_string(),
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

    struct MissingManagerEvidence;

    impl ManagerEvidencePort for MissingManagerEvidence {
        fn inspect(
            &self,
            _: &ToolCatalogMapping,
            _: &str,
        ) -> Result<ManagerStateEvidence, CoreError> {
            Ok(ManagerStateEvidence {
                installed: false,
                current_version: None,
                target_version: "1.0.0".to_string(),
                update_available: false,
                source: "Injected missing manager evidence".to_string(),
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
            let changed = inspection >= 2;
            Ok(ManagerStateEvidence {
                installed: true,
                current_version: Some(if changed { "1.0.0" } else { "0.1.0" }.to_string()),
                target_version: "1.0.0".to_string(),
                update_available: !changed,
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
            if inspection >= 4 {
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

    fn bun_artifact(temp: &TempDir) -> crate::domain::recipe::VerifiedArchiveBinary {
        let bytes = b"\x7fELFfixture Bun binary";
        let staged = temp.path().join("bun.staged");
        std::fs::write(&staged, bytes).expect("staged Bun fixture");
        let spec = crate::domain::recipe::pinned_bun_archive(
            crate::inventory::current_platform_slug(),
        )
        .expect("pinned Bun fixture target");
        crate::domain::recipe::VerifiedArchiveBinary {
            provider_id: "bun".to_string(),
            version: crate::domain::recipe::PINNED_BUN_VERSION.to_string(),
            source_url: crate::domain::recipe::pinned_bun_source_url(spec),
            archive_sha256: spec.sha256.to_string(),
            binary_sha256: crate::adapters::compute_sha256([bytes.to_vec()])
                .trim_start_matches("sha256:")
                .to_string(),
            staged_binary_path: staged.display().to_string(),
            target_binary_path: test_support::TestHost
                .expected_stm_bun_binary_path()
                .display()
                .to_string(),
        }
    }

    #[cfg(target_os = "macos")]
    fn homebrew_artifact(
        temp: &TempDir,
    ) -> crate::domain::recipe::VerifiedInstallerArtifact {
        let bytes = b"verified fixture package";
        let package_path = temp.path().join("Homebrew.pkg");
        std::fs::write(&package_path, bytes).expect("Homebrew package fixture");
        crate::domain::recipe::VerifiedInstallerArtifact {
            provider_id: "homebrew".to_string(),
            path: package_path.display().to_string(),
            version: "test".to_string(),
            source_url: "https://github.com/Homebrew/brew/releases/download/test/Homebrew.pkg"
                .to_string(),
            sha256: crate::adapters::compute_sha256([bytes.to_vec()])
                .trim_start_matches("sha256:")
                .to_string(),
            signer_team_id: "927JGANW46".to_string(),
            package_id: "sh.brew.homebrew".to_string(),
            previous_receipt_install_time: None,
            expected_executable_paths: vec!["/opt/homebrew/bin/brew".to_string()],
        }
    }

    #[cfg(target_os = "macos")]
    fn clean_machine_workspace(temp: &TempDir) -> FixtureWorkspace {
        fn copy_tree(source: &std::path::Path, target: &std::path::Path) {
            std::fs::create_dir_all(target).expect("fixture directory");
            for entry in std::fs::read_dir(source).expect("fixture tree") {
                let entry = entry.expect("fixture entry");
                let destination = target.join(entry.file_name());
                if entry.file_type().expect("fixture type").is_dir() {
                    copy_tree(&entry.path(), &destination);
                } else {
                    std::fs::copy(entry.path(), destination).expect("copy fixture");
                }
            }
        }

        let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        copy_tree(&project.join("catalog"), &temp.path().join("catalog"));
        copy_tree(
            &project.join("tests/fixtures"),
            &temp.path().join("tests/fixtures"),
        );
        let probes_path = temp.path().join("tests/fixtures/tools/probes.json");
        let mut probes: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&probes_path).expect("tool probes"))
                .expect("tool probe JSON");
        probes
            .as_array_mut()
            .expect("tool probe array")
            .retain(|probe| {
                !matches!(
                    probe["toolId"].as_str(),
                    Some("codex-cli" | "agentkit-cli" | "orca-ade")
                )
            });
        std::fs::write(
            probes_path,
            serde_json::to_vec(&probes).expect("clean-machine probes"),
        )
        .expect("write clean-machine probes");
        let updates_path = temp.path().join("tests/fixtures/tools/update-metadata.json");
        let mut updates: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&updates_path).expect("tool updates"))
                .expect("tool update JSON");
        updates
            .as_array_mut()
            .expect("tool update array")
            .retain(|update| {
                !matches!(
                    update["toolId"].as_str(),
                    Some("codex-cli" | "agentkit-cli" | "orca-ade")
                )
            });
        std::fs::write(
            updates_path,
            serde_json::to_vec(&updates).expect("clean-machine updates"),
        )
        .expect("write clean-machine updates");
        std::fs::write(
            temp.path().join("tests/fixtures/managers/npm/success.txt"),
            "\n",
        )
        .expect("write clean-machine npm inventory");
        std::fs::write(
            temp.path()
                .join("tests/fixtures/managers/homebrew/success.txt"),
            "\n",
        )
        .expect("write clean-machine Homebrew inventory");
        FixtureWorkspace::new(temp.path()).with_db_path(temp.path().join("stm.sqlite"))
    }

    fn request(resource_id: &str) -> LifecyclePlanRequest {
        LifecyclePlanRequest {
            resource_kind: LifecycleResourceKind::Tool,
            action: "update".to_string(),
            resource_id: resource_id.to_string(),
            source_analysis_handle: None,
            item_ids: None,
            children: Vec::new(),
            mapping_id: None,
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
        let english_summary = service
            .native_confirmation_summary(&plan.plan_id, "en")
            .expect("English native summary");
        assert!(english_summary.contains("install or update"));
        assert!(!english_summary.contains("Plan ID"));
        let vietnamese_summary = service
            .native_confirmation_summary(&plan.plan_id, "vi")
            .expect("Vietnamese native summary");
        assert!(vietnamese_summary.contains("cài đặt hoặc cập nhật"));
        assert!(executable.starts_with('/'));
        assert_eq!(
            &argv[..3],
            ["install", "--global", "@openai/codex@1.0.0"]
        );
        assert!(argv[3..]
            .iter()
            .any(|argument| argument == "--registry=https://registry.npmjs.org/"));
        assert!(argv[3..]
            .iter()
            .any(|argument| argument.starts_with("--userconfig=")));
        assert!(argv[3..]
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
        let journal_store =
            test_support::TestSnapshotStore::shared(temp.path().join("stm.sqlite"));
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

        let store = test_support::TestSnapshotStore::shared(temp.path().join("stm.sqlite"));
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
                children: Vec::new(),
                mapping_id: None,
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
        let store = test_support::TestSnapshotStore::shared(temp.path().join("stm.sqlite"));
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
        let store = test_support::TestSnapshotStore::shared(temp.path().join("stm.sqlite"));
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
    fn live_owner_evidence_selects_authoritative_provider_mapping() {
        let temp = TempDir::new().expect("tempdir");
        let workspace =
            FixtureWorkspace::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
                .with_db_path(temp.path().join("stm.sqlite"));
        let service = LifecycleService::with_ports(
            workspace,
            Arc::new(SuccessfulExecutor::default()),
            Arc::new(FixtureProbe),
            Arc::new(HomebrewCodexOwnerEvidence),
        );
        let plan = service
            .prepare(LifecyclePlanRequest {
                resource_kind: LifecycleResourceKind::Tool,
                action: "update".to_string(),
                resource_id: "codex-cli".to_string(),
                source_analysis_handle: None,
                item_ids: None,
                children: Vec::new(),
                mapping_id: None,
            })
            .expect("live owner plan");
        assert_eq!(plan.mapping_id, "homebrew:codex");
        assert_eq!(plan.owner, "Homebrew");
        assert!(matches!(
            plan.execution,
            LifecycleExecution::ManagedExecute { .. }
        ));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn live_owner_scan_continues_after_alternate_provider_failure() {
        let temp = TempDir::new().expect("tempdir");
        let workspace =
            FixtureWorkspace::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
                .with_db_path(temp.path().join("stm.sqlite"));
        let service = LifecycleService::with_ports(
            workspace,
            Arc::new(SuccessfulExecutor::default()),
            Arc::new(FixtureProbe),
            Arc::new(HomebrewCodexOwnerAfterNpmFailureEvidence),
        );
        let plan = service
            .prepare(LifecyclePlanRequest {
                resource_kind: LifecycleResourceKind::Tool,
                action: "update".to_string(),
                resource_id: "codex-cli".to_string(),
                source_analysis_handle: None,
                item_ids: None,
                children: Vec::new(),
                mapping_id: None,
            })
            .expect("later live owner plan");
        assert_eq!(plan.mapping_id, "homebrew:codex");
        assert!(matches!(
            plan.execution,
            LifecycleExecution::ManagedExecute { .. }
        ));
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
        assert_eq!(result.items[0].status, LifecycleItemStatus::Failed);
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
                children: Vec::new(),
                mapping_id: None,
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
                children: Vec::new(),
                mapping_id: None,
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
                children: Vec::new(),
                mapping_id: None,
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
    fn typed_dependency_dag_orders_children_and_blocks_failed_dependents() {
        let temp = TempDir::new().expect("tempdir");
        let workspace =
            FixtureWorkspace::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
                .with_db_path(temp.path().join("stm.sqlite"));
        let executor = Arc::new(FailsNativeInstallerExecutor::default());
        let service = LifecycleService::with_ports(
            workspace,
            executor.clone(),
            Arc::new(FixtureProbe),
            Arc::new(AlwaysUpdateManagerEvidence),
        );
        let plan = service
            .prepare(LifecyclePlanRequest {
                resource_kind: LifecycleResourceKind::Operation,
                action: "update-queue".to_string(),
                resource_id: "dependency-order".to_string(),
                source_analysis_handle: None,
                item_ids: None,
                children: vec![
                    LifecycleChildIntent {
                        resource_kind: LifecycleResourceKind::Tool,
                        resource_id: "cloudflared".to_string(),
                        desired_action: "update".to_string(),
                        mapping_id: Some("homebrew:cloudflared".to_string()),
                        depends_on: vec!["codex-cli".to_string()],
                    },
                    LifecycleChildIntent {
                        resource_kind: LifecycleResourceKind::Tool,
                        resource_id: "codex-cli".to_string(),
                        desired_action: "update".to_string(),
                        mapping_id: Some("npm:@openai/codex".to_string()),
                        depends_on: Vec::new(),
                    },
                ],
                mapping_id: None,
            })
            .expect("dependency plan");
        let LifecycleExecution::Batch { items } = &plan.execution else {
            panic!("batch");
        };
        assert_eq!(items[0].resource_id, "codex-cli");
        assert_eq!(items[1].resource_id, "cloudflared");

        let initial = service
            .start(&plan.plan_id, authorize(&plan))
            .expect("start dependency plan");
        let result = wait_for_completion(&service, &initial.operation_id);
        assert_eq!(executor.managed_calls.load(Ordering::SeqCst), 1);
        assert_eq!(result.items[0].status, LifecycleItemStatus::Failed);
        assert_eq!(result.items[1].status, LifecycleItemStatus::Skipped);
        assert_eq!(
            result.items[1].redacted_detail,
            "Required dependency did not complete successfully."
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn typed_dependency_dag_rejects_missing_and_cyclic_edges() {
        let temp = TempDir::new().expect("tempdir");
        let (service, _) = service(&temp);
        let child = |resource_id: &str, depends_on: Vec<String>| LifecycleChildIntent {
            resource_kind: LifecycleResourceKind::Tool,
            resource_id: resource_id.to_string(),
            desired_action: "update".to_string(),
            mapping_id: None,
            depends_on,
        };
        let request = |children| LifecyclePlanRequest {
            resource_kind: LifecycleResourceKind::Operation,
            action: "update-queue".to_string(),
            resource_id: "dependency-validation".to_string(),
            source_analysis_handle: None,
            item_ids: None,
            children,
            mapping_id: None,
        };

        let missing = service
            .prepare(request(vec![child(
                "codex-cli",
                vec!["missing-tool".to_string()],
            )]))
            .expect_err("unknown dependency");
        assert!(missing.to_string().contains("unknown dependency"));

        let cycle = service
            .prepare(request(vec![
                child("codex-cli", vec!["cloudflared".to_string()]),
                child("cloudflared", vec!["codex-cli".to_string()]),
            ]))
            .expect_err("cyclic dependencies");
        assert!(cycle.to_string().contains("contains a cycle"));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn failed_homebrew_bootstrap_skips_dependent_children() {
        let temp = TempDir::new().expect("tempdir");
        let artifact = homebrew_artifact(&temp);
        let workspace =
            FixtureWorkspace::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
                .with_db_path(temp.path().join("stm.sqlite"));
        let executor = Arc::new(FailsNativeInstallerExecutor::default());
        let service = LifecycleService::with_ports(
            workspace,
            executor.clone(),
            Arc::new(FixtureProbe),
            Arc::new(MissingManagerEvidence),
        );
        let request = LifecyclePlanRequest {
            resource_kind: LifecycleResourceKind::Operation,
            action: "setup-queue".to_string(),
            resource_id: "quick-setup".to_string(),
            source_analysis_handle: None,
            item_ids: Some(vec!["orbstack".to_string()]),
            children: vec![LifecycleChildIntent {
                resource_kind: LifecycleResourceKind::Tool,
                resource_id: "orbstack".to_string(),
                desired_action: "install".to_string(),
                mapping_id: Some("homebrew:orbstack".to_string()),
                depends_on: Vec::new(),
            }],
            mapping_id: None,
        };
        let plan = service
            .prepare_setup_with_bootstrap(request, &artifact)
            .expect("bootstrap plan");
        let LifecycleExecution::Batch { items } = &plan.execution else {
            panic!("batch");
        };
        assert!(matches!(
            items[0].execution,
            LifecycleExecution::NativeInstaller { .. }
        ));
        let initial = service
            .start(&plan.plan_id, authorize(&plan))
            .expect("start");
        let result = wait_for_completion(&service, &initial.operation_id);
        assert_eq!(executor.managed_calls.load(Ordering::SeqCst), 1);
        assert_eq!(result.items[0].status, LifecycleItemStatus::Failed);
        assert_eq!(result.items[1].status, LifecycleItemStatus::Skipped);
        assert!(result.items[1].redacted_detail.contains("dependency"));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn expired_consent_after_native_prompt_never_starts_installer() {
        let temp = TempDir::new().expect("tempdir");
        let workspace =
            FixtureWorkspace::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
                .with_db_path(temp.path().join("stm.sqlite"));
        let executor = Arc::new(FailsNativeInstallerExecutor::default());
        let service = LifecycleService::with_ports(
            workspace,
            executor.clone(),
            Arc::new(FixtureProbe),
            Arc::new(MissingManagerEvidence),
        );
        let mut plan = service
            .prepare_setup_with_bootstrap(
                LifecyclePlanRequest {
                    resource_kind: LifecycleResourceKind::Operation,
                    action: "setup-queue".to_string(),
                    resource_id: "expired-native-prompt".to_string(),
                    source_analysis_handle: None,
                    item_ids: Some(vec!["orbstack".to_string()]),
                    children: vec![LifecycleChildIntent {
                        resource_kind: LifecycleResourceKind::Tool,
                        resource_id: "orbstack".to_string(),
                        desired_action: "install".to_string(),
                        mapping_id: Some("homebrew:orbstack".to_string()),
                        depends_on: Vec::new(),
                    }],
                    mapping_id: None,
                },
                &homebrew_artifact(&temp),
            )
            .expect("native installer plan");
        plan.expires_at = "2000-01-01T00:00:00Z".to_string();
        plan.digest.clear();
        plan.digest = crate::adapters::compute_sha256([
            serde_json::to_vec(&plan).expect("expired plan digest"),
        ]);
        service
            .state
            .lock()
            .expect("lifecycle state")
            .plans
            .get_mut(&plan.plan_id)
            .expect("stored plan")
            .prepared
            .plan = plan.clone();

        let error = service
            .start(&plan.plan_id, authorize(&plan))
            .expect_err("expired native prompt consent");
        assert!(matches!(error, CoreError::LifecycleConsentDenied(_)));
        assert_eq!(executor.managed_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn clean_macos_profile_uses_one_homebrew_prerequisite_without_node() {
        let temp = TempDir::new().expect("tempdir");
        let workspace = clean_machine_workspace(&temp);
        let service = LifecycleService::with_ports(
            workspace,
            Arc::new(SuccessfulExecutor::default()),
            Arc::new(FixtureProbe),
            Arc::new(MissingManagerEvidence),
        );
        let children = [
            ("agentkit-cli", "homebrew:agentkit"),
            ("codex-cli", "homebrew:codex"),
            ("orca-ade", "homebrew:stablyai/orca/orca"),
        ]
        .into_iter()
        .map(|(resource_id, mapping_id)| LifecycleChildIntent {
            resource_kind: LifecycleResourceKind::Tool,
            resource_id: resource_id.to_string(),
            desired_action: "install".to_string(),
            mapping_id: Some(mapping_id.to_string()),
            depends_on: Vec::new(),
        })
        .collect::<Vec<_>>();
        let request = LifecyclePlanRequest {
            resource_kind: LifecycleResourceKind::Operation,
            action: "setup-queue".to_string(),
            resource_id: "quick-setup".to_string(),
            source_analysis_handle: None,
            item_ids: Some(
                children
                    .iter()
                    .map(|child| child.resource_id.clone())
                    .collect(),
            ),
            children,
            mapping_id: None,
        };
        let plan = service
            .prepare_setup_with_bootstrap(request, &homebrew_artifact(&temp))
            .expect("clean macOS bootstrap plan");
        let LifecycleExecution::Batch { items } = &plan.execution else {
            panic!("batch");
        };
        assert_eq!(items.len(), 4);
        assert!(matches!(
            items[0].execution,
            LifecycleExecution::NativeInstaller { .. }
        ));
        let mapping_ids = items
            .iter()
            .map(|item| item.mapping_id.as_str())
            .collect::<Vec<_>>();
        assert!(mapping_ids.contains(&"homebrew:agentkit"), "{mapping_ids:?}");
        assert!(mapping_ids.contains(&"homebrew:codex"), "{mapping_ids:?}");
        assert!(
            mapping_ids.contains(&"homebrew:stablyai/orca/orca")
                || mapping_ids.contains(&"vendor:com.orca.ade"),
            "{mapping_ids:?}"
        );
        assert!(items
            .iter()
            .all(|item| !item.mapping_id.starts_with("npm:")
                && !item.affected_paths.iter().any(|path| path.contains("node"))));

        let state = service.state.lock().expect("lifecycle state");
        let prepared = &state.plans.get(&plan.plan_id).expect("stored plan").prepared;
        assert_eq!(
            prepared
                .children
                .iter()
                .filter(|child| child.dependency_key == "homebrew")
                .count(),
            1
        );
        assert!(prepared.children[1..]
            .iter()
            .filter(|child| child.plan.mapping_id.starts_with("homebrew:"))
            .all(|child| child.depends_on == ["homebrew"]));
        assert!(prepared.children[1..]
            .iter()
            .filter(|child| child.plan.mapping_id.starts_with("vendor:"))
            .all(|child| child.depends_on.is_empty()));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn failed_bun_bootstrap_skips_only_its_dependent_child() {
        let temp = TempDir::new().expect("tempdir");
        let workspace = clean_machine_workspace(&temp);
        let executor = Arc::new(FailsBunArchiveExecutor::default());
        let service = LifecycleService::with_ports(
            workspace,
            executor.clone(),
            Arc::new(FixtureProbe),
            Arc::new(MissingManagerEvidence),
        );
        let request = LifecyclePlanRequest {
            resource_kind: LifecycleResourceKind::Operation,
            action: "setup-queue".to_string(),
            resource_id: "quick-setup".to_string(),
            source_analysis_handle: None,
            item_ids: Some(vec![
                "codex-cli".to_string(),
                "frontend-design".to_string(),
            ]),
            children: vec![
                LifecycleChildIntent {
                    resource_kind: LifecycleResourceKind::Tool,
                    resource_id: "codex-cli".to_string(),
                    desired_action: "install".to_string(),
                    mapping_id: Some("bun:@openai/codex".to_string()),
                    depends_on: Vec::new(),
                },
                LifecycleChildIntent {
                    resource_kind: LifecycleResourceKind::Skill,
                    resource_id: "frontend-design".to_string(),
                    desired_action: "review".to_string(),
                    mapping_id: None,
                    depends_on: Vec::new(),
                },
            ],
            mapping_id: None,
        };
        let artifact = bun_artifact(&temp);
        let plan = service
            .prepare_setup_with_bun_bootstrap(request, &artifact)
            .expect("Bun bootstrap plan");
        let LifecycleExecution::Batch { items } = &plan.execution else {
            panic!("batch");
        };
        assert!(matches!(
            items[0].execution,
            LifecycleExecution::ArchiveInstaller { .. }
        ));

        let initial = service
            .start(&plan.plan_id, authorize(&plan))
            .expect("start");
        let result = wait_for_completion(&service, &initial.operation_id);
        assert_eq!(executor.archive_calls.load(Ordering::SeqCst), 1);
        assert_eq!(result.items.len(), 3, "{result:?}");
        assert_eq!(result.items[0].status, LifecycleItemStatus::Failed);
        assert_eq!(result.items[1].status, LifecycleItemStatus::Skipped);
        assert_eq!(
            result.items[1].redacted_detail,
            "Required dependency did not complete successfully."
        );
        assert_eq!(result.items[2].status, LifecycleItemStatus::Skipped);
        assert_ne!(
            result.items[2].redacted_detail,
            "Required dependency did not complete successfully."
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn combined_provider_bootstraps_bind_each_child_to_its_exact_provider() {
        let temp = TempDir::new().expect("tempdir");
        let workspace = clean_machine_workspace(&temp);
        let service = LifecycleService::with_ports(
            workspace,
            Arc::new(SuccessfulExecutor::default()),
            Arc::new(FixtureProbe),
            Arc::new(MissingManagerEvidence),
        );
        let request = LifecyclePlanRequest {
            resource_kind: LifecycleResourceKind::Operation,
            action: "setup-queue".to_string(),
            resource_id: "quick-setup".to_string(),
            source_analysis_handle: None,
            item_ids: Some(vec!["orbstack".to_string(), "codex-cli".to_string()]),
            children: vec![
                LifecycleChildIntent {
                    resource_kind: LifecycleResourceKind::Tool,
                    resource_id: "orbstack".to_string(),
                    desired_action: "install".to_string(),
                    mapping_id: Some("homebrew:orbstack".to_string()),
                    depends_on: Vec::new(),
                },
                LifecycleChildIntent {
                    resource_kind: LifecycleResourceKind::Tool,
                    resource_id: "codex-cli".to_string(),
                    desired_action: "install".to_string(),
                    mapping_id: Some("bun:@openai/codex".to_string()),
                    depends_on: Vec::new(),
                },
            ],
            mapping_id: None,
        };
        let homebrew = homebrew_artifact(&temp);
        let bun = bun_artifact(&temp);
        let plan = service
            .prepare_setup_with_provider_bootstraps(request, Some(&homebrew), Some(&bun))
            .expect("combined bootstrap plan");
        let LifecycleExecution::Batch { items } = &plan.execution else {
            panic!("batch");
        };
        assert!(matches!(
            items[0].execution,
            LifecycleExecution::NativeInstaller { .. }
        ));
        assert!(matches!(
            items[1].execution,
            LifecycleExecution::ArchiveInstaller { .. }
        ));
        assert!(!plan_can_cancel(&plan));
        assert!(execution_sequence_can_cancel(&items[1..]));

        let state = service.state.lock().expect("lifecycle state");
        let prepared = &state
            .plans
            .get(&plan.plan_id)
            .expect("stored combined plan")
            .prepared;
        let homebrew_child = prepared
            .children
            .iter()
            .find(|child| child.plan.mapping_id == "homebrew:orbstack")
            .expect("Homebrew child");
        let bun_child = prepared
            .children
            .iter()
            .find(|child| child.plan.mapping_id == "bun:@openai/codex")
            .expect("Bun child");
        assert!(homebrew_child.staged);
        assert_eq!(homebrew_child.depends_on, ["homebrew"]);
        assert!(bun_child.staged);
        assert_eq!(bun_child.depends_on, ["bun"]);
    }

    #[test]
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "x86_64")
    ))]
    fn archive_plan_rejects_unpinned_bun_metadata() {
        let temp = TempDir::new().expect("tempdir");
        let mut artifact = bun_artifact(&temp);
        artifact.archive_sha256 = "0".repeat(64);
        let error = crate::lifecycle::planner::prepare_archive_installer_plan(
            &test_support::TestHost,
            LifecyclePlanRequest {
                resource_kind: LifecycleResourceKind::Operation,
                action: "bootstrap".to_string(),
                resource_id: "bun".to_string(),
                source_analysis_handle: None,
                item_ids: None,
                children: Vec::new(),
                mapping_id: None,
            },
            &artifact,
            1,
            SystemTime::now(),
        )
        .expect_err("unpinned Bun metadata");
        assert!(error
            .to_string()
            .contains("does not match the pinned Bun artifact"));
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
                children: Vec::new(),
                mapping_id: None,
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
        if test_support::TestHost.resolve_executable("brew").is_none() {
            return;
        }
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
        let store = test_support::TestSnapshotStore::shared(temp.path().join("stm.sqlite"));
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
                children: Vec::new(),
                mapping_id: None,
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
    fn imported_package_alias_review_cannot_handoff_or_execute() {
        let temp = TempDir::new().expect("tempdir");
        let (service, executor) = service(&temp);
        let plan = service
            .prepare(LifecyclePlanRequest {
                resource_kind: LifecycleResourceKind::Tool,
                action: "review".to_string(),
                resource_id: "com.docker.docker".to_string(),
                source_analysis_handle: None,
                item_ids: None,
                children: Vec::new(),
                mapping_id: None,
            })
            .expect("review-only");
        assert!(matches!(
            plan.execution,
            LifecycleExecution::DetectOnly { .. }
        ));
        assert_eq!(executor.managed_calls.load(Ordering::SeqCst), 0);
        assert_eq!(executor.handoff_calls.load(Ordering::SeqCst), 0);
    }
}
