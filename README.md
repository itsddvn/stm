# STM

STM (Smart Tools Management) is a local-first desktop application for managing developer tools, global AI agent skills, and MCP server bindings behind a locked React UI contract and a thin Tauri host.

## Repository layout

- `contracts/ui/`: locked UI Contract v1.1 inputs owned by the approved frontend phase.
- `crates/tools-manager-core/`: reusable Rust core with catalog validation, inventory/discovery, immutable lifecycle planning, evidence-bound consent, supervised execution, receipts, SQLite persistence, and UI-facing DTOs.
- `src-tauri/`: Tauri 2 desktop host with deny-by-default capabilities and explicit typed inventory, source-analysis, lifecycle, status, and cancellation commands.
- `tests/fixtures/`: deterministic manager, tool, skill, MCP, root, operation, and UI scenario evidence used by Rust and frontend contract tests.
- `catalog/schemas/`: durable JSON schemas for inventory, tools, skills, MCP bindings, auth references, operations, updates, and source analysis.

## Toolchain

- Node.js: `24.13.1`
- pnpm: `10.14.0`
- Rust: `1.88.0`
- Tauri host crate: `2.11.2`

## Intended commands

- `pnpm verify:ui-contract`
- `pnpm verify:phase-two-foundation`
- `pnpm verify:phase-three-core`
- `pnpm lint`
- `pnpm typecheck`
- `pnpm test`
- `pnpm test:desktop-integration`
- `pnpm build`
- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `pnpm tauri:build`

The browser build remains deterministic and fixture-backed for UI review. In Tauri desktop mode, the same locked interface consumes typed Rust inventory and lifecycle IPC without receiving generic shell, filesystem, database, or privilege access.
Tool mutation requires a supported manager mapping, live manager evidence, an immutable expiring plan, digest-bound consent, immediate revalidation, executable identity checks, supervised execution or explicit vendor handoff, a verified postcondition, and a durable redacted receipt. Receipt persistence is serialized; a completed mutation whose postcondition or receipt cannot be persisted is reported as recoverable rather than falsely successful.
The packaged Tauri application is single-instance. Lifecycle execution writes an `in_progress` receipt before spawning a manager, records owner/child process IDs durably, blocks competing live-process recovery, and requires a fresh reviewed plan after interruption.
For npm-managed tools, STM accepts a reviewed native npm shim or locks both the Node.js runtime and npm CLI script before executing `node <npm-cli.js> ...`; it never re-resolves a mutable shebang after review.
The platform lifecycle workflow exercises real install/rescan/no-op/uninstall paths for Homebrew formulae/casks, npm, APT, DNF, and WinGet plus package-scoped Pacman uninstall and non-root privilege denial only inside disposable CI environments. Pacman install/update remains detect-only and fail-closed because a digest-bound single-package transaction cannot be guaranteed after database refresh.
The Tauri development shell targets the locked Vite development server on `http://127.0.0.1:4173`.
