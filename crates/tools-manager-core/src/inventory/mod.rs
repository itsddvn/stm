use std::{collections::BTreeMap, fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    application::{
        adapters::FixtureWorkspace,
        catalog::{ToolCatalogEntry, ToolCatalogMapping, ToolCatalogSnapshot},
        versioning::{ToolUpdateEvidence, VersionCatalog},
    },
    domain::{
        inventory::{Freshness, InventoryState, OwnershipKind},
        tool::ToolRecord,
    },
    error::CoreError,
};

const PARSER_VERSION: &str = "phase3-fixture-v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ManagerKind {
    #[serde(rename = "winget")]
    Winget,
    #[serde(rename = "homebrew")]
    Homebrew,
    #[serde(rename = "npm")]
    Npm,
    #[serde(rename = "apt")]
    AptDpkg,
    #[serde(rename = "dnf")]
    DnfRpm,
    #[serde(rename = "pacman")]
    Pacman,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagerPackageRecord {
    pub id: String,
    pub version: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagerScanReport {
    pub manager: ManagerKind,
    pub status: ManagerScanStatus,
    pub parser_version: String,
    pub packages: Vec<ManagerPackageRecord>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManagerScanStatus {
    Success,
    Empty,
    Malformed,
    ManagerUnavailable,
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProbeEvidence {
    pub tool_id: String,
    pub alias: String,
    pub path: String,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OsAppEvidence {
    pub tool_id: String,
    pub display_name: String,
    pub version: String,
    pub owner_kind: OwnershipKind,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppReceiptEvidence {
    pub tool_id: String,
    pub owner: String,
    pub version: String,
    pub manager: String,
}

#[derive(Debug, Clone)]
pub struct InventorySnapshot {
    pub tools: Vec<ToolRecord>,
    pub managers: Vec<ManagerScanReport>,
    pub warnings: Vec<String>,
    pub freshness: Freshness,
}

pub fn scan_inventory(
    workspace: &FixtureWorkspace,
    catalog: &ToolCatalogSnapshot,
    versions: &VersionCatalog,
) -> Result<InventorySnapshot, CoreError> {
    let managers = load_manager_reports(workspace)?;
    let receipts: Vec<AppReceiptEvidence> =
        workspace.read_json("tests/fixtures/tools/app-receipts.json")?;
    let os_apps: Vec<OsAppEvidence> = workspace.read_json("tests/fixtures/tools/os-apps.json")?;
    let probes: Vec<ProbeEvidence> = workspace.read_json("tests/fixtures/tools/probes.json")?;

    let receipts_by_tool: BTreeMap<_, _> = receipts
        .into_iter()
        .map(|record| (record.tool_id.clone(), record))
        .collect();
    let os_by_tool: BTreeMap<_, _> = os_apps
        .into_iter()
        .map(|record| (record.tool_id.clone(), record))
        .collect();
    let probes_by_tool: BTreeMap<_, _> = probes
        .into_iter()
        .map(|record| (record.tool_id.clone(), record))
        .collect();
    let manager_lookup = managers
        .iter()
        .map(|report| (manager_label(&report.manager).to_string(), report))
        .collect::<BTreeMap<_, _>>();

    let mut warnings = collect_probe_collisions(&probes_by_tool);
    let platform = current_platform_slug();
    let platform_label = current_platform_label();
    let native_manager = current_native_linux_manager();
    let mut tools = Vec::new();

    for entry in &catalog.tools {
        let mapping = mapping_for_platform(entry, platform, native_manager);
        let Some(mapping) = mapping else {
            tools.push(missing_tool(entry, platform_label, "mapping.unsupported"));
            continue;
        };

        let update = versions.tool_updates.get(&entry.id);
        let record = reconcile_tool(
            entry,
            mapping,
            ReconcileEvidence {
                update,
                receipt: receipts_by_tool.get(&entry.id),
                manager: manager_lookup.get(&mapping.manager).copied(),
                os_app: os_by_tool.get(&entry.id),
                probe: probes_by_tool.get(&entry.id),
                platform_label,
                warnings: &mut warnings,
            },
        );
        tools.push(record);
    }

    Ok(InventorySnapshot {
        tools,
        managers,
        warnings,
        freshness: if versions
            .tool_updates
            .values()
            .all(|update| update.freshness == Freshness::Fresh)
        {
            Freshness::Fresh
        } else {
            Freshness::Stale
        },
    })
}

struct ReconcileEvidence<'a> {
    update: Option<&'a ToolUpdateEvidence>,
    receipt: Option<&'a AppReceiptEvidence>,
    manager: Option<&'a ManagerScanReport>,
    os_app: Option<&'a OsAppEvidence>,
    probe: Option<&'a ProbeEvidence>,
    platform_label: &'a str,
    warnings: &'a mut Vec<String>,
}

fn reconcile_tool(
    entry: &ToolCatalogEntry,
    mapping: &crate::application::catalog::ToolCatalogMapping,
    evidence: ReconcileEvidence<'_>,
) -> ToolRecord {
    let ReconcileEvidence {
        update,
        receipt,
        manager,
        os_app,
        probe,
        platform_label,
        warnings,
    } = evidence;
    let manager_status = manager.map(|report| report.status.clone());
    let manager_package = manager.and_then(|report| {
        report
            .packages
            .iter()
            .find(|package| package.id == mapping.package_id)
    });

    let (state, owner, ownership_kind, installed_version, reason_code, lifecycle_confidence) =
        if let Some(receipt) = receipt {
            let update_available = update.map(|item| item.update_available).unwrap_or(false);
            (
                if update_available {
                    InventoryState::ManagedUpdateAvailable
                } else {
                    InventoryState::ManagedCurrent
                },
                receipt.owner.clone(),
                mapping.ownership_kind.clone(),
                Some(receipt.version.clone()),
                None,
                "authoritative app receipt".to_string(),
            )
        } else if let Some(package) = manager_package {
            let update_available = update.map(|item| item.update_available).unwrap_or(false);
            (
                if update_available {
                    InventoryState::ManagedUpdateAvailable
                } else {
                    InventoryState::ManagedCurrent
                },
                display_manager(&mapping.manager),
                OwnershipKind::ManagerOwned,
                Some(package.version.clone()),
                None,
                format!("manager inventory via {}", mapping.manager),
            )
        } else if let Some(os_app) = os_app {
            (
                if mapping.execution_mode == crate::domain::inventory::ExecutionMode::DetectOnly {
                    InventoryState::External
                } else {
                    InventoryState::ManagedCurrent
                },
                os_app.source.clone(),
                os_app.owner_kind.clone(),
                Some(os_app.version.clone()),
                None,
                "os metadata evidence".to_string(),
            )
        } else if let Some(probe) = probe {
            let parsed = parse_probe_version(&entry.probe_key, &probe.output);
            if parsed.is_none() {
                warnings.push(format!("probe output could not be parsed for {}", entry.id));
            }
            (
                InventoryState::External,
                "External".to_string(),
                OwnershipKind::External,
                parsed,
                None,
                "allowlisted probe evidence only".to_string(),
            )
        } else {
            match manager_status {
                Some(ManagerScanStatus::ManagerUnavailable) => (
                    InventoryState::ManagerUnavailable,
                    display_manager(&mapping.manager),
                    OwnershipKind::Unknown,
                    None,
                    Some("manager.unavailable".to_string()),
                    "required manager missing".to_string(),
                ),
                Some(ManagerScanStatus::TimedOut) => (
                    InventoryState::Unknown,
                    display_manager(&mapping.manager),
                    OwnershipKind::Unknown,
                    None,
                    Some("inventory.partial".to_string()),
                    "manager timed out".to_string(),
                ),
                _ if matches!(
                    mapping.mapping_status,
                    crate::domain::inventory::MappingStatus::Unsupported
                ) =>
                {
                    (
                        InventoryState::Unsupported,
                        display_manager(&mapping.manager),
                        mapping.ownership_kind.clone(),
                        None,
                        Some("mapping.unsupported".to_string()),
                        "no supported mapping on this platform".to_string(),
                    )
                }
                _ if matches!(
                    mapping.mapping_status,
                    crate::domain::inventory::MappingStatus::Blocked
                ) =>
                {
                    (
                        InventoryState::Blocked,
                        display_manager(&mapping.manager),
                        mapping.ownership_kind.clone(),
                        None,
                        Some("mapping.blocked".to_string()),
                        "policy gated mapping".to_string(),
                    )
                }
                _ => (
                    InventoryState::Missing,
                    display_manager(&mapping.manager),
                    mapping.ownership_kind.clone(),
                    None,
                    None,
                    "catalog mapping available".to_string(),
                ),
            }
        };

    ToolRecord {
        id: entry.id.clone(),
        name: entry.name.clone(),
        summary: entry.summary.clone(),
        kind: entry.kind.clone(),
        groups: entry
            .groups
            .iter()
            .map(|group| title_case_group(group))
            .collect(),
        recommended: entry.recommended,
        catalog_status: entry.catalog_status.clone(),
        mapping_status: mapping.mapping_status.clone(),
        state,
        owner,
        ownership_kind,
        execution_mode: mapping.execution_mode.clone(),
        installed_version: installed_version.clone(),
        available_version: update
            .map(|item| item.target_version.clone())
            .or(installed_version),
        manager: display_manager(&mapping.manager),
        package_id: mapping.package_id.clone(),
        platform: platform_label.to_string(),
        privilege: mapping.privilege.clone(),
        lifecycle_confidence,
        reason_code,
    }
}

fn missing_tool(entry: &ToolCatalogEntry, platform_label: &str, reason_code: &str) -> ToolRecord {
    ToolRecord {
        id: entry.id.clone(),
        name: entry.name.clone(),
        summary: entry.summary.clone(),
        kind: entry.kind.clone(),
        groups: entry
            .groups
            .iter()
            .map(|group| title_case_group(group))
            .collect(),
        recommended: entry.recommended,
        catalog_status: entry.catalog_status.clone(),
        mapping_status: crate::domain::inventory::MappingStatus::Unsupported,
        state: InventoryState::Unsupported,
        owner: "Unsupported".to_string(),
        ownership_kind: OwnershipKind::Unknown,
        execution_mode: crate::domain::inventory::ExecutionMode::DetectOnly,
        installed_version: None,
        available_version: None,
        manager: "Unsupported".to_string(),
        package_id: String::new(),
        platform: platform_label.to_string(),
        privilege: crate::domain::inventory::PrivilegeRequirement::Unknown,
        lifecycle_confidence: "no mapping".to_string(),
        reason_code: Some(reason_code.to_string()),
    }
}

fn load_manager_reports(workspace: &FixtureWorkspace) -> Result<Vec<ManagerScanReport>, CoreError> {
    let fixtures = [
        (
            ManagerKind::Winget,
            "tests/fixtures/managers/winget/success.txt",
        ),
        (
            ManagerKind::Homebrew,
            "tests/fixtures/managers/homebrew/success.txt",
        ),
        (ManagerKind::Npm, "tests/fixtures/managers/npm/success.txt"),
        (
            ManagerKind::AptDpkg,
            "tests/fixtures/managers/apt/success.txt",
        ),
        (
            ManagerKind::DnfRpm,
            "tests/fixtures/managers/dnf/success.txt",
        ),
        (
            ManagerKind::Pacman,
            "tests/fixtures/managers/pacman/success.txt",
        ),
    ];

    fixtures
        .into_iter()
        .map(|(manager, path)| parse_manager_fixture(manager, &workspace.resolve(path)))
        .collect()
}

pub fn parse_manager_fixture(
    manager: ManagerKind,
    path: &Path,
) -> Result<ManagerScanReport, CoreError> {
    let raw = fs::read_to_string(path)?;
    if let Some(status) = parse_directive(&raw)? {
        return Ok(ManagerScanReport {
            manager,
            status,
            parser_version: PARSER_VERSION.to_string(),
            packages: Vec::new(),
            warnings: vec!["status encoded by fixture".to_string()],
        });
    }

    let packages = match manager {
        ManagerKind::Winget => parse_pipe_table(&raw),
        ManagerKind::Homebrew | ManagerKind::Npm => parse_space_pairs(&raw),
        ManagerKind::AptDpkg => parse_tab_pairs(&raw),
        ManagerKind::DnfRpm => parse_pipe_table(&raw),
        ManagerKind::Pacman => parse_space_pairs(&raw),
    };

    match packages {
        Some(packages) if packages.is_empty() => Ok(ManagerScanReport {
            manager,
            status: ManagerScanStatus::Empty,
            parser_version: PARSER_VERSION.to_string(),
            packages,
            warnings: Vec::new(),
        }),
        Some(packages) => Ok(ManagerScanReport {
            manager,
            status: ManagerScanStatus::Success,
            parser_version: PARSER_VERSION.to_string(),
            packages,
            warnings: Vec::new(),
        }),
        None => Ok(ManagerScanReport {
            manager,
            status: ManagerScanStatus::Malformed,
            parser_version: PARSER_VERSION.to_string(),
            packages: Vec::new(),
            warnings: vec!["fixture parse failed".to_string()],
        }),
    }
}

fn parse_directive(raw: &str) -> Result<Option<ManagerScanStatus>, CoreError> {
    let Some(line) = raw.lines().find(|line| !line.trim().is_empty()) else {
        return Ok(None);
    };
    let Some(value) = line.trim().strip_prefix("# status:") else {
        return Ok(None);
    };
    Ok(Some(match value.trim() {
        "manager_unavailable" => ManagerScanStatus::ManagerUnavailable,
        "timed_out" => ManagerScanStatus::TimedOut,
        other => {
            return Err(CoreError::MalformedInput(format!(
                "unsupported manager fixture directive: {other}"
            )))
        }
    }))
}

fn parse_pipe_table(raw: &str) -> Option<Vec<ManagerPackageRecord>> {
    if raw.trim().is_empty() {
        return Some(Vec::new());
    }
    let mut lines = raw.lines().filter(|line| !line.trim().is_empty());
    let header = lines.next()?;
    if !header.contains("Name") || !header.contains("Id") || !header.contains("Version") {
        return None;
    }
    let mut packages = Vec::new();
    for line in lines {
        let parts: Vec<_> = line.split('|').map(str::trim).collect();
        if parts.len() != 3 {
            return None;
        }
        packages.push(ManagerPackageRecord {
            display_name: parts[0].to_string(),
            id: parts[1].to_string(),
            version: parts[2].to_string(),
        });
    }
    Some(packages)
}

fn parse_space_pairs(raw: &str) -> Option<Vec<ManagerPackageRecord>> {
    if raw.trim().is_empty() {
        return Some(Vec::new());
    }
    let mut packages = Vec::new();
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        let parts: Vec<_> = line.split_whitespace().collect();
        if parts.len() != 2 {
            return None;
        }
        packages.push(ManagerPackageRecord {
            id: parts[0].to_string(),
            display_name: parts[0].to_string(),
            version: parts[1].to_string(),
        });
    }
    Some(packages)
}

fn parse_tab_pairs(raw: &str) -> Option<Vec<ManagerPackageRecord>> {
    if raw.trim().is_empty() {
        return Some(Vec::new());
    }
    let mut packages = Vec::new();
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        let parts: Vec<_> = line.split('\t').collect();
        if parts.len() != 2 {
            return None;
        }
        packages.push(ManagerPackageRecord {
            id: parts[0].to_string(),
            display_name: parts[0].to_string(),
            version: parts[1].to_string(),
        });
    }
    Some(packages)
}

fn parse_probe_version(probe_key: &str, output: &str) -> Option<String> {
    let normalized = output.trim();
    let candidate = normalized
        .split_whitespace()
        .find(|part| part.chars().any(|character| character.is_ascii_digit()))?;
    match probe_key {
        "git" => candidate.strip_prefix('v').unwrap_or(candidate),
        _ => candidate.strip_prefix('v').unwrap_or(candidate),
    }
    .to_string()
    .into()
}

fn collect_probe_collisions(probes: &BTreeMap<String, ProbeEvidence>) -> Vec<String> {
    let mut seen = BTreeMap::<String, String>::new();
    let mut warnings = Vec::new();
    for (tool_id, probe) in probes {
        if let Some(existing) = seen.insert(probe.alias.to_ascii_lowercase(), tool_id.clone()) {
            warnings.push(format!(
                "probe alias collision between {existing} and {tool_id}: {}",
                probe.alias
            ));
        }
    }
    warnings
}

pub fn current_platform_slug() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "macos_arm64",
        ("macos", _) => "macos_x64",
        ("windows", "x86_64") => "windows_x64",
        ("linux", "x86_64") => "linux_x64",
        ("linux", "aarch64") => "linux_arm64",
        _ => "unsupported",
    }
}

pub(crate) fn mapping_for_platform<'a>(
    entry: &'a ToolCatalogEntry,
    platform: &str,
    native_manager: Option<&str>,
) -> Option<&'a ToolCatalogMapping> {
    let mut mappings = entry
        .mappings
        .iter()
        .filter(|mapping| mapping.platform == platform);
    if !platform.starts_with("linux_") {
        return mappings.next();
    }
    let native_manager = native_manager?;
    mappings.find(|mapping| mapping.manager == native_manager)
}

pub(crate) fn current_native_linux_manager() -> Option<&'static str> {
    if std::env::consts::OS != "linux" {
        return None;
    }
    fs::read_to_string("/etc/os-release")
        .ok()
        .as_deref()
        .and_then(linux_manager_from_os_release)
        .or_else(linux_manager_from_installed_commands)
}

fn linux_manager_from_os_release(document: &str) -> Option<&'static str> {
    let mut identifiers = Vec::new();
    for line in document.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key != "ID" && key != "ID_LIKE" {
            continue;
        }
        identifiers.extend(
            value
                .trim_matches(['"', '\''])
                .split_ascii_whitespace()
                .map(str::to_ascii_lowercase),
        );
    }
    if identifiers
        .iter()
        .any(|value| matches!(value.as_str(), "debian" | "ubuntu" | "linuxmint" | "pop"))
    {
        Some("apt")
    } else if identifiers.iter().any(|value| {
        matches!(
            value.as_str(),
            "fedora" | "rhel" | "centos" | "rocky" | "almalinux"
        )
    }) {
        Some("dnf")
    } else if identifiers
        .iter()
        .any(|value| matches!(value.as_str(), "arch" | "manjaro" | "endeavouros"))
    {
        Some("pacman")
    } else {
        None
    }
}

fn linux_manager_from_installed_commands() -> Option<&'static str> {
    let available = [
        ("apt", Path::new("/usr/bin/apt-get")),
        ("dnf", Path::new("/usr/bin/dnf")),
        ("pacman", Path::new("/usr/bin/pacman")),
    ]
    .into_iter()
    .filter(|(_, path)| path.is_file())
    .map(|(manager, _)| manager)
    .collect::<Vec<_>>();
    (available.len() == 1).then_some(available[0])
}

fn current_platform_label() -> &'static str {
    match current_platform_slug() {
        "macos_arm64" => "macOS arm64",
        "macos_x64" => "macOS x64",
        "windows_x64" => "Windows x64",
        "linux_x64" => "Linux x64",
        "linux_arm64" => "Linux arm64",
        _ => "Unsupported",
    }
}

fn title_case_group(group: &str) -> String {
    group
        .split('_')
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

fn display_manager(manager: &str) -> String {
    match manager {
        "winget" => "WinGet".to_string(),
        "homebrew" => "Homebrew".to_string(),
        "apt" => "APT/dpkg".to_string(),
        "dnf" => "DNF/RPM".to_string(),
        "pacman" => "Pacman".to_string(),
        other => other.to_string(),
    }
}

fn manager_label(manager: &ManagerKind) -> &'static str {
    match manager {
        ManagerKind::Winget => "winget",
        ManagerKind::Homebrew => "homebrew",
        ManagerKind::Npm => "npm",
        ManagerKind::AptDpkg => "apt",
        ManagerKind::DnfRpm => "dnf",
        ManagerKind::Pacman => "pacman",
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::application::{catalog::load_tool_catalog, versioning::load_version_catalog};

    use super::*;

    fn workspace() -> FixtureWorkspace {
        FixtureWorkspace::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
    }

    #[test]
    fn parses_all_manager_fixture_statuses() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let winget = parse_manager_fixture(
            ManagerKind::Winget,
            &root.join("tests/fixtures/managers/winget/manager-unavailable.txt"),
        )
        .expect("winget");
        assert_eq!(winget.status, ManagerScanStatus::ManagerUnavailable);

        let pacman = parse_manager_fixture(
            ManagerKind::Pacman,
            &root.join("tests/fixtures/managers/pacman/version-variant.txt"),
        )
        .expect("pacman");
        assert_eq!(pacman.status, ManagerScanStatus::Success);
    }

    #[test]
    fn reconciles_authority_precedence_and_candidates() {
        let catalog = load_tool_catalog(&workspace()).expect("catalog");
        let versions = load_version_catalog(&workspace()).expect("versions");
        let inventory = scan_inventory(&workspace(), &catalog, &versions).expect("inventory");

        let orca = inventory
            .tools
            .iter()
            .find(|tool| tool.id == "orca-ade")
            .expect("orca");
        assert_eq!(orca.owner, "Vendor updater");

        let cursor = inventory
            .tools
            .iter()
            .find(|tool| tool.id == "cursor")
            .expect("cursor");
        assert!(!cursor.recommended);
    }

    #[test]
    fn resolves_native_linux_manager_from_distribution_identity() {
        assert_eq!(
            linux_manager_from_os_release("ID=ubuntu\nID_LIKE=\"debian\"\n"),
            Some("apt")
        );
        assert_eq!(
            linux_manager_from_os_release("ID=rocky\nID_LIKE=\"rhel centos fedora\"\n"),
            Some("dnf")
        );
        assert_eq!(
            linux_manager_from_os_release("ID=manjaro\nID_LIKE=arch\n"),
            Some("pacman")
        );
        assert_eq!(linux_manager_from_os_release("ID=nixos\n"), None);
    }
}
