use std::{
    fs::{self, File, OpenOptions},
    io,
    path::PathBuf,
    process::{Command, Stdio},
    sync::Arc,
};

use stm_core::{
    feasibility::process_supervisor::{
        AllowedCommand, AllowlistedProcessSupervisor, ArgRule, CancelSignal, ExecutionRequest,
        ExecutionStatus,
    },
    lifecycle::{
        command_environment, ExecutableIdentity, LifecycleExecutionPort, ManagedExecutionResult,
    },
    CoreError,
};

use crate::host::{approved_bun_path, executable_identity, resolve_executable};

#[derive(Debug, Default)]
pub struct RealLifecycleExecutor;

impl LifecycleExecutionPort for RealLifecycleExecutor {
    fn execute_managed(
        &self,
        executable: &str,
        argv: &[String],
        expected_identities: &[ExecutableIdentity],
        on_spawn: &(dyn Fn(u32) -> Result<(), CoreError> + Send + Sync),
        cancel: &CancelSignal,
    ) -> Result<ManagedExecutionResult, CoreError> {
        if !expected_identities
            .iter()
            .any(|identity| identity.canonical_path == PathBuf::from(executable))
        {
            return Err(CoreError::LifecycleEvidenceChanged(
                "reviewed execution boundary is missing the selected executable".to_string(),
            ));
        }
        for expected_identity in expected_identities {
            let current_identity = executable_identity(expected_identity.canonical_path.clone())?;
            if !matches_reviewed_identity(expected_identity, &current_identity) {
                return Err(CoreError::LifecycleEvidenceChanged(
                    "executable identity changed immediately before spawn".to_string(),
                ));
            }
        }
        let supervisor = AllowlistedProcessSupervisor::new([AllowedCommand {
            alias: "reviewed-lifecycle-command".to_string(),
            executable: PathBuf::from(executable),
            args: argv.iter().cloned().map(ArgRule::Exact).collect(),
            environment: command_environment(executable),
        }]);
        let outcome = supervisor.execute_with_spawn_callback(
            &ExecutionRequest {
                command_alias: "reviewed-lifecycle-command".to_string(),
                args: argv.to_vec(),
                timeout_ms: 10 * 60 * 1000,
                output_limit_bytes: 64 * 1024,
            },
            cancel,
            on_spawn,
        )?;
        let (success, cancelled, detail) = match outcome.status {
            ExecutionStatus::Cancelled => (
                false,
                true,
                "Managed operation cancelled; output was discarded.",
            ),
            ExecutionStatus::TimedOut => (
                false,
                false,
                "Managed operation timed out; output was discarded.",
            ),
            ExecutionStatus::OutputLimitExceeded => (
                false,
                false,
                "Managed operation exceeded the output boundary and was stopped.",
            ),
            ExecutionStatus::Completed if outcome.exit_code == Some(0) => (
                true,
                false,
                "Authoritative manager completed successfully; sensitive output was discarded.",
            ),
            ExecutionStatus::Completed => (
                false,
                false,
                "Authoritative manager returned a non-zero status; sensitive output was discarded.",
            ),
        };
        Ok(ManagedExecutionResult {
            success,

            cancelled,
            redacted_detail: detail.to_string(),
        })
    }
    fn install_archive_binary(
        &self,
        staged_path: &str,
        target_path: &str,
        expected_identities: &[ExecutableIdentity],
        cancel: &CancelSignal,
    ) -> Result<ManagedExecutionResult, CoreError> {
        if cancel.is_cancelled() {
            return Ok(ManagedExecutionResult {
                success: false,
                cancelled: true,
                redacted_detail: "Bun provider installation cancelled before copy.".to_string(),
            });
        }
        let staged = fs::canonicalize(staged_path)?;
        let expected = expected_identities
            .iter()
            .find(|identity| identity.canonical_path == staged)
            .ok_or_else(|| {
                CoreError::LifecycleEvidenceChanged(
                    "reviewed Bun staged binary identity is missing".to_string(),
                )
            })?;
        let current = executable_identity(staged.clone())?;
        if !matches_reviewed_identity(expected, &current) {
            return Err(CoreError::LifecycleEvidenceChanged(
                "Bun staged binary changed immediately before install".to_string(),
            ));
        }
        let target = PathBuf::from(target_path);
        if !target.is_absolute() || !approved_bun_path(&target) {
            return Err(CoreError::InvalidPath(target.display().to_string()));
        }
        let parent = target
            .parent()
            .ok_or_else(|| CoreError::InvalidPath(target.display().to_string()))?;
        create_archive_parent(parent)?;
        let temp = target.with_extension("stm-installing");
        if temp.exists() {
            let metadata = fs::symlink_metadata(&temp)?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(CoreError::PathEscape(temp));
            }
            fs::remove_file(&temp)?;
        }
        let result = (|| -> Result<bool, CoreError> {
            let mut source = File::open(&staged)?;
            let mut destination = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temp)?;
            io::copy(&mut source, &mut destination)?;
            destination.sync_all()?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                destination.set_permissions(fs::Permissions::from_mode(0o500))?;
            }
            let copied = executable_identity(temp.clone())?;
            if copied.sha256 != expected.sha256 {
                return Err(CoreError::LifecycleEvidenceChanged(
                    "installed Bun binary digest does not match staged identity".to_string(),
                ));
            }
            if cancel.is_cancelled() {
                return Ok(true);
            }
            #[cfg(target_os = "windows")]
            if target.exists() {
                fs::remove_file(&target)?;
            }
            fs::rename(&temp, &target)?;
            Ok(false)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        if result? {
            let _ = fs::remove_file(&temp);
            return Ok(ManagedExecutionResult {
                success: false,
                cancelled: true,
                redacted_detail: "Bun provider installation cancelled before commit.".to_string(),
            });
        }
        Ok(ManagedExecutionResult {
            success: true,
            cancelled: false,
            redacted_detail: "Verified Bun binary installed to STM user data.".to_string(),
        })
    }

    fn execute_native_installer(
        &self,
        executable: &str,
        argv: &[String],
        expected_identities: &[ExecutableIdentity],
        on_spawn: &(dyn Fn(u32) -> Result<(), CoreError> + Send + Sync),
    ) -> Result<ManagedExecutionResult, CoreError> {
        if !expected_identities
            .iter()
            .any(|identity| identity.canonical_path == PathBuf::from(executable))
        {
            return Err(CoreError::LifecycleEvidenceChanged(
                "reviewed native installer boundary is missing".to_string(),
            ));
        }
        for expected_identity in expected_identities {
            let current_identity = executable_identity(expected_identity.canonical_path.clone())?;
            if !matches_reviewed_identity(expected_identity, &current_identity) {
                return Err(CoreError::LifecycleEvidenceChanged(
                    "native installer identity changed immediately before spawn".to_string(),
                ));
            }
        }
        let mut command = Command::new(executable);
        command
            .args(argv)
            .env_clear()
            .envs(command_environment(executable))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command
            .spawn()
            .map_err(|error| CoreError::ProcessSpawn(error.to_string()))?;
        if let Err(error) = on_spawn(child.id()) {
            let _ = child.wait();
            return Err(error);
        }
        let status = child
            .wait()
            .map_err(|error| CoreError::ProcessExecution(error.to_string()))?;
        Ok(ManagedExecutionResult {
            success: status.success(),
            cancelled: false,
            redacted_detail: if status.success() {
                "Native installer closed successfully."
            } else {
                "Native installer closed without success."
            }
            .to_string(),
        })
    }

    fn open_vendor_handoff(&self, target: &str) -> Result<(), CoreError> {
        let parsed = url::Url::parse(target)?;
        if parsed.scheme() != "https"
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(CoreError::CommandDenied(
                "vendor handoff must be a credential-free HTTPS URL".to_string(),
            ));
        }
        open::that(parsed.as_str())
            .map_err(|error| CoreError::ProcessExecution(format!("vendor handoff failed: {error}")))
    }

    fn verify_bun_bootstrap(
        &self,
        target_path: &str,
        binary_sha256: &str,
        expected_version: &str,
    ) -> Result<(), CoreError> {
        verify_bun_bootstrap_postcondition(target_path, binary_sha256, expected_version)
    }

    fn verify_migration_target(
        &self,
        paths: &[String],
        expected_version: &str,
    ) -> Result<(), CoreError> {
        verify_migration_target_paths(paths, expected_version)
    }

    fn verify_homebrew_bootstrap(
        &self,
        package_id: &str,
        expected_version: &str,
        previous_install_time: Option<u64>,
    ) -> Result<(), CoreError> {
        verify_homebrew_bootstrap_postcondition(
            package_id,
            expected_version,
            previous_install_time,
            self,
        )
    }
}

fn create_archive_parent(path: &std::path::Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true).mode(0o700).create(path)
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(path)
    }
}

fn matches_reviewed_identity(expected: &ExecutableIdentity, current: &ExecutableIdentity) -> bool {
    expected.canonical_path == current.canonical_path
        && expected.length == current.length
        && expected.modified_epoch_seconds == current.modified_epoch_seconds
        && expected.owner_id == current.owner_id
        && expected.sha256 == current.sha256
}

fn verify_bun_bootstrap_postcondition(
    target_path: &str,
    binary_sha256: &str,
    expected_version: &str,
) -> Result<(), CoreError> {
    let identity = executable_identity(PathBuf::from(target_path))?;
    if identity.sha256 != binary_sha256 {
        return Err(CoreError::LifecycleEvidenceChanged(
            "installed Bun binary digest mismatch".to_string(),
        ));
    }
    verify_exact_version(
        identity.canonical_path,
        expected_version,
        "bun-bootstrap-version",
        true,
    )
}

fn verify_migration_target_paths(
    paths: &[String],
    expected_version: &str,
) -> Result<(), CoreError> {
    let identity = paths
        .iter()
        .find_map(|path| executable_identity(PathBuf::from(path)).ok())
        .ok_or_else(|| {
            CoreError::ProcessExecution(
                "no reviewed migration target executable exists".to_string(),
            )
        })?;
    verify_exact_version(
        identity.canonical_path,
        expected_version,
        "migration-target-version",
        false,
    )
}

fn verify_exact_version(
    executable: PathBuf,
    expected_version: &str,
    alias: &str,
    exact_stdout: bool,
) -> Result<(), CoreError> {
    let executable_text = executable
        .to_str()
        .ok_or_else(|| CoreError::InvalidPath(executable.display().to_string()))?
        .to_string();
    let supervisor = AllowlistedProcessSupervisor::new([AllowedCommand {
        alias: alias.to_string(),
        executable,
        args: vec![ArgRule::Exact("--version".to_string())],
        environment: command_environment(&executable_text),
    }]);
    let outcome = supervisor.execute(
        &ExecutionRequest {
            command_alias: alias.to_string(),
            args: vec!["--version".to_string()],
            timeout_ms: 5_000,
            output_limit_bytes: 4 * 1024,
        },
        &CancelSignal::default(),
    )?;
    let matched = if exact_stdout {
        outcome.stdout.trim() == expected_version
    } else {
        outcome
            .stdout
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().last())
            == Some(expected_version)
    };
    if outcome.status != ExecutionStatus::Completed || outcome.exit_code != Some(0) || !matched {
        return Err(CoreError::ProcessExecution(if exact_stdout {
            "installed Bun version does not match the reviewed target".to_string()
        } else {
            "migration target executable version does not exactly match the reviewed target"
                .to_string()
        }));
    }
    Ok(())
}

fn verify_homebrew_bootstrap_postcondition(
    package_id: &str,
    expected_version: &str,
    previous_install_time: Option<u64>,
    executor: &dyn LifecycleExecutionPort,
) -> Result<(), CoreError> {
    if package_id != "sh.brew.homebrew" {
        return Err(CoreError::LifecycleEvidenceChanged(
            "unexpected Homebrew package identifier".to_string(),
        ));
    }
    let receipt = Command::new("/usr/sbin/pkgutil")
        .args(["--pkg-info", package_id])
        .output()
        .map_err(|error| CoreError::ProcessExecution(error.to_string()))?;
    if !receipt.status.success() {
        return Err(CoreError::ProcessExecution(
            "Homebrew package receipt is absent".to_string(),
        ));
    }
    let receipt_text = String::from_utf8_lossy(&receipt.stdout);
    let installed_version = pkg_info_field(&receipt_text, "version").ok_or_else(|| {
        CoreError::ProcessExecution("Homebrew receipt version is absent".to_string())
    })?;
    if installed_version != expected_version {
        return Err(CoreError::LifecycleEvidenceChanged(format!(
            "Homebrew receipt version changed: expected {expected_version}, got {installed_version}"
        )));
    }
    let install_time = pkg_info_field(&receipt_text, "install-time")
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| {
            CoreError::ProcessExecution("Homebrew receipt install time is absent".to_string())
        })?;
    if previous_install_time.is_some_and(|previous| install_time <= previous) {
        return Err(CoreError::LifecycleEvidenceChanged(
            "Homebrew receipt was not refreshed by this installer operation".to_string(),
        ));
    }
    let receipt_files = Command::new("/usr/sbin/pkgutil")
        .args(["--files", package_id])
        .output()
        .map_err(|error| CoreError::ProcessExecution(error.to_string()))?;
    let files = String::from_utf8_lossy(&receipt_files.stdout);
    if !receipt_files.status.success()
        || !files
            .lines()
            .any(|path| path == "bin/brew" || path.ends_with("/bin/brew"))
    {
        return Err(CoreError::LifecycleEvidenceChanged(
            "Homebrew package receipt does not own the expected brew executable".to_string(),
        ));
    }
    let brew = resolve_executable("brew").ok_or_else(|| {
        CoreError::ProcessExecution("trusted Homebrew executable is absent".to_string())
    })?;
    let identity = executable_identity(brew.clone())?;
    let outcome = executor.execute_managed(
        brew.to_str()
            .ok_or_else(|| CoreError::InvalidPath(brew.display().to_string()))?,
        &["--version".to_string()],
        &[identity],
        &|_| Ok(()),
        &CancelSignal::default(),
    )?;
    if !outcome.success {
        return Err(CoreError::ProcessExecution(
            "Homebrew executable identity probe failed".to_string(),
        ));
    }
    Ok(())
}

fn pkg_info_field<'a>(document: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("{key}: ");
    document
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(str::trim)
}

pub fn real_executor() -> Arc<dyn LifecycleExecutionPort> {
    Arc::new(RealLifecycleExecutor)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::OsString, path::Path, sync::Mutex};

    use tempfile::TempDir;

    static STM_DATA_DIR_LOCK: Mutex<()> = Mutex::new(());

    fn with_stm_data_dir<T>(root: &Path, test: impl FnOnce() -> T) -> T {
        struct Restore(Option<OsString>);

        impl Drop for Restore {
            fn drop(&mut self) {
                if let Some(value) = self.0.take() {
                    std::env::set_var("STM_DATA_DIR", value);
                } else {
                    std::env::remove_var("STM_DATA_DIR");
                }
            }
        }

        let _lock = STM_DATA_DIR_LOCK.lock().expect("STM_DATA_DIR lock");
        let _restore = Restore(std::env::var_os("STM_DATA_DIR"));
        std::env::set_var("STM_DATA_DIR", root);
        test()
    }

    fn managed_bun_target(root: &Path) -> PathBuf {
        root.join("providers/bun/1.4.0/bin")
            .join(if cfg!(target_os = "windows") {
                "bun.exe"
            } else {
                "bun"
            })
    }

    #[test]
    fn canonical_identity_ignores_the_original_lookup_spelling() {
        let expected = ExecutableIdentity {
            path: PathBuf::from("/usr/local/bin/manager"),
            canonical_path: PathBuf::from("/opt/manager/bin/manager"),
            length: 42,
            modified_epoch_seconds: 17,
            owner_id: 0,
            sha256: "abc".to_string(),
        };
        let current = ExecutableIdentity {
            path: expected.canonical_path.clone(),
            ..expected.clone()
        };
        assert!(matches_reviewed_identity(&expected, &current));
        assert!(!matches_reviewed_identity(
            &expected,
            &ExecutableIdentity {
                sha256: "changed".to_string(),
                ..current
            }
        ));
    }

    #[test]
    fn homebrew_commands_disable_unreviewed_side_effects() {
        let environment = command_environment("/opt/homebrew/bin/brew");
        assert_eq!(
            environment
                .get("HOMEBREW_NO_AUTO_UPDATE")
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(
            environment
                .get("HOMEBREW_NO_INSTALL_CLEANUP")
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(
            environment
                .get("HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK")
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(
            command_environment("/opt/homebrew/Library/Homebrew/brew.sh"),
            environment
        );
        assert!(command_environment("/usr/local/bin/npm").is_empty());
    }

    #[test]
    fn bun_commands_use_only_the_reviewed_version_root_and_path() {
        let executable = PathBuf::from("reviewed")
            .join("providers/bun/1.4.0/bin")
            .join(if cfg!(target_os = "windows") {
                "bun.exe"
            } else {
                "bun"
            });
        let bin = executable.parent().expect("Bun bin");
        let root = bin.parent().expect("Bun version root");
        let expected_root = root.display().to_string();
        let environment = command_environment(&executable.display().to_string());

        assert_eq!(
            environment.get("BUN_INSTALL").map(String::as_str),
            Some(expected_root.as_str())
        );
        let expected_path = if cfg!(target_os = "windows") {
            bin.display().to_string()
        } else {
            format!("{}:/usr/bin:/bin", bin.display())
        };
        assert_eq!(
            environment.get("PATH").map(String::as_str),
            Some(expected_path.as_str())
        );
    }

    #[test]
    fn archive_installer_copies_the_reviewed_hash_and_sets_executable_permissions() {
        let temp = TempDir::new().expect("tempdir");
        with_stm_data_dir(temp.path(), || {
            let bytes = b"\x7fELFfixture Bun binary";
            let staged = temp.path().join("bun.staged");
            fs::write(&staged, bytes).expect("staged Bun fixture");
            let expected = executable_identity(staged.clone()).expect("staged identity");
            let target = managed_bun_target(temp.path());

            let result = RealLifecycleExecutor
                .install_archive_binary(
                    &staged.display().to_string(),
                    &target.display().to_string(),
                    std::slice::from_ref(&expected),
                    &CancelSignal::default(),
                )
                .expect("archive install");

            assert!(result.success);
            assert!(!result.cancelled);
            assert_eq!(fs::read(&target).expect("installed Bun"), bytes);
            assert_eq!(
                executable_identity(target.clone())
                    .expect("installed identity")
                    .sha256,
                expected.sha256
            );
            assert!(!target.with_extension("stm-installing").exists());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    fs::metadata(&target)
                        .expect("installed metadata")
                        .permissions()
                        .mode()
                        & 0o777,
                    0o500
                );
            }
        });
    }

    #[test]
    fn archive_installer_rejects_changed_hash_and_honors_pre_copy_cancellation() {
        let temp = TempDir::new().expect("tempdir");
        with_stm_data_dir(temp.path(), || {
            let staged = temp.path().join("bun.staged");
            fs::write(&staged, b"\x7fELFfixture Bun binary").expect("staged Bun fixture");
            let expected = executable_identity(staged.clone()).expect("staged identity");
            let target = managed_bun_target(temp.path());

            let cancel = CancelSignal::default();
            cancel.cancel();
            let cancelled = RealLifecycleExecutor
                .install_archive_binary(
                    &staged.display().to_string(),
                    &target.display().to_string(),
                    std::slice::from_ref(&expected),
                    &cancel,
                )
                .expect("cancelled archive install");
            assert!(cancelled.cancelled);
            assert!(!cancelled.success);
            assert!(!target.exists());

            let changed = ExecutableIdentity {
                sha256: "changed".to_string(),
                ..expected
            };
            let error = RealLifecycleExecutor
                .install_archive_binary(
                    &staged.display().to_string(),
                    &target.display().to_string(),
                    &[changed],
                    &CancelSignal::default(),
                )
                .expect_err("changed reviewed hash must fail");
            assert!(matches!(error, CoreError::LifecycleEvidenceChanged(_)));
            assert!(!target.exists());
        });
    }
}
