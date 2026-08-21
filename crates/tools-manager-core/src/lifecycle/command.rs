use std::{
    collections::BTreeMap,
    env, fs,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

#[cfg(target_os = "windows")]
use std::{ffi::OsString, os::windows::ffi::OsStringExt};

#[cfg(target_os = "windows")]
use windows_sys::Win32::{
    Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS},
    Storage::Packaging::Appx::{GetPackagePathByFullName, GetPackagesByPackageFamily},
};

use serde::Serialize;
use sha2::{Digest, Sha256};

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

pub(super) struct NpmInvocation {
    pub executable: PathBuf,
    pub prefix_args: Vec<String>,
    pub identity_paths: Vec<PathBuf>,
}

pub fn compile_manager_command(
    mapping: &ToolCatalogMapping,
    action: &str,
    target_version: Option<&str>,
) -> Result<Option<CompiledManagerCommand>, CoreError> {
    let Some((binary, mut manager_argv)) =
        command_vector_with_target(mapping, action, target_version)?
    else {
        return Ok(None);
    };
    let Some(manager_executable) = resolve_executable(binary) else {
        return Ok(None);
    };
    if mapping.adapter == "npm_package" {
        let Some(invocation) = resolve_npm_invocation(&manager_executable) else {
            return Ok(None);
        };
        let mut argv = invocation.prefix_args;
        argv.append(&mut manager_argv);
        let identities = invocation
            .identity_paths
            .into_iter()
            .map(executable_identity)
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(Some(CompiledManagerCommand {
            executable: invocation.executable,
            argv,
            identities,
        }));
    }
    let manager_identity = executable_identity(manager_executable)?;

    if mapping.privilege == PrivilegeRequirement::Required {
        if !mapping.platform.starts_with("linux_") {
            return Ok(None);
        }
        let running_as_root = process_is_root();
        let broker_identity = if running_as_root {
            None
        } else {
            resolve_executable("pkexec")
                .map(executable_identity)
                .transpose()?
        };
        return Ok(apply_linux_privilege(
            manager_identity,
            manager_argv,
            broker_identity,
            running_as_root,
        ));
    }

    Ok(Some(CompiledManagerCommand {
        executable: manager_identity.canonical_path.clone(),
        argv: manager_argv,
        identities: vec![manager_identity],
    }))
}

fn apply_linux_privilege(
    manager_identity: ExecutableIdentity,
    manager_argv: Vec<String>,
    broker_identity: Option<ExecutableIdentity>,
    running_as_root: bool,
) -> Option<CompiledManagerCommand> {
    if running_as_root {
        return Some(CompiledManagerCommand {
            executable: manager_identity.canonical_path.clone(),
            argv: manager_argv,
            identities: vec![manager_identity],
        });
    }
    let broker_identity = broker_identity?;
    let mut argv = vec![manager_identity.canonical_path.display().to_string()];
    argv.extend(manager_argv);
    Some(CompiledManagerCommand {
        executable: broker_identity.canonical_path.clone(),
        argv,
        identities: vec![broker_identity, manager_identity],
    })
}

#[cfg(target_os = "linux")]
fn process_is_root() -> bool {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find_map(|line| line.strip_prefix("Uid:"))
                .and_then(|uids| uids.split_whitespace().nth(1))
                .and_then(|effective| effective.parse::<u32>().ok())
        })
        == Some(0)
}

#[cfg(not(target_os = "linux"))]
fn process_is_root() -> bool {
    false
}

pub fn manager_evidence_executable(
    mapping: &ToolCatalogMapping,
    action: &str,
) -> Result<Option<PathBuf>, CoreError> {
    let Some((binary, _)) = command_vector_with_target(mapping, action, None)? else {
        return Ok(None);
    };
    let Some(manager_executable) = resolve_executable(binary) else {
        return Ok(None);
    };
    let executable = if mapping.adapter == "npm_package" {
        let Some(invocation) = resolve_npm_invocation(&manager_executable) else {
            return Ok(None);
        };
        invocation.executable
    } else {
        manager_executable
    };
    Ok(Some(fs::canonicalize(executable)?))
}
#[cfg(test)]
fn command_vector(
    mapping: &ToolCatalogMapping,
    action: &str,
) -> Result<Option<(&'static str, Vec<String>)>, CoreError> {
    command_vector_with_target(mapping, action, None)
}

fn command_vector_with_target(
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

pub(super) fn npm_source_args() -> Vec<String> {
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

pub fn executable_identity(path: PathBuf) -> Result<ExecutableIdentity, CoreError> {
    let canonical_path = fs::canonicalize(&path)?;
    let metadata = fs::metadata(&canonical_path)?;
    if !metadata.is_file() {
        return Err(CoreError::CommandDenied(
            canonical_path.display().to_string(),
        ));
    }
    let modified_epoch_seconds = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let sha256 = file_sha256(&canonical_path)?;
    Ok(ExecutableIdentity {
        path,
        canonical_path,
        length: metadata.len(),
        modified_epoch_seconds,
        owner_id: owner_id(&metadata),
        sha256,
    })
}

fn file_sha256(path: &Path) -> Result<String, CoreError> {
    let mut reader = BufReader::new(fs::File::open(path)?);
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(crate) fn resolve_executable(name: &str) -> Option<PathBuf> {
    standard_candidates(name)
        .into_iter()
        .find(|path| path.is_file())
}

pub(crate) fn resolve_node_for_npm(npm_executable: &Path) -> Option<PathBuf> {
    npm_executable
        .parent()
        .map(|parent| parent.join(if cfg!(windows) { "node.exe" } else { "node" }))
        .filter(|path| path.is_file())
        .or_else(|| resolve_executable("node"))
}

pub(super) fn resolve_npm_invocation(npm_executable: &Path) -> Option<NpmInvocation> {
    let npm_executable = fs::canonicalize(npm_executable).ok()?;
    if npm_executable.extension().and_then(|value| value.to_str()) == Some("js") {
        let node = fs::canonicalize(resolve_node_for_npm(&npm_executable)?).ok()?;
        return Some(NpmInvocation {
            executable: node.clone(),
            prefix_args: vec![npm_executable.display().to_string()],
            identity_paths: vec![node, npm_executable],
        });
    }
    Some(NpmInvocation {
        executable: npm_executable.clone(),
        prefix_args: Vec::new(),
        identity_paths: vec![npm_executable],
    })
}

fn nvm_candidate(name: &str) -> Option<PathBuf> {
    let home = fs::canonicalize(PathBuf::from(env::var_os("HOME")?)).ok()?;
    let root = fs::canonicalize(home.join(".nvm/versions/node")).ok()?;
    let bin = fs::canonicalize(PathBuf::from(env::var_os("NVM_BIN")?)).ok()?;
    if !bin.starts_with(root) {
        return None;
    }
    let candidate = bin.join(name);
    candidate.is_file().then_some(candidate)
}

fn node_runtime_candidates(name: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(candidate) = nvm_candidate(name) {
        candidates.push(candidate);
    }
    if let Some(home) = env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join(".volta/bin").join(name));
    }
    candidates.extend(
        ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"]
            .into_iter()
            .map(|root| PathBuf::from(root).join(name)),
    );
    candidates
}

fn standard_candidates(name: &str) -> Vec<PathBuf> {
    match name {
        "brew" => vec![
            PathBuf::from("/opt/homebrew/bin/brew"),
            PathBuf::from("/usr/local/bin/brew"),
        ],
        "npm" => node_runtime_candidates("npm"),
        "node" => node_runtime_candidates("node"),
        "winget" => winget_package_candidates(),
        "apt-get" => vec![PathBuf::from("/usr/bin/apt-get")],
        "apt-cache" => vec![PathBuf::from("/usr/bin/apt-cache")],
        "dpkg-query" => vec![PathBuf::from("/usr/bin/dpkg-query")],
        "dnf" => vec![PathBuf::from("/usr/bin/dnf"), PathBuf::from("/bin/dnf")],
        "rpm" => vec![PathBuf::from("/usr/bin/rpm"), PathBuf::from("/bin/rpm")],
        "pacman" => vec![PathBuf::from("/usr/bin/pacman")],
        "pkexec" => vec![PathBuf::from("/usr/bin/pkexec")],
        _ => Vec::new(),
    }
}

#[cfg(target_os = "windows")]
fn winget_package_candidates() -> Vec<PathBuf> {
    let package_family = "Microsoft.DesktopAppInstaller_8wekyb3d8bbwe"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut package_count = 0_u32;
    let mut names_buffer_length = 0_u32;
    let status = unsafe {
        GetPackagesByPackageFamily(
            package_family.as_ptr(),
            &mut package_count,
            std::ptr::null_mut(),
            &mut names_buffer_length,
            std::ptr::null_mut(),
        )
    };
    if status != ERROR_INSUFFICIENT_BUFFER || package_count == 0 || names_buffer_length == 0 {
        return Vec::new();
    }

    let mut package_names = vec![std::ptr::null_mut(); package_count as usize];
    let mut names_buffer = vec![0_u16; names_buffer_length as usize];
    let status = unsafe {
        GetPackagesByPackageFamily(
            package_family.as_ptr(),
            &mut package_count,
            package_names.as_mut_ptr(),
            &mut names_buffer_length,
            names_buffer.as_mut_ptr(),
        )
    };
    if status != ERROR_SUCCESS {
        return Vec::new();
    }

    let mut candidates = package_names
        .into_iter()
        .filter_map(|package_name| winget_package_path(package_name))
        .map(|root| root.join("winget.exe"))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    // Prefer the newest registered App Installer package when multiple versions remain staged.
    candidates.sort_by(|left, right| right.cmp(left));
    candidates
}

#[cfg(target_os = "windows")]
fn winget_package_path(package_name: *mut u16) -> Option<PathBuf> {
    if package_name.is_null() {
        return None;
    }
    let mut path_length = 0_u32;
    let status =
        unsafe { GetPackagePathByFullName(package_name, &mut path_length, std::ptr::null_mut()) };
    if status != ERROR_INSUFFICIENT_BUFFER || path_length == 0 {
        return None;
    }

    let mut path = vec![0_u16; path_length as usize];
    let status =
        unsafe { GetPackagePathByFullName(package_name, &mut path_length, path.as_mut_ptr()) };
    if status != ERROR_SUCCESS {
        return None;
    }
    let end = path
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(path.len());
    Some(PathBuf::from(OsString::from_wide(&path[..end])))
}

#[cfg(not(target_os = "windows"))]
fn winget_package_candidates() -> Vec<PathBuf> {
    Vec::new()
}

pub(super) fn validate_package_id(adapter: &str, value: &str) -> Result<(), CoreError> {
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
    let valid = if adapter == "npm_package" {
        if let Some(scoped) = value.strip_prefix('@') {
            let mut parts = scoped.split('/');
            matches!(
                (parts.next(), parts.next(), parts.next()),
                (Some(scope), Some(package), None)
                    if valid_segment(scope, false) && valid_segment(package, false)
            )
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

fn validate_target_version(adapter: &str, value: &str) -> Result<(), CoreError> {
    let common = !value.is_empty()
        && value.len() <= 128
        && !value.starts_with('-')
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".+_~-:".contains(character));
    let valid = common
        && (adapter != "npm_package"
            || value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || ".+_-".contains(character)));
    if valid {
        Ok(())
    } else {
        Err(CoreError::ArgumentDenied(value.to_string()))
    }
}

#[cfg(unix)]
fn owner_id(metadata: &fs::Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt;
    metadata.uid()
}

#[cfg(not(unix))]
fn owner_id(_: &fs::Metadata) -> u32 {
    0
}
pub(super) fn command_environment(executable: &str) -> BTreeMap<String, String> {
    let file_name = Path::new(executable)
        .file_name()
        .and_then(|name| name.to_str());
    if !matches!(file_name, Some("brew" | "brew.sh")) {
        return BTreeMap::new();
    }
    [
        ("HOMEBREW_NO_AUTO_UPDATE", "1"),
        ("HOMEBREW_NO_INSTALL_CLEANUP", "1"),
        ("HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK", "1"),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value.to_string()))
    .collect()
}

#[cfg(test)]
mod tests {
    use crate::domain::inventory::{
        ExecutionMode, MappingStatus, OwnershipKind, PrivilegeRequirement,
    };

    use super::*;

    fn mapping(adapter: &str, package_id: &str) -> ToolCatalogMapping {
        ToolCatalogMapping {
            platform: "test".to_string(),
            manager: adapter.to_string(),
            package_id: package_id.to_string(),
            adapter: adapter.to_string(),
            mapping_status: MappingStatus::Supported,
            execution_mode: ExecutionMode::ManagedExecute,
            ownership_kind: OwnershipKind::ManagerOwned,
            privilege: PrivilegeRequirement::None,
            update_authority: "manager".to_string(),
        }
    }

    #[test]
    fn compiles_reviewed_manager_vectors() {
        let brew = command_vector(&mapping("homebrew_cask", "orbstack"), "update")
            .expect("brew vector")
            .expect("managed command");
        assert_eq!(
            brew,
            (
                "brew",
                vec!["upgrade", "--cask", "orbstack"]
                    .into_iter()
                    .map(str::to_string)
                    .collect()
            )
        );

        let winget = command_vector(&mapping("winget_package", "Git.Git"), "install")
            .expect("winget vector")
            .expect("managed command");
        assert_eq!(winget.0, "winget");
        assert_eq!(winget.1[0..4], ["install", "--id", "Git.Git", "--exact"]);

        let brew_uninstall =
            command_vector(&mapping("homebrew_formula", "cloudflared"), "uninstall")
                .expect("brew uninstall vector")
                .expect("managed command");
        assert_eq!(
            brew_uninstall,
            (
                "brew",
                vec!["uninstall", "cloudflared"]
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
            )
        );
        let pinned_winget = command_vector_with_target(
            &mapping("winget_package", "Git.Git"),
            "update",
            Some("2.52.0"),
        )
        .expect("pinned winget vector")
        .expect("managed command");
        assert!(pinned_winget
            .1
            .windows(2)
            .any(|arguments| arguments == ["--version", "2.52.0"]));

        let winget_uninstall = command_vector(&mapping("winget_package", "Git.Git"), "uninstall")
            .expect("winget uninstall vector")
            .expect("managed command");
        assert_eq!(
            winget_uninstall.1[0..4],
            ["uninstall", "--id", "Git.Git", "--exact"]
        );

        let npm = command_vector_with_target(
            &mapping("npm_package", "@openai/codex"),
            "update",
            Some("0.32.1"),
        )
        .expect("npm vector")
        .expect("managed command");
        let mut expected_npm = vec![
            "install".to_string(),
            "--global".to_string(),
            "@openai/codex@0.32.1".to_string(),
        ];
        expected_npm.extend(npm_source_args());
        assert_eq!(npm, ("npm", expected_npm));
        assert!(pinned_winget
            .1
            .windows(2)
            .any(|arguments| arguments == ["--source", "winget"]));
        assert!(winget_uninstall
            .1
            .windows(2)
            .any(|arguments| arguments == ["--source", "winget"]));
    }

    #[test]
    fn preserves_linux_manager_specific_lifecycle_semantics() {
        let apt = command_vector(&mapping("apt_package", "git"), "update")
            .expect("APT vector")
            .expect("managed command");
        assert_eq!(
            apt.1,
            ["install", "--only-upgrade", "--yes", "git"].map(str::to_string)
        );

        let dnf = command_vector(&mapping("dnf_package", "git"), "uninstall")
            .expect("DNF vector")
            .expect("managed command");
        assert_eq!(dnf.1, ["remove", "--assumeyes", "git"].map(str::to_string));

        assert!(command_vector(&mapping("pacman_package", "git"), "update")
            .expect("Pacman policy")
            .is_none());
        let pacman_uninstall = command_vector(&mapping("pacman_package", "git"), "uninstall")
            .expect("Pacman uninstall vector")
            .expect("managed uninstall command");
        assert_eq!(
            pacman_uninstall.1,
            ["-Rns", "--noconfirm", "git"].map(str::to_string)
        );
    }
    #[test]
    fn non_root_linux_privilege_fails_closed_without_reviewed_broker() {
        let manager = ExecutableIdentity {
            path: PathBuf::from("/usr/bin/apt-get"),
            canonical_path: PathBuf::from("/usr/bin/apt-get"),
            length: 1,
            modified_epoch_seconds: 1,
            owner_id: 0,
            sha256: "manager".to_string(),
        };
        assert!(apply_linux_privilege(
            manager.clone(),
            vec!["install".to_string(), "git".to_string()],
            None,
            false,
        )
        .is_none());

        let broker = ExecutableIdentity {
            path: PathBuf::from("/usr/bin/pkexec"),
            canonical_path: PathBuf::from("/usr/bin/pkexec"),
            sha256: "broker".to_string(),
            ..manager.clone()
        };
        let compiled = apply_linux_privilege(
            manager,
            vec!["install".to_string(), "git".to_string()],
            Some(broker),
            false,
        )
        .expect("brokered command");
        assert_eq!(compiled.executable, PathBuf::from("/usr/bin/pkexec"));
        assert_eq!(compiled.argv[0], "/usr/bin/apt-get");
    }
    #[test]
    fn npm_invocation_distinguishes_native_shims_from_javascript_cli_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let native = temp.path().join("npm");
        fs::write(&native, b"native shim").expect("native shim");
        let native_invocation = resolve_npm_invocation(&native).expect("native invocation");
        assert!(native_invocation.prefix_args.is_empty());
        assert_eq!(
            native_invocation.executable,
            fs::canonicalize(&native).expect("canonical native shim")
        );

        let script = temp.path().join("npm-cli.js");
        let node = temp
            .path()
            .join(if cfg!(windows) { "node.exe" } else { "node" });
        fs::write(&script, b"#!/usr/bin/env node").expect("npm script");
        fs::write(&node, b"node runtime").expect("node runtime");
        let script_invocation = resolve_npm_invocation(&script).expect("script invocation");
        assert_eq!(
            script_invocation.executable,
            fs::canonicalize(&node).expect("canonical node")
        );
        assert_eq!(
            script_invocation.prefix_args,
            vec![fs::canonicalize(&script)
                .expect("canonical npm script")
                .display()
                .to_string()]
        );
    }

    #[test]
    fn rejects_catalog_values_that_escape_package_identifier_grammar() {
        for (adapter, package_id) in [
            ("homebrew_formula", "--formula"),
            ("homebrew_formula", "https://attacker.invalid/pkg"),
            ("apt_package", "../../tmp/package"),
            ("npm_package", "file:/tmp/package"),
            ("npm_package", "@scope/package/extra"),
        ] {
            let error = command_vector(&mapping(adapter, package_id), "install")
                .expect_err("non-package catalog value must be denied");
            assert!(matches!(error, CoreError::ArgumentDenied(_)));
        }
        assert!(validate_package_id("npm_package", "@openai/codex").is_ok());
        assert!(validate_package_id("homebrew_formula", "owner/tap/formula@2").is_ok());
        let target_error = command_vector_with_target(
            &mapping("npm_package", "@openai/codex"),
            "install",
            Some("file:/tmp/package"),
        )
        .expect_err("non-version npm target must be denied");
        assert!(matches!(target_error, CoreError::ArgumentDenied(_)));
    }
}
