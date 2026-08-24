use std::{
    env, fs,
    path::{Path, PathBuf},
};

use stm_core::domain::{
    provider::{DetectedProvider, ProviderInventory, ProviderKind, ProviderTrust},
    recipe::PINNED_BUN_VERSION,
};

const APPROVED_UNIX_ROOTS: &[&str] = &[
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/usr/bin",
    "/bin",
    "/opt/homebrew/opt",
];

pub fn detect_provider_inventory() -> ProviderInventory {
    ProviderInventory {
        generation: current_generation(),
        homebrew: detect_named(ProviderKind::Homebrew, &["brew"]),
        bun: detect_named(ProviderKind::Bun, &["bun"]),
        node: detect_named(ProviderKind::Node, &["node"]),
        npm: detect_named(ProviderKind::Npm, &["npm"]),
    }
}

fn detect_named(kind: ProviderKind, names: &[&str]) -> Option<DetectedProvider> {
    let link = names
        .iter()
        .find_map(|name| resolve_candidate(kind, name))?;
    let canonical = fs::canonicalize(&link).ok()?;
    let trust = classify_trust(kind, &canonical);
    Some(DetectedProvider {
        kind,
        path: canonical.display().to_string(),
        version: None,
        trust,
    })
}

fn resolve_candidate(kind: ProviderKind, name: &str) -> Option<PathBuf> {
    let candidates = if kind == ProviderKind::Bun {
        let file_name = if cfg!(target_os = "windows") {
            "bun.exe"
        } else {
            name
        };
        let mut candidates = vec![managed_bun_binary(file_name)];
        if let Some(home) = user_home_dir() {
            candidates.push(home.join(".bun/bin").join(file_name));
        }
        candidates
    } else {
        APPROVED_UNIX_ROOTS
            .iter()
            .map(|root| PathBuf::from(root).join(name))
            .collect()
    };
    candidates
        .into_iter()
        .find(|path| is_resolvable_executable(path))
}

fn user_home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn approved_path_matches(path: &Path, candidate: &Path) -> bool {
    path == candidate
        || fs::canonicalize(candidate)
            .ok()
            .as_deref()
            .is_some_and(|canonical| canonical == path)
}

fn managed_bun_binary(file_name: &str) -> PathBuf {
    crate::preferences::default_data_dir()
        .join("providers")
        .join("bun")
        .join(PINNED_BUN_VERSION)
        .join("bin")
        .join(file_name)
}

fn is_resolvable_executable(path: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if metadata.file_type().is_symlink() {
        return fs::canonicalize(path)
            .ok()
            .is_some_and(|target| is_file_executable(&target));
    }
    is_file_executable(path)
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

fn classify_trust(kind: ProviderKind, path: &Path) -> ProviderTrust {
    if kind == ProviderKind::Bun {
        let file_name = if cfg!(target_os = "windows") {
            "bun.exe"
        } else {
            "bun"
        };
        let stm_bun = managed_bun_binary(file_name);
        let user_bun = user_home_dir().map(|home| home.join(".bun/bin").join(file_name));
        return if approved_path_matches(path, &stm_bun)
            || user_bun
                .as_ref()
                .is_some_and(|candidate| approved_path_matches(path, candidate))
        {
            ProviderTrust::ApprovedRoot
        } else {
            ProviderTrust::UntrustedPath
        };
    }
    if APPROVED_UNIX_ROOTS
        .iter()
        .map(Path::new)
        .any(|root| path.starts_with(root))
        || path.starts_with("/opt/homebrew")
        || path.starts_with("/usr/local")
    {
        ProviderTrust::ApprovedRoot
    } else {
        ProviderTrust::UntrustedPath
    }
}

fn current_generation() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bun_home_does_not_approve_brew_or_npm() {
        let home = env::var("HOME").unwrap_or_else(|_| "/Users/test".into());
        let fake_brew = PathBuf::from(&home).join(".bun/bin/brew");
        let bun = PathBuf::from(&home).join(".bun/bin/bun");
        assert_eq!(
            classify_trust(ProviderKind::Homebrew, &fake_brew),
            ProviderTrust::UntrustedPath
        );
        assert_eq!(
            classify_trust(ProviderKind::Homebrew, Path::new("/opt/homebrew/bin/brew")),
            ProviderTrust::ApprovedRoot
        );
        assert_eq!(
            classify_trust(ProviderKind::Bun, &bun),
            ProviderTrust::ApprovedRoot
        );
        assert_eq!(
            classify_trust(ProviderKind::Npm, Path::new("/tmp/evil-npm")),
            ProviderTrust::UntrustedPath
        );
        assert_eq!(
            classify_trust(
                ProviderKind::Node,
                Path::new("/opt/homebrew/Cellar/node/24.0.0/bin/node"),
            ),
            ProviderTrust::ApprovedRoot
        );
    }

    #[test]
    fn bun_trust_requires_the_exact_managed_binary_path() {
        let file_name = if cfg!(target_os = "windows") {
            "bun.exe"
        } else {
            "bun"
        };
        let managed_root = crate::preferences::default_data_dir()
            .join("providers")
            .join("bun")
            .join(PINNED_BUN_VERSION);
        let managed_binary = managed_root.join("bin").join(file_name);

        assert_eq!(
            classify_trust(ProviderKind::Bun, &managed_binary),
            ProviderTrust::ApprovedRoot
        );
        for unreviewed in [
            managed_root.join(file_name),
            managed_root.join("bin/nested").join(file_name),
            managed_root.join("bin").join(format!("{file_name}.copy")),
            crate::preferences::default_data_dir()
                .join("providers/bun/1.4.1/bin")
                .join(file_name),
        ] {
            assert_eq!(
                classify_trust(ProviderKind::Bun, &unreviewed),
                ProviderTrust::UntrustedPath,
                "{} must not inherit trust from a managed ancestor",
                unreviewed.display()
            );
        }
    }
}
