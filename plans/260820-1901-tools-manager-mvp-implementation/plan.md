---
title: "STM (Smart Tools Management) MVP Implementation"
description: "Design and lock the complete Mobbin-informed desktop interface first, then implement the independent Tauri MVP for safe tool, global AI Agent Skill, and MCP server inventory plus consent-gated lifecycle operations."
status: in-progress
priority: P1
effort: "31-41 engineer-weeks; re-estimate after Phase 2"
branch: main
tags: [feature, frontend, backend, database, security]
blockedBy: []
blocks: []
created: 2026-08-20
---

# STM (Smart Tools Management) MVP Implementation

## Overview

Build a greenfield, local-first desktop application with three first-class management areas: developer tools, global AI Agent Skills, and MCP servers. Delivery is UI-first: use Mobbin MCP to research relevant product patterns, build and verify the complete fixture-backed React interface, and lock UI Contract v1 before any Rust domain, inventory, storage, or lifecycle logic begins. Tauri 2 later hosts the approved interface; a reusable Rust core implements its locked view-state and action contracts.

Source authority: [market and MVP report v0.5.0](../reports/researcher-2026-08-20-tools-manager-market-and-mvp.md), [approved audit](../reports/reviewer-2026-08-20-tools-manager-report-audit.md), and the project-lead STM/source-URL/MCP revision recorded in Validation Session 2.

## Scope Contract

- **Outcome:** approved and locked STM desktop UI followed by a signed Windows, macOS, and Linux MVP that inventories ten Recommended tools, global Codex/Claude Code/AgentKit-compatible skills, and configured MCP servers; accepts reviewed source URLs; detects updates; and mutates only through approved capabilities after consent.
- **Constraints:** Mobbin is reference-only; no Mobbin proprietary imagery is copied; pasted URLs are untrusted input; Phase 1 performs no network resolution or installation; MCP credentials remain environment or OS-credential references; no backend logic begins before UI Contract v1 approval; later phases conform to the locked UI; no project skill scan, arbitrary shell text, direct vendor binary lifecycle, persistent privileged helper, background daemon, cloud account, or silent ownership change.
- **Non-goals:** implementing logic during the UI prototype phase, copying another product's brand or layout, public CLI, team policy, arbitrary registries, project-local skills, direct unverified URL installation, bundle automation, unmanaged asset mutation, and storing MCP secret values in STM.
- **Acceptance:** the running fixture-backed interface passes visual, responsive, interaction, and accessibility review and is locked; report §2.5 passes; mapping actions are capability-gated; skill writes are receipt-backed and conflict-safe; signed release candidates pass the selected platform matrix.

## Goals

| # | Goal | Priority |
|---|------|----------|
| 1 | Research with Mobbin MCP, build the complete fixture-backed STM UI with reviewed URL intake and MCP management, verify it, and lock UI Contract v1 | P1 |
| 2 | Establish backend contracts and prove platform and client feasibility against the locked UI contract | P1 |
| 3 | Deliver the read-only tool, global-skill, and MCP configuration inventory core behind the approved view states | P1 |
| 4 | Connect the approved desktop UI to real read-only Tauri/Rust behavior without redesign | P1 |
| 5 | Activate safe tool lifecycle through the approved plan, consent, progress, and result flows | P1 |
| 6 | Activate trusted, receipt-backed global skill lifecycle through the approved UI flows | P1 |
| 7 | Activate reviewed MCP add, update, enable, disable, and removal flows without storing raw credentials | P1 |
| 8 | Sign, update, test, and release across the approved platform matrix | P1 |

## Phases

| # | Phase | Status |
|---|-------|--------|
| 1 | [Mobbin-Guided Interface Prototype and UI Contract Lock](./phase-01-mobbin-guided-interface-contract.md) | Completed — UI Contract v1.1 locked |
| 2 | [Foundation Contracts and Feasibility](./phase-02-foundation-contracts-and-feasibility.md) | Completed |
| 3 | [Read-only Core](./phase-03-read-only-core.md) | Completed |
| 4 | [Desktop Read-only Integration](./phase-04-desktop-read-only-integration.md) | Completed |
| 5 | [Safe Tool Lifecycle](./phase-05-safe-tool-lifecycle.md) | In progress — local verification green; platform CI pending |
| 6 | [Trusted Global Skill Lifecycle](./phase-06-trusted-global-skill-lifecycle.md) | Blocked — Phase 5 and trust gate |
| 7 | [MCP Server Lifecycle](./phase-07-mcp-server-lifecycle.md) | Blocked — Phases 5-6 and MCP gate |
| 8 | [Cross-platform Release Hardening](./phase-08-cross-platform-release-hardening.md) | Blocked — Phases 5-7 and release gates |

## Dependencies and Locks

- Phase 1 has no implementation dependency. It is the mandatory design and approval gate.
- Every Phase 2-8 file declares the approved UI contract as a blocking gate. Phase 5-8 now consume locked UI Contract v1.1; transitive phase dependencies do not weaken this explicit lock.
- Phase 2 depends on Phase 1 and adds Tauri/Rust plus supported MCP-client feasibility without changing the approved interface.
- Phase 3 depends on Phase 2; Phase 4 depends on Phase 3 and replaces fixture IPC with real read-only IPC for tools, skills, and MCP configurations.
- Phase 5 depends on Phases 3 and 4. Phase 6 depends on Phases 4 and 5 so it reuses the implemented immutable planning/consent substrate rather than duplicating it.
- Phase 6 is additionally gated by the trusted skill catalog publisher, review, signing/authentication, and update mechanism decision.
- Phase 7 depends on Phases 4, 5, and 6; MCP changes reuse the same immutable plan, consent, receipt, partial-failure, and recovery substrate while preserving client-specific configuration semantics.
- Phase 8 depends on Phases 4, 5, 6, and 7 and is gated by the supported OS/architecture matrix plus signing/notarization credentials.
- Intentional UI changes reopen Phase 1, re-lock every affected later phase, require a running-UI verification and project-lead approval, bump the UI contract version, and propagate the decision through the plan before implementation resumes.
- No overlapping unfinished project plan was found.
- Official technical references: [Tauri project setup](https://v2.tauri.app/start/create-project/), [Tauri command scopes](https://v2.tauri.app/security/scope/), [Tauri testing](https://v2.tauri.app/develop/tests/), [Tauri updater](https://v2.tauri.app/plugin/updater/).

## Success Criteria

- [x] `ak plan validate plans/260820-1901-tools-manager-mvp-implementation` passes for the complete plan directory.
- [x] Mobbin MCP references are recorded as canonical links and adaptation notes; proprietary Mobbin images and branded assets are not copied into the product.
- [x] The complete fixture-backed UI was verified in a running browser, approved by the project lead, locked as UI Contract v1.0 before Phase 2, and reapproved and relocked as UI Contract v1.1 before Phase 5.
- [ ] Every later phase consumes the locked routes, view states, actions, copy, tokens, fixtures, interactions, responsive rules, accessibility behavior, and visual baselines.
- [x] Read-only inventory ships before any mutation path is enabled.
- [x] Every mutation checks mapping status, execution mode, detected owner, immutable plan, and explicit consent.
- [x] Global skill scanning never traverses project roots and deduplicates physical roots.
- [x] Product self-update remains separate from tool and skill lifecycle.
- [x] No unresolved contract contradiction remains before implementation handoff.

## Scope Challenge

- Existing code: none; only approved reports and plan artifacts.
- Minimum: a UI-first approval gate plus six implementation slices is justified by the user's interface-first decision and the platform, privilege, supply-chain, and release boundaries. Enhancements remain deferred.
- Complexity: one fixture-backed React interface and UI contract, one Tauri shell, one reusable Rust core crate, schema/catalog data, fixtures, and platform CI.
- Selected mode: HOLD SCOPE. Preserve the complete audited MVP direction while changing delivery order so UI is approved and locked before logic.

## Open Questions

1. Trusted skill catalog publisher/review/authentication mechanism must resolve before Phase 6 implementation.
2. Supported MCP client configuration schemas, trust policy, and credential-reference mechanism must resolve in Phase 2 before Phase 7 implementation.
3. Supported OS versions and CPU architectures must resolve in Phase 2 before the Phase 8 release matrix is frozen.

## Current State

- Phase 1 is complete for UI Contract v1.1. The approved visual system and seven routes remain stable; trusted plan/result, opaque plan identity, full evidence-bound consent, independent bulk child plans, operation-ID progress/cancel, per-item receipt/redaction, and fresh-plan retry/recovery are represented across lifecycle surfaces.
- Manifest `1.1.0` records project-lead approval and is locked against 83 artifacts. The lifecycle viewport matrix is present and verified at 1024x720, 1280x800, and 1440x900.
- Phase 2 is complete: the pinned Rust/Tauri workspace, typed contracts, safe process supervisor, SQLite migration, fixture-backed manager/skill/MCP feasibility adapters, deny-by-default host capabilities, architecture records, and CI gates are implemented. The live macOS Tauri shell renders UI Contract v1; all frontend and Rust quality gates pass. Phase 3 may begin.
- Phase 3 is complete: ten Recommended and forty Candidate catalog entries, five manager families, compiled tool probes, owner reconciliation, bounded global skill scanning, redacted MCP discovery, transactional SQLite snapshots, authority-scoped update detection, deterministic headless scan events, and locked read models are implemented. Phase 3 verification, all 28 Rust tests, and all frontend quality gates pass; Phase 4 may begin.
- Phase 4 is complete: the approved React surface selects a typed Tauri runtime client in desktop mode, binds Rust refresh/cancellation/diagnostics/source-analysis behavior, polls and subscribes to progress events, preserves last-good snapshots, retains deterministic browser fixtures, and exposes no lifecycle mutation command to the webview. UI Contract v1, renderer integration tests, frontend/Rust gates, release compilation, native macOS launch, and persisted snapshot checks passed. Phase 5 may begin against locked UI Contract v1.1.
- Phase 5 implementation is locally complete: bounded source analysis, immutable planning and consent, deny-by-default mapping policy, reviewed manager commands, vendor handoff, privilege boundaries, process supervision, durable receipts/recovery, typed desktop IPC, and locked UI bindings are implemented. The full local Rust/frontend/desktop/build gates and runtime post-verification pass; independent correctness and security reviews are READY. Phase 5 remains in progress until the dedicated Windows, macOS, Ubuntu, Fedora, Arch, and non-root lifecycle jobs pass in GitHub Actions.

## Validation Log

### Session 1 — 2026-08-20

**Trigger:** Project lead required a Mobbin-informed, interface-first delivery order and a lock preventing later phases from reshaping the approved UI.

**Questions asked:** 0. The instruction was explicit.

#### Confirmed Decisions

- Use Mobbin MCP `search_screens` and `search_flows` as reference research before authoring the interface.
- Build the complete fixture-backed React UI before implementing Rust/Tauri domain, inventory, storage, or lifecycle logic.
- Verify the running UI and require project-lead approval before later phases begin.
- Lock every later phase to UI Contract v1. Backend implementation adapts to the approved interface rather than redesigning it.
- Allow plan and UI changes only through an explicit reopen, re-verification, approval, contract-version bump, and cross-phase propagation process.

#### Impact on Phases

- Added Phase 1 for Mobbin research, complete UI prototyping, verification, approval, and contract lock.
- Renumbered the former six phases to Phases 2-7.
- Expanded Phase 2 with missing manager and skill-adapter feasibility spikes.
- Converted Phase 4 from UI construction to integration of the approved UI with real read-only IPC.
- Made Phase 6 depend on Phase 5 to reuse the implemented planning/consent substrate.
- Added the explicit UI lock and reopen procedure to every later phase.

#### Verification Results

- **Tier:** Full
- **Claims checked:** 53
- **Verified:** 53 | **Failed:** 0 | **Unverified:** 0
- `ak plan validate` passed for the complete plan directory.
- `ak plan parse` resolved seven ordered phases and 173 checklist items.
- All 39 local plan links and anchors resolve.
- All dependency references resolve to earlier phases.
- Every Phase 2-7 file carries the explicit UI Contract v1 blocking gate.
- External trust, platform-matrix, and signing decisions remain intentional phase gates rather than structural validation failures.

#### Whole-Plan Consistency Sweep

- Files reread: `plan.md`, all seven `phase-*.md` files, source report v0.4.1, and audit v0.2.1.
- Decision deltas checked: 5 — UI-first delivery, phase renumbering, missing feasibility spikes, shared planning/consent dependency, and source-authority status/version.
- Reconciled stale file names, phase references, report versions, dependencies, UI ownership, and implementation boundaries.
- Stale references found: 0.
- Unresolved contradictions: 0.
- Recommendation: Phase 1 may start. Phases 2-7 remain locked until UI Contract v1 is approved and verified.


### Session 2 — 2026-08-20

**Trigger:** Project lead requested STM naming, reviewed tool/skill link-installation flows, and a persistent MCP management surface before UI Contract v1 approval.

**Questions asked:** 1. MCP belongs in a persistent primary-management surface; source-link installation uses pasted HTTPS URLs.

#### Confirmed Decisions

- Rename the application to STM and retain Smart Tools Management as the expanded name.
- Add reviewed HTTPS source-URL intake to Tools and Skills; no install or configuration preview can proceed without deterministic analysis and fresh consent.
- Add MCP Servers as a seventh primary route with inventory, client bindings, transport, capabilities, auth-reference, trust, health, add/configuration review, consent, result, denial, and removal states.
- Keep the interface fixture-backed and non-mutating until project-lead approval and UI Contract v1 lock.

#### Implementation Evidence

- Mobbin MCP research added source-link installation and MCP inventory/setup references to the canonical board.
- The running interface exposes Dashboard, Tools, Skills, MCP Servers, Updates, Operation History, and Settings at 1024x720, 1280x800, and 1440x900 without horizontal overflow.
- Browser checks exercised tool and skill source review, invalid embedded-credential denial, MCP add/configure/enable/disable/removal review, fresh consent gates, product recovery consent, blocked actions, route focus, and reduced-motion behavior.
- `pnpm verify:ui-contract` passed with manifest status `review` and 70 artifacts.
- `pnpm typecheck`, all eight focused contract tests, and `pnpm build` passed.
- Eleven revised PNG baselines are generated under `assets/designs/tools-manager-ui/baselines/`.
- `ak plan validate` passed; `ak plan parse` resolved eight ordered phases and 220 checklist items; all 46 local plan links resolve.
- Focused code re-review confirmed query/fragment credential redaction and scoped MCP add/configure/enable/disable/removal previews; no Critical or Important finding remains.

#### Approval and Lock

- Project lead approved the revised STM running interface and eleven baselines.
- `contracts/ui/ui-contract.manifest.json` is locked at version `1.0.0` with approval timestamp `2026-08-20T18:32:28Z`.
- `pnpm lock:ui-contract` generated SHA-256 digests for 71 artifacts; `pnpm verify:ui-contract` passes in locked mode.
- Phase 1 is complete and Phase 2 may begin against UI Contract v1.

### Session 3 — 2026-08-21

**Trigger:** Execute Phase 5 autonomously, run complete verification, update plan status, and continue only after the phase passes.

**Questions asked:** 0. The implementation contract and UI lock were already approved.

#### Implementation Evidence

- Added typed source analysis and immutable lifecycle plan/consent/revalidation services; reviewed manager command vectors remain compiled code rather than catalog input.
- Added managed WinGet, Homebrew formula/cask, npm, APT, and DNF command paths; Pacman install/update remains detect-only and only package-scoped uninstall is executable.
- Added vendor-handoff separation, privilege fail-closed behavior, exact environment boundaries, cancellation, manager exclusion, opaque operation IDs, pre-spawn durable receipts, child-process tracking, restart reconciliation, redacted history, retry/recovery unioning, and source reanalysis routing.
- Added Tauri single-instance enforcement and serialized snapshot/postcondition merges to prevent competing mutation or scan overwrite.
- Added the dedicated lifecycle platform workflow for Windows, macOS, Ubuntu, Fedora, Arch, and non-root privilege denial.

#### Verification Results

- `cargo test --workspace --all-targets`: 60 passed; three disposable-platform tests intentionally ignored outside their dedicated jobs.
- `pnpm verify:phase-two-foundation`, `pnpm verify:phase-three-core`, `pnpm verify:phase-four-desktop`, `pnpm test:desktop-integration`, `pnpm test`, `pnpm typecheck`, `pnpm lint`, UI Contract v1.1 verification, production web build, and packaged Tauri release build passed.
- Runtime post-verification passed recovery-plan consent review, credential-bearing source rejection/redaction, and packaged single-instance behavior.
- Independent correctness and security reviewers returned READY; no evidence-backed P1/P2 remains.
- Phase 5 remains `in-progress`: disposable platform jobs require GitHub Actions execution. Phase 6 remains blocked by Phase 5 and the unresolved trusted skill catalog publisher/review/authentication gate.

<!-- slug: tools-manager-mvp-implementation -->
