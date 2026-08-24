# STM System Architecture

Status: updated in Phase 5 on 2026-08-21 against locked UI Contract v1.1.

## Dependency direction

The architecture is intentionally one-way:

1. Locked UI contract
2. Typed application DTOs
3. Thin Tauri host commands
4. Reusable `tools-manager-core`
5. Platform ports and feasibility adapters

The Rust core does not depend on Tauri. All desktop-runtime concerns stay in `src-tauri/`.

## Runtime boundaries

- React remains the presentation layer and may only consume typed IPC results.
- The webview receives no generic shell, filesystem, database, or privilege plugin access.
- Tauri capabilities allow only named inventory, diagnostics, source-analysis, lifecycle-plan, execution-status, and cancellation commands plus minimal core window and event permissions.
- Process execution is routed through an allowlisted supervisor that accepts array args only, enforces timeouts and output bounds, supports cancellation, and never accepts catalog-provided executable paths or arguments.
- The packaged desktop registers Tauri's single-instance plugin before application state and command wiring; a second launch focuses the existing window instead of constructing a competing mutation runtime.

## Persistence and snapshots

- SQLite ownership sits in the Rust core.
- SQLite persists coherent inventory snapshots, redacted operation receipts, complete lifecycle results, and scan diagnostics through transactional writes plus last-good recovery.
- Connections use WAL with a bounded busy timeout; snapshot and receipt mutations are serialized, and snapshot backup occurs after a WAL checkpoint.
- Lifecycle consent is bound to the exact plan digest, evidence, and expiry; persisted requests/results support restart-safe receipt inspection and filtered retry/recovery planning, never authorization for a future run.
- Managed execution persists an `in_progress` receipt before spawn, then durably records owner and child process IDs. Startup reconciles only when those processes are gone; a live owner or child blocks new planning/execution rather than permitting concurrent mutation.
- No direct database access is exposed to the webview.

## Inventory and discovery

- Tool inventory and lifecycle evidence adapters cover WinGet, Homebrew formula/cask, npm, APT/dpkg, DNF/RPM, and Pacman while preserving each manager's command and update semantics.
- Manager fixtures cover success, empty, malformed, manager-unavailable, timed-out, version-variant, ownership, update, no-op, and state-drift evidence. Disposable CI runners separately exercise real Homebrew formula/cask, npm, APT, DNF, and WinGet install/rescan/no-op/uninstall behavior.
- Global skill scanning is bounded to configured global roots and rejects project-local roots plus symlink escapes.
- MCP discovery reads only bounded regular files beneath the canonical user home and normalizes Codex, Claude Code, and Cursor configurations into canonical servers with client-specific transport, command or endpoint, arguments, capabilities, auth references, scope, and enablement.
- MCP auth values are never surfaced or persisted. Environment names and approved credential handles remain references; unavailable references block lifecycle planning.
- MCP mutations resolve approved declarative mappings, bind every client config digest into an immutable consent plan, acquire persistent cross-process file locks, revalidate all targets before the first write, and use atomic replacement with per-client outcomes.
- Receipt-backed MCP backups are authenticated XChaCha20-Poly1305 envelopes. The backup key is created and retrieved through macOS Keychain, Windows Credential Manager, or Linux Secret Service; plaintext credentials never enter backup artifacts or SQLite.

## Elevation

- Elevation remains a compiled platform strategy boundary, not a frontend or catalog concern.
- Windows lifecycle uses manager-native WinGet/UAC behavior; STM never captures credentials.
- macOS manager-owned Homebrew mappings run without app-managed elevation; vendor-owned applications use explicit handoff.
- Linux wraps only the reviewed manager executable and exact arguments with the approved `pkexec` broker when required.
- Scans, metadata checks, source review, and handoff preview never elevate. No password capture or persistent privileged helper is permitted.

## Tool lifecycle

- HTTPS source review probes only exact catalog-matched sources, keeps redirects on the approved origin, and bounds time, response size, and redirect count.
- Immutable plans bind canonical and mapping identities, live manager versions, execution mode, exact executable/args, privilege, affected records, limitations, and expiry. Install/update commands pin the reviewed target where the manager supports a package-scoped exact target.
- Managed execution revalidates current manager evidence plus every executable identity immediately before spawn. npm execution either uses a reviewed native shim or locks both `node` and the resolved npm CLI script before invoking the script explicitly. State drift, expired consent, missing privilege broker, unsupported ownership, and detect-only mappings fail closed.
- Operations are serialized per authoritative manager inside the single desktop instance, preserve per-item batch results, persist complete redacted results, and trigger an authoritative inventory refresh before updated state reaches the UI. Snapshot refresh and verified lifecycle postcondition merges share one lock so a fixture scan cannot overwrite newer live manager evidence. A mutation with an unverified postcondition or failed durable receipt is `recoverable`, not falsely successful.
- Homebrew execution disables auto-update, cleanup, and installed-dependent checks. Linux privilege-required mutations execute only directly as root or through a revalidated `pkexec` broker with the reviewed manager path and exact argument vector. Pacman install/update remains detect-only because `-Syu` refreshes repository state after consent and can violate the reviewed target; uninstall remains package-scoped.
- Vendor handoff remains non-transactional and never claims app-managed rollback.

## MCP lifecycle

- Approved stdio add plans come only from the versioned MCP mapping catalog. Absolute resource roots remain typed arguments, executable identities are locked before consent, and health executes only the MCP `initialize` exchange under a bounded environment, timeout, and output limit.
- Reviewed remote add and update plans must match approved endpoint, capability, supported-client, and credential-reference metadata. Unknown endpoints and unavailable references remain review-only.
- Existing configure, enable, disable, and remove actions open a direct immutable plan; only add or source-changing flows require source analysis. Retry, keep-partial, and rollback actions always prepare fresh evidence and consent.
- Recovery restores an encrypted backup only when the current target still matches the replacement digest. User changes, backup tampering, symlink escapes, stale plans, and concurrent overlapping writes fail closed without overwrite.

## Catalog and update separation

- Catalog entries may select a known adapter or mapping mode only.
- Catalog data may not inject executable paths, shell strings, or arbitrary arguments.
- STM product self-update remains separate from tool, skill, and MCP lifecycle.

## UI contract control

- UI Contract v1.1 remains the consumer contract.
- Intentional interface changes must reopen Phase 1, rerun visual verification, obtain approval, bump the contract version, and regenerate the UI lock before backend changes continue.

## Contract validation

- `scripts/verify-phase-two-foundation.mjs` validates the Phase 2 schema set, required fixture matrix, and supporting documentation/workflow artifacts without mutating locked UI sources.
- `scripts/verify-phase-three-core.mjs` validates the Phase 3 catalog, fixture matrix, Tauri read-command allowlists, and CI wiring without mutating locked UI sources.
- `.github/workflows/platform-contracts.yml` compiles and runs the Rust core contract suite on Linux, macOS, and Windows, then runs real Homebrew formula/cask, npm, APT, DNF, Pacman uninstall, and WinGet mutation paths plus non-root privilege denial only in disposable runners.
