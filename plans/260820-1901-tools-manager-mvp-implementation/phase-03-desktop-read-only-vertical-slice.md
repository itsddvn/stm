---
phase: 3
title: "Phase 3: Desktop Read Only Vertical Slice"
status: todo
priority: P1
effort: "3-4 engineer-weeks"
dependencies: [2]
---

# Phase 3: Desktop Read Only Vertical Slice

## Context Links

- [Plan overview](./plan.md)
- [Read-only core](./phase-02-read-only-core.md)
- [Desktop UX contract](../reports/researcher-2026-08-20-tools-manager-market-and-mvp.md#10-desktop-ux)
- [Tauri testing](https://v2.tauri.app/develop/tests/)

## Overview

Deliver the first demonstrable product: a packaged development desktop app that scans, reconciles, filters, and explains tools and global skills without mutation. UI consumes stable application-service DTOs and never duplicates ownership or policy logic.

## Key Insights

- Recommendation is curation; platform support and lifecycle confidence must be visible separately.
- Update visibility is useful before mutation exists.
- Error, unsupported, manager-missing, stale, and partial-scan states are primary UI states, not edge-only dialogs.

## Requirements

- [ ] Provide Dashboard, Tools, Skills, Updates, Operation History, and Settings navigation.
- [ ] Show the ten Recommended tools with group filters, tool kind, platform mapping, owner, installed/available version, execution mode, and confidence.
- [ ] Show one canonical skill with all logical client targets and one physical-install representation where roots overlap.
- [ ] Support manual refresh, session auto-check, progress, cancellation, freshness, diagnostics, and retry.
- [ ] Keep every update item unselected by default; mutation controls are disabled until later phases authorize them.
- [ ] Meet keyboard navigation, focus, contrast, reduced-motion, screen-reader labels, and responsive minimum-window requirements.

## Architecture

- React feature modules own presentation and local interaction only.
- One typed IPC client translates Tauri commands/events into query state.
- Rust application service returns display-ready state enums and reason codes; UI maps codes to copy.
- Renderer tests mock IPC at the client boundary. Desktop E2E uses the Tauri-recommended WebdriverIO path where feasible.

## Related Code Files

- Create: `/Users/itsddvn/projects/tools-managers/src/app/`, `/Users/itsddvn/projects/tools-managers/src/components/`, `/Users/itsddvn/projects/tools-managers/src/lib/ipc/`
- Create: `/Users/itsddvn/projects/tools-managers/src/features/dashboard/`, `tools/`, `skills/`, `updates/`, `history/`, `settings/`
- Create: `/Users/itsddvn/projects/tools-managers/src/styles/` and `/Users/itsddvn/projects/tools-managers/src/test/`
- Create: `/Users/itsddvn/projects/tools-managers/e2e-tests/`
- Modify: `/Users/itsddvn/projects/tools-managers/src-tauri/src/commands/` and `/Users/itsddvn/projects/tools-managers/crates/tools-manager-core/src/application/`
- Modify: `/Users/itsddvn/projects/tools-managers/package.json`, `/Users/itsddvn/projects/tools-managers/vite.config.ts`

## Implementation Steps

1. Define TypeScript DTO types generated from or contract-tested against Rust serialization; reject hand-maintained drift.
2. Build application shell, navigation, global scan status, error boundary, notifications, and diagnostic access.
3. Build Dashboard summary with counts by inventory state, update availability, manager/client health, and freshness.
4. Build Tools list/detail with search, multiple group filters, kind, state, owner, support, recommendation, and alternatives warning for Docker Desktop versus OrbStack.
5. Build Skills list/detail with source/revision, targets, physical-root deduplication, digest/modification state, scripts/assets/symlink flags, and compatibility.
6. Build Updates queue as read-only metadata: current/target, source authority, execution mode, disabled reason, last checked, and explicit default-unselected state.
7. Build Settings for enabled read adapters, approved global roots, refresh behavior, catalog channel, and diagnostics; prevent project-local root selection.
8. Add refresh orchestration with progress events, cancellation, stale-result rejection, and last-good snapshot presentation.
9. Add component, accessibility, IPC contract, renderer integration, and desktop smoke tests.
10. Package unsigned development builds on the primary OS to verify real webview, paths, dialogs, and process behavior.

## Todo

- [ ] All six navigation surfaces render empty, loading, success, partial, stale, and failure states.
- [ ] Tool/skill filters are deterministic and preserve multi-group membership.
- [ ] Recommended status is not presented as installability.
- [ ] No read-only UI control can create or execute an operation plan.
- [ ] Keyboard-only and screen-reader smoke passes critical browse/detail flows.
- [ ] Renderer and Tauri smoke suites run in CI for the supported host subset.

## Success Criteria

- [ ] User can launch app, scan, inspect all ten Recommended tools, inspect global skills, view updates, cancel refresh, and export redacted diagnostics.
- [ ] UI correctly explains `detect_only`, `handoff_only`, `unsupported`, `blocked`, manager unavailable, system-owned, external, and unknown states.
- [ ] Overlapping client roots display one physical installation with multiple client bindings.
- [ ] Frontend contains no package ownership, version comparison, privilege, or trust decision logic.

## Risk Assessment

- **DTO drift:** generate bindings or enforce serialization fixtures in both languages.
- **Platform UI differences:** exercise real packaged dev builds, not browser tests alone.
- **Too much UI before lifecycle proof:** keep actions disabled and reuse read models for later phases.

## Security Considerations

- Tauri capabilities allow only named application commands; no generic shell, filesystem, or SQL access from webview.
- Diagnostics redact home paths, usernames, command output secrets, environment contents, and tokens.
- External links require an allowlisted scheme and explicit opener behavior.

## Next Steps

Use the stable read-only UI to expose operation previews in Phase 4 without changing inventory ownership semantics.
