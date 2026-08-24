use std::sync::Arc;

#[cfg(test)]
use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    adapters::FixtureWorkspace,
    error::CoreError,
    feasibility::process_supervisor::CancelSignal,
    skill_lifecycle::{
        cleanup_private_staging, GitResolverLimits, PublicGithubSkillResolver,
        ReviewedGitExecutable, SkillSourceSpec, SkillStagingEvidence,
    },
};

#[cfg(test)]
use crate::skill_lifecycle::{validate_staged_tree, TreeValidationPolicy};

use super::command::resolve_executable;

pub(super) trait SkillSourceResolverPort: Send + Sync {
    fn resolve(
        &self,
        workspace: &FixtureWorkspace,
        source: &SkillSourceSpec,
        cancel: &CancelSignal,
    ) -> Result<SkillStagingEvidence, CoreError>;

    fn cleanup(
        &self,
        workspace: &FixtureWorkspace,
        evidence: &SkillStagingEvidence,
    ) -> Result<(), CoreError> {
        cleanup_private_staging(&workspace.db_path(), evidence)
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct RealSkillSourceResolver;

impl SkillSourceResolverPort for RealSkillSourceResolver {
    fn resolve(
        &self,
        workspace: &FixtureWorkspace,
        source: &SkillSourceSpec,
        cancel: &CancelSignal,
    ) -> Result<SkillStagingEvidence, CoreError> {
        let executable = resolve_executable("git").ok_or_else(|| {
            CoreError::CommandDenied("reviewed Git executable was not found".to_string())
        })?;

        let git = ReviewedGitExecutable::new(executable)?;
        PublicGithubSkillResolver::new(git, workspace.db_path(), GitResolverLimits::default())?
            .resolve(source, cancel)
    }
}

pub(super) fn real_skill_source_resolver() -> Arc<dyn SkillSourceResolverPort> {
    Arc::new(RealSkillSourceResolver)
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, Default)]
struct FixtureSkillSourceResolver;

#[cfg(test)]
static FIXTURE_STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[cfg(test)]
impl SkillSourceResolverPort for FixtureSkillSourceResolver {
    fn resolve(
        &self,
        workspace: &FixtureWorkspace,
        source: &SkillSourceSpec,
        _cancel: &CancelSignal,
    ) -> Result<SkillStagingEvidence, CoreError> {
        let fixture = workspace
            .project_root()
            .join("tests/fixtures/skill-lifecycle/frontend-design");
        let db_path = workspace.db_path();
        let parent = db_path.parent().ok_or_else(|| {
            CoreError::InvalidPath("workspace database must have a parent directory".to_string())
        })?;
        let operation = parent.join(".stm-skill-staging").join(format!(
            "fixture-resolve-{}",
            FIXTURE_STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let tree = operation.join("tree");
        create_private_directory(&tree)?;
        copy_fixture_tree(&fixture, &tree)?;
        let evidence = validate_staged_tree(&tree, TreeValidationPolicy::default())?;
        if evidence.tree_sha256 != source.tree_sha256 {
            let _ = fs::remove_dir_all(operation);
            return Err(CoreError::LifecycleEvidenceChanged(
                "trusted skill fixture digest does not match catalog provenance".to_string(),
            ));
        }
        Ok(evidence)
    }
}

#[cfg(test)]
pub(super) fn fixture_skill_source_resolver() -> Arc<dyn SkillSourceResolverPort> {
    Arc::new(FixtureSkillSourceResolver)
}

#[cfg(test)]
fn copy_fixture_tree(source: &Path, destination: &Path) -> Result<(), CoreError> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let metadata = fs::symlink_metadata(&source_path)?;
        if metadata.file_type().is_symlink() {
            return Err(CoreError::PathEscape(source_path));
        }
        let destination_path = destination.join(entry.file_name());
        if metadata.is_dir() {
            fs::create_dir(&destination_path)?;
            copy_fixture_tree(&source_path, &destination_path)?;
        } else if metadata.is_file() {
            fs::copy(source_path, destination_path)?;
        } else {
            return Err(CoreError::InvalidPath(
                "trusted skill fixture contains an unsupported entry".to_string(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
fn create_private_directory(path: &Path) -> Result<(), CoreError> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let operation = path.parent().ok_or_else(|| {
            CoreError::InvalidPath("private staging tree has no operation parent".to_string())
        })?;
        let root = operation.parent().ok_or_else(|| {
            CoreError::InvalidPath("private staging operation has no root".to_string())
        })?;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
        fs::set_permissions(operation, fs::Permissions::from_mode(0o700))?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
