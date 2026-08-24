use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::{
    application::{
        adapters::{compute_sha256, FixtureWorkspace},
        versioning::VersionCatalog,
    },
    domain::{
        inventory::InventoryState,
        skill::{
            GlobalSkillEntry, SkillClientName, SkillDiffKind, SkillDiffRecord, SkillRecord,
            SkillRootResolution, SkillScanReport, SkillTargetRecord, SkillTargetState,
        },
    },
    error::CoreError,
};

mod runtime_inventory;

const MAX_SKILL_FILES: usize = 32;
const MAX_SKILL_BYTES: usize = 128 * 1024;
static MATERIALIZED_HOME_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillRootDeclaration {
    pub client: SkillClientName,
    pub root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedSkillReceipt {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub revision: String,
    pub digest: String,
    pub purposes: Vec<String>,
    pub targets: Vec<ManagedSkillTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedSkillTarget {
    pub client: SkillClientName,
    pub relative_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SkillStateOverride {
    pub state: Option<InventoryState>,
    #[serde(default)]
    pub target_states: BTreeMap<String, SkillTargetState>,
    #[serde(default)]
    pub risk_flags: Vec<String>,
    #[serde(default)]
    pub diff: Vec<SkillDiffRecord>,
}

#[derive(Debug, Clone)]
pub struct SkillInventorySnapshot {
    pub skills: Vec<SkillRecord>,
    pub report: SkillScanReport,
}

#[derive(Debug, Clone)]
struct ScannedSkill {
    slug: String,
    manifest_path: PathBuf,
    digest: String,
    summary: String,
    contains_scripts: bool,
    rejected_reason: Option<String>,
    clients: Vec<(SkillClientName, PathBuf)>,
}

pub fn scan_skills(
    workspace: &FixtureWorkspace,
    versions: &VersionCatalog,
) -> Result<SkillInventorySnapshot, CoreError> {
    if workspace.has_skill_home_override() {
        return runtime_inventory::scan_runtime_skills(workspace, versions);
    }
    let declarations: Vec<SkillRootDeclaration> =
        workspace.read_json("tests/fixtures/roots/skill-roots.json")?;
    let receipts: Vec<ManagedSkillReceipt> =
        workspace.read_json("tests/fixtures/skills/receipts.json")?;
    let overrides = workspace
        .read_json_if_exists::<BTreeMap<String, SkillStateOverride>>(
            "tests/fixtures/skills/state-overrides.json",
        )?
        .unwrap_or_default();
    let project_root = fs::canonicalize(workspace.project_root())?;
    let materialized_home = materialize_fixture_home(workspace)?;
    let receipts_by_id: BTreeMap<_, _> = receipts
        .into_iter()
        .map(|receipt| (receipt.id.clone(), receipt))
        .collect();

    let mut roots = Vec::new();
    let mut warnings = Vec::new();
    let mut seen_roots = BTreeSet::new();
    let mut scanned = BTreeMap::<String, ScannedSkill>::new();

    for declaration in declarations {
        let declared_path =
            resolve_skill_path(&declaration.root, materialized_home.path(), workspace);
        let canonical_root = match fs::canonicalize(&declared_path) {
            Ok(path) => path,
            Err(_) => {
                roots.push(SkillRootResolution {
                    client: declaration.client,
                    declared_root: declared_path.display().to_string(),
                    canonical_root: None,
                    accepted: false,
                    reason: Some("root_missing".to_string()),
                });
                continue;
            }
        };

        if canonical_root.starts_with(&project_root) {
            roots.push(SkillRootResolution {
                client: declaration.client,
                declared_root: declared_path.display().to_string(),
                canonical_root: Some(canonical_root.display().to_string()),
                accepted: false,
                reason: Some("project_root_rejected".to_string()),
            });
            continue;
        }

        let accepted = seen_roots.insert(canonical_root.clone());
        roots.push(SkillRootResolution {
            client: declaration.client.clone(),
            declared_root: declared_path.display().to_string(),
            canonical_root: Some(canonical_root.display().to_string()),
            accepted,
            reason: if accepted {
                None
            } else {
                Some("duplicate_physical_root".to_string())
            },
        });
        if !accepted {
            continue;
        }

        for entry in fs::read_dir(&canonical_root)? {
            let entry = entry?;
            let path = entry.path();
            if !entry.file_type()?.is_dir() && !entry.file_type()?.is_symlink() {
                continue;
            }

            let canonical_skill = fs::canonicalize(&path)?;
            let slug = entry.file_name().to_string_lossy().into_owned();

            if !canonical_skill.starts_with(&canonical_root) {
                warnings.push(format!("rejected skill symlink escape: {slug}"));
                scanned.insert(
                    slug.clone(),
                    ScannedSkill {
                        slug,
                        manifest_path: path.join("SKILL.md"),
                        digest: String::new(),
                        summary: String::new(),
                        contains_scripts: false,
                        rejected_reason: Some("symlink_escape_rejected".to_string()),
                        clients: vec![(declaration.client.clone(), path)],
                    },
                );
                continue;
            }

            let manifest = canonical_skill.join("SKILL.md");
            if !manifest.exists() {
                warnings.push(format!("skill {slug} missing SKILL.md"));
                continue;
            }

            let scan = scan_skill_directory(&slug, &canonical_skill, &manifest)?;
            scanned
                .entry(slug.clone())
                .and_modify(|existing| {
                    existing
                        .clients
                        .push((declaration.client.clone(), canonical_skill.clone()))
                })
                .or_insert(ScannedSkill {
                    slug,
                    manifest_path: manifest,
                    digest: scan.0,
                    summary: scan.1,
                    contains_scripts: scan.2,
                    rejected_reason: scan.3,
                    clients: vec![(declaration.client.clone(), canonical_skill)],
                });
        }
    }

    let report = SkillScanReport {
        roots,
        skills: scanned
            .values()
            .flat_map(|skill| {
                skill
                    .clients
                    .iter()
                    .map(move |(client, root)| GlobalSkillEntry {
                        client: client.clone(),
                        slug: skill.slug.clone(),
                        root: root.display().to_string(),
                        manifest_path: skill.manifest_path.display().to_string(),
                        rejected_reason: skill.rejected_reason.clone(),
                    })
            })
            .collect(),
        warnings,
    };

    let mut records = Vec::new();

    for (id, receipt) in &receipts_by_id {
        let scan = scanned.get(id);
        let state_override = overrides.get(id);
        let available_revision = versions.skill_updates.get(id).and_then(|update| {
            update
                .update_available
                .then(|| update.target_revision.clone())
        });
        let digest_matches = match scan {
            Some(scan) if receipt.digest == "auto" => !scan.digest.is_empty(),
            Some(scan) => scan.digest == receipt.digest,
            None => false,
        };
        let derived_state = match scan {
            Some(scan) if scan.rejected_reason.is_some() => InventoryState::Invalid,
            Some(_) if !digest_matches => InventoryState::Modified,
            Some(_) if available_revision.is_some() => InventoryState::ManagedUpdateAvailable,
            Some(_) => InventoryState::ManagedCurrent,
            None => InventoryState::Missing,
        };
        let state = state_override
            .and_then(|value| value.state.clone())
            .unwrap_or(derived_state);

        let diff = state_override
            .filter(|value| !value.diff.is_empty())
            .map(|value| value.diff.clone())
            .unwrap_or_else(|| default_diff_for_state(&state));

        let risk_flags = state_override
            .filter(|value| !value.risk_flags.is_empty())
            .map(|value| value.risk_flags.clone())
            .unwrap_or_else(|| {
                if scan.map(|item| item.contains_scripts).unwrap_or(false) {
                    vec!["Contains scripts".to_string()]
                } else {
                    Vec::new()
                }
            });

        records.push(SkillRecord {
            id: receipt.id.clone(),
            name: receipt.name.clone(),
            description: receipt.description.clone(),
            source: receipt.source.clone(),
            revision: receipt.revision.clone(),
            available_revision,
            digest: scan
                .map(|item| item.digest.clone())
                .unwrap_or_else(|| receipt.digest.clone()),
            state,
            purposes: receipt.purposes.clone(),
            targets: receipt
                .targets
                .iter()
                .map(|target| SkillTargetRecord {
                    client: target.client.clone(),
                    path: resolve_skill_path(
                        &target.relative_path,
                        materialized_home.path(),
                        workspace,
                    )
                    .display()
                    .to_string(),
                    state: state_override
                        .and_then(|value| {
                            value.target_states.get(client_key(&target.client)).cloned()
                        })
                        .unwrap_or(match scan {
                            Some(_) => SkillTargetState::Current,
                            None => SkillTargetState::Missing,
                        }),
                })
                .collect(),
            risk_flags,
            diff,
        });
    }

    for (id, scan) in scanned {
        if receipts_by_id.contains_key(&id) || scan.rejected_reason.is_some() {
            continue;
        }
        records.push(SkillRecord {
            id: id.clone(),
            name: title_from_slug(&id),
            description: scan.summary.clone(),
            source: "external".to_string(),
            revision: "unmanaged".to_string(),
            available_revision: None,
            digest: scan.digest.clone(),
            state: InventoryState::External,
            purposes: vec!["External".to_string()],
            targets: scan
                .clients
                .into_iter()
                .map(|(client, path)| SkillTargetRecord {
                    client,
                    path: path.display().to_string(),
                    state: SkillTargetState::Current,
                })
                .collect(),
            risk_flags: if scan.contains_scripts {
                vec!["Contains scripts".to_string(), "No app receipt".to_string()]
            } else {
                vec!["No app receipt".to_string()]
            },
            diff: Vec::new(),
        });
    }

    Ok(SkillInventorySnapshot {
        skills: records,
        report,
    })
}

fn default_diff_for_state(state: &InventoryState) -> Vec<SkillDiffRecord> {
    match state {
        InventoryState::Modified => vec![SkillDiffRecord {
            file: "SKILL.md".to_string(),
            change: SkillDiffKind::Modified,
            summary: "Local manifest differs from the receipt digest.".to_string(),
        }],
        InventoryState::Missing => vec![SkillDiffRecord {
            file: "SKILL.md".to_string(),
            change: SkillDiffKind::Added,
            summary: "Receipt target is missing on disk.".to_string(),
        }],
        _ => Vec::new(),
    }
}

fn client_key(client: &SkillClientName) -> &'static str {
    match client {
        SkillClientName::Codex => "Codex",
        SkillClientName::ClaudeCode => "Claude Code",
        SkillClientName::AgentKit => "AgentKit",
    }
}

fn scan_skill_directory(
    slug: &str,
    root: &Path,
    manifest: &Path,
) -> Result<(String, String, bool, Option<String>), CoreError> {
    let manifest_text = fs::read_to_string(manifest)?;
    if manifest_text.trim().is_empty() {
        return Ok((
            String::new(),
            String::new(),
            false,
            Some("invalid_manifest".to_string()),
        ));
    }

    let mut files = Vec::new();
    collect_skill_files(root, root, &mut files)?;
    if files.len() > MAX_SKILL_FILES {
        return Ok((
            String::new(),
            String::new(),
            false,
            Some("tree_limit_exceeded".to_string()),
        ));
    }

    let mut total_bytes = 0_usize;
    let mut contains_scripts = false;
    let digest = compute_sha256(files.iter().map(|path| {
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string();
        if relative.starts_with("scripts/") {
            contains_scripts = true;
        }
        let contents = fs::read(path).unwrap_or_default();
        total_bytes += contents.len();
        let mut bytes = relative.into_bytes();
        bytes.push(0);
        bytes.extend_from_slice(&contents);
        bytes
    }));

    let summary = manifest_text
        .lines()
        .find(|line| line.starts_with('#'))
        .map(|line| line.trim_start_matches('#').trim().to_string())
        .unwrap_or_else(|| title_from_slug(slug));

    Ok((
        if total_bytes > MAX_SKILL_BYTES {
            String::new()
        } else {
            digest
        },
        summary,
        contains_scripts,
        if total_bytes > MAX_SKILL_BYTES {
            Some("size_limit_exceeded".to_string())
        } else {
            None
        },
    ))
}

fn collect_skill_files(
    root: &Path,
    cursor: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), CoreError> {
    for entry in fs::read_dir(cursor)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            let canonical = fs::canonicalize(&path)?;
            if !canonical.starts_with(root) {
                return Err(CoreError::PathEscape(canonical));
            }
        }
        if file_type.is_dir() {
            collect_skill_files(root, &path, files)?;
        } else if file_type.is_file() {
            files.push(path);
        }
    }
    files.sort();
    Ok(())
}

struct MaterializedFixtureHome {
    path: PathBuf,
}

impl MaterializedFixtureHome {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for MaterializedFixtureHome {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn materialize_fixture_home(
    workspace: &FixtureWorkspace,
) -> Result<MaterializedFixtureHome, CoreError> {
    let source_home = workspace.resolve("tests/fixtures/skills/home");
    let sequence = MATERIALIZED_HOME_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let target_home = std::env::temp_dir().join(format!(
        "stm-phase-three-skill-home-{}-{sequence}",
        std::process::id()
    ));
    if source_home.exists() {
        copy_tree(&source_home, &target_home)?;
    } else {
        fs::create_dir_all(&target_home)?;
    }
    Ok(MaterializedFixtureHome { path: target_home })
}

fn copy_tree(source: &Path, target: &Path) -> Result<(), CoreError> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_tree(&from, &to)?;
        } else if file_type.is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn resolve_skill_path(raw: &str, home: &Path, workspace: &FixtureWorkspace) -> PathBuf {
    if let Some(suffix) = raw.strip_prefix("$HOME/") {
        return home.join(suffix);
    }
    if let Some(suffix) = raw.strip_prefix("$PROJECT_ROOT/") {
        return workspace.project_root().join(suffix);
    }
    workspace.resolve(raw)
}

fn title_from_slug(slug: &str) -> String {
    slug.split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use tempfile::TempDir;

    use crate::application::versioning::load_version_catalog;

    use super::*;

    fn workspace() -> FixtureWorkspace {
        FixtureWorkspace::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
    }

    #[test]
    fn scans_managed_modified_conflict_missing_and_external_skills() {
        let versions = load_version_catalog(&workspace()).expect("versions");
        let snapshot = scan_skills(&workspace(), &versions).expect("skills");
        assert!(snapshot
            .skills
            .iter()
            .any(|skill| skill.id == "frontend-design"
                && skill.state == InventoryState::ManagedUpdateAvailable));
        assert!(snapshot
            .skills
            .iter()
            .any(|skill| skill.id == "release-pilot" && skill.state == InventoryState::Modified));
        assert!(snapshot
            .skills
            .iter()
            .any(|skill| skill.id == "browser-control" && skill.state == InventoryState::Conflict));
        assert!(snapshot.skills.iter().any(
            |skill| skill.id == "database-operations" && skill.state == InventoryState::Missing
        ));
        assert!(snapshot
            .skills
            .iter()
            .any(|skill| skill.id == "docx" && skill.state == InventoryState::External));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_project_roots_and_symlink_escape_children() {
        let temp = TempDir::new().expect("workspace tempdir");
        let external = TempDir::new().expect("global tempdir");
        let project_root = temp.path().join("project");
        let global_root = external.path().join("global");
        let outside = external.path().join("outside");
        fs::create_dir_all(project_root.join("skill-a")).expect("project");
        fs::create_dir_all(global_root.join("safe-skill")).expect("global");
        fs::create_dir_all(&outside).expect("outside");
        fs::write(global_root.join("safe-skill/SKILL.md"), "# Safe").expect("safe");
        fs::write(outside.join("SKILL.md"), "# Outside").expect("outside skill");
        std::os::unix::fs::symlink(&outside, global_root.join("escape-skill")).expect("symlink");

        let roots = vec![
            SkillRootDeclaration {
                client: SkillClientName::Codex,
                root: global_root.display().to_string(),
            },
            SkillRootDeclaration {
                client: SkillClientName::ClaudeCode,
                root: project_root.display().to_string(),
            },
        ];
        fs::create_dir_all(temp.path().join("tests/fixtures/roots")).expect("roots dir");
        fs::create_dir_all(temp.path().join("tests/fixtures/skills")).expect("skills dir");
        fs::write(
            temp.path().join("tests/fixtures/roots/skill-roots.json"),
            serde_json::to_string(&roots).expect("json"),
        )
        .expect("roots");
        fs::write(
            temp.path().join("tests/fixtures/skills/receipts.json"),
            "[]",
        )
        .expect("receipts");
        fs::create_dir_all(temp.path().join("tests/fixtures/tools")).expect("tools");
        fs::write(
            temp.path()
                .join("tests/fixtures/skills/update-metadata.json"),
            "[]",
        )
        .expect("updates");
        fs::create_dir_all(temp.path().join("tests/fixtures/catalog")).expect("catalog");
        fs::write(
            temp.path()
                .join("tests/fixtures/catalog/product-update.json"),
            "{\"currentVersion\":\"0.1.0\",\"targetVersion\":\"0.2.0\",\"available\":true}",
        )
        .expect("product");

        let workspace = FixtureWorkspace::new(temp.path());
        let versions = VersionCatalog::default();
        let snapshot = scan_skills(&workspace, &versions).expect("skills");
        assert!(snapshot
            .report
            .roots
            .iter()
            .any(|root| root.reason.as_deref() == Some("project_root_rejected")));
        assert!(snapshot
            .report
            .skills
            .iter()
            .any(|skill| skill.rejected_reason.as_deref() == Some("symlink_escape_rejected")));
    }
}
