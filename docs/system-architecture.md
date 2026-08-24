# STM System Architecture

Status: current desktop/runtime architecture.

## Dependency direction

The architecture is intentionally one-way:

1. React delivery and review-stage UI contract
2. Thin typed Tauri commands and desktop composition
3. Application use cases and capability services in `stm-core`
4. Domain policy and infrastructure ports in `stm-core`
5. Concrete platform adapters in `stm-runtime`

The core crate depends on neither Tauri nor concrete SQLite, HTTP, process, or OS APIs. `src-tauri/src/state.rs` composes one shared lifecycle/storage graph from runtime implementations and injects it into the application facade.

## Runtime boundaries

- React remains the presentation layer and may only consume typed IPC results.
- The webview receives no generic shell, filesystem, database, or privilege plugin access.
- Tauri capabilities allow only named inventory, diagnostics, source-analysis, quick-setup, setup-preference, portable-setup, lifecycle-plan, execution-status, and cancellation commands plus minimal core window and event permissions.
- Process execution is routed through an allowlisted supervisor that accepts array args only, enforces timeouts and output bounds, supports cancellation, and never accepts catalog-provided executable paths or arguments.
- The packaged desktop registers Tauri's single-instance plugin before application state and command wiring; a second launch focuses the existing window instead of constructing a competing mutation runtime.
- Every mutating desktop run still requires a reviewed plan plus a native host confirmation dialog before execution starts.

## Persistence and snapshots

- SQLite ownership sits in `stm-runtime`; the storage contracts and serialized snapshot/receipt shapes remain in `stm-core`.
- SQLite persists coherent inventory snapshots, redacted operation receipts, complete lifecycle results, and in-progress child checkpoints through transactional writes plus last-good recovery.
- Connections use WAL with a bounded busy timeout; snapshot, checkpoint, and receipt mutations are serialized, and snapshot backup occurs after a WAL checkpoint.
- Lifecycle consent is bound to the exact plan digest, evidence, and expiry; persisted requests/results support restart-safe receipt inspection and filtered retry/recovery planning, never authorization for a future run.
- Managed execution persists an `in_progress` receipt before spawn, then durably records owner and child process IDs. Startup reconciles only when those processes are gone; a live owner or child blocks new planning/execution rather than permitting concurrent mutation.
- Runtime-owned preferences persist Quick Setup dismissal plus install-provider preference outside the webview and are rewritten atomically.
- No direct database access is exposed to the webview.

## Inventory and discovery

- Native tool inventory refreshes recommended tools from live current-platform manager evidence and approved executable locations; fixture inventory remains test/browser-only.
- Provider inventory is detected separately for Homebrew, Bun, Node, and npm. Canonical Homebrew binaries under `/opt/homebrew` or `/usr/local` are approved under the accepted same-UID threat boundary; Bun approval is limited to the exact user-scoped `.bun/bin` binary or the pinned STM runtime-provider path.
- Deterministic manager fixtures cover parser and policy tests. Disposable CI runners separately exercise real Homebrew formula/cask, npm, APT, DNF, and WinGet install/rescan/no-op/uninstall behavior.
- Global skill scanning is bounded to configured global roots and rejects project-local roots plus symlink escapes.
- MCP discovery reads only bounded regular files beneath the canonical user home and normalizes Codex, Claude Code, and Cursor configurations into canonical servers with client-specific transport, command or endpoint, arguments, capabilities, auth references, scope, and per-client enablement state; duplicate logical bindings collapse.
- MCP auth values are never surfaced or persisted. Portable export carries only bounded credential reference IDs, drops file-backed references, and unavailable references block lifecycle planning.
- MCP mutations resolve approved declarative mappings, bind every client config digest into an immutable consent plan, acquire persistent cross-process file locks, revalidate all targets before the first write, and use atomic replacement with per-client outcomes.
- Receipt-backed MCP backups are authenticated XChaCha20-Poly1305 envelopes. Runtime adapters create and retrieve the backup key through macOS Keychain, Windows Credential Manager, or Linux Secret Service; plaintext credentials never enter backup artifacts or SQLite.

## Elevation

- Elevation remains a compiled platform strategy boundary, not a frontend or catalog concern.
- Windows lifecycle uses manager-native WinGet/UAC behavior; STM never captures credentials.
- macOS manager-owned Homebrew mappings run without app-managed elevation; provider bootstrap uses Apple Installer.app handoff, not a shell installer.
- Linux wraps only the reviewed manager executable and exact arguments with the approved `pkexec` broker when required.
- Scans, metadata checks, source review, Quick Setup review, and portable import/export never elevate. No password capture or persistent privileged helper is permitted.

## Threat model boundary

- Reviewed lifecycle protections cover catalog identity, provider trust, exact executable identities, bounded source probes, digest/signature drift, receipt verification, and fail-closed revalidation.
- Same-UID local malware or post-review tampering inside the user's account is out of scope for this trust boundary. STM does not claim to harden that scenario; it instead avoids broader authority and fails closed when trusted evidence changes.

## Tool lifecycle

- HTTPS source review probes only exact catalog-matched sources, keeps redirects on the approved origin, and bounds time, response size, and redirect count.
- Immutable plans bind canonical and mapping identities, live manager versions, execution mode, exact executable/args, privilege, affected records, limitations, and expiry. Install/update commands pin the reviewed target where the manager supports a package-scoped exact target.
- Quick Setup normalizes the client checklist into a server-owned `setup-queue`, replaces client-supplied mapping IDs with resolved provider mappings, converts unknown non-tool resources into review-only children, and rejects duplicates, oversized batches, missing dependency IDs, or cyclic dependency graphs.
- Typed child dependencies are topologically ordered, bound into the reviewed request digest, and skipped unless every prerequisite reaches a verified success state. Managed execution revalidates current manager evidence plus every executable identity immediately before spawn. npm execution requires both trusted Node and npm providers and either uses a reviewed native shim or locks both `node` and the resolved npm CLI script before invoking the script explicitly.
- Operations are serialized per authoritative manager inside the single desktop instance, preserve per-item batch results, persist complete redacted results, and trigger a live authoritative inventory refresh before updated state reaches the UI. Snapshot refresh and verified lifecycle postcondition merges share one lock so stale data cannot overwrite newer live manager evidence. A mutation with an unverified postcondition or failed durable receipt is `recoverable`, not falsely successful.
- Batch execution checkpoints each completed child in SQLite. If a child checkpoint cannot be persisted, the current child becomes recoverable and remaining siblings are skipped instead of running past the durable boundary.
- If a setup queue needs Homebrew and no approved Homebrew provider exists, the runtime bootstraps only the official Homebrew `6.0.18` pkg from approved GitHub HTTPS release hosts, verifies digest `dc892c034bf7c5567489bd02c34301e9cc63faf246c69372639c943cf5006d12`, signer team `927JGANW46`, and package ID `sh.brew.homebrew`, launches Apple Installer.app explicitly through `/usr/bin/open -W -a`, and requires a fresh receipt version, refreshed install time, and owned `brew` executable before dependent Homebrew children run.
- Missing Homebrew and Bun providers become independent, minimized prerequisite nodes only when selected recipes require them. Bun uses the pinned official release metadata owned by `domain/recipe.rs`, bounded exact-entry extraction in `stm-runtime`, an exact STM user-data target, and digest/version postconditions before Bun-package dependents compile. Provider-dependent children must recompile to the same reviewed recipe fingerprint before managed execution is allowed.
- Homebrew execution disables auto-update, cleanup, and installed-dependent checks so the reviewed metadata and selected package boundary remain stable. Linux privilege-required mutations execute only directly as root or through a revalidated `pkexec` broker with the reviewed manager path and exact argument vector. Pacman install/update remains detect-only because `-Syu` refreshes repository state after consent and can violate the reviewed target; uninstall remains package-scoped.

## Portable setup and migration

- Portable setup is additive only. Imports require the exact target, typed `tool`/`skill`/`mcp` resources, bounded credential reference IDs, no machine paths, no shell/executable/script fields, and at most 64 KiB or 32 resources.
- Unknown skills and MCP resources stay review-only until the local machine resolves them against its own trust boundaries.
- Export requires a fresh authoritative scan and omits provider preferences, raw secrets, file-backed auth references, commands, and receipts.
- The only authorized side-by-side migration recipe is Codex npm → Homebrew. Fresh server eligibility requires npm ownership plus an approved Homebrew provider; target install, exact prefix/version activation, checkpoint, and optional npm cleanup execute in that order. Shared Codex config is not copied. All other migrations remain fail-closed.

## MCP lifecycle

- Approved stdio add plans come only from the versioned MCP mapping catalog. Absolute resource roots remain typed arguments, executable identities are locked before consent, and health executes only the MCP `initialize` exchange under a bounded environment, timeout, and output limit.
- Reviewed remote add and update plans must match approved endpoint, capability, supported-client, and credential-reference metadata. Unknown endpoints and unavailable references remain review-only.
- Existing configure, enable, disable, and remove actions open a direct immutable plan; only add or source-changing flows require source analysis. Retry, keep-partial, and rollback actions always prepare fresh evidence and consent.
- Recovery restores an encrypted backup only when the current target still matches the replacement digest. User changes, backup tampering, symlink escapes, stale plans, and concurrent overlapping writes fail closed without overwrite.

## Catalog and update separation

- Catalog mappings declare a typed recipe step plus a known adapter/mapping mode; incompatible steps and unknown fields fail validation.
- Catalog data may not inject executable paths, shell strings, or arbitrary arguments.
- STM product self-update remains separate from tool, skill, MCP, and provider lifecycle.

## UI contract control

- UI Contract v1.1 remains the review consumer contract.
- `contracts/ui/ui-contract.manifest.json` currently reports status `review`; artifact digests are enforced only when the manifest moves to `locked`.
- Intentional interface changes still require manifest/artifact refresh and renewed review evidence before any future lock can be claimed.

## Contract validation

- `scripts/verify-phase-two-foundation.mjs` validates the schema set, required fixture matrix, and supporting documentation/workflow artifacts without mutating UI sources.
- `scripts/verify-phase-three-core.mjs` validates the catalog, fixture matrix, Tauri read-command allowlists, and CI wiring without mutating UI sources.
- `.github/workflows/platform-contracts.yml` compiles the core policy contracts and runtime platform adapters on Linux, macOS, and Windows, then runs real Homebrew formula/cask, npm, APT, DNF, Pacman uninstall, and WinGet mutation paths plus non-root privilege denial only in disposable runners.
