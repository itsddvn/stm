---
title: Quick Setup capability delivery
date: 2026-08-23 17:14
component: quick-setup, capability services, portable configuration
status: ongoing
history_only: true
agentwiki_publish: skipped
---

# Quick Setup capability delivery

## Context

Session history for Quick Setup, capability services, and portable configuration. Current authority remains the active plan, evergreen docs, schemas, and source.

## What happened

- Completed pinned Bun bootstrap and independent Homebrew/Bun prerequisites. Centralized Bun version, release asset, archive digest, and entry metadata.
- Added exact target, binary digest, version postcondition, restricted process environment, and fail-closed executor verification.
- Completed typed child dependency ordering, cycle/missing-edge rejection, dependency-bound consent, and failure isolation.
- Added typed catalog recipe steps and adapter-step compatibility validation.
- Moved SQLite, bounded HTTP, executable discovery, process execution, manager evidence, vendor handoff, and OS liveness from core into runtime. Desktop now composes one shared lifecycle/storage graph.
- Renamed the architecture crates to canonical `stm-core` and `stm-runtime` names across directories, Cargo packages/imports, CI, scripts, docs, plans, and reports. No legacy alias or tracked reference remains.
- Replaced native fixture-derived tool actions with live manager/executable evidence, moved the desktop database to user data, accepted canonical Homebrew roots under the same-UID boundary, and verified current-machine install/update plans without applying unsolicited mutations.
- Added typed Vietnamese/English localization with Vietnamese default, persistent switching, concise lifecycle summaries, localized native confirmation, and collapsed technical evidence.
- Kept missing npm/Node selection fail-closed. Current catalog/preferences expose no valid path that selects npm while its required Node provider is absent.
- Added clean-machine, provider failure, dependency failure, and expired-native-prompt scenarios.
- Final gates passed: Rust format/clippy and 103 tests, frontend lint/typecheck and 25 tests, production build, contract validators, live native host plan compilation, and an explicit macOS `.app` bundle. Install/update and bilingual UI evidence: `.artifacts/report/20260823-194558-install-i18n/report.html`.
- Native portable import runtime proof remained blocked because the harness process broker resolves missing `/opt/homebrew/Cellar/omp/17.4.2/bin/omp` and the mounted browser daemon is unavailable.

## Reflection

The runtime split removed concrete infrastructure from the policy crate without changing desktop IPC. Native install/update planning now uses live evidence and concise localized review; the remaining runtime gap is limited to portable-import evidence, so the UI lock remains pending.

## Decisions

- Same-UID malware/provider/artifact tampering and the final revalidation-to-kernel-open micro-window remain outside the accepted threat model.
- No speculative Node installer or unsupported migration mapping.
- UI contract remains `review` until native portable-import verification and project-lead approval.

## Next

- Restore the harness process broker/browser daemon and run a foreign-target native portable import with fresh evidence.
- Obtain project-lead UI approval, then regenerate the UI lock.
- AgentWiki publishing skipped because the CLI is unavailable.
