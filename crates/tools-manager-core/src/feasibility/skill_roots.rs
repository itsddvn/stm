use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    domain::skill::{GlobalSkillEntry, SkillClientName, SkillRootResolution, SkillScanReport},
    error::CoreError,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillRootScanner {
    pub project_root: PathBuf,
}

impl SkillRootScanner {
    pub fn scan(
        &self,
        declared_roots: &[(SkillClientName, PathBuf)],
    ) -> Result<SkillScanReport, CoreError> {
        let project_root = fs::canonicalize(&self.project_root)?;
        let mut seen = BTreeSet::new();
        let mut roots = Vec::new();
        let mut skills = Vec::new();

        for (client, declared_root) in declared_roots {
            let canonical = match fs::canonicalize(declared_root) {
                Ok(path) => path,
                Err(_) => {
                    roots.push(SkillRootResolution {
                        client: client.clone(),
                        declared_root: declared_root.display().to_string(),
                        canonical_root: None,
                        accepted: false,
                        reason: Some("root_missing".to_string()),
                    });
                    continue;
                }
            };

            if canonical.starts_with(&project_root) {
                roots.push(SkillRootResolution {
                    client: client.clone(),
                    declared_root: declared_root.display().to_string(),
                    canonical_root: Some(canonical.display().to_string()),
                    accepted: false,
                    reason: Some("project_root_rejected".to_string()),
                });
                continue;
            }

            if !seen.insert(canonical.clone()) {
                roots.push(SkillRootResolution {
                    client: client.clone(),
                    declared_root: declared_root.display().to_string(),
                    canonical_root: Some(canonical.display().to_string()),
                    accepted: false,
                    reason: Some("duplicate_physical_root".to_string()),
                });
                continue;
            }

            roots.push(SkillRootResolution {
                client: client.clone(),
                declared_root: declared_root.display().to_string(),
                canonical_root: Some(canonical.display().to_string()),
                accepted: true,
                reason: None,
            });

            for entry in fs::read_dir(&canonical)? {
                let entry = entry?;
                let child_path = entry.path();
                let file_type = entry.file_type()?;
                if !file_type.is_dir() && !file_type.is_symlink() {
                    continue;
                }

                let canonical_child = fs::canonicalize(&child_path)?;
                if !canonical_child.starts_with(&canonical) {
                    skills.push(GlobalSkillEntry {
                        client: client.clone(),
                        slug: entry.file_name().to_string_lossy().into_owned(),
                        root: canonical.display().to_string(),
                        manifest_path: child_path.join("SKILL.md").display().to_string(),
                        rejected_reason: Some("symlink_escape_rejected".to_string()),
                    });
                    continue;
                }

                let manifest = canonical_child.join("SKILL.md");
                if manifest.is_file() {
                    skills.push(GlobalSkillEntry {
                        client: client.clone(),
                        slug: entry.file_name().to_string_lossy().into_owned(),
                        root: canonical.display().to_string(),
                        manifest_path: manifest.display().to_string(),
                        rejected_reason: None,
                    });
                }
            }
        }

        Ok(SkillScanReport {
            roots,
            skills,
            warnings: Vec::new(),
        })
    }
}

pub fn configured_root(client: &SkillClientName, home: &Path) -> PathBuf {
    match client {
        SkillClientName::Codex => home.join(".codex/skills"),
        SkillClientName::ClaudeCode => home.join(".claude/skills"),
        SkillClientName::AgentKit => home.join(".agents/skills"),
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use tempfile::TempDir;

    use super::*;

    #[cfg(unix)]
    #[test]
    fn scanner_rejects_project_local_and_symlink_escape_roots() {
        let temp = TempDir::new().expect("tempdir");
        let project_root = temp.path().join("project");
        let codex_root = temp.path().join("codex");
        let external_target = temp.path().join("outside-skill");

        fs::create_dir_all(project_root.join("nested-skill")).expect("project tree");
        fs::create_dir_all(codex_root.join("safe-skill")).expect("codex tree");
        fs::create_dir_all(&external_target).expect("outside tree");
        fs::write(codex_root.join("safe-skill/SKILL.md"), "# Safe").expect("safe skill");
        fs::write(external_target.join("SKILL.md"), "# Escape").expect("escape skill");
        std::os::unix::fs::symlink(&external_target, codex_root.join("escape-skill"))
            .expect("symlink");

        let scanner = SkillRootScanner {
            project_root: project_root.clone(),
        };

        let report = scanner
            .scan(&[
                (SkillClientName::Codex, codex_root.clone()),
                (SkillClientName::ClaudeCode, project_root.clone()),
            ])
            .expect("scan");

        assert!(report
            .roots
            .iter()
            .any(|root| root.reason.as_deref() == Some("project_root_rejected")));
        assert!(report
            .skills
            .iter()
            .any(|skill| skill.rejected_reason.as_deref() == Some("symlink_escape_rejected")));
        assert!(report
            .skills
            .iter()
            .any(|skill| skill.slug == "safe-skill" && skill.rejected_reason.is_none()));
    }

    #[test]
    fn configured_roots_match_expected_global_locations() {
        let home = PathBuf::from("/tmp/home");
        assert!(configured_root(&SkillClientName::Codex, &home).ends_with(".codex/skills"));
        assert!(configured_root(&SkillClientName::ClaudeCode, &home).ends_with(".claude/skills"));
        assert!(configured_root(&SkillClientName::AgentKit, &home).ends_with(".agents/skills"));
    }

    #[test]
    fn scanner_marks_missing_and_duplicate_physical_roots() {
        let temp = TempDir::new().expect("tempdir");
        let project_root = temp.path().join("project");
        let shared_root = temp.path().join("shared");

        fs::create_dir_all(project_root.join("local-skill")).expect("project tree");
        fs::create_dir_all(shared_root.join("skill-a")).expect("shared tree");
        fs::write(shared_root.join("skill-a/SKILL.md"), "# Shared").expect("shared skill");

        let scanner = SkillRootScanner { project_root };

        let report = scanner
            .scan(&[
                (SkillClientName::Codex, shared_root.clone()),
                (SkillClientName::ClaudeCode, shared_root),
                (SkillClientName::AgentKit, temp.path().join("missing")),
            ])
            .expect("scan");

        assert!(report
            .roots
            .iter()
            .any(|root| root.reason.as_deref() == Some("duplicate_physical_root")));
        assert!(report
            .roots
            .iter()
            .any(|root| root.reason.as_deref() == Some("root_missing")));
    }
}
