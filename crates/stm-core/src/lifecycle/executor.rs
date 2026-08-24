use crate::{error::CoreError, feasibility::process_supervisor::CancelSignal};

use super::command::ExecutableIdentity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedExecutionResult {
    pub success: bool,
    pub cancelled: bool,
    pub redacted_detail: String,
}

pub trait LifecycleExecutionPort: Send + Sync {
    fn execute_managed(
        &self,
        executable: &str,
        argv: &[String],
        expected_identities: &[ExecutableIdentity],
        on_spawn: &(dyn Fn(u32) -> Result<(), CoreError> + Send + Sync),
        cancel: &CancelSignal,
    ) -> Result<ManagedExecutionResult, CoreError>;

    fn execute_native_installer(
        &self,
        executable: &str,
        argv: &[String],
        expected_identities: &[ExecutableIdentity],
        on_spawn: &(dyn Fn(u32) -> Result<(), CoreError> + Send + Sync),
    ) -> Result<ManagedExecutionResult, CoreError> {
        self.execute_managed(
            executable,
            argv,
            expected_identities,
            on_spawn,
            &CancelSignal::default(),
        )
    }

    fn install_archive_binary(
        &self,
        _staged_path: &str,
        _target_path: &str,
        _expected_identities: &[ExecutableIdentity],
        _cancel: &CancelSignal,
    ) -> Result<ManagedExecutionResult, CoreError> {
        Err(CoreError::CommandDenied(
            "archive installation is unavailable".to_string(),
        ))
    }

    fn open_vendor_handoff(&self, target: &str) -> Result<(), CoreError>;

    fn verify_bun_bootstrap(
        &self,
        _target_path: &str,
        _binary_sha256: &str,
        _expected_version: &str,
    ) -> Result<(), CoreError> {
        Err(CoreError::CommandDenied(
            "Bun bootstrap verification is unavailable".to_string(),
        ))
    }

    fn verify_migration_target(
        &self,
        _paths: &[String],
        _expected_version: &str,
    ) -> Result<(), CoreError> {
        Err(CoreError::CommandDenied(
            "migration target verification is unavailable".to_string(),
        ))
    }

    fn verify_homebrew_bootstrap(
        &self,
        _package_id: &str,
        _expected_version: &str,
        _previous_install_time: Option<u64>,
    ) -> Result<(), CoreError> {
        Err(CoreError::CommandDenied(
            "Homebrew bootstrap verification is unavailable".to_string(),
        ))
    }
}
