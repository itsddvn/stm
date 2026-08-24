use super::{
    digest::normalized_relative_path, validate_staged_tree, SkillSourceSpec, SkillStagingEvidence,
    TreeValidationPolicy,
};
use crate::{
    error::CoreError,
    feasibility::process_supervisor::{
        AllowedCommand, AllowlistedProcessSupervisor, ArgRule, CancelSignal, ExecutionRequest,
        ExecutionStatus, RawExecutionOutcome,
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use url::Url;

static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct ReviewedGitExecutable {
    canonical_path: PathBuf,
    length: u64,
    modified: Option<std::time::SystemTime>,
}
impl ReviewedGitExecutable {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, CoreError> {
        let path = path.into();
        if !path.is_absolute() {
            return Err(CoreError::CommandDenied(
                "Git executable must be an absolute reviewed path".into(),
            ));
        }
        let canonical_path = fs::canonicalize(path)?;
        let metadata = fs::metadata(&canonical_path)?;
        if !metadata.is_file() {
            return Err(CoreError::CommandDenied(
                "reviewed Git executable is not a file".into(),
            ));
        }
        Ok(Self {
            canonical_path,
            length: metadata.len(),
            modified: metadata.modified().ok(),
        })
    }
    pub fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }
    fn revalidate(&self) -> Result<(), CoreError> {
        let path = fs::canonicalize(&self.canonical_path)?;
        let metadata = fs::metadata(&path)?;
        if path != self.canonical_path
            || !metadata.is_file()
            || metadata.len() != self.length
            || metadata.modified().ok() != self.modified
        {
            return Err(CoreError::LifecycleEvidenceChanged(
                "reviewed Git executable identity changed".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitResolverLimits {
    pub tree: TreeValidationPolicy,
    pub process_timeout_ms: u64,
    pub max_tree_metadata_bytes: usize,
    pub max_path_bytes: usize,
}
impl Default for GitResolverLimits {
    fn default() -> Self {
        Self {
            tree: TreeValidationPolicy::default(),
            process_timeout_ms: 60_000,
            max_tree_metadata_bytes: 1024 * 1024,
            max_path_bytes: 512,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PublicGithubSkillResolver {
    git: ReviewedGitExecutable,
    staging_root: PathBuf,
    limits: GitResolverLimits,
}
impl PublicGithubSkillResolver {
    pub fn new(
        git: ReviewedGitExecutable,
        workspace_db_path: impl AsRef<Path>,
        limits: GitResolverLimits,
    ) -> Result<Self, CoreError> {
        let parent = workspace_db_path.as_ref().parent().ok_or_else(|| {
            CoreError::InvalidPath("workspace database must have a parent directory".into())
        })?;
        let staging_root = parent.join(".stm-skill-staging");
        private_dir(&staging_root)?;
        Ok(Self {
            git,
            staging_root,
            limits,
        })
    }
    pub fn resolve(
        &self,
        source: &SkillSourceSpec,
        cancel: &CancelSignal,
    ) -> Result<SkillStagingEvidence, CoreError> {
        let repository = validate_source(source, self.limits.max_path_bytes)?;
        self.git.revalidate()?;
        let operation_root = self.staging_root.join(format!(
            "resolve-{}-{}",
            std::process::id(),
            STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        private_dir(&operation_root)?;
        let result = self.resolve_inner(
            source,
            &repository,
            &operation_root.join("objects.git"),
            &operation_root.join("tree"),
            cancel,
        );
        if result.is_err() {
            let _ = fs::remove_dir_all(operation_root);
        }
        result
    }
    pub fn cleanup_staging(&self, evidence: &SkillStagingEvidence) -> Result<(), CoreError> {
        cleanup_private_staging_tree(&self.staging_root, evidence)
    }
    fn resolve_inner(
        &self,
        source: &SkillSourceSpec,
        repository: &str,
        repo_dir: &Path,
        tree_dir: &Path,
        cancel: &CancelSignal,
    ) -> Result<SkillStagingEvidence, CoreError> {
        self.git(
            vec![
                "-c".into(),
                "init.templateDir=".into(),
                "init".into(),
                "--bare".into(),
                repo_dir.display().to_string(),
            ],
            16 * 1024,
            cancel,
        )?;
        self.git(
            vec![
                "-C".into(),
                repo_dir.display().to_string(),
                "fetch".into(),
                "--no-tags".into(),
                "--no-recurse-submodules".into(),
                "--depth=1".into(),
                "--filter=blob:none".into(),
                repository.into(),
                source.commit.clone(),
            ],
            64 * 1024,
            cancel,
        )?;
        let resolved = self.git(
            vec![
                "-C".into(),
                repo_dir.display().to_string(),
                "rev-parse".into(),
                "--verify".into(),
                "FETCH_HEAD^{commit}".into(),
            ],
            256,
            cancel,
        )?;
        if std::str::from_utf8(&resolved.stdout)
            .map_err(|_| CoreError::MalformedInput("Git returned non-text commit identity".into()))?
            .trim()
            != source.commit
        {
            return Err(CoreError::LifecycleEvidenceChanged(
                "fetched commit does not match catalog provenance".into(),
            ));
        }
        let listing = self.git(
            vec![
                "-C".into(),
                repo_dir.display().to_string(),
                "ls-tree".into(),
                "-r".into(),
                "-z".into(),
                "-l".into(),
                "--full-tree".into(),
                source.commit.clone(),
                "--".into(),
                source.subpath.clone(),
            ],
            self.limits.max_tree_metadata_bytes,
            cancel,
        )?;
        let entries = parse_ls_tree(&listing.stdout, &source.subpath, self.limits)?;
        private_dir(tree_dir)?;
        for entry in entries {
            let blob = self.git(
                vec![
                    "-C".into(),
                    repo_dir.display().to_string(),
                    "cat-file".into(),
                    "blob".into(),
                    entry.object_id,
                ],
                self.limits.tree.max_file_bytes as usize + 1,
                cancel,
            )?;
            if blob.stdout.len() as u64 != entry.size {
                return Err(CoreError::LifecycleEvidenceChanged(
                    "Git blob length changed during staging".into(),
                ));
            }
            let destination = tree_dir.join(&entry.relative_path);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&destination, blob.stdout)?;
            set_mode(&destination, &entry.mode)?;
        }
        let _ = fs::remove_dir_all(repo_dir);
        let evidence = validate_staged_tree(tree_dir, self.limits.tree)?;
        if evidence.tree_sha256 != source.tree_sha256 {
            return Err(CoreError::LifecycleEvidenceChanged(
                "staged tree digest does not match catalog provenance".into(),
            ));
        }
        Ok(evidence)
    }
    fn git(
        &self,
        mut args: Vec<String>,
        limit: usize,
        cancel: &CancelSignal,
    ) -> Result<RawExecutionOutcome, CoreError> {
        self.git.revalidate()?;
        let mut all = vec![
            "-c".into(),
            "credential.helper=".into(),
            "-c".into(),
            "core.hooksPath=/dev/null".into(),
            "-c".into(),
            "protocol.file.allow=never".into(),
            "-c".into(),
            "http.followRedirects=false".into(),
            "-c".into(),
            "filter.lfs.smudge=".into(),
            "-c".into(),
            "filter.lfs.required=false".into(),
        ];
        all.append(&mut args);
        let environment = BTreeMap::from([
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_CONFIG_GLOBAL".into(), null_device().into()),
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ("GIT_ASKPASS".into(), null_device().into()),
            ("GIT_LFS_SKIP_SMUDGE".into(), "1".into()),
            ("GIT_OPTIONAL_LOCKS".into(), "0".into()),
            ("GIT_PROTOCOL_FROM_USER".into(), "0".into()),
        ]);
        let supervisor = AllowlistedProcessSupervisor::new([AllowedCommand {
            alias: "reviewed-skill-git".into(),
            executable: self.git.canonical_path.clone(),
            args: all.iter().cloned().map(ArgRule::Exact).collect(),
            environment,
        }]);
        let outcome = supervisor.execute_raw(
            &ExecutionRequest {
                command_alias: "reviewed-skill-git".into(),
                args: all,
                timeout_ms: self.limits.process_timeout_ms,
                output_limit_bytes: limit,
            },
            cancel,
        )?;
        match outcome.status {
            ExecutionStatus::Completed if outcome.exit_code == Some(0) => Ok(outcome),
            ExecutionStatus::TimedOut => Err(CoreError::ProcessExecution(
                "bounded Git operation timed out".into(),
            )),
            ExecutionStatus::Cancelled => Err(CoreError::ProcessExecution(
                "bounded Git operation was cancelled".into(),
            )),
            ExecutionStatus::OutputLimitExceeded => Err(CoreError::ProcessExecution(
                "bounded Git operation exceeded output limit".into(),
            )),
            ExecutionStatus::Completed => Err(CoreError::ProcessExecution(
                "bounded Git operation failed".into(),
            )),
        }
    }
}

pub fn cleanup_private_staging(
    workspace_db_path: &Path,
    evidence: &SkillStagingEvidence,
) -> Result<(), CoreError> {
    let parent = workspace_db_path.parent().ok_or_else(|| {
        CoreError::InvalidPath("workspace database must have a parent directory".into())
    })?;
    cleanup_private_staging_tree(&parent.join(".stm-skill-staging"), evidence)
}

pub fn cleanup_abandoned_private_staging(workspace_db_path: &Path) -> Result<(), CoreError> {
    let parent = workspace_db_path.parent().ok_or_else(|| {
        CoreError::InvalidPath("workspace database must have a parent directory".into())
    })?;
    let staging_root = parent.join(".stm-skill-staging");
    private_dir(&staging_root)?;
    for entry in fs::read_dir(&staging_root)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || metadata.is_file() {
            fs::remove_file(path)?;
        } else if metadata.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            return Err(CoreError::InvalidPath(
                "private skill staging contains an unsupported entry".into(),
            ));
        }
    }
    Ok(())
}

fn cleanup_private_staging_tree(
    staging_root: &Path,
    evidence: &SkillStagingEvidence,
) -> Result<(), CoreError> {
    let tree = PathBuf::from(&evidence.private_staging_path);
    let operation = tree
        .parent()
        .ok_or_else(|| CoreError::InvalidPath("invalid staging path".into()))?;
    let root = fs::canonicalize(staging_root)?;
    let operation = fs::canonicalize(operation)?;
    if operation.parent() != Some(root.as_path())
        || tree.file_name().and_then(|value| value.to_str()) != Some("tree")
    {
        return Err(CoreError::PathEscape(tree));
    }
    fs::remove_dir_all(operation)?;
    Ok(())
}

struct GitTreeEntry {
    mode: String,
    object_id: String,
    size: u64,
    relative_path: String,
}
fn parse_ls_tree(
    bytes: &[u8],
    subpath: &str,
    limits: GitResolverLimits,
) -> Result<Vec<GitTreeEntry>, CoreError> {
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    let prefix = format!("{subpath}/");
    for record in bytes.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let tab = record
            .iter()
            .position(|b| *b == b'\t')
            .ok_or_else(|| CoreError::MalformedInput("malformed Git tree record".into()))?;
        let meta = std::str::from_utf8(&record[..tab])
            .map_err(|_| CoreError::MalformedInput("non-UTF-8 Git metadata".into()))?;
        let path = std::str::from_utf8(&record[tab + 1..])
            .map_err(|_| CoreError::InvalidPath("non-UTF-8 Git path".into()))?;
        let fields = meta.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 4 {
            return Err(CoreError::MalformedInput(
                "malformed Git tree metadata".into(),
            ));
        }
        let (mode, kind, object_id) = (fields[0], fields[1], fields[2]);
        if mode == "120000" {
            return Err(CoreError::InvalidPath("Git tree contains a symlink".into()));
        }
        if mode == "160000" || kind == "commit" {
            return Err(CoreError::InvalidPath(
                "Git tree contains a submodule".into(),
            ));
        }
        if kind != "blob" || !matches!(mode, "100644" | "100755") {
            return Err(CoreError::InvalidPath("unsupported Git object".into()));
        }
        if object_id.len() != 40 || !object_id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(CoreError::MalformedInput(
                "malformed Git object identity".into(),
            ));
        }
        let size: u64 = fields[3]
            .parse()
            .map_err(|_| CoreError::MalformedInput("malformed Git blob size".into()))?;
        let relative = path
            .strip_prefix(&prefix)
            .ok_or_else(|| CoreError::PathEscape(path.into()))?;
        if relative.len() > limits.max_path_bytes {
            return Err(CoreError::InvalidPath(
                "Git path exceeds length limit".into(),
            ));
        }
        let relative = normalized_relative_path(Path::new(relative))?;
        if !seen.insert(relative.clone()) {
            return Err(CoreError::MalformedInput("duplicate Git path".into()));
        }
        if size > limits.tree.max_file_bytes {
            return Err(CoreError::MalformedInput("oversized Git blob".into()));
        }
        if entries.len() >= limits.tree.max_files {
            return Err(CoreError::MalformedInput(
                "Git tree exceeds file limit".into(),
            ));
        }
        entries.push(GitTreeEntry {
            mode: mode.into(),
            object_id: object_id.to_ascii_lowercase(),
            size,
            relative_path: relative,
        });
    }
    if entries.is_empty() {
        return Err(CoreError::MalformedInput("Git subpath has no files".into()));
    }
    entries.sort_by(|a, b| a.relative_path.as_bytes().cmp(b.relative_path.as_bytes()));
    let total = entries
        .iter()
        .try_fold(0_u64, |n, e| n.checked_add(e.size))
        .ok_or_else(|| CoreError::MalformedInput("Git tree size overflow".into()))?;
    if total > limits.tree.max_total_bytes {
        return Err(CoreError::MalformedInput(
            "Git tree exceeds total size limit".into(),
        ));
    }
    Ok(entries)
}

fn validate_source(source: &SkillSourceSpec, max_path: usize) -> Result<String, CoreError> {
    if source.repository.contains('%') || source.repository.contains('\\') {
        return Err(CoreError::MalformedInput(
            "non-canonical repository URL".into(),
        ));
    }
    let url = Url::parse(&source.repository)?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(CoreError::MalformedInput(
            "only credential-free github.com HTTPS repositories are supported".into(),
        ));
    }
    let parts: Vec<_> = url.path_segments().into_iter().flatten().collect();
    if parts.len() != 2 || !parts[1].ends_with(".git") {
        return Err(CoreError::MalformedInput(
            "repository must be owner/repository.git".into(),
        ));
    }
    let owner = parts[0];
    let repo = parts[1].strip_suffix(".git").unwrap_or_default();
    let valid_owner = !owner.is_empty()
        && owner.len() <= 39
        && !owner.starts_with('-')
        && !owner.ends_with('-')
        && owner
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-');
    let valid_repo = !repo.is_empty()
        && repo.len() <= 100
        && repo != "."
        && repo != ".."
        && repo
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    let canonical = format!("https://github.com/{owner}/{repo}.git");
    if !valid_owner || !valid_repo || canonical != source.repository {
        return Err(CoreError::MalformedInput(
            "repository identity is not canonical".into(),
        ));
    }
    if source.commit.len() != 40
        || !source
            .commit
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
    {
        return Err(CoreError::MalformedInput(
            "commit must be full lowercase 40-hex".into(),
        ));
    }
    if source.tree_sha256.len() != 64
        || !source
            .tree_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
    {
        return Err(CoreError::MalformedInput(
            "tree digest must be lowercase SHA-256".into(),
        ));
    }
    if source.subpath.len() > max_path {
        return Err(CoreError::InvalidPath("subpath exceeds limit".into()));
    }
    if normalized_relative_path(Path::new(&source.subpath))? != source.subpath
        || source.subpath.split('/').any(|p| p == ".git")
    {
        return Err(CoreError::InvalidPath("subpath is not canonical".into()));
    }
    Ok(canonical)
}

fn private_dir(path: &Path) -> Result<(), CoreError> {
    if path.exists() {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(CoreError::PathEscape(path.to_path_buf()));
        }
    } else {
        fs::create_dir_all(path)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
#[cfg(unix)]
fn set_mode(path: &Path, mode: &str) -> Result<(), CoreError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(
        path,
        fs::Permissions::from_mode(if mode == "100755" { 0o700 } else { 0o600 }),
    )?;
    Ok(())
}
#[cfg(not(unix))]
fn set_mode(_: &Path, _: &str) -> Result<(), CoreError> {
    Ok(())
}
#[cfg(windows)]
fn null_device() -> &'static str {
    "NUL"
}
#[cfg(not(windows))]
fn null_device() -> &'static str {
    "/dev/null"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill_catalog::AuthenticatedSkillCatalog;
    use tempfile::TempDir;

    fn source(repository: &str, subpath: &str) -> SkillSourceSpec {
        SkillSourceSpec {
            repository: repository.into(),
            subpath: subpath.into(),
            commit: "0123456789abcdef0123456789abcdef01234567".into(),
            tree_sha256: "a".repeat(64),
        }
    }

    #[test]
    fn malicious_sources_are_rejected() {
        for url in [
            "http://github.com/o/r.git",
            "https://u:p@github.com/o/r.git",
            "https://github.com/o/r",
            "https://github.com.evil/o/r.git",
        ] {
            assert!(validate_source(&source(url, "skills/safe"), 512).is_err());
        }
        assert!(
            validate_source(&source("https://github.com/o/r.git", "skills/../safe"), 512).is_err()
        );
    }

    #[test]
    fn unsafe_tree_entries_are_rejected() {
        let limits = GitResolverLimits::default();
        let oid = "a".repeat(40);
        assert!(parse_ls_tree(
            format!("120000 blob {oid} 4\tskills/safe/link\0").as_bytes(),
            "skills/safe",
            limits
        )
        .is_err());
        assert!(parse_ls_tree(
            format!("160000 commit {oid} 0\tskills/safe/sub\0").as_bytes(),
            "skills/safe",
            limits
        )
        .is_err());
        assert!(parse_ls_tree(
            format!(
                "100644 blob {oid} {}\tskills/safe/x\0",
                limits.tree.max_file_bytes + 1
            )
            .as_bytes(),
            "skills/safe",
            limits
        )
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "requires public GitHub network access"]
    fn published_catalog_source_resolves_to_pinned_tree() {
        let catalog: AuthenticatedSkillCatalog = serde_json::from_slice(include_bytes!(
            "../../../../catalog/skills/stable/catalog.json"
        ))
        .expect("bundled catalog");
        let entry = catalog.skills.first().expect("catalog skill");
        let source = SkillSourceSpec {
            repository: entry.source.repository.clone(),
            subpath: entry.source.subpath.clone(),
            commit: entry.source.commit.clone(),
            tree_sha256: entry.source.tree_sha256.clone(),
        };
        let temporary = TempDir::new().expect("temporary runtime directory");
        let git_path = std::env::var_os("STM_TEST_GIT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/usr/bin/git"));
        let resolver = PublicGithubSkillResolver::new(
            ReviewedGitExecutable::new(git_path).expect("reviewed Git executable"),
            temporary.path().join("stm.sqlite"),
            GitResolverLimits::default(),
        )
        .expect("resolver");

        let evidence = resolver
            .resolve(&source, &CancelSignal::default())
            .expect("published source resolves");
        assert_eq!(evidence.tree_sha256, source.tree_sha256);
        assert_eq!(evidence.manifest.name, entry.id);
        let staging_path = PathBuf::from(&evidence.private_staging_path);
        resolver
            .cleanup_staging(&evidence)
            .expect("staging cleanup");
        assert!(!staging_path.exists());
    }
}
