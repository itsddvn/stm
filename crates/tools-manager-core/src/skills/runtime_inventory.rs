use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use crate::{
    adapters::FixtureWorkspace,
    application::versioning::VersionCatalog,
    domain::{
        inventory::InventoryState,
        skill::{
            GlobalSkillEntry, SkillClientName, SkillDiffKind, SkillDiffRecord, SkillRootResolution,
            SkillScanReport, SkillTargetRecord, SkillTargetState,
        },
    },
    error::CoreError,
    skill_catalog::{load_current_authenticated_catalog, TrustedSkillEntry},
    skill_lifecycle::{
        validate_staged_tree, ManagedSkillReceipt, SkillStagingEvidence, SkillTargetSpec,
        TreeValidationPolicy,
    },
    storage::SqliteSnapshotStore,
};

use super::{SkillInventorySnapshot, SkillRecord};

#[derive(Debug)]
struct DiskSkill {
    client: SkillClientName,
    slug: String,
    path: PathBuf,
    evidence: Option<SkillStagingEvidence>,
    rejected_reason: Option<String>,
}

pub(super) fn scan_runtime_skills(
    workspace: &FixtureWorkspace,
    _versions: &VersionCatalog,
) -> Result<SkillInventorySnapshot, CoreError> {
    let home = workspace.skill_home()?;
    let project_root = fs::canonicalize(workspace.project_root())?;
    let root_specs = runtime_roots(&home);
    let mut roots = Vec::new();
    let mut warnings = Vec::new();
    let mut disk = Vec::new();

    for (client, root) in root_specs {
        let canonical_root = match fs::canonicalize(&root) {
            Ok(path) if !path.starts_with(&project_root) => path,
            Ok(path) => {
                roots.push(SkillRootResolution {
                    client,
                    declared_root: root.display().to_string(),
                    canonical_root: Some(path.display().to_string()),
                    accepted: false,
                    reason: Some("project_root_rejected".to_string()),
                });
                continue;
            }
            Err(_) => {
                roots.push(SkillRootResolution {
                    client,
                    declared_root: root.display().to_string(),
                    canonical_root: None,
                    accepted: false,
                    reason: Some("root_missing".to_string()),
                });
                continue;
            }
        };
        roots.push(SkillRootResolution {
            client: client.clone(),
            declared_root: root.display().to_string(),
            canonical_root: Some(canonical_root.display().to_string()),
            accepted: true,
            reason: None,
        });
        for entry in fs::read_dir(&canonical_root)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if !file_type.is_dir() && !file_type.is_symlink() {
                continue;
            }
            let slug = entry.file_name().to_string_lossy().to_string();
            let canonical = match fs::canonicalize(&path) {
                Ok(candidate) if candidate.starts_with(&canonical_root) => candidate,
                _ => {
                    warnings.push(format!("rejected skill symlink escape: {slug}"));
                    disk.push(DiskSkill {
                        client: client.clone(),
                        slug,
                        path,
                        evidence: None,
                        rejected_reason: Some("symlink_escape_rejected".to_string()),
                    });
                    continue;
                }
            };
            match validate_staged_tree(&canonical, TreeValidationPolicy::default()) {
                Ok(evidence) => disk.push(DiskSkill {
                    client: client.clone(),
                    slug,
                    path: canonical,
                    evidence: Some(evidence),
                    rejected_reason: None,
                }),
                Err(error) => {
                    warnings.push(format!("skill {slug} failed bounded validation: {error}"));
                    disk.push(DiskSkill {
                        client: client.clone(),
                        slug,
                        path: canonical,
                        evidence: None,
                        rejected_reason: Some("content_validation_failed".to_string()),
                    });
                }
            }
        }
    }

    let report = SkillScanReport {
        roots,
        skills: disk
            .iter()
            .map(|skill| GlobalSkillEntry {
                client: skill.client.clone(),
                slug: skill.slug.clone(),
                root: skill.path.display().to_string(),
                manifest_path: skill.path.join("SKILL.md").display().to_string(),
                rejected_reason: skill.rejected_reason.clone(),
            })
            .collect(),
        warnings,
    };

    let (store, _) = SqliteSnapshotStore::open(workspace.db_path())?;
    let mut receipts = BTreeMap::<String, Vec<(String, ManagedSkillReceipt)>>::new();
    for (key, receipt) in store.load_managed_skill_receipts()? {
        receipts
            .entry(receipt.skill_id.clone())
            .or_default()
            .push((key, receipt));
    }
    let verified = load_current_authenticated_catalog(&workspace.db_path()).map_err(|_| {
        CoreError::LifecycleEvidenceChanged(
            "authenticated skill catalog is unavailable for inventory reconciliation".to_string(),
        )
    })?;
    let catalog = verified
        .catalog
        .skills
        .iter()
        .map(|entry| (entry.id.clone(), entry))
        .collect::<BTreeMap<_, _>>();
    let mut ids = catalog.keys().cloned().collect::<BTreeSet<_>>();
    ids.extend(receipts.keys().cloned());

    let mut records = Vec::new();
    for id in ids {
        let entry = catalog.get(&id).copied();
        let managed = receipts.get(&id).cloned().unwrap_or_default();
        records.push(reconcile_managed_skill(&home, &id, entry, &managed, &disk)?);
    }

    let managed_ids = records
        .iter()
        .map(|record| record.id.clone())
        .collect::<BTreeSet<_>>();
    let mut external = BTreeMap::<String, Vec<&DiskSkill>>::new();
    for skill in &disk {
        if !managed_ids.contains(&skill.slug) {
            external.entry(skill.slug.clone()).or_default().push(skill);
        }
    }
    for (slug, targets) in external {
        let evidence = targets.iter().find_map(|target| target.evidence.as_ref());
        let invalid = targets
            .iter()
            .any(|target| target.rejected_reason.is_some());
        records.push(SkillRecord {
            id: slug.clone(),
            name: title_from_slug(&slug),
            description: evidence
                .map(|value| value.manifest.description.clone())
                .unwrap_or_else(|| "Local skill could not be validated.".to_string()),
            source: "external".to_string(),
            revision: "unmanaged".to_string(),
            available_revision: None,
            digest: evidence
                .map(|value| value.tree_sha256.clone())
                .unwrap_or_default(),
            state: if invalid {
                InventoryState::Invalid
            } else {
                InventoryState::External
            },
            purposes: vec!["External".to_string()],
            targets: targets
                .iter()
                .map(|target| SkillTargetRecord {
                    client: target.client.clone(),
                    path: target.path.display().to_string(),
                    state: if target.rejected_reason.is_some() {
                        SkillTargetState::Failed
                    } else {
                        SkillTargetState::Current
                    },
                })
                .collect(),
            risk_flags: evidence
                .map(|value| risk_flags(value, true))
                .unwrap_or_else(|| {
                    vec![
                        "Content validation failed".to_string(),
                        "No app receipt".to_string(),
                    ]
                }),
            diff: Vec::new(),
        });
    }
    records.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(SkillInventorySnapshot {
        skills: records,
        report,
    })
}

fn reconcile_managed_skill(
    home: &Path,
    id: &str,
    entry: Option<&TrustedSkillEntry>,
    receipts: &[(String, ManagedSkillReceipt)],
    disk: &[DiskSkill],
) -> Result<SkillRecord, CoreError> {
    let targets = managed_targets(entry, receipts);
    let mut target_records = Vec::new();
    let mut diffs = Vec::new();
    let mut states = Vec::new();
    let mut first_evidence = None;
    for target in targets {
        let path = target_path(home, &target);
        let receipt = receipts
            .iter()
            .find(|(_, receipt)| receipt.target == target)
            .map(|(_, receipt)| receipt);
        let scanned = disk
            .iter()
            .find(|skill| skill.client == target.client && skill.path == path);
        let state = match (receipt, scanned) {
            (_, Some(skill)) if skill.rejected_reason.is_some() => SkillTargetState::Failed,
            (Some(receipt), Some(skill)) => match &skill.evidence {
                Some(evidence) if evidence.tree_sha256 == receipt.tree_sha256 => {
                    first_evidence.get_or_insert(evidence);
                    SkillTargetState::Current
                }
                Some(evidence) => {
                    first_evidence.get_or_insert(evidence);
                    diffs.extend(file_diff(
                        client_label(&target.client),
                        &receipt.file_manifest,
                        evidence,
                    ));
                    SkillTargetState::Modified
                }
                None => SkillTargetState::Failed,
            },
            (None, Some(skill)) if skill.evidence.is_some() => SkillTargetState::Modified,
            _ => SkillTargetState::Missing,
        };
        if state == SkillTargetState::Missing && receipt.is_some() {
            diffs.push(SkillDiffRecord {
                file: format!("{}/SKILL.md", client_label(&target.client)),
                change: SkillDiffKind::Removed,
                summary: "Managed target is missing on disk.".to_string(),
            });
        }
        states.push(state.clone());
        target_records.push(SkillTargetRecord {
            client: target.client,
            path: path.display().to_string(),
            state,
        });
    }

    let has_receipts = !receipts.is_empty();
    let has_current = states.contains(&SkillTargetState::Current);
    let has_missing = states.contains(&SkillTargetState::Missing);
    let has_modified = states.contains(&SkillTargetState::Modified);
    let has_failed = states.contains(&SkillTargetState::Failed);
    let desired_changed = entry.is_some_and(|entry| {
        receipts
            .iter()
            .any(|(_, receipt)| receipt.source.commit != entry.source.commit)
    });
    let state = if has_failed {
        InventoryState::Invalid
    } else if !has_receipts && (has_modified || has_current) {
        InventoryState::Conflict
    } else if has_modified {
        InventoryState::Modified
    } else if has_missing && has_current {
        InventoryState::Conflict
    } else if has_missing {
        InventoryState::Missing
    } else if desired_changed {
        InventoryState::ManagedUpdateAvailable
    } else {
        InventoryState::ManagedCurrent
    };
    let first_receipt = receipts.first().map(|(_, receipt)| receipt);
    let source = entry
        .map(|entry| format!("{}#{}", entry.source.repository, entry.source.commit))
        .or_else(|| {
            first_receipt
                .map(|receipt| format!("{}#{}", receipt.source.repository, receipt.source.commit))
        })
        .unwrap_or_else(|| "authenticated catalog".to_string());
    let revision_commit = first_receipt
        .map(|receipt| receipt.source.commit.as_str())
        .or_else(|| entry.map(|entry| entry.source.commit.as_str()))
        .unwrap_or("unknown");
    let available_revision = desired_changed.then(|| {
        let commit = &entry
            .expect("desired change requires catalog entry")
            .source
            .commit;
        format!("Git {}", &commit[..commit.len().min(12)])
    });
    let mut flags = entry
        .map(|entry| entry.risk_flags.clone())
        .unwrap_or_default();
    if let Some(evidence) = first_evidence {
        for flag in risk_flags(evidence, false) {
            if !flags.contains(&flag) {
                flags.push(flag);
            }
        }
    }
    Ok(SkillRecord {
        id: id.to_string(),
        name: entry
            .map(|entry| entry.name.clone())
            .unwrap_or_else(|| title_from_slug(id)),
        description: entry
            .map(|entry| entry.description.clone())
            .unwrap_or_else(|| "Receipt-backed managed skill".to_string()),
        source,
        revision: format!("Git {}", &revision_commit[..revision_commit.len().min(12)]),
        available_revision,
        digest: first_evidence
            .map(|evidence| evidence.tree_sha256.clone())
            .or_else(|| first_receipt.map(|receipt| receipt.tree_sha256.clone()))
            .unwrap_or_default(),
        state,
        purposes: entry
            .map(|entry| entry.purposes.clone())
            .unwrap_or_else(|| vec!["Managed skill".to_string()]),
        targets: target_records,
        risk_flags: flags,
        diff: diffs,
    })
}

fn managed_targets(
    entry: Option<&TrustedSkillEntry>,
    receipts: &[(String, ManagedSkillReceipt)],
) -> Vec<SkillTargetSpec> {
    let mut targets = entry
        .map(|entry| {
            entry
                .targets
                .iter()
                .map(|target| SkillTargetSpec {
                    client: target.client.clone(),
                    target_path: target.relative_path.clone(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for (_, receipt) in receipts {
        if !targets.contains(&receipt.target) {
            targets.push(receipt.target.clone());
        }
    }
    targets
}

fn target_path(home: &Path, target: &SkillTargetSpec) -> PathBuf {
    let root = match target.client {
        SkillClientName::Codex => home.join(".codex/skills"),
        SkillClientName::ClaudeCode => home.join(".claude/skills"),
        SkillClientName::AgentKit => home.join(".agents/skills"),
    };
    root.join(&target.target_path)
}

fn runtime_roots(home: &Path) -> Vec<(SkillClientName, PathBuf)> {
    vec![
        (SkillClientName::Codex, home.join(".codex/skills")),
        (SkillClientName::ClaudeCode, home.join(".claude/skills")),
        (SkillClientName::AgentKit, home.join(".agents/skills")),
    ]
}

fn file_diff(
    client: &str,
    previous: &[crate::skill_lifecycle::StagedFileEvidence],
    current: &SkillStagingEvidence,
) -> Vec<SkillDiffRecord> {
    let previous = previous
        .iter()
        .map(|file| (file.path.as_str(), file.sha256.as_str()))
        .collect::<BTreeMap<_, _>>();
    let current_files = current
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.sha256.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut diff = Vec::new();
    for (path, digest) in &current_files {
        let change = match previous.get(path) {
            None => Some(SkillDiffKind::Added),
            Some(previous_digest) if previous_digest != digest => Some(SkillDiffKind::Modified),
            _ => None,
        };
        if let Some(change) = change {
            diff.push(SkillDiffRecord {
                file: format!("{client}/{path}"),
                change,
                summary: "Local file differs from the managed receipt.".to_string(),
            });
        }
    }
    for path in previous.keys() {
        if !current_files.contains_key(path) {
            diff.push(SkillDiffRecord {
                file: format!("{client}/{path}"),
                change: SkillDiffKind::Removed,
                summary: "Managed file is missing from the target.".to_string(),
            });
        }
    }
    diff
}

fn risk_flags(evidence: &SkillStagingEvidence, external: bool) -> Vec<String> {
    let mut flags = Vec::new();
    if !evidence.risk.scripts.is_empty() {
        flags.push("Contains scripts".to_string());
    }
    if !evidence.risk.requirements.is_empty() {
        flags.push("Contains dependency declarations".to_string());
    }
    if external {
        flags.push("No app receipt".to_string());
    }
    flags
}

fn client_label(client: &SkillClientName) -> &'static str {
    match client {
        SkillClientName::Codex => "Codex",
        SkillClientName::ClaudeCode => "Claude Code",
        SkillClientName::AgentKit => "AgentKit",
    }
}

fn title_from_slug(slug: &str) -> String {
    slug.split('-')
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| format!("{}{}", first.to_ascii_uppercase(), chars.as_str()))
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}
