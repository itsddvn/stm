---
title: "Quick Setup Portable Configuration and Capability Services"
description: "Ship nontechnical Quick Setup, portable desired-state config, and bounded installer/updater/validator/optimizer capabilities."
status: in-progress
priority: P1
effort: "3-5 engineer-weeks"
tags: [feature, architecture, cross-platform, lifecycle, security]
blockedBy: []
blocks: []
created: 2026-08-22
---

# Quick Setup Portable Configuration and Capability Services

## Overview

STM becomes a recipe-driven installer for nontechnical users. First launch and Dashboard/Tools open Quick Setup. A user selects tools, reviews one plain-language plan, and STM installs supported items automatically. Settings can export/import a target-specific setup and change install providers.

This plan is the implementation authority. Conversation history is not required.

Current product still inventories recommended tools one-by-one. Bundle install, portable config, provider preference, and capability modules do not exist. Existing consent, ownership, postcondition, receipt, and no-secret invariants stay mandatory.

## Scope Contract

- **Outcome:** a nontechnical user can apply platform defaults or import a target-specific setup, review once, and get supported tools installed without learning package managers.
- **Constraints:** every mutation is plan-, evidence-, consent-, and receipt-bound. Catalog recipes never contain shell scripts. Imported files are untrusted desired state. Pacman install/update remains guidance-only. Product self-update stays a separate trust channel.
- **Non-goals:** silent dependency install, exact-machine clone, destructive sync, secret export, arbitrary command replay, persistent daemons, one crate per capability, Optimizer apply UX, and unsupported-platform success claims.
- **Acceptance:** every selected resource resolves to automatic recipe, unavoidable OS/vendor prompt, or a specific blocked reason before consent. Supported recipes download, verify, install, rescan, and persist a durable per-child result after each postcondition.

## Settled Decisions

### Audience and flow

- Primary user is nontechnical.
- Required user path: select → one review → Start.
- Technical commands live under expandable details.
- First launch opens Quick Setup. Skip is permanent and does not reopen automatically.
- Quick Setup remains available from Dashboard and Tools.
- Source choice: System recommendations or Import setup config.
- System recommendations preselect platform-default tools. Trusted skills and approved MCP servers are optional add-ons, unchecked.
- Installed/current items are no-ops. Update-available items stay selected as Update.
- Mixed review is allowed. Managed, handoff, guidance, blocked, and no-op outcomes stay distinct.

### Architecture

Five layers:

1. Delivery: React, UI contract, Tauri commands.
2. Application: Quick Setup, portable config, inventory refresh, review workflows.
3. Capabilities: installer, updater, validator, optimizer, lifecycle coordinator.
4. Domain and ports: identities, desired state, recipes, plans, consent, results, errors, port traits.
5. Infrastructure: adapters, process supervisor, filesystem, SQLite, keyring, network.

Three crates:

- `stm-core`: application, capabilities, domain, ports, catalog, inventory, lifecycle. No Tauri. No concrete OS/SQLite/keyring impl.
- `stm-runtime`: adapters, storage, process, filesystem, network, credentials.
- `stm-desktop`: UI, Tauri IPC, composition root, product updater.

Capabilities are in-process modules, not daemons and not separate crates.

| Capability | Owns | Does not own |
|---|---|---|
| Validator | Read-only identity, source, platform, provider, dependency, compatibility, risk | Execution, consent, persistence |
| Installer | Dependency expansion, install/no-op plan prep, recipe step selection | Generic execution, receipts |
| Updater | Update availability, target selection, current no-op | Product self-update |
| Optimizer | Typed setting proposals | Silent mutation. Apply later through lifecycle coordinator |
| Lifecycle coordinator | Plan, consent, single-use, locks, spawn, cancel, postcondition, receipt, recovery | Resource-specific recipe choice |

Shared kernels:

- Catalog/Profile: identity, defaults, recipes, trust.
- Inventory/Evidence: installed state, owner, versions, provider availability.
- Lifecycle/Operations: authorization and mutation.

Dependency direction: Delivery → Application → Capabilities → Domain/Ports. Infrastructure implements ports. Only `stm-desktop` constructs concrete runtime impls.

### Providers

Homebrew, Bun, and npm are install providers with different coverage.

- Homebrew: preferred macOS provider for apps and CLIs.
- Bun: preferred JavaScript-native alternative.
- npm: existing-owner or compatibility fallback. Never bootstrapped without Node.js.

First-launch choices: Automatic, Prefer Homebrew, Prefer Bun. Automatic is the default.

Detect Homebrew, Bun, and Node.js/npm before recommending anything. Preserve an existing authoritative owner per resource.

Missing-provider recommendation: Homebrew for broad macOS coverage; Bun for JavaScript-only selections. Bun cannot install desktop apps. STM may still add Homebrew or a signed-artifact recipe for incompatible items.

macOS resolve order:

1. Existing authoritative owner
2. Explicit compatible user preference
3. Homebrew
4. Bun
5. npm
6. Signed artifact / native installer
7. Vendor handoff

Skip an unsupported provider. Never force it.

Minimize the shared prerequisite graph. Do not install both Node.js and Bun for the same selection unless a selected recipe requires both.

### Bootstrap

- Missing Homebrew: download official Homebrew macOS `.pkg` from a compiled HTTPS origin, pin URL/digest/signer Team ID, verify those identities, launch the native installer, wait for the unavoidable macOS authorization prompt, rescan `/opt/homebrew` or `/usr/local`, then continue. Never run `curl | bash`. Bootstrap postcondition is Failed with no receipt unless the expected prefix and brew executable identity exist.
- Missing Bun, and Bun is actually required: install a pinned official Bun release binary to a reviewed absolute path. STM executes that path directly. Do not depend on ambient PATH. The supervisor may add only the reviewed Bun bin directory.
- Missing npm, and a selected recipe actually requires npm: install Node.js LTS through an approved recipe, then verify `node` and bundled `npm` by absolute identity. Do not install npm alone.
- Existing owner wins over preference. Preference changes affect new installs only.
- One review binds the bootstrap artifact and the dependent recipes, not precompiled dependent executables. After a verified bootstrap, dependents are compiled from the reviewed recipe and revalidated against fresh provider identity. Authorization expiry is checked before every mutating child. A long OS prompt that expires the plan checkpoints completed children and requires a fresh review for remaining work.

### macOS app/CLI recipes

Prefer Homebrew whenever a verified mapping exists. If Homebrew cannot install the resource, use Bun, then npm, then signed artifact, then vendor handoff.

Catalog corrections required before execution:

- Orca ADE source is `onorca.dev` / `stablyai/orca`. Current `orca.so` is the wrong product.
- Oh My Pi source is `can1357/oh-my-pi`. Current `ohmypi.dev` is unavailable.
- Codex CLI preferred clean-macOS recipe is official Homebrew cask `codex`. Keep npm `@openai/codex` as fallback/existing-owner.
- GitHub CLI stays blocked until a verified mapping exists. Candidate metadata alone is not execution authority.

### Profiles

Common defaults: Git, AgentKit CLI, Codex CLI, cloudflared.

Overrides:

- macOS: OrbStack, Orca, cmux desktop
- Windows: Docker Desktop
- Ubuntu/Fedora/Arch: Docker Engine

Stable targets: macOS arm64/x64, Windows x64, Ubuntu/Fedora/Arch x64. No experimental ARM64 profiles in this delivery.

Oh My Pi and Grok Build remain optional catalog entries. Grok stays detect/guidance until a verified recipe exists.

Arch Pacman install/update stays guidance-only.

### Provider settings and migration

Settings shows current preference, detected providers, owned-resource counts, Change preference, and Migrate tools.

Changing preference does not migrate existing tools.

Migration is a typed, gated state machine per compatible resource. The request names source mapping ID, target recipe/mapping ID, explicit target executable path, config-preservation allowlist, and reviewed cleanup choice.

1. Preflight source and target mappings against fresh ownership evidence
2. Install and verify the target-owned copy by explicit path
3. Copy only the reviewed non-secret config allowlist; never persist config bytes in receipts
4. Switch PATH/shim ownership under affected-path locks
5. Verify the active executable is the target identity
6. Review old-owner uninstall, preselected
7. Uninstall old owner only after target activation is freshly verified; the cleanup child is not executable if any prior required step failed
8. Verify old receipt gone and new executable still active

Failed cleanup is partial/recoverable. Do not remove the verified new install. System-owned resources and resources without a verified migration recipe stay unchanged with a reason.

### Portable config

- Scope: tools, global skills, MCP bindings. Provider preference is local durable state and is never imported as authority.
- Settings has Portable Setup export/import. Quick Setup result also exports.
- Export preselects all resources from a freshly completed inventory/provider scan. Fail instead of exporting a stale snapshot.
- One target platform per file. User chooses the target before export.
- Import on a different platform is blocked with an explanation.
- Import preselects every resource in the file.
- Import is additive. Never uninstall resources absent from the file.
- Resolve latest compatible versions on the target using local preference and live evidence.
- Custom/non-catalog resources import as Review required. Do not network-probe imported custom URLs. File commands/paths never become execution authority.
- MCP exports only opaque credential-reference IDs that match a strict identifier grammar and exist in the credential store. Byte-scan exported files for known local secrets before commit.
- File never contains raw secrets, machine paths, executable commands, or receipts.
- Host opens native import/export dialogs. JavaScript never supplies an arbitrary filesystem path.

### Catalog recipe model

Catalog entries declare typed steps. Compiled adapters execute them.

Allowed step types: `manager-package`, `signed-artifact`, `dmg-application`, `pkg-installer`, `windows-installer`, `deb-package`, `rpm-package`, `archive-binary`, `app-image`, `vendor-handoff`, `rescan`, `verify-postcondition`.

A recipe is `supported` only after schema validation, publisher verification, and platform contract tests. Unsigned or untested recipes cannot auto-execute.

### Execution contract

- Mixed plans use typed child intents `{ resourceKind, resourceId, desiredAction, mappingId? }`, not bare update IDs.
- Batch execution is a dependency DAG. Dependents stay blocked until required parents verify. Independent branches may continue.
- Persist a per-child journal immediately after each postcondition. Recovery resumes from those checkpoints.
- Config/PATH/shim mutations declare normalized lock keys and share the single process-wide coordinator instance.
- Provider binaries must match approved roots/receipts/signatures before they can own or execute resources. PATH-only discoveries are untrusted.
- `manager-package` values remain provider-specific validated newtypes. Keep current dash/path/tap/npm deny rules.
- Archive extraction is bounded, symlink-free, and staged privately before atomic install.

## Red Team Review

### Session — 2026-08-22
**Findings:** 15 accepted after dedupe, 4 rejected.
**Severity breakdown:** 4 Critical accepted, 11 High accepted.

| # | Finding | Severity | Disposition | Applied To |
|---|---|---|---|---|
| 1 | Dependency-aware batch and gated cleanup | Critical | Accept | plan, phase 4, 5 |
| 2 | Staged consent-safe bootstrap | Critical | Accept | plan, phase 4 |
| 3 | Typed migration mapping IDs | Critical | Accept | plan, phase 5 |
| 4 | Migration state machine | Critical | Accept | phase 5 |
| 5 | Per-child durable checkpoints | High | Accept | plan, phase 4 |
| 6 | Failed bootstrap is Failed | High | Accept | plan, phase 4 |
| 7 | One shared coordinator instance | High | Accept | phase 2 |
| 8 | PATH/shim lock keys | High | Accept | plan, phase 4 |
| 9 | Absolute Bun/provider identities | High | Accept | plan, phase 4 |
| 10 | Durable PreferencesStore | High | Accept | phase 3, 5 |
| 11 | Platform-profile schema and all ten-tool gates | High | Accept | phase 3 |
| 12 | Typed desired-state intents | High | Accept | phase 4 |
| 13 | Managed-current action consumers | High | Accept | phase 4 |
| 14 | UI review-then-lock | High | Accept | phase 5, 7 |
| 15 | Host-owned portable I/O and no import probes | High | Accept | phase 6 |

Rejected: defer crate split (user packaging decision); delete Optimizer module (user capability request; keep empty module, no apply UX); drop migration/portable from this delivery (user scope); unbounded plan-cache only (implement during coordinator work).

### Whole-Plan Consistency Sweep
- Files reread: plan.md and phase-01 through phase-07
- Decision deltas checked: 8
- Reconciled stale references: phase-07 live Quick Setup evidence and remaining native-import/UI-lock blockers
- Unresolved contradictions: 0

## Current Gaps

- Phase checklists are 59/61 complete. Phases 1-6 are complete; Phase 7 remains in progress.
- Workspace verification is green: `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (97 passed, 4 ignored), `pnpm lint`, `pnpm typecheck`, `pnpm test` (21 tests), `pnpm test:desktop-integration` (3 tests), `pnpm build`, `pnpm verify:ui-contract`, `pnpm verify:phase-two-foundation`, and `pnpm verify:phase-three-core`.
- Live Quick Setup browser verification passed for select → review → consent → terminal success. Evidence: `.artifacts/report/20260823-170305-quick-setup/report.html`.
- Live Settings provider preference and reviewed Codex npm → Homebrew migration passed, but native portable import runtime verification is blocked because the harness process broker resolves the missing executable `/opt/homebrew/Cellar/omp/17.4.2/bin/omp` and the mounted browser daemon is unavailable. Evidence: `.artifacts/report/20260823-170935-provider-import/report.html`.
- `contracts/ui/ui-contract.manifest.json` remains `review`; `pnpm lock:ui-contract` stays blocked pending project-lead approval after the native import runtime pass.
- All side-by-side migrations except Codex npm → Homebrew remain fail-closed.

## Implementation Order

Do not implement UI before recipe/provider contracts exist. Do not split crates and change install semantics in the same phase.

## Goals

| # | Goal | Priority |
|---|------|----------|
| 1 | Split core/runtime and expose bounded capabilities | P1 |
| 2 | Ship verified recipes and provider resolution | P1 |
| 3 | Ship mixed install/update planning | P1 |
| 4 | Ship Quick Setup, provider settings, and migration | P1 |
| 5 | Ship portable configuration | P1 |
| 6 | Verify live behavior and lock the UI contract | P1 |

## Phases

| # | Phase | Status |
|---|-------|--------|
| 1 | [Locked Architecture Contract](./phase-01-start.md) | Completed |
| 2 | [Core Runtime Split](./phase-02-core-runtime-split.md) | Completed |
| 3 | [Catalog Recipes And Providers](./phase-03-catalog-recipes-and-providers.md) | Completed |
| 4 | [Capabilities And Mixed Planning](./phase-04-capabilities-and-mixed-planning.md) | Completed |
| 5 | [Quick Setup And Settings](./phase-05-quick-setup-and-settings.md) | Completed |
| 6 | [Portable Configuration](./phase-06-portable-configuration.md) | Completed |
| 7 | [Verification And Contract Lock](./phase-07-verification-and-contract-lock.md) | In progress |

## Success Criteria

- [ ] Supported selected tools install from one review without the user choosing package-manager commands.
- [x] Existing owners are preserved until an explicit reviewed migration.
- [x] Homebrew and Bun bootstrap use pinned verified artifacts, never remote shell scripts.
- [x] Portable import cannot execute file-supplied commands or expose secrets.
- [ ] UI contract, focused tests, and live Quick Setup verification pass.

## Implementation Start

Start on a `feat/...` branch. Do not implement on dirty `main`.

Begin at Phase 2. Phase 1 is already decided.

<!-- slug: quick-setup-portable-configuration-and-capability-services -->
