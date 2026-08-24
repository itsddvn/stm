# Donor Ownership Manifest

Status: drafted from donor `51879de84f0d520b3c00cbb92cbf3bccddc9859a` against quick-setup tip `6c016d56ea3ae06c18e7afb3077e564aa2ad1095`.

## Workspace and contracts

- `Cargo.toml`: workspace must keep `crates/stm-core`, `crates/stm-runtime`, `src-tauri`, and add `crates/release-verifier`.
- `Cargo.lock`: merged dependency graph for split crates plus updater/release tooling.
- `package.json`: keep quick-setup scripts and add release/skill/MCP verifier scripts; phase-four tests must target `stm-core`, not `donor monolith`.
- `contracts/ui/lifecycle-contract.ts`
- `contracts/ui/ui-contract.lock.json`
- `docs/system-architecture.md`
- Historical plan docs under `plans/260820-*` and `plans/260822-*`: preserve both histories; quick-setup wording stays authoritative where architecture split is discussed.

Canonical owner: workspace/contracts/docs.

## `stm-core` policy and domain

- `donor-monolith/src/application/service.rs` -> merge into `crates/stm-core/src/application/service.rs`
  - Keep quick-setup provider bootstrap, portable config, localized review, native confirmation flow.
  - Port donor product-update, skill, and MCP application-facing DTO/workflow surface only through ports.
- `donor-monolith/src/lifecycle/planner.rs` -> merge into `crates/stm-core/src/lifecycle/planner.rs`
  - Tool lifecycle policy stays in core.
  - Donor skill and MCP planning stays in core only for immutable plan assembly.
- `donor-monolith/src/lifecycle/mod.rs` -> merge into `crates/stm-core/src/lifecycle/mod.rs`
  - Export only policy/application contracts.
- `donor-monolith/src/lifecycle/service.rs` -> merge into `crates/stm-core/src/lifecycle/service.rs`
  - Keep restart recovery, pre-spawn durable receipt, per-child checkpoints, postcondition merge, partial-failure isolation.
  - Replace direct concrete dependencies with runtime-backed ports.
- `crates/stm-core/src/catalog/mod.rs`
- `crates/stm-core/src/lifecycle/mcp_planner.rs`
- `crates/stm-core/src/lifecycle/skill_planner.rs`

Canonical owner: `stm-core`.

## `stm-runtime` concrete adapters

- `donor-monolith/migrations/0004_skill_lifecycle.sql` -> `crates/stm-runtime/migrations/0004_skill_lifecycle.sql`
- `donor-monolith/migrations/0005_mcp_lifecycle.sql` -> `crates/stm-runtime/migrations/0005_mcp_lifecycle.sql`
- `crates/stm-runtime/src/storage.rs`
  - Own authenticated skill catalog persistence.
  - Own managed skill receipts/backups/recovery persistence.
  - Own MCP receipts/backups/recovery persistence.
- `donor-monolith/src/lifecycle/command.rs`
  - Reviewed executable identity, npm/node locking, stdio command compilation.
- `donor-monolith/src/mcp/backup_crypto.rs`
  - XChaCha20-Poly1305 backup encryption and OS credential-store key loading.
- `donor-monolith/src/mcp/health.rs`
  - Bounded MCP initialize health checks for stdio and remote endpoints.
- `donor-monolith/src/mcp/lifecycle.rs`
  - Config backup, atomic replacement, file locking, recovery, config digests, target-path validation.
- `donor-monolith/src/skill_catalog/persistence.rs`
  - Last-known-good authenticated catalog persistence.
- `donor-monolith/src/skill_catalog/remote.rs`
  - Fixed-origin HTTPS catalog fetcher.
- `donor-monolith/src/skill_lifecycle/materializer.rs`
  - Filesystem mutation, backups, restore, recovery.
- `donor-monolith/src/skill_lifecycle/resolver.rs`
  - Reviewed `git` execution and bounded staging tree materialization.
- `donor-monolith/src/versioning/runtime.rs`
  - Runtime-backed authenticated skill catalog state when building update inventory.

Canonical owner: `stm-runtime`.

## Core/runtime split notes

- `donor-monolith/src/skill_catalog/mod.rs`
  - Keep verification models and signature/hash policy in `stm-core`.
  - Move remote fetch and persisted-state I/O to `stm-runtime`.
- `donor-monolith/src/skill_lifecycle/mod.rs`
  - Keep serializable request/receipt/outcome models in `stm-core`.
  - Move resolver/materializer implementations to `stm-runtime`.
- `donor-monolith/src/skill_lifecycle/digest.rs`
  - Safe option: move to `stm-runtime` with resolver/materializer because it traverses staged filesystem trees.
- `donor-monolith/src/mcp/policy.rs`
  - Keep declarative mapping validation and trust policy in `stm-core`.
- `donor-monolith/src/lifecycle/skill_source.rs`
  - Keep the trait in `stm-core`; move the real `git`-backed implementation to `stm-runtime`.

## `src-tauri` composition

- `src-tauri/Cargo.toml`
  - Depend on `stm-core`, `stm-runtime`, `tauri-plugin-dialog`, and `tauri-plugin-updater`.
- `src-tauri/src/state.rs`
  - Compose split runtime services, preferences, product-update runtime, and duplicate-start protection.
- `src-tauri/src/commands.rs`
  - Keep quick-setup, portable import/export, native confirmation, and add typed product-update dispatch/status.
- `src-tauri/src/lib.rs`
  - Gate updater plugin to protected release config only; internal builds expose typed unavailable state.
- `src-tauri/src/product_update.rs`
- `src-tauri/src/product_update_contract.rs`
- `src-tauri/src/product_update_receipt.rs`
- `src-tauri/src/signed_update_metadata.rs`

Canonical owner: `src-tauri`.

## Frontend/UI surface

- `src/app/app-shell.tsx`
- `src/components/use-lifecycle-operation.ts`
- `src/features/tools/tool-operation-dialog.tsx`
- `src/features/updates/update-review-dialog.tsx`

Canonical owner: frontend review surface.

Requirements:

- Preserve Vietnamese default and persistent English switching.
- Keep concise localized review/results.
- Keep native confirmation before execution.
- Add product-update availability without widening webview authority.

## Generated sources

- `src-tauri/gen/schemas/acl-manifests.json`
- `src-tauri/gen/schemas/desktop-schema.json`
- `src-tauri/gen/schemas/macOS-schema.json`

Canonical owner: generated outputs.

Resolution rule:

- Resolve source permissions/config first.
- Regenerate later; do not treat current generated JSON as conflict authority.

## Legacy paths to remove after port

- `donor-monolith/src/application/service.rs`
- `donor-monolith/src/lifecycle/command.rs`
- `donor-monolith/src/lifecycle/mod.rs`
- `donor-monolith/src/lifecycle/planner.rs`
- `donor-monolith/src/lifecycle/service.rs`
- `donor-monolith/src/skill_catalog/`
- `donor-monolith/src/skill_lifecycle/`

No `donor monolith` path or import should remain in the final tree.
