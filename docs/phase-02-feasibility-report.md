# Phase 2 Feasibility Report

Date: 2026-08-20
Status: implemented and verified on macOS arm64 with Rust `1.88.0`.

## Delivered

- Tauri 2 host scaffold in `src-tauri/` with explicit command permissions and no generic shell or filesystem webview capability.
- Reusable `stm-core` crate with serializable domain contracts, UI DTOs, ports, and feasibility modules.
- Allowlisted process supervision with array args, timeout, output bounding, and cancellation signal support.
- Fixture-backed WinGet, Homebrew, and APT parser coverage.
- Manager fixture matrix expanded to success, empty, malformed, manager-unavailable, timed-out, and version-variant cases for each supported manager family.
- Global skill-root scanning for Codex, Claude Code, and AgentKit-compatible locations with project-root rejection, deduplication, and symlink-escape guards.
- Codex, Claude Code, and Cursor MCP fixture parsing with transport normalization, disabled-binding capture, malformed-entry isolation, unsupported-schema rejection, logical server deduplication, and auth-reference redaction.
- SQLite `open + migrate` proof and documented non-persistent elevation strategy.
- Phase 2 contract verification script for schema, fixture, and documentation coverage in CI.

## Preliminary OS and CPU matrix

| OS | CPU | Phase 2 status | Notes |
| --- | --- | --- | --- |
| Windows 11 | x64 | Supported for continued MVP work | WinGet mapping modeled; UAC relaunch remains manager-scoped |
| macOS 14+ | arm64 | Supported for continued MVP work | Homebrew mapping modeled; desktop broker path documented |
| Ubuntu 24.04 | x64 | Supported for continued MVP work | APT read-only mapping modeled; broker availability may degrade mutations later |
| Windows ARM64 | ARM64 | Detect-only | No Phase 2 evidence beyond Tauri upstream support surface |
| macOS Intel | x64 | Detect-only | No local smoke evidence in this phase |
| Linux ARM64 | ARM64 | Unsupported for now | No manager or packaging evidence captured |

## Evidence paths

- Manager fixtures: `tests/fixtures/feasibility/managers/**`
- MCP fixtures: `tests/fixtures/feasibility/mcp/**`
- Global skill fixtures: `tests/fixtures/feasibility/skills/**`
- Core feasibility code: `crates/stm-core/src/feasibility/**`
- Desktop host: `src-tauri/**`

## Verification

- `pnpm verify:ui-contract`, `pnpm verify:phase-two-foundation`, `pnpm lint`, `pnpm typecheck`, all eight Vitest contract tests, and `pnpm build` pass.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and all fourteen Rust tests pass.
- The Tauri development host compiles, launches, and renders the approved STM interface against the locked Vite surface at `127.0.0.1:4173`.
- `.github/workflows/quality.yml` runs the same frontend and Rust gates on Ubuntu.

## Limits carried forward

- UI remains fixture-backed until Phase 4 integration.
- Elevation is strategy-only in Phase 2; no privileged helper, password capture, or mutation path is implemented.
- MCP parsing is fixture-driven and read-only; client writeback stays in a later phase.

Unresolved questions:

- None
