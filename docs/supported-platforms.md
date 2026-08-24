# Supported Platforms

Status: release matrix frozen for STM 0.1 stable candidates on 2026-08-21.

The machine-readable authority is `release/platform-matrix.json`. A target is stable only when its native installer, signed updater artifact, locked UI contract, inventory adapters, enabled lifecycle mappings, privilege behavior, encrypted recovery, and smoke suite pass on the named runner.

## Stable release targets

| Platform | Architecture | Minimum | CI runner | Bundles |
|---|---|---|---|---|
| macOS | Apple Silicon | macOS 13 | `macos-15` | `.app`, `.dmg`, signed updater archive |
| macOS | Intel | macOS 13 | `macos-15-intel` | `.app`, `.dmg`, signed updater archive |
| Windows | x64 | Windows 10 22H2 | `windows-2025` | MSI, NSIS, signed updater archive |
| Linux | x64 | Ubuntu 22.04 or glibc 2.35 | `ubuntu-24.04` | AppImage, DEB, signed updater archive |

Windows ARM64 and Linux ARM64 remain experimental. Their GitHub-hosted runners are recorded in the matrix, but STM does not publish a stable support claim until signed installer, updater, native-webview, and lifecycle smoke evidence exists.

## Lifecycle capability boundaries

- macOS: Homebrew formula and cask mutations are supported when live ownership evidence matches; vendor-owned apps use handoff. No app-managed administrator credential capture.
- Windows: WinGet package-scoped lifecycle is supported through native WinGet/UAC behavior. STM does not install a privileged helper.
- Ubuntu/Debian: APT/dpkg lifecycle is supported. Privileged mutations run only as root or through the reviewed `pkexec` broker.
- Fedora and Arch contract runners validate DNF/RPM and package-scoped Pacman uninstall behavior. They are compatibility evidence, not stable installer-support claims for 0.1.
- npm lifecycle is supported where the reviewed Node/npm executable identity can be resolved and revalidated.
- Skills and MCP configuration remain user-scoped on every platform. Project-local skill writes and generic webview filesystem access are prohibited.

Unsupported managers, transports, credential sources, or ownership states remain explicit read-only or blocked UI states. They never silently inherit a supported claim from another platform.

## Release evidence

`.github/workflows/release.yml` builds the stable matrix as a signed draft. Promotion requires artifact checksum/signature verification plus manual fresh-machine install, update, restart, inventory, one supported lifecycle operation, recovery, diagnostics-redaction, accessibility, and critical-screen review. Release signing/notarization credentials are not configured in this repository, so public release promotion remains blocked until the protected `signed-release` environment supplies them.
