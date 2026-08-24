# STM

STM (Smart Tools Management) is a local-first desktop application for managing developer tools, global AI agent skills, and MCP server bindings behind a review-stage React UI contract and a thin Tauri host.

## Repository layout

- `contracts/ui/`: review-stage UI Contract v1.1 inputs, manifest-owned artifacts, and typed setup/lifecycle/view-model boundaries.
- `crates/stm-core/`: reusable Rust domain, catalog/profile, capability, application, lifecycle-policy, portable-setup, and infrastructure-port contracts.
- `crates/stm-runtime/`: concrete SQLite, bounded HTTP, provider discovery/bootstrap, manager evidence, executable identity, process supervision, vendor handoff, and OS liveness adapters.
- `src-tauri/`: Tauri 2 composition root with deny-by-default capabilities and explicit typed inventory, diagnostics, quick-setup, portable-setup, lifecycle, status, and cancellation commands.
- `tests/fixtures/`: deterministic manager, tool, skill, MCP, root, operation, portable-setup, and UI scenario evidence used by Rust and frontend contract tests.
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
- `pnpm verify:release-contract`
- `pnpm verify:no-secrets`
- `pnpm release:config <generated-config-path>`
- `pnpm verify:release-artifacts <bundle-root> <version> [latest.json]`

The browser build remains deterministic and fixture-backed for UI review. Tauri desktop mode refreshes recommended tools from live, current-platform manager and executable evidence, stores mutable state under the per-user STM data directory, and keeps test fixtures out of native tool decisions.
The interface defaults to Vietnamese, persists a Vietnamese/English language choice, and shows a short install/update summary. Exact commands, mappings, digests, and receipts remain available only under optional technical details.
Quick Setup opens on first launch until dismissed, can be reopened from Dashboard, Tools, or Settings, persists provider preference plus dismissal state, and normalizes selections into a reviewed `setup-queue` before any lifecycle plan is prepared.
If a reviewed `setup-queue` needs Homebrew and no approved Homebrew provider is present, STM downloads only the pinned Homebrew `6.0.18` package from approved HTTPS release hosts, verifies SHA-256 `dc892c034bf7c5567489bd02c34301e9cc63faf246c69372639c943cf5006d12`, Developer ID Installer team `927JGANW46`, and package ID `sh.brew.homebrew`, then opens Apple Installer.app through `/usr/bin/open`. Dependent Homebrew children are staged and recompiled only after bootstrap postconditions match fresh evidence.
Tool mutation still requires a supported manager mapping, live manager evidence, an immutable expiring plan, digest-bound consent, immediate revalidation, executable identity checks, supervised execution or explicit vendor handoff, a verified postcondition, and a durable redacted receipt. Receipt persistence is serialized; a completed mutation whose postcondition or receipt cannot be persisted is reported as recoverable rather than falsely successful.
The packaged Tauri application is single-instance. Lifecycle execution writes an `in_progress` receipt before spawning a manager, records owner/child process IDs durably, blocks competing live-process recovery, and requires a fresh reviewed plan after interruption. A mutating desktop run also retains a separate native confirmation step after review consent and before execution.
Portable setup uses native desktop import/export dialogs. Imports require the exact target, reject machine paths plus shell/executable/script content, cap files at 64 KiB and 32 resources, and mark unknown skill or MCP resources for review instead of claiming executable migration. Exports require a fresh authoritative scan, include only additive target-scoped resources plus bounded credential reference IDs, omit provider preferences, file-backed references, receipts, and commands, and never claim side-by-side migration.
The only authorized side-by-side migration recipe is Codex npm → Homebrew. It installs and verifies the prefix-bound Homebrew Codex executable before optional npm cleanup, keeps the shared Codex home unchanged, checkpoints each transition, and exposes cleanup retry only after the verified target remains active. All other migrations remain unavailable and fail-closed.
The platform lifecycle workflow exercises real install/rescan/no-op/uninstall paths for Homebrew formulae/casks, npm, APT, DNF, and WinGet plus package-scoped Pacman uninstall and non-root privilege denial only inside disposable CI environments. Pacman install/update remains detect-only and fail-closed because a digest-bound single-package transaction cannot be guaranteed after database refresh.
Trusted global skill lifecycle verifies exact-byte Ed25519 catalog metadata against compiled trust roots, resolves immutable Git commits and tree digests, validates staged source trees, and materializes approved global targets with conflict-aware receipts and rollback.
MCP lifecycle normalizes bounded Codex, Claude Code, and Cursor configurations into client-specific bindings. Mutations use approved declarative mappings, immutable digest-bound plans, explicit consent, immediate per-target revalidation, cross-process file locks, atomic replacement, and authenticated XChaCha20-Poly1305 backups whose key is held by the operating-system credential store. Raw MCP credential values never enter SQLite, receipts, logs, or plaintext backup artifacts.
MCP protocol health sends only bounded `initialize` requests. Trusted stdio health binds the reviewed executable identity and remote health injects only approved credential references; neither path invokes domain tools.
Product self-update uses a separately authenticated channel: exact `latest.json` bytes are Minisign-verified, accepted metadata is monotonic by version and digest, plans bind target/URL/manifest/artifact signature fingerprints, consent is single-use, and a pending-install record is reconciled after updater-forced restart. Native updater artifacts receive a second signature verification before installation.
Stable release targets and minimum versions are defined in [`release/platform-matrix.json`](./release/platform-matrix.json) and summarized in [`docs/supported-platforms.md`](./docs/supported-platforms.md). Signed release configuration, protected credentials, draft promotion, updater policy, and reinstall recovery are documented in [`docs/deployment-guide.md`](./docs/deployment-guide.md); the trust and incident boundaries are documented in [`docs/security-model.md`](./docs/security-model.md).
The base desktop build is intentionally internal and does not emit release bundles or updater artifacts. Public candidates are built only by the protected signed-release workflow; missing signing/notarization credentials remain a release blocker rather than producing an unsigned public artifact.
The Tauri development shell targets the locked Vite development server on `http://127.0.0.1:4173`.
