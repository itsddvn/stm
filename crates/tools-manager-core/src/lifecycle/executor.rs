use std::{path::PathBuf, sync::Arc};

use super::command::{command_environment, executable_identity, ExecutableIdentity};
use crate::{
    error::CoreError,
    feasibility::process_supervisor::{
        AllowedCommand, AllowlistedProcessSupervisor, ArgRule, CancelSignal, ExecutionRequest,
        ExecutionStatus,
    },
};

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
    fn open_vendor_handoff(&self, target: &str) -> Result<(), CoreError>;
}

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
}

fn matches_reviewed_identity(expected: &ExecutableIdentity, current: &ExecutableIdentity) -> bool {
    expected.canonical_path == current.canonical_path
        && expected.length == current.length
        && expected.modified_epoch_seconds == current.modified_epoch_seconds
        && expected.owner_id == current.owner_id
        && expected.sha256 == current.sha256
}

pub fn real_executor() -> Arc<dyn LifecycleExecutionPort> {
    Arc::new(RealLifecycleExecutor)
}
#[cfg(test)]
mod tests {
    use super::*;

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
}
