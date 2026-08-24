use std::{
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

use sha2::{Digest, Sha256};
use stm_core::{
    catalog::ToolCatalogMapping,
    domain::{inventory::PrivilegeRequirement, recipe::PINNED_BUN_VERSION},
    lifecycle::{CompiledManagerCommand, ExecutableIdentity},
    ports::HostExecutableResolver,
    CoreError,
};

#[derive(Debug, Default)]
pub struct RealHostExecutableResolver;

pub fn compile_mcp_stdio(
    command: &str,
    args: &[String],
) -> Result<Option<CompiledManagerCommand>, CoreError> {
    let host = RealHostExecutableResolver;
    let Some(launcher) = host.resolve_executable(command) else {
        return Ok(None);
    };
    if command == "npx" {
        let canonical_launcher = fs::canonicalize(&launcher)?;
        let script = if canonical_launcher
            .extension()
            .and_then(|value| value.to_str())
            == Some("js")
        {
            canonical_launcher
        } else {
            launcher
                .parent()
                .map(|parent| parent.join("node_modules/npm/bin/npx-cli.js"))
                .filter(|path| path.is_file())
                .ok_or_else(|| {
                    CoreError::ProcessSpawn(
                        "trusted npx JavaScript entry point is unavailable".into(),
                    )
                })?
        };
        let node = resolve_node_for_npm(&launcher).ok_or_else(|| {
            CoreError::ProcessSpawn("trusted Node.js runtime is unavailable".into())
        })?;
        let node_identity = host.executable_identity(node)?;
        let script_identity = host.executable_identity(script)?;
        let mut argv = vec![script_identity.canonical_path.display().to_string()];
        argv.extend_from_slice(args);
        return Ok(Some(CompiledManagerCommand {
            executable: node_identity.canonical_path.clone(),
            argv,
            identities: vec![node_identity, script_identity],
        }));
    }
    let identity = host.executable_identity(launcher)?;
    Ok(Some(CompiledManagerCommand {
        executable: identity.canonical_path.clone(),
        argv: args.to_vec(),
        identities: vec![identity],
    }))
}

pub(crate) struct NpmInvocation {
    pub executable: PathBuf,
    pub prefix_args: Vec<String>,
    pub identity_paths: Vec<PathBuf>,
}

impl HostExecutableResolver for RealHostExecutableResolver {
    fn compile_manager_command(
        &self,
        mapping: &ToolCatalogMapping,
        action: &str,
        target_version: Option<&str>,
    ) -> Result<Option<CompiledManagerCommand>, CoreError> {
        let Some((binary, mut manager_argv)) =
            stm_core::lifecycle::manager_command_vector(mapping, action, target_version)?
        else {
            return Ok(None);
        };
        let Some(manager_executable) = self.resolve_executable(binary) else {
            return Ok(None);
        };
        if mapping.adapter == "npm_package" {
            let Some(invocation) = self.resolve_npm_invocation(&manager_executable) else {
                return Ok(None);
            };
            let mut argv = invocation.prefix_args;
            argv.append(&mut manager_argv);
            let identities = invocation
                .identity_paths
                .into_iter()
                .map(|path| self.executable_identity(path))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(Some(CompiledManagerCommand {
                executable: invocation.executable,
                argv,
                identities,
            }));
        }
        let manager_identity = self.executable_identity(manager_executable)?;
        if mapping.privilege == PrivilegeRequirement::Required {
            if !mapping.platform.starts_with("linux_") {
                return Ok(None);
            }
            let running_as_root = process_is_root();
            let broker_identity = if running_as_root {
                None
            } else {
                self.resolve_executable("pkexec")
                    .map(|path| self.executable_identity(path))
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

    fn manager_evidence_executable(
        &self,
        mapping: &ToolCatalogMapping,
        action: &str,
    ) -> Result<Option<PathBuf>, CoreError> {
        let Some((binary, _)) = stm_core::lifecycle::manager_command_vector(mapping, action, None)?
        else {
            return Ok(None);
        };
        let Some(manager_executable) = self.resolve_executable(binary) else {
            return Ok(None);
        };
        let executable = if mapping.adapter == "npm_package" {
            let Some(invocation) = self.resolve_npm_invocation(&manager_executable) else {
                return Ok(None);
            };
            invocation.executable
        } else {
            manager_executable
        };
        Ok(Some(fs::canonicalize(executable)?))
    }

    fn executable_identity(&self, path: PathBuf) -> Result<ExecutableIdentity, CoreError> {
        executable_identity(path)
    }

    fn resolve_executable(&self, name: &str) -> Option<PathBuf> {
        resolve_executable(name)
    }

    fn expected_stm_bun_binary_path(&self) -> PathBuf {
        expected_stm_bun_binary_path()
    }
}

impl RealHostExecutableResolver {
    pub(crate) fn resolve_npm_invocation(&self, npm_executable: &Path) -> Option<NpmInvocation> {
        resolve_npm_invocation(npm_executable)
    }
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
    Ok(ExecutableIdentity {
        path,
        canonical_path: canonical_path.clone(),
        length: metadata.len(),
        modified_epoch_seconds,
        owner_id: owner_id(&metadata),
        sha256: file_sha256(&canonical_path)?,
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
    standard_candidates(name).into_iter().find_map(|path| {
        let canonical = fs::canonicalize(path).ok()?;
        if !(approved_manager_path(&canonical)
            || name == "bun" && approved_bun_path(&canonical)
            || approved_user_tool_path(name, &canonical))
        {
            return None;
        }
        if canonical.extension().and_then(|value| value.to_str()) == Some("js") {
            return canonical.is_file().then_some(canonical);
        }
        is_file_executable(&canonical).then_some(canonical)
    })
}

fn is_file_executable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub(crate) fn approved_bun_path(path: &Path) -> bool {
    let file_name = if cfg!(target_os = "windows") {
        "bun.exe"
    } else {
        "bun"
    };
    approved_path_matches(path, &stm_bun_bin_dir().join(file_name))
        || user_home_dir()
            .is_some_and(|home| approved_path_matches(path, &home.join(".bun/bin").join(file_name)))
}

#[cfg(target_os = "windows")]
fn approved_manager_path(path: &Path) -> bool {
    let text = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    text.contains("/windowsapps/microsoft.desktopappinstaller_") && text.ends_with("/winget.exe")
}

#[cfg(not(target_os = "windows"))]
fn approved_manager_path(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.starts_with("/opt/homebrew/")
        || text.starts_with("/usr/local/")
        || text.starts_with("/usr/bin/")
        || text.starts_with("/bin/")
        || active_nvm_version_root().is_some_and(|root| path.starts_with(root))
}

#[cfg(not(target_os = "windows"))]
fn active_nvm_version_root() -> Option<PathBuf> {
    let home = fs::canonicalize(PathBuf::from(env::var_os("HOME")?)).ok()?;
    let versions = fs::canonicalize(home.join(".nvm/versions/node")).ok()?;
    let bin = fs::canonicalize(PathBuf::from(env::var_os("NVM_BIN")?)).ok()?;
    if !bin.starts_with(&versions) || bin.file_name()? != "bin" {
        return None;
    }
    bin.parent().map(PathBuf::from)
}

fn resolve_node_for_npm(npm_executable: &Path) -> Option<PathBuf> {
    #[cfg(not(target_os = "windows"))]
    if let Some(root) = active_nvm_version_root() {
        if npm_executable.starts_with(&root) {
            let node = fs::canonicalize(root.join("bin/node")).ok()?;
            return (node.is_file() && approved_manager_path(&node)).then_some(node);
        }
    }
    resolve_executable("node")
}

fn resolve_npm_invocation(npm_executable: &Path) -> Option<NpmInvocation> {
    let npm_executable = fs::canonicalize(npm_executable).ok()?;
    if !approved_manager_path(&npm_executable) {
        return None;
    }
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

fn node_runtime_candidates(name: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    #[cfg(not(target_os = "windows"))]
    if let Some(root) = active_nvm_version_root() {
        candidates.push(root.join("bin").join(name));
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
        "bun" => {
            let file_name = if cfg!(target_os = "windows") {
                "bun.exe"
            } else {
                "bun"
            };
            let mut candidates = vec![stm_bun_bin_dir().join(file_name)];
            if let Some(home) = user_home_dir() {
                candidates.push(home.join(".bun/bin").join(file_name));
            }
            candidates
        }
        "ak" => user_home_dir()
            .map(|home| vec![home.join(".local/bin/ak")])
            .unwrap_or_default(),
        "git" | "cmux" | "codex" | "cloudflared" | "omp" | "grok" => node_runtime_candidates(name),
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

fn approved_user_tool_path(name: &str, path: &Path) -> bool {
    name == "ak"
        && user_home_dir()
            .is_some_and(|home| approved_path_matches(path, &home.join(".local/bin/ak")))
}

fn approved_path_matches(path: &Path, candidate: &Path) -> bool {
    path == candidate
        || fs::canonicalize(candidate)
            .ok()
            .as_deref()
            .is_some_and(|canonical| canonical == path)
}

fn user_home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn stm_bun_bin_dir() -> PathBuf {
    stm_data_dir()
        .join("providers")
        .join("bun")
        .join(PINNED_BUN_VERSION)
        .join("bin")
}

pub fn expected_stm_bun_binary_path() -> PathBuf {
    stm_bun_bin_dir().join(if cfg!(target_os = "windows") {
        "bun.exe"
    } else {
        "bun"
    })
}

fn stm_data_dir() -> PathBuf {
    if let Some(dir) = env::var_os("STM_DATA_DIR") {
        return PathBuf::from(dir);
    }
    let home = user_home_dir().unwrap_or_else(env::temp_dir);
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/stm")
    } else if cfg!(target_os = "windows") {
        env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or(home)
            .join("stm")
    } else {
        home.join(".local/share/stm")
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
        .filter_map(winget_package_path)
        .map(|root| root.join("winget.exe"))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
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

#[cfg(unix)]
fn owner_id(metadata: &fs::Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt;
    metadata.uid()
}
#[cfg(not(unix))]
fn owner_id(_: &fs::Metadata) -> u32 {
    0
}
