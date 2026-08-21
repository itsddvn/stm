#[cfg(target_os = "linux")]
mod linux {
    use crate::{
        catalog::ToolCatalogMapping,
        domain::inventory::{ExecutionMode, MappingStatus, OwnershipKind, PrivilegeRequirement},
        feasibility::process_supervisor::CancelSignal,
    };

    use super::super::{
        command::compile_manager_command, executor::real_executor, linux::inspect_linux_manager,
    };

    fn mapping(manager: &str, adapter: &str, package_id: &str) -> ToolCatalogMapping {
        ToolCatalogMapping {
            platform: "linux_x64".to_string(),
            manager: manager.to_string(),
            package_id: package_id.to_string(),
            adapter: adapter.to_string(),
            mapping_status: MappingStatus::Supported,
            execution_mode: ExecutionMode::ManagedExecute,
            ownership_kind: OwnershipKind::ManagerOwned,
            privilege: PrivilegeRequirement::Required,
            update_authority: "manager".to_string(),
        }
    }

    fn require_disposable_runner() {
        assert_eq!(
            std::env::var("STM_DISPOSABLE_LIFECYCLE").as_deref(),
            Ok("1"),
            "refusing to mutate packages outside a disposable lifecycle runner"
        );
    }

    fn execute(mapping: &ToolCatalogMapping, action: &str, target: Option<&str>) {
        let command = compile_manager_command(mapping, action, target)
            .expect("compile manager command")
            .expect("manager executable");
        let result = real_executor()
            .execute_managed(
                command.executable.to_str().expect("UTF-8 executable"),
                &command.argv,
                &command.identities,
                &CancelSignal::default(),
            )
            .expect("execute reviewed command");
        assert!(
            result.success,
            "{} {} failed: {result:?}",
            mapping.manager, action
        );
    }

    fn exercise_install_rescan_noop_uninstall(mapping: ToolCatalogMapping) {
        require_disposable_runner();
        let initial = inspect_linux_manager(&mapping).expect("initial manager evidence");
        if initial.installed {
            execute(&mapping, "uninstall", None);
        }

        let available = inspect_linux_manager(&mapping).expect("available manager evidence");
        assert!(!available.installed);
        execute(&mapping, "install", Some(&available.target_version));

        let installed = inspect_linux_manager(&mapping).expect("installed manager evidence");
        assert!(installed.installed);
        assert_eq!(
            installed.current_version.as_deref(),
            Some(available.target_version.as_str())
        );
        assert!(
            !installed.update_available,
            "fresh install must converge to no-op state"
        );

        execute(&mapping, "uninstall", None);
        let removed = inspect_linux_manager(&mapping).expect("removed manager evidence");
        assert!(!removed.installed);
    }

    fn exercise_uninstall_rescan(mapping: ToolCatalogMapping) {
        require_disposable_runner();
        let initial = inspect_linux_manager(&mapping).expect("initial manager evidence");
        assert!(initial.installed, "fixture package must begin installed");
        execute(&mapping, "uninstall", None);
        let removed = inspect_linux_manager(&mapping).expect("removed manager evidence");
        assert!(!removed.installed);
    }

    #[test]
    #[ignore = "requires a disposable root Ubuntu runner with refreshed APT metadata"]
    fn disposable_apt_install_rescan_noop_uninstall() {
        exercise_install_rescan_noop_uninstall(mapping("apt", "apt_package", "sl"));
    }

    #[test]
    #[ignore = "requires a disposable root Fedora runner with DNF repositories"]
    fn disposable_dnf_install_rescan_noop_uninstall() {
        exercise_install_rescan_noop_uninstall(mapping("dnf", "dnf_package", "jq"));
    }

    #[test]
    fn pacman_install_and_update_fail_closed_before_spawn() {
        let mapping = mapping("pacman", "pacman_package", "git");
        assert!(compile_manager_command(&mapping, "install", Some("2.51.0"))
            .expect("Pacman install policy")
            .is_none());
        assert!(compile_manager_command(&mapping, "update", Some("2.51.0"))
            .expect("Pacman update policy")
            .is_none());
    }

    #[test]
    #[ignore = "requires a disposable root Arch runner with an installed Git package"]
    fn disposable_pacman_uninstall_rescan() {
        exercise_uninstall_rescan(mapping("pacman", "pacman_package", "git"));
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use crate::{
        catalog::ToolCatalogMapping,
        domain::inventory::{ExecutionMode, MappingStatus, OwnershipKind, PrivilegeRequirement},
        feasibility::process_supervisor::CancelSignal,
    };

    use super::super::{
        command::{compile_manager_command, manager_evidence_executable},
        evidence::{real_manager_evidence, ManagerStateEvidence},
        executor::real_executor,
    };

    fn mapping(manager: &str, adapter: &str, package_id: &str) -> ToolCatalogMapping {
        ToolCatalogMapping {
            platform: "macos_arm64".to_string(),
            manager: manager.to_string(),
            package_id: package_id.to_string(),
            adapter: adapter.to_string(),
            mapping_status: MappingStatus::Supported,
            execution_mode: ExecutionMode::ManagedExecute,
            ownership_kind: OwnershipKind::ManagerOwned,
            privilege: PrivilegeRequirement::None,
            update_authority: "manager".to_string(),
        }
    }

    fn inspect(mapping: &ToolCatalogMapping) -> ManagerStateEvidence {
        let executable = manager_evidence_executable(mapping, "install")
            .expect("resolve manager")
            .expect("manager executable");
        real_manager_evidence()
            .inspect(mapping, executable.to_str().expect("UTF-8 executable"))
            .expect("manager evidence")
    }

    fn execute(mapping: &ToolCatalogMapping, action: &str, target: Option<&str>) {
        let command = compile_manager_command(mapping, action, target)
            .expect("compile manager command")
            .expect("manager executable");
        let result = real_executor()
            .execute_managed(
                command.executable.to_str().expect("UTF-8 executable"),
                &command.argv,
                &command.identities,
                &|_| Ok(()),
                &CancelSignal::default(),
            )
            .expect("execute manager command");
        assert!(
            result.success,
            "{} {action} failed: {result:?}",
            mapping.manager
        );
    }

    fn exercise_install_rescan_noop_uninstall(mapping: ToolCatalogMapping) {
        assert_eq!(
            std::env::var("STM_DISPOSABLE_LIFECYCLE").as_deref(),
            Ok("1"),
            "refusing to mutate packages outside a disposable lifecycle runner"
        );
        let initial = inspect(&mapping);
        if initial.installed {
            execute(&mapping, "uninstall", None);
        }
        let available = inspect(&mapping);
        assert!(!available.installed);
        execute(&mapping, "install", Some(&available.target_version));
        let installed = inspect(&mapping);
        assert!(installed.installed);
        assert_eq!(
            installed.current_version.as_deref(),
            Some(available.target_version.as_str())
        );
        assert!(!installed.update_available);
        execute(&mapping, "uninstall", None);
        assert!(!inspect(&mapping).installed);
    }

    #[test]
    #[ignore = "requires a disposable macOS runner with Homebrew"]
    fn disposable_homebrew_formula_install_rescan_noop_uninstall() {
        exercise_install_rescan_noop_uninstall(mapping("homebrew", "homebrew_formula", "tree"));
    }

    #[test]
    #[ignore = "requires a disposable macOS runner with Homebrew casks"]
    fn disposable_homebrew_cask_install_rescan_noop_uninstall() {
        exercise_install_rescan_noop_uninstall(mapping(
            "homebrew",
            "homebrew_cask",
            "font-inconsolata",
        ));
    }

    #[test]
    #[ignore = "requires a disposable macOS runner with Node.js and npm"]
    fn disposable_npm_install_rescan_noop_uninstall() {
        exercise_install_rescan_noop_uninstall(mapping("npm", "npm_package", "@openai/codex"));
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use crate::{
        catalog::ToolCatalogMapping,
        domain::inventory::{ExecutionMode, MappingStatus, OwnershipKind, PrivilegeRequirement},
        feasibility::process_supervisor::CancelSignal,
    };

    use super::super::{
        command::{compile_manager_command, manager_evidence_executable},
        evidence::{real_manager_evidence, ManagerStateEvidence},
        executor::real_executor,
    };

    fn mapping() -> ToolCatalogMapping {
        ToolCatalogMapping {
            platform: "windows_x64".to_string(),
            manager: "winget".to_string(),
            package_id: "sharkdp.bat".to_string(),
            adapter: "winget_package".to_string(),
            mapping_status: MappingStatus::Supported,
            execution_mode: ExecutionMode::ManagedExecute,
            ownership_kind: OwnershipKind::ManagerOwned,
            privilege: PrivilegeRequirement::None,
            update_authority: "manager".to_string(),
        }
    }

    fn inspect(mapping: &ToolCatalogMapping) -> ManagerStateEvidence {
        let executable = manager_evidence_executable(mapping, "install")
            .expect("resolve WinGet")
            .expect("WinGet executable");
        real_manager_evidence()
            .inspect(mapping, executable.to_str().expect("UTF-8 executable"))
            .expect("WinGet evidence")
    }

    fn execute(mapping: &ToolCatalogMapping, action: &str, target: Option<&str>) {
        let command = compile_manager_command(mapping, action, target)
            .expect("compile WinGet command")
            .expect("WinGet executable");
        let result = real_executor()
            .execute_managed(
                command.executable.to_str().expect("UTF-8 executable"),
                &command.argv,
                &command.identities,
                &CancelSignal::default(),
            )
            .expect("execute WinGet command");
        assert!(result.success, "WinGet {action} failed: {result:?}");
    }

    #[test]
    #[ignore = "requires a disposable Windows runner with WinGet sources"]
    fn disposable_winget_install_rescan_noop_uninstall() {
        assert_eq!(
            std::env::var("STM_DISPOSABLE_LIFECYCLE").as_deref(),
            Ok("1"),
            "refusing to mutate packages outside a disposable lifecycle runner"
        );
        let mapping = mapping();
        let initial = inspect(&mapping);
        if initial.installed {
            execute(&mapping, "uninstall", None);
        }
        let available = inspect(&mapping);
        assert!(!available.installed);
        execute(&mapping, "install", Some(&available.target_version));
        let installed = inspect(&mapping);
        assert!(installed.installed);
        assert_eq!(
            installed.current_version.as_deref(),
            Some(available.target_version.as_str())
        );
        assert!(!installed.update_available);
        execute(&mapping, "uninstall", None);
        assert!(!inspect(&mapping).installed);
    }
}
