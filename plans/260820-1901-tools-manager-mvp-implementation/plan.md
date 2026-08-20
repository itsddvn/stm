---
title: "Tools Manager MVP Implementation"
description: "Build the independent Tauri desktop MVP for safe developer-tool and global AI Agent Skill inventory, update detection, and consent-gated lifecycle operations."
status: pending
priority: P1
effort: "24-32 engineer-weeks; re-estimate after Phase 1"
branch: main
tags: [feature, frontend, backend, database, security]
blockedBy: []
blocks: []
created: 2026-08-20
---

# Tools Manager MVP Implementation

## Overview

Build a greenfield, local-first desktop application with two surfaces: Tools Manager and Skills Manager. Tauri 2 hosts a React/TypeScript UI; a reusable Rust core owns catalog, inventory, trust, planning, receipts, and adapters. Delivery starts read-only, then promotes each platform mapping independently to managed execution or vendor handoff.

Source authority: [market and MVP report v0.4.0](../reports/researcher-2026-08-20-tools-manager-market-and-mvp.md) and [approved audit](../reports/reviewer-2026-08-20-tools-manager-report-audit.md).

## Scope Contract

- **Outcome:** signed Windows, macOS, and Linux desktop MVP that inventories ten Recommended tools and global Codex/Claude Code/AgentKit-compatible skills, detects updates, and mutates only through approved mapping capabilities after consent.
- **Constraints:** no project skill scan, arbitrary shell text, direct vendor binary lifecycle, persistent privileged helper, background daemon, cloud account, or silent ownership change.
- **Non-goals:** public CLI, team policy, arbitrary registries, project-local skills, direct GitHub release installation, bundle automation, and unmanaged asset mutation.
- **Acceptance:** report §2.5 passes; mapping actions are capability-gated; skill writes are receipt-backed and conflict-safe; signed release candidates pass the selected platform matrix.

## Goals

| # | Goal | Priority |
|---|------|----------|
| 1 | Establish contracts and prove platform feasibility before product code expands | P1 |
| 2 | Deliver read-only tool and global-skill inventory as the first stable core | P1 |
| 3 | Expose the read-only vertical slice through an accessible desktop UI | P1 |
| 4 | Promote safe tool mappings to managed execution or vendor handoff | P1 |
| 5 | Add trusted, receipt-backed global skill lifecycle | P1 |
| 6 | Sign, update, test, and release across the approved platform matrix | P1 |

## Phases

| # | Phase | Status |
|---|-------|--------|
| 1 | [Foundation Contracts and Feasibility](./phase-01-start.md) | Pending |
| 2 | [Read-only Core](./phase-02-read-only-core.md) | Pending |
| 3 | [Desktop Read-only Vertical Slice](./phase-03-desktop-read-only-vertical-slice.md) | Pending |
| 4 | [Safe Tool Lifecycle](./phase-04-safe-tool-lifecycle.md) | Pending |
| 5 | [Trusted Global Skill Lifecycle](./phase-05-trusted-global-skill-lifecycle.md) | Pending |
| 6 | [Cross-platform Release Hardening](./phase-06-cross-platform-release-hardening.md) | Pending |

## Dependencies

- No overlapping unfinished project plan found.
- Phase 2 depends on Phase 1 contracts; every later phase depends on the stable read-only core.
- Phase 5 is gated by trusted skill catalog publisher/review/authentication decision.
- Phase 6 is gated by supported OS version/CPU architecture decision and signing credentials.
- Official technical references: [Tauri project setup](https://v2.tauri.app/start/create-project/), [Tauri command scopes](https://v2.tauri.app/security/scope/), [Tauri testing](https://v2.tauri.app/develop/tests/), [Tauri updater](https://v2.tauri.app/plugin/updater/).

## Success Criteria

- [ ] All six phase files pass `ak plan validate` and expose measurable exit gates.
- [ ] Read-only inventory ships before any mutation path is enabled.
- [ ] Every mutation checks mapping status, execution mode, detected owner, immutable plan, and explicit consent.
- [ ] Global skill scanning never traverses project roots and deduplicates physical roots.
- [ ] Product self-update remains separate from tool and skill lifecycle.
- [ ] No unresolved contract contradiction remains before implementation handoff.

## Scope Challenge

- Existing code: none; only approved reports and plan artifacts.
- Minimum: six gated slices are justified by platform, privilege, supply-chain, and release boundaries; enhancements remain deferred.
- Complexity: one Tauri shell, one reusable Rust core crate, one React application, schema/catalog data, fixtures, and platform CI.
- Selected mode: HOLD SCOPE, inferred from approval to apply the complete audited MVP direction.

## Open Questions

1. Trusted skill catalog publisher/review/authentication mechanism—must resolve before Phase 5 implementation.
2. Supported OS versions and CPU architectures—must resolve in Phase 1 before Phase 6 matrix is frozen.
<!-- slug: tools-manager-mvp-implementation -->
