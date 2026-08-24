use std::{
    collections::BTreeMap,
    env,
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::{
    catalog::ToolCatalogMapping,
    domain::{inventory::PrivilegeRequirement, lifecycle::LifecyclePrivilege},
    error::CoreError,
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableIdentity {
    pub path: PathBuf,
    pub canonical_path: PathBuf,
    pub length: u64,
    pub modified_epoch_seconds: u64,
    pub owner_id: u32,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct CompiledManagerCommand {
    pub executable: PathBuf,
    pub argv: Vec<String>,
    pub identities: Vec<ExecutableIdentity>,
}

pub fn manager_command_vector(
    mapping: &ToolCatalogMapping,
    action: &str,
    target_version: Option<&str>,
) -> Result<Option<(&'static str, Vec<String>)>, CoreError> {
    validate_package_id(&mapping.adapter, &mapping.package_id)?;
    if let Some(version) = target_version {
        validate_target_version(&mapping.adapter, version)?;
    }
    let package = mapping.package_id.clone();
    let command = match mapping.adapter.as_str() {
        "homebrew_formula" | "homebrew_cask" => {
            let verb = match action {
                "install" => "install",
                "update" => "upgrade",
                "uninstall" => "uninstall",
                _ => return Ok(None),
            };
            let mut argv = vec![verb.to_string()];
            if mapping.adapter == "homebrew_cask" {
                argv.push("--cask".to_string());
            }
            argv.push(package);
            ("brew", argv)
        }
        "winget_package" => {
            let verb = match action {
                "install" => "install",
                "update" => "upgrade",
                "uninstall" => "uninstall",
                _ => return Ok(None),
            };
            let mut argv = vec![
                verb.to_string(),
                "--id".to_string(),
                package,
                "--exact".to_string(),
                "--source".to_string(),
                "winget".to_string(),
                "--disable-interactivity".to_string(),
            ];
            if action != "uninstall" {
                if let Some(version) = target_version {
                    argv.extend(["--version".to_string(), version.to_string()]);
                }
                argv.extend([
                    "--accept-package-agreements".to_string(),
                    "--accept-source-agreements".to_string(),
                ]);
            }
            ("winget", argv)
        }
        "npm_package" => {
            let mut argv = match action {
                "install" | "update" => {
                    let package = target_version
                        .map(|version| format!("{package}@{version}"))
                        .unwrap_or(package);
                    vec!["install".to_string(), "--global".to_string(), package]
                }
                "uninstall" => vec!["uninstall".to_string(), "--global".to_string(), package],
                _ => return Ok(None),
            };
            argv.extend(npm_source_args());
            ("npm", argv)
        }
        "bun_package" => {
            let argv = match action {
                "install" | "update" => {
                    let package = target_version
                        .map(|version| format!("{package}@{version}"))
                        .unwrap_or(package);
                    vec![
                        "add".to_string(),
                        "--global".to_string(),
                        "--exact".to_string(),
                        "--ignore-scripts".to_string(),
                        "--registry".to_string(),
                        "https://registry.npmjs.org".to_string(),
                        package,
                    ]
                }
                "uninstall" => vec!["remove".to_string(), "--global".to_string(), package],
                _ => return Ok(None),
            };
            ("bun", argv)
        }
        "apt_package" => {
            let pinned_package = target_version
                .map(|version| format!("{package}={version}"))
                .unwrap_or_else(|| package.clone());
            let argv = match action {
                "install" => vec!["install".to_string(), "--yes".to_string(), pinned_package],
                "update" => vec![
                    "install".to_string(),
                    "--only-upgrade".to_string(),
                    "--yes".to_string(),
                    pinned_package,
                ],
                "uninstall" => vec!["remove".to_string(), "--yes".to_string(), package],
                _ => return Ok(None),
            };
            ("apt-get", argv)
        }
        "dnf_package" => {
            let verb = match action {
                "install" => "install",
                "update" => "upgrade",
                "uninstall" => "remove",
                _ => return Ok(None),
            };
            let package = if action == "uninstall" {
                package
            } else {
                target_version
                    .map(|version| format!("{package}-{version}"))
                    .unwrap_or(package)
            };
            (
                "dnf",
                vec![verb, "--assumeyes", package.as_str()]
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
            )
        }
        "pacman_package" => {
            let argv = match action {
                "uninstall" => vec!["-Rns", "--noconfirm", package.as_str()],
                "install" | "update" => return Ok(None),
                _ => return Ok(None),
            };
            ("pacman", argv.into_iter().map(str::to_string).collect())
        }
        _ => return Ok(None),
    };
    Ok(Some(command))
}

pub fn lifecycle_privilege(mapping: &ToolCatalogMapping) -> LifecyclePrivilege {
    match mapping.privilege {
        PrivilegeRequirement::Required => LifecyclePrivilege::ElevationRequired,
        PrivilegeRequirement::Unknown => LifecyclePrivilege::UserConfirmation,
        PrivilegeRequirement::None => LifecyclePrivilege::None,
    }
}

pub fn npm_source_args() -> Vec<String> {
    let (empty_user_config, empty_global_config) = if cfg!(windows) {
        ("NUL", r"C:\Windows\System32\stm-empty-npmrc")
    } else {
        ("/dev/null", "/etc/stm-empty-npmrc")
    };
    vec![
        "--registry=https://registry.npmjs.org/".to_string(),
        format!("--userconfig={empty_user_config}"),
        format!("--globalconfig={empty_global_config}"),
    ]
}

pub fn validate_package_id(adapter: &str, value: &str) -> Result<(), CoreError> {
    let valid_segment = |segment: &str, allow_at: bool| {
        !segment.is_empty()
            && segment != "."
            && segment != ".."
            && !segment.starts_with('-')
            && !segment.starts_with('.')
            && segment.chars().all(|character| {
                character.is_ascii_alphanumeric()
                    || "-_.".contains(character)
                    || (allow_at && character == '@')
            })
    };
    let npm_compatible = matches!(adapter, "npm_package" | "bun_package");
    let valid = if npm_compatible {
        if let Some(scoped) = value.strip_prefix('@') {
            let mut parts = scoped.split('/');
            matches!((parts.next(), parts.next(), parts.next()),
                (Some(scope), Some(package), None)
                    if valid_segment(scope, false) && valid_segment(package, false))
        } else {
            !value.contains('/') && valid_segment(value, false)
        }
    } else if adapter.starts_with("homebrew_") {
        let parts: Vec<_> = value.split('/').collect();
        (1..=3).contains(&parts.len()) && parts.iter().all(|segment| valid_segment(segment, true))
    } else {
        !value.contains('/') && valid_segment(value, false)
    };
    if valid {
        Ok(())
    } else {
        Err(CoreError::ArgumentDenied(value.to_string()))
    }
}

pub fn validate_target_version(adapter: &str, value: &str) -> Result<(), CoreError> {
    let common = !value.is_empty()
        && value.len() <= 128
        && !value.starts_with('-')
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || ".+_~-:".contains(character)
                || (adapter.starts_with("homebrew_") && character == ',')
        });
    let valid = common
        && (!matches!(adapter, "npm_package" | "bun_package")
            || value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || ".+_-".contains(character)));
    if valid {
        Ok(())
    } else {
        Err(CoreError::ArgumentDenied(value.to_string()))
    }
}

pub fn command_environment(executable: &str) -> BTreeMap<String, String> {
    let path = Path::new(executable);
    let file_name = path.file_name().and_then(|name| name.to_str());
    if matches!(file_name, Some("brew" | "brew.sh")) {
        return [
            ("HOMEBREW_NO_AUTO_UPDATE", "1"),
            ("HOMEBREW_NO_INSTALL_CLEANUP", "1"),
            ("HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK", "1"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    }
    if matches!(file_name, Some("bun" | "bun.exe")) {
        let Some(bin) = path.parent() else {
            return BTreeMap::new();
        };
        let root = bin.parent().unwrap_or(bin);
        let mut environment = BTreeMap::new();
        if let Some(home) = env::var_os("HOME") {
            environment.insert("HOME".to_string(), home.to_string_lossy().to_string());
        }
        environment.insert("BUN_INSTALL".to_string(), root.display().to_string());
        let reviewed_path = if cfg!(target_os = "windows") {
            bin.display().to_string()
        } else {
            format!("{}:/usr/bin:/bin", bin.display())
        };
        environment.insert("PATH".to_string(), reviewed_path);
        return environment;
    }
    BTreeMap::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::inventory::{
        ExecutionMode, MappingStatus, OwnershipKind, PrivilegeRequirement,
    };

    fn mapping(adapter: &str, package_id: &str) -> ToolCatalogMapping {
        ToolCatalogMapping {
            platform: "test".to_string(),
            manager: adapter.to_string(),
            package_id: package_id.to_string(),
            adapter: adapter.to_string(),
            step: crate::domain::recipe::RecipeStepType::ManagerPackage,
            mapping_status: MappingStatus::Supported,
            execution_mode: ExecutionMode::ManagedExecute,
            ownership_kind: OwnershipKind::ManagerOwned,
            privilege: PrivilegeRequirement::None,
            update_authority: "manager".to_string(),
        }
    }

    #[test]
    fn preserves_reviewed_vectors_and_pinning() {
        let brew = manager_command_vector(&mapping("homebrew_cask", "orbstack"), "update", None)
            .unwrap()
            .unwrap();
        assert_eq!(
            brew,
            (
                "brew",
                ["upgrade", "--cask", "orbstack"]
                    .map(str::to_string)
                    .to_vec()
            )
        );
        let winget = manager_command_vector(
            &mapping("winget_package", "Git.Git"),
            "install",
            Some("2.52.0"),
        )
        .unwrap()
        .unwrap();
        assert!(winget
            .1
            .windows(2)
            .any(|args| args == ["--version", "2.52.0"]));
        let npm = manager_command_vector(
            &mapping("npm_package", "@openai/codex"),
            "update",
            Some("0.32.1"),
        )
        .unwrap()
        .unwrap();
        assert_eq!(&npm.1[..3], ["install", "--global", "@openai/codex@0.32.1"]);
        let bun = manager_command_vector(
            &mapping("bun_package", "@openai/codex"),
            "install",
            Some("0.32.1"),
        )
        .unwrap()
        .unwrap();
        assert!(bun
            .1
            .windows(2)
            .any(|args| args == ["--ignore-scripts", "--registry"]));
    }

    #[test]
    fn preserves_linux_manager_semantics() {
        let apt = manager_command_vector(&mapping("apt_package", "git"), "update", None)
            .unwrap()
            .unwrap();
        assert_eq!(
            apt.1,
            ["install", "--only-upgrade", "--yes", "git"].map(str::to_string)
        );
        let dnf = manager_command_vector(&mapping("dnf_package", "git"), "uninstall", None)
            .unwrap()
            .unwrap();
        assert_eq!(dnf.1, ["remove", "--assumeyes", "git"].map(str::to_string));
        assert!(
            manager_command_vector(&mapping("pacman_package", "git"), "update", None)
                .unwrap()
                .is_none()
        );
        let pacman = manager_command_vector(&mapping("pacman_package", "git"), "uninstall", None)
            .unwrap()
            .unwrap();
        assert_eq!(pacman.1, ["-Rns", "--noconfirm", "git"].map(str::to_string));
    }

    #[test]
    fn rejects_package_and_version_escapes() {
        for (adapter, package_id) in [
            ("homebrew_formula", "--formula"),
            ("homebrew_formula", "https://attacker.invalid/pkg"),
            ("apt_package", "../../tmp/package"),
            ("npm_package", "file:/tmp/package"),
            ("npm_package", "@scope/package/extra"),
        ] {
            assert!(matches!(
                manager_command_vector(&mapping(adapter, package_id), "install", None),
                Err(CoreError::ArgumentDenied(_))
            ));
        }
        assert!(validate_package_id("npm_package", "@openai/codex").is_ok());
        assert!(validate_package_id("homebrew_formula", "owner/tap/formula@2").is_ok());
        assert!(matches!(
            manager_command_vector(
                &mapping("npm_package", "@openai/codex"),
                "install",
                Some("file:/tmp/package")
            ),
            Err(CoreError::ArgumentDenied(_))
        ));
    }

    #[test]
    fn accepts_homebrew_cask_build_versions_only_for_homebrew() {
        assert!(validate_target_version("homebrew_cask", "2.2.3,20963").is_ok());
        assert!(validate_target_version("npm_package", "2.2.3,20963").is_err());
    }
}
