use std::{
    fs,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use stm_core::{
    skill_lifecycle::{
        SkillManifestEvidence, SkillRiskEvidence, SkillStagingEvidence, StagedFileEvidence,
        TreeValidationPolicy,
    },
    CoreError,
};

#[derive(Debug)]
struct Candidate {
    absolute: PathBuf,
    relative: String,
    size: u64,
}

pub fn compute_tree_digest(root: &Path, policy: TreeValidationPolicy) -> Result<String, CoreError> {
    Ok(validate_staged_tree(root, policy)?.tree_sha256)
}

pub fn validate_staged_tree(
    root: &Path,
    policy: TreeValidationPolicy,
) -> Result<SkillStagingEvidence, CoreError> {
    let root_metadata = fs::symlink_metadata(root)?;
    if !root_metadata.file_type().is_dir() || root_metadata.file_type().is_symlink() {
        return Err(CoreError::InvalidPath(
            "skill staging root must be a real directory".to_string(),
        ));
    }

    let mut candidates = Vec::new();
    collect_candidates(root, root, policy, &mut candidates)?;
    if candidates.is_empty() {
        return Err(CoreError::MalformedInput(
            "skill tree contains no files".to_string(),
        ));
    }
    candidates.sort_by(|left, right| left.relative.as_bytes().cmp(right.relative.as_bytes()));

    let mut tree_hasher = Sha256::new();
    let mut files = Vec::with_capacity(candidates.len());
    let mut scripts = Vec::new();
    let mut requirements = Vec::new();
    let mut manifest = None;
    let mut total_bytes = 0_u64;

    for candidate in candidates {
        let bytes = fs::read(&candidate.absolute)?;
        if bytes.len() as u64 != candidate.size {
            return Err(CoreError::LifecycleEvidenceChanged(
                "staged file size changed during validation".to_string(),
            ));
        }
        validate_text_content(&candidate.relative, &bytes)?;
        total_bytes = total_bytes
            .checked_add(candidate.size)
            .ok_or_else(|| CoreError::MalformedInput("skill tree size overflow".to_string()))?;
        if total_bytes > policy.max_total_bytes {
            return Err(CoreError::MalformedInput(
                "skill tree exceeds total size limit".to_string(),
            ));
        }

        let path_bytes = candidate.relative.as_bytes();
        let path_len = u32::try_from(path_bytes.len()).map_err(|_| {
            CoreError::MalformedInput("skill path exceeds digest framing limit".to_string())
        })?;
        tree_hasher.update(path_len.to_be_bytes());
        tree_hasher.update(path_bytes);
        tree_hasher.update(candidate.size.to_be_bytes());
        tree_hasher.update(&bytes);

        let mode = file_mode(&candidate.absolute)?;
        if is_script(&candidate.relative, &mode) {
            scripts.push(candidate.relative.clone());
        }
        if is_requirement_file(&candidate.relative) {
            requirements.push(candidate.relative.clone());
        }
        if candidate.relative == "SKILL.md" {
            manifest = Some(parse_manifest(&bytes)?);
        }
        files.push(StagedFileEvidence {
            path: candidate.relative,
            git_mode: mode,
            size_bytes: candidate.size,
            sha256: hex_digest(Sha256::digest(&bytes).as_slice()),
        });
    }

    let manifest = manifest.ok_or_else(|| {
        CoreError::MalformedInput("skill tree is missing root SKILL.md".to_string())
    })?;
    Ok(SkillStagingEvidence {
        private_staging_path: root.display().to_string(),
        tree_sha256: hex_digest(tree_hasher.finalize().as_slice()),
        file_count: files.len(),
        total_bytes,
        manifest,
        files,
        risk: SkillRiskEvidence {
            scripts,
            requirements,
        },
    })
}

fn collect_candidates(
    root: &Path,
    directory: &Path,
    policy: TreeValidationPolicy,
    candidates: &mut Vec<Candidate>,
) -> Result<(), CoreError> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        let relative_path = path
            .strip_prefix(root)
            .map_err(|_| CoreError::PathEscape(path.clone()))?;
        let relative = normalized_relative_path(relative_path)?;
        let depth = relative.split('/').count();
        if depth > policy.max_depth {
            return Err(CoreError::InvalidPath(
                "skill tree exceeds path depth limit".to_string(),
            ));
        }
        if metadata.file_type().is_symlink() {
            return Err(CoreError::InvalidPath(
                "skill tree contains a symlink".to_string(),
            ));
        }
        if metadata.file_type().is_dir() {
            collect_candidates(root, &path, policy, candidates)?;
            continue;
        }
        if !metadata.file_type().is_file() {
            return Err(CoreError::InvalidPath(
                "skill tree contains a special file".to_string(),
            ));
        }
        if metadata.len() > policy.max_file_bytes {
            return Err(CoreError::MalformedInput(
                "skill file exceeds size limit".to_string(),
            ));
        }
        if candidates.len() >= policy.max_files {
            return Err(CoreError::MalformedInput(
                "skill tree exceeds file count limit".to_string(),
            ));
        }
        candidates.push(Candidate {
            absolute: path,
            relative,
            size: metadata.len(),
        });
    }
    Ok(())
}

pub(super) fn normalized_relative_path(path: &Path) -> Result<String, CoreError> {
    if path.is_absolute() {
        return Err(CoreError::InvalidPath(
            "absolute skill path rejected".to_string(),
        ));
    }
    let value = path
        .to_str()
        .ok_or_else(|| CoreError::InvalidPath("skill path must be valid UTF-8".to_string()))?;
    if value.is_empty() || value.contains('\\') || value.contains('\0') {
        return Err(CoreError::InvalidPath(
            "skill path is not normalized POSIX UTF-8".to_string(),
        ));
    }
    let components: Vec<_> = value.split('/').collect();
    if components
        .iter()
        .any(|part| part.is_empty() || *part == "." || *part == "..")
    {
        return Err(CoreError::InvalidPath(
            "skill path contains traversal or empty components".to_string(),
        ));
    }
    Ok(components.join("/"))
}

fn validate_text_content(path: &str, bytes: &[u8]) -> Result<(), CoreError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| CoreError::MalformedInput(format!("unsupported binary content in {path}")))?;
    if text
        .bytes()
        .any(|byte| byte == 0 || (byte < 0x20 && !matches!(byte, b'\n' | b'\r' | b'\t')))
    {
        return Err(CoreError::MalformedInput(format!(
            "unsupported binary content in {path}"
        )));
    }
    Ok(())
}

fn parse_manifest(bytes: &[u8]) -> Result<SkillManifestEvidence, CoreError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| CoreError::MalformedInput("SKILL.md must be UTF-8".to_string()))?;
    let mut lines = text.lines();
    if lines.next() != Some("---") {
        return Err(CoreError::MalformedInput(
            "SKILL.md must begin with YAML frontmatter".to_string(),
        ));
    }
    let mut name = None;
    let mut description = None;
    let mut closed = false;
    for line in lines.by_ref() {
        if line == "---" {
            closed = true;
            break;
        }
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if line.starts_with(char::is_whitespace) || line.contains('\t') {
            return Err(CoreError::MalformedInput(
                "SKILL.md frontmatter uses unsupported YAML structure".to_string(),
            ));
        }
        let (key, raw_value) = line.split_once(':').ok_or_else(|| {
            CoreError::MalformedInput("SKILL.md frontmatter is malformed".to_string())
        })?;
        let value = yaml_scalar(raw_value.trim())?;
        match key.trim() {
            "name" if name.is_none() => name = Some(value),
            "description" if description.is_none() => description = Some(value),
            "name" | "description" => {
                return Err(CoreError::MalformedInput(
                    "SKILL.md frontmatter contains duplicate required fields".to_string(),
                ))
            }
            _ => {}
        }
    }
    if !closed {
        return Err(CoreError::MalformedInput(
            "SKILL.md frontmatter is not closed".to_string(),
        ));
    }
    let name = name.ok_or_else(|| {
        CoreError::MalformedInput("SKILL.md frontmatter is missing name".to_string())
    })?;
    let description = description.ok_or_else(|| {
        CoreError::MalformedInput("SKILL.md frontmatter is missing description".to_string())
    })?;
    if name.is_empty()
        || name.len() > 64
        || name.starts_with('-')
        || name.ends_with('-')
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(CoreError::MalformedInput(
            "SKILL.md name is not a valid skill name".to_string(),
        ));
    }
    if description.is_empty() || description.len() > 1024 {
        return Err(CoreError::MalformedInput(
            "SKILL.md description is empty or too long".to_string(),
        ));
    }
    Ok(SkillManifestEvidence { name, description })
}

fn yaml_scalar(value: &str) -> Result<String, CoreError> {
    if value.is_empty() || matches!(value, "|" | ">") {
        return Err(CoreError::MalformedInput(
            "SKILL.md required fields must use scalar values".to_string(),
        ));
    }
    if let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        return Ok(inner.replace("\\\"", "\"").replace("\\\\", "\\"));
    }
    if let Some(inner) = value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
        return Ok(inner.replace("''", "'"));
    }
    Ok(value.to_string())
}

fn is_script(path: &str, mode: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    mode == "100755"
        || [
            ".sh", ".bash", ".zsh", ".fish", ".py", ".rb", ".pl", ".js", ".mjs", ".cjs", ".ts",
            ".ps1", ".bat", ".cmd",
        ]
        .iter()
        .any(|suffix| lower.ends_with(suffix))
}

fn is_requirement_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    matches!(
        name.as_str(),
        "requirements.txt"
            | "package.json"
            | "package-lock.json"
            | "pnpm-lock.yaml"
            | "yarn.lock"
            | "pyproject.toml"
            | "poetry.lock"
            | "cargo.toml"
            | "cargo.lock"
            | "gemfile"
            | "gemfile.lock"
    )
}

#[cfg(unix)]
fn file_mode(path: &Path) -> Result<String, CoreError> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path)?.permissions().mode();
    Ok(if mode & 0o111 != 0 {
        "100755"
    } else {
        "100644"
    }
    .to_string())
}

#[cfg(not(unix))]
fn file_mode(_path: &Path) -> Result<String, CoreError> {
    Ok("100644".to_string())
}

pub(super) fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn valid_tree() -> TempDir {
        let temp = TempDir::new().expect("tempdir");
        fs::write(
            temp.path().join("SKILL.md"),
            "---\nname: safe-skill\ndescription: Safe fixture\n---\n# Safe\n",
        )
        .expect("manifest");
        fs::create_dir(temp.path().join("scripts")).expect("scripts dir");
        fs::write(
            temp.path().join("scripts/run.py"),
            "print('not executed')\n",
        )
        .expect("script");
        temp
    }

    #[test]
    fn deterministic_digest_and_risk_evidence() {
        let temp = valid_tree();
        let first =
            validate_staged_tree(temp.path(), TreeValidationPolicy::default()).expect("valid");
        let second =
            validate_staged_tree(temp.path(), TreeValidationPolicy::default()).expect("valid");
        assert_eq!(first.tree_sha256, second.tree_sha256);
        assert_eq!(first.risk.scripts, vec!["scripts/run.py"]);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_and_binary_content() {
        use std::os::unix::fs::symlink;
        let symlink_tree = valid_tree();
        symlink("/tmp", symlink_tree.path().join("escape")).expect("symlink");
        assert!(
            validate_staged_tree(symlink_tree.path(), TreeValidationPolicy::default()).is_err()
        );
        let binary_tree = valid_tree();
        fs::write(binary_tree.path().join("payload.bin"), [0_u8, 1, 2]).expect("binary");
        assert!(validate_staged_tree(binary_tree.path(), TreeValidationPolicy::default()).is_err());
    }

    #[test]
    fn rejects_oversize_and_malformed_manifest() {
        let oversize = valid_tree();
        fs::write(oversize.path().join("large.txt"), vec![b'x'; 33]).expect("large");
        let policy = TreeValidationPolicy {
            max_file_bytes: 32,
            ..TreeValidationPolicy::default()
        };
        assert!(validate_staged_tree(oversize.path(), policy).is_err());
        let malformed = valid_tree();
        fs::write(malformed.path().join("SKILL.md"), "# no frontmatter\n").expect("manifest");
        assert!(validate_staged_tree(malformed.path(), TreeValidationPolicy::default()).is_err());
    }
}
