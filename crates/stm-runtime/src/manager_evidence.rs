use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value as JsonValue;
use stm_core::{
    catalog::ToolCatalogMapping,
    feasibility::process_supervisor::{
        AllowedCommand, AllowlistedProcessSupervisor, ArgRule, CancelSignal, ExecutionRequest,
        ExecutionStatus,
    },
    lifecycle::{
        command_environment, npm_source_args, validate_package_id, validate_target_version,
        ManagerEvidencePort, ManagerStateEvidence,
    },
    ports::HostExecutableResolver,
    CoreError,
};

use crate::host::RealHostExecutableResolver;

#[derive(Debug)]
pub struct RealManagerEvidence {
    host: Arc<RealHostExecutableResolver>,
}

impl RealManagerEvidence {
    pub fn new(host: Arc<RealHostExecutableResolver>) -> Self {
        Self { host }
    }
}

impl ManagerEvidencePort for RealManagerEvidence {
    fn inspect(
        &self,
        mapping: &ToolCatalogMapping,
        executable: &str,
    ) -> Result<ManagerStateEvidence, CoreError> {
        match mapping.adapter.as_str() {
            "homebrew_formula" | "homebrew_cask" => inspect_homebrew(mapping, executable),
            "winget_package" => inspect_winget(mapping, executable),
            "npm_package" => inspect_npm(&self.host, mapping, executable),
            "bun_package" => inspect_bun(&self.host, mapping, executable),
            "apt_package" | "dnf_package" | "pacman_package" => {
                crate::linux::inspect_linux_manager(&self.host, mapping)
            }
            adapter => Err(CoreError::CommandDenied(format!(
                "no live lifecycle evidence adapter for {adapter}"
            ))),
        }
    }
}

fn inspect_homebrew(
    mapping: &ToolCatalogMapping,
    executable: &str,
) -> Result<ManagerStateEvidence, CoreError> {
    let mut args = vec!["info".to_string(), "--json=v2".to_string()];
    if mapping.adapter == "homebrew_cask" {
        args.push("--cask".to_string());
    }
    args.push(mapping.package_id.clone());
    let output = read_only_command(executable, args, &[0])?;
    let document: JsonValue = serde_json::from_str(&output)?;
    let entry = if mapping.adapter == "homebrew_cask" {
        document.get("casks")
    } else {
        document.get("formulae")
    }
    .and_then(JsonValue::as_array)
    .and_then(|items| items.first())
    .ok_or_else(|| {
        CoreError::MalformedInput("Homebrew returned no matching package metadata".to_string())
    })?;

    let current_version = homebrew_installed_version(entry);
    let target_version = if mapping.adapter == "homebrew_cask" {
        entry
            .get("version")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| CoreError::MalformedInput("Homebrew cask version missing".to_string()))?
            .to_string()
    } else {
        entry
            .get("versions")
            .and_then(|versions| versions.get("stable"))
            .and_then(JsonValue::as_str)
            .ok_or_else(|| {
                CoreError::MalformedInput("Homebrew formula version missing".to_string())
            })?
            .to_string()
    };

    let mut outdated_args = vec!["outdated".to_string(), "--json=v2".to_string()];
    if mapping.adapter == "homebrew_cask" {
        outdated_args.push("--cask".to_string());
    }
    outdated_args.push(mapping.package_id.clone());
    let outdated = read_only_command(executable, outdated_args, &[0, 1])?;
    let outdated: JsonValue = serde_json::from_str(&outdated)?;
    let outdated_entry = homebrew_package_entry(&outdated, &mapping.package_id);
    let target_version = outdated_entry
        .and_then(|entry| entry.get("current_version"))
        .and_then(JsonValue::as_str)
        .map(str::to_string)
        .unwrap_or(target_version);

    Ok(ManagerStateEvidence {
        installed: current_version.is_some(),
        current_version,
        target_version,
        update_available: outdated_entry.is_some(),
        source: "Live Homebrew JSON metadata".to_string(),
    })
}

fn inspect_npm(
    host: &RealHostExecutableResolver,
    mapping: &ToolCatalogMapping,
    executable: &str,
) -> Result<ManagerStateEvidence, CoreError> {
    validate_package_id(&mapping.adapter, &mapping.package_id)?;
    let npm_executable = host
        .resolve_executable("npm")
        .ok_or_else(|| CoreError::CommandDenied("reviewed npm executable not found".to_string()))?;
    let invocation = host
        .resolve_npm_invocation(&npm_executable)
        .ok_or_else(|| {
            CoreError::CommandDenied("reviewed npm invocation is invalid".to_string())
        })?;
    if fs::canonicalize(Path::new(executable))? != invocation.executable {
        return Err(CoreError::LifecycleEvidenceChanged(
            "npm execution identity changed before evidence collection".to_string(),
        ));
    }
    let mut installed_args = invocation.prefix_args.clone();
    installed_args.extend([
        "list".to_string(),
        "--global".to_string(),
        "--depth=0".to_string(),
        "--json".to_string(),
        mapping.package_id.clone(),
    ]);
    installed_args.extend(npm_source_args());
    let installed_output = read_only_command(executable, installed_args, &[0, 1])?;
    let installed: JsonValue = serde_json::from_str(&installed_output)?;
    let current_version = installed
        .get("dependencies")
        .and_then(|dependencies| dependencies.get(&mapping.package_id))
        .and_then(|package| package.get("version"))
        .and_then(JsonValue::as_str)
        .map(str::to_string);

    let mut target_args = invocation.prefix_args;
    target_args.extend([
        "view".to_string(),
        mapping.package_id.clone(),
        "version".to_string(),
        "--json".to_string(),
    ]);
    target_args.extend(npm_source_args());
    let target_output = read_only_command(executable, target_args, &[0])?;
    let target: JsonValue = serde_json::from_str(&target_output)?;
    let target_version = target
        .as_str()
        .or_else(|| {
            target
                .as_array()
                .and_then(|versions| versions.last())
                .and_then(JsonValue::as_str)
        })
        .filter(|version| !version.is_empty())
        .ok_or_else(|| CoreError::MalformedInput("npm registry version missing".to_string()))?
        .to_string();

    Ok(ManagerStateEvidence {
        installed: current_version.is_some(),
        update_available: current_version.as_deref() != Some(target_version.as_str()),
        current_version,
        target_version,
        source: "Live npm global inventory and registry metadata from https://registry.npmjs.org/"
            .to_string(),
    })
}

fn inspect_bun(
    host: &RealHostExecutableResolver,
    mapping: &ToolCatalogMapping,
    executable: &str,
) -> Result<ManagerStateEvidence, CoreError> {
    validate_package_id(&mapping.adapter, &mapping.package_id)?;
    let bun_executable = host
        .resolve_executable("bun")
        .ok_or_else(|| CoreError::CommandDenied("reviewed Bun executable not found".to_string()))?;
    if fs::canonicalize(Path::new(executable))? != bun_executable {
        return Err(CoreError::LifecycleEvidenceChanged(
            "Bun execution identity changed before evidence collection".to_string(),
        ));
    }
    let current_version = bun_global_package_version(&bun_executable, &mapping.package_id)?;
    let target_output = read_only_command(
        executable,
        vec![
            "pm".to_string(),
            "view".to_string(),
            mapping.package_id.clone(),
            "version".to_string(),
            "--json".to_string(),
            "--registry".to_string(),
            "https://registry.npmjs.org".to_string(),
        ],
        &[0],
    )?;
    let target: JsonValue = serde_json::from_str(&target_output)?;
    let target_version = target
        .as_str()
        .or_else(|| target.get("version").and_then(JsonValue::as_str))
        .ok_or_else(|| CoreError::MalformedInput("Bun registry version missing".to_string()))?
        .to_string();
    validate_target_version(&mapping.adapter, &target_version)?;

    Ok(ManagerStateEvidence {
        installed: current_version.is_some(),
        update_available: current_version.as_deref() != Some(target_version.as_str()),
        current_version,
        target_version,
        source:
            "Live Bun global package manifest and registry metadata from https://registry.npmjs.org/"
                .to_string(),
    })
}

fn bun_global_package_version(
    bun_executable: &Path,
    package_id: &str,
) -> Result<Option<String>, CoreError> {
    let root = bun_executable
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| CoreError::InvalidPath(bun_executable.display().to_string()))?;
    let canonical_root = fs::canonicalize(root)?;
    let mut package_path = root.join("install/global/node_modules");
    for segment in package_id.split('/') {
        package_path.push(segment);
    }
    package_path.push("package.json");
    if !package_path.exists() {
        return Ok(None);
    }
    let canonical_manifest = fs::canonicalize(&package_path)?;
    if !canonical_manifest.starts_with(&canonical_root) {
        return Err(CoreError::PathEscape(canonical_manifest));
    }
    let metadata = fs::metadata(&canonical_manifest)?;
    if !metadata.is_file() || metadata.len() > 256 * 1024 {
        return Err(CoreError::MalformedInput(
            "Bun global package manifest is invalid or oversized".to_string(),
        ));
    }
    let document: JsonValue = serde_json::from_slice(&fs::read(canonical_manifest)?)?;
    if document.get("name").and_then(JsonValue::as_str) != Some(package_id) {
        return Err(CoreError::LifecycleEvidenceChanged(
            "Bun global package identity does not match the reviewed package".to_string(),
        ));
    }
    let version = document
        .get("version")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| CoreError::MalformedInput("Bun package version missing".to_string()))?;
    validate_target_version("bun_package", version)?;
    Ok(Some(version.to_string()))
}

fn inspect_winget(
    mapping: &ToolCatalogMapping,
    executable: &str,
) -> Result<ManagerStateEvidence, CoreError> {
    let export_directory = std::env::temp_dir().join(format!(
        "stm-winget-evidence-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir(&export_directory)?;
    let export_path = export_directory.join("packages.json");
    let export_path_argument = export_path
        .to_str()
        .ok_or_else(|| CoreError::CommandDenied("WinGet export path is not UTF-8".to_string()))?
        .to_string();
    let result = (|| {
        read_only_command(
            executable,
            vec![
                "export".to_string(),
                "--output".to_string(),
                export_path_argument,
                "--source".to_string(),
                "winget".to_string(),
                "--include-versions".to_string(),
                "--accept-source-agreements".to_string(),
                "--disable-interactivity".to_string(),
            ],
            &[0],
        )?;
        let metadata = fs::metadata(&export_path)?;
        if metadata.len() > 2 * 1024 * 1024 {
            return Err(CoreError::ProcessExecution(
                "WinGet export exceeded the 2 MiB evidence boundary".to_string(),
            ));
        }
        let installed: JsonValue = serde_json::from_slice(&fs::read(&export_path)?)?;
        let current_version = package_version(&installed, &mapping.package_id);
        let versions = read_only_command(
            executable,
            vec![
                "show".to_string(),
                "--id".to_string(),
                mapping.package_id.clone(),
                "--source".to_string(),
                "winget".to_string(),
                "--exact".to_string(),
                "--versions".to_string(),
                "--disable-interactivity".to_string(),
                "--accept-source-agreements".to_string(),
            ],
            &[0],
        )?;
        let target_version = first_winget_version(&versions).ok_or_else(|| {
            CoreError::MalformedInput("WinGet package version missing".to_string())
        })?;
        Ok(ManagerStateEvidence {
            installed: current_version.is_some(),
            update_available: current_version
                .as_deref()
                .is_some_and(|current| current != target_version),
            current_version,
            target_version,
            source: "Live WinGet export and version metadata from source winget".to_string(),
        })
    })();
    let _ = fs::remove_dir_all(export_directory);
    result
}

fn first_winget_version(output: &str) -> Option<String> {
    let mut after_separator = false;
    for line in output.lines() {
        let value = line.trim();
        if value.len() >= 3 && value.chars().all(|character| character == '-') {
            after_separator = true;
            continue;
        }
        if after_separator && !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn homebrew_package_entry<'a>(value: &'a JsonValue, package_id: &str) -> Option<&'a JsonValue> {
    ["formulae", "casks"].iter().find_map(|kind| {
        value
            .get(*kind)
            .and_then(JsonValue::as_array)
            .and_then(|items| {
                items.iter().find(|item| {
                    ["name", "token", "full_name"]
                        .iter()
                        .filter_map(|key| item.get(*key))
                        .filter_map(JsonValue::as_str)
                        .any(|value| value.eq_ignore_ascii_case(package_id))
                })
            })
    })
}

fn homebrew_installed_version(entry: &JsonValue) -> Option<String> {
    entry
        .get("installed")
        .and_then(|installed| {
            installed.as_str().map(str::to_string).or_else(|| {
                let value = installed.as_array()?.last()?;
                value
                    .as_str()
                    .or_else(|| value.get("version").and_then(JsonValue::as_str))
                    .map(str::to_string)
            })
        })
        .filter(|version| !version.is_empty())
}

fn package_version(value: &JsonValue, package_id: &str) -> Option<String> {
    match value {
        JsonValue::Object(object) => {
            let matches = ["PackageIdentifier", "Id", "id"]
                .iter()
                .filter_map(|key| object.get(*key))
                .filter_map(JsonValue::as_str)
                .any(|value| value.eq_ignore_ascii_case(package_id));
            if matches {
                for key in ["PackageVersion", "Version", "version"] {
                    if let Some(version) = object.get(key).and_then(JsonValue::as_str) {
                        return Some(version.to_string());
                    }
                }
            }
            object
                .values()
                .find_map(|value| package_version(value, package_id))
        }
        JsonValue::Array(items) => items
            .iter()
            .find_map(|value| package_version(value, package_id)),
        _ => None,
    }
}

pub(super) fn read_only_command(
    executable: &str,
    args: Vec<String>,
    accepted_exit_codes: &[i32],
) -> Result<String, CoreError> {
    let supervisor = AllowlistedProcessSupervisor::new([AllowedCommand {
        alias: "lifecycle-manager-evidence".to_string(),
        executable: PathBuf::from(executable),
        args: args.iter().cloned().map(ArgRule::Exact).collect(),
        environment: command_environment(executable),
    }]);
    let outcome = supervisor.execute(
        &ExecutionRequest {
            command_alias: "lifecycle-manager-evidence".to_string(),
            args,
            timeout_ms: 60_000,
            output_limit_bytes: 256 * 1024,
        },
        &CancelSignal::default(),
    )?;
    if outcome.status != ExecutionStatus::Completed
        || !outcome
            .exit_code
            .is_some_and(|code| accepted_exit_codes.contains(&code))
    {
        return Err(CoreError::ProcessExecution(
            "authoritative manager metadata query failed".to_string(),
        ));
    }
    Ok(outcome.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_homebrew_installed_version_from_formula_and_cask_metadata() {
        assert_eq!(
            homebrew_installed_version(&serde_json::json!({
                "installed": [{ "version": "1.2.3" }]
            }))
            .as_deref(),
            Some("1.2.3")
        );
        assert_eq!(
            homebrew_installed_version(&serde_json::json!({
                "installed": "latest"
            }))
            .as_deref(),
            Some("latest")
        );
        assert_eq!(
            homebrew_installed_version(&serde_json::json!({
                "installed": null
            })),
            None
        );
    }

    #[test]
    fn reads_winget_structured_export_and_locale_independent_version_rows() {
        let fixture = serde_json::json!({
            "Sources": [{ "Packages": [{
                "PackageIdentifier": "Git.Git",
                "Version": "2.51.0"
            }] }]
        });
        assert_eq!(
            package_version(&fixture, "git.git").as_deref(),
            Some("2.51.0")
        );
        assert_eq!(
            first_winget_version("Version\n-------\n2.52.0\n2.51.0\n").as_deref(),
            Some("2.52.0")
        );
    }
}
