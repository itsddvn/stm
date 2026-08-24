use serde::{Deserialize, Serialize};

pub const PINNED_BUN_VERSION: &str = "1.4.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedBunArchive {
    pub target: &'static str,
    pub asset: &'static str,
    pub sha256: &'static str,
    pub entry: &'static str,
}

pub const PINNED_BUN_ARCHIVES: &[PinnedBunArchive] = &[
    PinnedBunArchive {
        target: "macos_arm64",
        asset: "bun-darwin-aarch64.zip",
        sha256: "c669e97f6164e1c96e0701748db98dfa77492908cbd8394c7557134a735de381",
        entry: "bun-darwin-aarch64/bun",
    },
    PinnedBunArchive {
        target: "macos_x64",
        asset: "bun-darwin-x64.zip",
        sha256: "1d0211b8f1dc991182344687ad15e72ee86f154845a5f7fa477994cd341dd9b0",
        entry: "bun-darwin-x64/bun",
    },
    PinnedBunArchive {
        target: "linux_x64",
        asset: "bun-linux-x64.zip",
        sha256: "2d03fb5fb83ac8b567aca0a281b2ce1a1a19d488f56c2968d88c3f25e92fe452",
        entry: "bun-linux-x64/bun",
    },
    PinnedBunArchive {
        target: "windows_x64",
        asset: "bun-windows-x64.zip",
        sha256: "e6f093d39da486b20262ca8cdd5ed6a9e8bc9c2f275b78e6d3a0c5b28cc95901",
        entry: "bun-windows-x64/bun.exe",
    },
];

pub fn pinned_bun_source_url(spec: PinnedBunArchive) -> String {
    format!(
        "https://github.com/oven-sh/bun/releases/download/bun-v{PINNED_BUN_VERSION}/{}",
        spec.asset
    )
}

pub fn pinned_bun_archive(target: &str) -> Option<PinnedBunArchive> {
    PINNED_BUN_ARCHIVES
        .iter()
        .find(|spec| spec.target == target)
        .copied()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecipeStepType {
    ManagerPackage,
    SignedArtifact,
    DmgApplication,
    PkgInstaller,
    WindowsInstaller,
    DebPackage,
    RpmPackage,
    ArchiveBinary,
    AppImage,
    VendorHandoff,
    Rescan,
    VerifyPostcondition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecipePreference {
    pub adapter: String,
    pub package_id: String,
    pub step: RecipeStepType,
    pub tap: Option<String>,
    pub publisher: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedRecipe {
    pub resource_id: String,
    pub desired_action: String,
    pub adapter: String,
    pub package_id: String,
    pub mapping_id: String,
    pub step: RecipeStepType,
    pub blocked_reason: Option<String>,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedInstallerArtifact {
    pub provider_id: String,
    pub path: String,
    pub version: String,
    pub source_url: String,
    pub sha256: String,
    pub signer_team_id: String,
    pub package_id: String,
    pub previous_receipt_install_time: Option<u64>,
    pub expected_executable_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedArchiveBinary {
    pub provider_id: String,
    pub version: String,
    pub source_url: String,
    pub archive_sha256: String,
    pub binary_sha256: String,
    pub staged_binary_path: String,
    pub target_binary_path: String,
}
