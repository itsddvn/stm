# Phase 3 Read-Only Core Evidence

Date: 2026-08-20
Scope: final Phase 3 wiring only. No locked UI artifacts or plan files changed.

## Evidence

- Canonical recommended catalog remains locked to exactly ten ids:
  `git`, `orca-ade`, `cmux-desktop`, `docker-desktop`, `orbstack`, `agentkit-cli`, `oh-my-pi`, `codex-cli`, `grok-build`, `cloudflared`.
- Fixture matrix remains present for manager success, empty, malformed, manager-unavailable, timed-out, and version-variant states across `winget`, `homebrew`, `apt`, `dnf`, and `pacman`.
- `tools-manager-core` now exports `adapters`, `catalog`, `inventory`, `skills`, `mcp`, `storage`, and `versioning` at the crate root; `application` re-exports them instead of path-mounting duplicate module copies.
- Application service exposes deterministic `headless_scan()` output:
  snapshot DTO, refresh status DTO, diagnostics report, ordered scan events, and `elevationRequested = false`.
- Tauri host exposes `headless_scan`, registers it in the invoke handler, and emits `phase-three-scan` progress events with typed payloads.
- `pnpm verify:phase-three-core` is wired in `package.json`, and CI invokes that command directly.

## Notes

- `tests/fixtures/mcp/claude-code/config.json` now carries `authRequired: true` on `Sentry`, matching the verification contract for auth-reference coverage.
- Local verification completed with Rust `1.88.0`: `cargo fmt --check`, strict workspace Clippy, and all 28 Rust tests pass. The controller also verified the locked UI contract, Phase 3 artifact matrix, frontend lint/typecheck/tests/build, and corrected SHA-256 lock comparison plus parallel skill-fixture isolation.
