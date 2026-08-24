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
| 5 | [Safe Tool Lifecycle](./phase-05-safe-tool-lifecycle.md) | Completed |
| 6 | [Trusted Global Skill Lifecycle](./phase-06-trusted-global-skill-lifecycle.md) | Completed |
| 7 | [MCP Server Lifecycle](./phase-07-mcp-server-lifecycle.md) | Completed |
| 8 | [Cross-platform Release Hardening](./phase-08-cross-platform-release-hardening.md) | Blocked — protected signing/notarization credentials and signed fresh-machine evidence |

## Dependencies and Locks

- Phase 1 has no implementation dependency. It is the mandatory design and approval gate.
- Every Phase 2-8 file declares the approved UI contract as a blocking gate. Phase 5-8 now consume locked UI Contract v1.1; transitive phase dependencies do not weaken this explicit lock.
- Phase 2 depends on Phase 1 and adds Tauri/Rust plus supported MCP-client feasibility without changing the approved interface.
- Phase 3 depends on Phase 2; Phase 4 depends on Phase 3 and replaces fixture IPC with real read-only IPC for tools, skills, and MCP configurations.
- Phase 5 depends on Phases 3 and 4. Phase 6 depends on Phases 4 and 5 so it reuses the implemented immutable planning/consent substrate rather than duplicating it.
- Phase 6 trust gate is approved: public `itsddvn/stm-skill-catalog` distribution, project-lead review, exact-byte detached Ed25519 signatures, compiled trust roots, monotonic expiry-bound snapshots, fixed HTTPS updates, bundled fallback, and app-release key rotation.
- Phase 7 depends on Phases 4, 5, and 6; MCP changes reuse the same immutable plan, consent, receipt, partial-failure, and recovery substrate while preserving client-specific configuration semantics.
- Phase 8 depends on completed Phases 4-7. Its stable matrix is frozen; public release readiness remains gated by protected signing/notarization credentials plus signed fresh-machine and previous-version upgrade evidence.
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

1. Protected updater, Apple signing/notarization, and Windows signing credentials plus disposable stable-matrix machines are required to complete the signed release gate.

## Current State

- Phase 1 is complete for UI Contract v1.1. The approved visual system and seven routes remain stable; trusted plan/result, opaque plan identity, full evidence-bound consent, independent bulk child plans, operation-ID progress/cancel, per-item receipt/redaction, and fresh-plan retry/recovery are represented across lifecycle surfaces.
- Manifest `1.1.0` records project-lead approval and is locked against 83 artifacts. The lifecycle viewport matrix is present and verified at 1024x720, 1280x800, and 1440x900.
- Phase 2 is complete: the pinned Rust/Tauri workspace, typed contracts, safe process supervisor, SQLite migration, fixture-backed manager/skill/MCP feasibility adapters, deny-by-default host capabilities, architecture records, and CI gates are implemented. The live macOS Tauri shell renders UI Contract v1; all frontend and Rust quality gates pass. Phase 3 may begin.
- Phase 3 is complete: ten Recommended and forty Candidate catalog entries, five manager families, compiled tool probes, owner reconciliation, bounded global skill scanning, redacted MCP discovery, transactional SQLite snapshots, authority-scoped update detection, deterministic headless scan events, and locked read models are implemented. Phase 3 verification, all 28 Rust tests, and all frontend quality gates pass; Phase 4 may begin.
- Phase 4 is complete: the approved React surface selects a typed Tauri runtime client in desktop mode, binds Rust refresh/cancellation/diagnostics/source-analysis behavior, polls and subscribes to progress events, preserves last-good snapshots, retains deterministic browser fixtures, and exposes no lifecycle mutation command to the webview. UI Contract v1, renderer integration tests, frontend/Rust gates, release compilation, native macOS launch, and persisted snapshot checks passed. Phase 5 may begin against locked UI Contract v1.1.
- Phase 5 is complete: bounded source analysis, immutable planning and consent, deny-by-default mapping policy, reviewed manager commands, vendor handoff, privilege boundaries, process supervision, durable receipts/recovery, typed desktop IPC, and locked UI bindings are implemented. Local Rust/frontend/desktop/build gates, runtime post-verification, and independent correctness/security reviews passed. GitHub Actions [Lifecycle Platform Contracts run 32440872905](https://github.com/itsddvn/stm/actions/runs/32440872905), [Quality run 32440872961](https://github.com/itsddvn/stm/actions/runs/32440872961), and [UI Contract run 32440872906](https://github.com/itsddvn/stm/actions/runs/32440872906) are green with Node 24-native action runtimes. Phase 6 may begin against the approved trust gate.
- Phase 6 is complete: authenticated exact-byte catalog activation, pinned Ed25519 trust roots, immutable Git resolution, staged tree validation, receipt-backed global materialization, conflict detection, partial failure, rollback, typed desktop IPC, cross-platform tests, and locked UI binding are implemented.
- Phase 7 is complete: bounded Codex/Claude Code/Cursor discovery, client-specific normalization, approved stdio and remote mappings, capability/auth-reference binding, direct immutable UI plans, explicit consent, cross-process config locking, atomic mutation, protocol initialization health, encrypted OS-keyring-backed backups, partial outcomes, keep-partial, and digest-safe rollback/recovery are implemented. Rust/frontend/catalog/UI-contract/release gates and isolated runtime disable/rollback verification pass.
- Phase 8 local hardening is implemented: four-target stable/two-target experimental matrix, release-only CSP and updater config injection, typed consent-bound product updater with separate durable receipts, protected signed-draft workflow, dependency/security/secret/SBOM/CodeQL/provenance gates, streaming artifact verification, behavioral tooling checks, support/security/deployment docs, and internal packaged launch. Public release remains blocked on protected credentials and signed cross-platform install/upgrade evidence.

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

- Workspace Rust tests, frontend/desktop contracts, UI Contract v1.1 verification, production web build, and packaged Tauri release build passed locally.
- Runtime post-verification passed recovery-plan consent review, credential-bearing source rejection/redaction, and packaged single-instance behavior.
- Independent correctness and security reviewers returned READY; no evidence-backed P1/P2 remains.
- GitHub Actions [Lifecycle Platform Contracts run 32440872905](https://github.com/itsddvn/stm/actions/runs/32440872905) passed all Windows, macOS, Ubuntu, Fedora, Arch, non-root, and cross-platform core jobs. [Quality run 32440872961](https://github.com/itsddvn/stm/actions/runs/32440872961) and [UI Contract run 32440872906](https://github.com/itsddvn/stm/actions/runs/32440872906) also passed after action runtimes moved to Node 24.
- Phase 5 is complete. Session 4 resolves the trusted skill catalog gate and starts Phase 6.


### Session 4 — 2026-08-21

**Trigger:** Continue autonomously after Phase 5 passes and resolve the trusted skill catalog gate without changing UI Contract v1.1.

**Questions asked:** 0. The project lead explicitly delegated implementation decisions needed to continue.

#### Confirmed Decisions

- Publish catalog metadata from a dedicated public `itsddvn/stm-skill-catalog` repository; the private application repository remains the verifier and schema authority.
- Require project-lead review plus automated schema, provenance, digest, path, and duplicate-identity checks before stable publication.
- Authenticate exact manifest bytes with detached Ed25519 signatures and application-pinned trust roots; remote content cannot add keys.
- Bind monotonic version, channel, creation/expiry, payload SHA-256, and byte length; reject downgrade, expiry, same-version drift, unknown/revoked keys, and invalid signatures.
- Fetch only fixed bounded HTTPS paths, retain bundled/last-known-good snapshots, rotate keys through signed application releases, and defer private Git credentials and non-GitHub hosts.

#### Impact on Phases

- Phase 6 was unblocked to implement the authenticated catalog, immutable public Git resolution, staged validation, receipt-backed materialization, and approved locked UI bindings.
- Phase 7 retained the shared lifecycle gates and remained dependent on completed Phase 6 behavior.


### Session 5 — 2026-08-21

**Trigger:** Continue autonomously through Phase 6 and Phase 7, then review every Stage 1 acceptance gap before advancing.

**Questions asked:** 0. The project lead explicitly required continuous implementation and allowed plan corrections needed for readiness.

#### Completed Decisions

- Phase 6 consumes only authenticated, monotonic, expiry-bound skill catalogs and immutable Git/tree provenance before materialization.
- Phase 7 supports Codex, Claude Code, and Cursor MCP bindings with approved mapping metadata, unavailable-reference fail-closed behavior, and no generic shell or webview filesystem authority.
- MCP config backups use authenticated XChaCha20-Poly1305 envelopes; the key is held by macOS Keychain, Windows Credential Manager, or Linux Secret Service.
- Existing MCP configure/enable/disable/remove actions open direct immutable plans. Only add or source-changing flows analyze a source URL; the approved route, action, copy, and evidence hierarchy remain unchanged.
- Persistent file locks plus digest checks serialize overlapping config writers; recovery never overwrites a target that differs from the reviewed replacement.

#### Verification Results

- Rust formatting, clippy with warnings denied, 96 core tests, MCP-focused regression tests, frontend lint/typecheck, 16 UI tests, approved MCP catalog verification, UI Contract v1.1 lock verification, production web build, and release Tauri build pass.
- Fixture-browser runtime proves direct MCP plan review, consent gating, execution progress, refresh, and terminal success.
- Isolated real-service runtime proves encrypted backup creation with plaintext exclusion and exact receipt-backed rollback.
- Review status is recorded in the Phase 7 completion evidence and post-verification report.

#### Phase Impact

- Phases 6 and 7 are complete.
- Phase 8 matrix and local release hardening are implemented; protected signing/notarization credentials and signed fresh-machine evidence remain external blockers.


### Session 6 — 2026-08-21

**Trigger:** Continue all reachable release hardening after Phase 7 Stage 1 passed, without making an unsigned public-release claim.

**Questions asked:** 0. Repository evidence supported a conservative stable matrix; missing credentials are an external gate, not an implementation choice.

#### Completed Decisions

- Stable releases target macOS arm64/x64, Windows x64, and Ubuntu/glibc x64. Windows and Linux ARM64 remain experimental until signed native-webview and lifecycle smoke exists.
- Internal builds keep release bundles/updater artifacts disabled and omit the updater plugin. Protected release builds inject the reviewed public key into a mode-0600 generated config and fail when required credentials are absent.
- Product update planning, consent, metadata revalidation, signed download/install, status, and durable receipts remain typed Rust-host behavior separate from tool, skill, and MCP state.
- Signed workflow output remains a draft. Updater metadata is promoted last, only after artifact and fresh-machine gates.

#### Local Verification

- Release contract/tooling/secret checks, UI Contract v1.1, frontend lint/typecheck and 19 tests, Rust format/clippy and 103 tests, internal Tauri release build, generated release-config build, workflow YAML parsing, dependency audit, and packaged launch pass.
- Fixture UI verifies an exact signed product plan and an independent recoverable terminal boundary.
- Artifact verifier accepts a valid signed fixture and rejects wrong-version metadata; release config generation rejects a missing updater key and does not print the injected key.
- Focused Phase 8 correctness and security re-reviews pass after exact provenance, step-scoped/pinned signing, signed metadata/artifact verification, single-use consent, Windows restart reconciliation, and product History dispatch fixes.

#### External Blockers

- Protected updater private key/password and reviewed updater public key.
- Apple signing certificate, identity, account app password, team ID, and notarization access.
- Windows signing PFX/password/thumbprint.
- Signed draft install/update/recovery and critical-screen evidence on every stable matrix target.

<!-- slug: tools-manager-mvp-implementation -->
