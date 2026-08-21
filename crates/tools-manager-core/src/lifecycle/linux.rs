use crate::{catalog::ToolCatalogMapping, error::CoreError};

use super::{
    command::resolve_executable,
    evidence::{read_only_command, ManagerStateEvidence},
};

pub(super) fn inspect_linux_manager(
    mapping: &ToolCatalogMapping,
) -> Result<ManagerStateEvidence, CoreError> {
    match mapping.adapter.as_str() {
        "apt_package" => inspect_apt(mapping),
        "dnf_package" => inspect_dnf(mapping),
        "pacman_package" => inspect_pacman(mapping),
        adapter => Err(CoreError::CommandDenied(format!(
            "no Linux lifecycle evidence adapter for {adapter}"
        ))),
    }
}

fn inspect_apt(mapping: &ToolCatalogMapping) -> Result<ManagerStateEvidence, CoreError> {
    let dpkg_query = required_executable("dpkg-query")?;
    let apt_cache = required_executable("apt-cache")?;
    let installed_output = read_only_command(
        path_text(&dpkg_query),
        vec![
            "-W".to_string(),
            "-f=${Status}\t${Version}\n".to_string(),
            mapping.package_id.clone(),
        ],
        &[0, 1],
    )?;
    let current_version = parse_dpkg_version(&installed_output);
    let candidate_output = read_only_command(
        path_text(&apt_cache),
        vec![
            "show".to_string(),
            "--no-all-versions".to_string(),
            mapping.package_id.clone(),
        ],
        &[0],
    )?;
    let target_version = parse_control_field(&candidate_output, "Version")
        .ok_or_else(|| CoreError::MalformedInput("APT candidate version missing".to_string()))?;
    Ok(manager_evidence(
        current_version,
        target_version,
        "Live APT/dpkg metadata",
    ))
}

fn inspect_dnf(mapping: &ToolCatalogMapping) -> Result<ManagerStateEvidence, CoreError> {
    let rpm = required_executable("rpm")?;
    let dnf = required_executable("dnf")?;
    let installed_output = read_only_command(
        path_text(&rpm),
        vec![
            "-q".to_string(),
            "--qf".to_string(),
            "%{EVR}\n".to_string(),
            mapping.package_id.clone(),
        ],
        &[0, 1],
    )?;
    let current_version = first_value_line(&installed_output);
    let candidate_output = read_only_command(
        path_text(&dnf),
        vec![
            "--quiet".to_string(),
            "repoquery".to_string(),
            "--latest-limit=1".to_string(),
            "--qf".to_string(),
            "%{evr}".to_string(),
            mapping.package_id.clone(),
        ],
        &[0],
    )?;
    let target_version = first_value_line(&candidate_output)
        .ok_or_else(|| CoreError::MalformedInput("DNF candidate version missing".to_string()))?;
    Ok(manager_evidence(
        current_version,
        target_version,
        "Live DNF/RPM metadata",
    ))
}

fn inspect_pacman(mapping: &ToolCatalogMapping) -> Result<ManagerStateEvidence, CoreError> {
    let pacman = required_executable("pacman")?;
    let installed_output = read_only_command(
        path_text(&pacman),
        vec!["-Q".to_string(), mapping.package_id.clone()],
        &[0, 1],
    )?;
    let current_version = parse_pacman_query(&installed_output, &mapping.package_id);
    let candidate_output = read_only_command(
        path_text(&pacman),
        vec![
            "-Sp".to_string(),
            "--print-format".to_string(),
            "%v".to_string(),
            mapping.package_id.clone(),
        ],
        &[0],
    )?;
    let target_version = first_value_line(&candidate_output)
        .ok_or_else(|| CoreError::MalformedInput("Pacman candidate version missing".to_string()))?;
    Ok(manager_evidence(
        current_version,
        target_version,
        "Live Pacman metadata",
    ))
}

fn manager_evidence(
    current_version: Option<String>,
    target_version: String,
    source: &str,
) -> ManagerStateEvidence {
    ManagerStateEvidence {
        installed: current_version.is_some(),
        update_available: current_version
            .as_deref()
            .is_some_and(|current| current != target_version),
        current_version,
        target_version,
        source: source.to_string(),
    }
}

fn required_executable(name: &str) -> Result<std::path::PathBuf, CoreError> {
    resolve_executable(name).ok_or_else(|| {
        CoreError::CommandDenied(format!("required manager command missing: {name}"))
    })
}

fn path_text(path: &std::path::Path) -> &str {
    path.to_str().unwrap_or("")
}

fn parse_dpkg_version(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        line.strip_prefix("install ok installed\t")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn parse_control_field(output: &str, field: &str) -> Option<String> {
    let prefix = format!("{field}:");
    output.lines().find_map(|line| {
        line.strip_prefix(&prefix)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn parse_pacman_query(output: &str, package_id: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let (name, version) = line.trim().split_once(' ')?;
        name.eq(package_id)
            .then(|| version.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn first_value_line(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_native_linux_manager_versions_without_localized_labels() {
        assert_eq!(
            parse_dpkg_version("install ok installed\t1:2.45.2-1ubuntu1\n").as_deref(),
            Some("1:2.45.2-1ubuntu1")
        );
        assert_eq!(
            parse_control_field("Package: git\nVersion: 1:2.45.2-1ubuntu1\n", "Version").as_deref(),
            Some("1:2.45.2-1ubuntu1")
        );
        assert_eq!(
            parse_pacman_query("git 2.51.0-1\n", "git").as_deref(),
            Some("2.51.0-1")
        );
    }
}
