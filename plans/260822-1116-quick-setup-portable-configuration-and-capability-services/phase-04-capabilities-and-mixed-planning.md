---
phase: 4
title: "Capabilities And Mixed Planning"
status: completed
priority: P1
effort: "4-5d"
dependencies: [3]
---

# Phase 4: Capabilities And Mixed Planning

## Overview

Make Installer, Updater, and Validator real. Mixed desired-state plans can contain install, update, handoff, guidance, no-op, and blocked children. Bootstrap Homebrew and Bun through typed recipes.
- Functional: mixed requests use typed child intents `{ resourceKind, resourceId, desiredAction, mappingId? }`.
- Functional: batch execution is a dependency DAG. Dependents stay blocked until required parents verify.
- Functional: missing Homebrew uses official `.pkg` recipe, not a shell installer. Bootstrap failure is Failed with no receipt.
- Functional: missing Bun uses a pinned official binary at a reviewed absolute path only when a selected recipe requires Bun.
- Functional: `managed_current` presentation and dialog dispatch are not Update.
- Non-functional: revalidation, manager and path locks, in-progress receipts, per-child journals, and postconditions remain mandatory. Authorization expiry is checked before every mutating child.

## Architecture

Application use case expands selection → Validator reports → Installer/Updater create child intents → Lifecycle coordinator builds one batch plan.

Installer owns prerequisite expansion. Coordinator owns execution.

Add recipe adapters and executor variants for step types needed by first supported defaults: manager-package, pkg-installer, signed-artifact/archive-binary, vendor-handoff, rescan, verify-postcondition. Route each variant through the execution port. Archive extraction is bounded and symlink-free.

Do not invent Optimizer proposal types. Keep an empty module only.

Fix presentation mapping in Rust DTO, frontend fixture, action-contract union, tool detail, and tool dialog. A managed-current click never emits `action: "update"`.

- Modify: `crates/stm-core/src/lifecycle/planner.rs`
- Modify: `crates/stm-core/src/lifecycle/service.rs`
- Modify: `crates/stm-core/src/lifecycle/command.rs`
- Modify: `crates/stm-core/src/application/dto.rs`
- Modify: `src/fixtures/presentation-action-fixtures.ts`
- Create: capability service implementations
- Create: Homebrew pkg and Bun binary adapters in runtime

1. Replace update-queue-only batch matching with typed desired-state child intents. Migrate the existing update-queue producer.
2. Implement Validator reports for support, owner, provider, dependencies, and blocked reasons.
3. Implement Installer child-plan preparation including prerequisite graph minimization and dependency edges.
4. Implement Homebrew `.pkg` bootstrap with pinned origin, digest, signer Team ID, Failed-if-absent postcondition, and staged dependent compilation after success.
5. Implement Bun official-binary adapter using reviewed absolute identities and supervisor PATH allowlist.
6. Persist a per-child journal after each postcondition. Check plan expiry before each mutating child.
7. Keep vendor handoff and detect-only as first-class child results.
8. Fix current-tool action labels and both dialog consumers.
9. Add unit tests for mixed batch, existing-owner preservation, bootstrap failure isolation, and gated dependents.

- [x] Mixed desired-state planner
- [x] Validator reports
- [x] Homebrew pkg bootstrap
- [x] Bun binary bootstrap
- [x] Fix current-state Update label
- [x] Independent child results

## Success Criteria

- [x] Clean-macOS plan for AgentKit + Codex + Orca creates Homebrew prerequisite, Homebrew installs, and Orca cask/handoff without adding Node.js solely for Codex.
- [x] npm-owned Codex does not migrate during install.
- [x] Failed Homebrew bootstrap blocks only brew-dependent children.
- [x] Current tools no longer show Preview Managed Update.

## Risk Assessment

Native `.pkg` install requires an OS authorization prompt. Model it as an unavoidable user step, not a silent managed command.
