use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ManagerKind {
    #[serde(rename = "winget")]
    Winget,
    #[serde(rename = "homebrew")]
    Homebrew,
    #[serde(rename = "apt")]
    Apt,
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
pub struct ManagerProbeReport {
    pub manager: ManagerKind,
    pub status: ManagerProbeStatus,
    pub packages: Vec<ManagerPackageRecord>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManagerProbeStatus {
    Success,
    Empty,
    Malformed,
    ManagerUnavailable,
    TimedOut,
}

pub fn parse_fixture(manager: ManagerKind, path: &Path) -> Result<ManagerProbeReport, CoreError> {
    let raw = fs::read_to_string(path)?;
    if let Some(status) = parse_directive(&raw)? {
        return Ok(ManagerProbeReport {
            manager,
            status,
            packages: Vec::new(),
            warnings: vec!["Fixture encodes manager status metadata".to_string()],
        });
    }

    let packages = match manager {
        ManagerKind::Winget => parse_winget(&raw),
        ManagerKind::Homebrew => parse_brew(&raw),
        ManagerKind::Apt => parse_apt(&raw),
    };

    match packages {
        Some(packages) if packages.is_empty() => Ok(ManagerProbeReport {
            manager,
            status: ManagerProbeStatus::Empty,
            packages,
            warnings: Vec::new(),
        }),
        Some(packages) => Ok(ManagerProbeReport {
            manager,
            status: ManagerProbeStatus::Success,
            packages,
            warnings: Vec::new(),
        }),
        None => Ok(ManagerProbeReport {
            manager,
            status: ManagerProbeStatus::Malformed,
            packages: Vec::new(),
            warnings: vec!["Fixture parse failed".to_string()],
        }),
    }
}

fn parse_directive(raw: &str) -> Result<Option<ManagerProbeStatus>, CoreError> {
    let Some(line) = raw.lines().find(|line| !line.trim().is_empty()) else {
        return Ok(None);
    };
    let Some(value) = line.trim().strip_prefix("# status:") else {
        return Ok(None);
    };

    let status = match value.trim() {
        "manager_unavailable" => ManagerProbeStatus::ManagerUnavailable,
        "timed_out" => ManagerProbeStatus::TimedOut,
        other => {
            return Err(CoreError::MalformedInput(format!(
                "unsupported manager fixture status directive: {other}"
            )));
        }
    };

    Ok(Some(status))
}

fn parse_winget(raw: &str) -> Option<Vec<ManagerPackageRecord>> {
    if raw.trim().is_empty() {
        return Some(Vec::new());
    }

    let mut lines = raw.lines();
    let header = lines.next()?;
    if !header.contains("Name") || !header.contains("Id") || !header.contains("Version") {
        return None;
    }

    let mut rows = Vec::new();
    for line in lines.filter(|line| !line.trim().is_empty()) {
        let parts: Vec<_> = line.split('|').map(str::trim).collect();
        if parts.len() != 3 {
            return None;
        }
        rows.push(ManagerPackageRecord {
            display_name: parts[0].to_string(),
            id: parts[1].to_string(),
            version: parts[2].to_string(),
        });
    }
    Some(rows)
}

fn parse_brew(raw: &str) -> Option<Vec<ManagerPackageRecord>> {
    if raw.trim().is_empty() {
        return Some(Vec::new());
    }

    let mut rows = Vec::new();
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        let parts: Vec<_> = line.split_whitespace().collect();
        if parts.len() != 2 {
            return None;
        }
        let id = parts[0];
        let version = parts[1];
        rows.push(ManagerPackageRecord {
            id: id.to_string(),
            display_name: id.to_string(),
            version: version.to_string(),
        });
    }
    Some(rows)
}

fn parse_apt(raw: &str) -> Option<Vec<ManagerPackageRecord>> {
    if raw.trim().is_empty() {
        return Some(Vec::new());
    }

    let mut rows = Vec::new();
    for line in raw.lines().filter(|line| !line.trim().is_empty()) {
        let parts: Vec<_> = line.split('\t').collect();
        if parts.len() != 2 {
            return None;
        }
        rows.push(ManagerPackageRecord {
            id: parts[0].to_string(),
            display_name: parts[0].to_string(),
            version: parts[1].to_string(),
        });
    }
    Some(rows)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn fixture(path: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/feasibility")
            .join(path)
    }

    #[test]
    fn parses_manager_fixtures() {
        let winget = parse_fixture(ManagerKind::Winget, &fixture("managers/winget/success.txt"))
            .expect("winget fixture");
        assert_eq!(winget.status, ManagerProbeStatus::Success);
        assert_eq!(winget.packages.len(), 2);

        let brew = parse_fixture(
            ManagerKind::Homebrew,
            &fixture("managers/homebrew/empty.txt"),
        )
        .expect("brew fixture");
        assert_eq!(brew.status, ManagerProbeStatus::Empty);

        let apt = parse_fixture(ManagerKind::Apt, &fixture("managers/apt/malformed.txt"))
            .expect("apt fixture");
        assert_eq!(apt.status, ManagerProbeStatus::Malformed);
    }

    #[test]
    fn parses_manager_fixture_status_markers_and_version_variants() {
        let winget_unavailable = parse_fixture(
            ManagerKind::Winget,
            &fixture("managers/winget/manager-unavailable.txt"),
        )
        .expect("winget unavailable");
        assert_eq!(
            winget_unavailable.status,
            ManagerProbeStatus::ManagerUnavailable
        );

        let brew_timeout = parse_fixture(
            ManagerKind::Homebrew,
            &fixture("managers/homebrew/timed-out.txt"),
        )
        .expect("brew timeout");
        assert_eq!(brew_timeout.status, ManagerProbeStatus::TimedOut);

        let apt_variant = parse_fixture(
            ManagerKind::Apt,
            &fixture("managers/apt/version-variant.txt"),
        )
        .expect("apt variant");
        assert_eq!(apt_variant.status, ManagerProbeStatus::Success);
        assert_eq!(apt_variant.packages.len(), 2);
    }
}
