---
phase: 7
title: "Verification And Contract Lock"
status: in-progress
priority: P1
effort: "2d"
dependencies: [6]
---

# Phase 7: Verification And Contract Lock

## Overview

Prove the new flow against real contracts and live UI, then lock the UI and update evergreen architecture.

## Requirements

- Functional: focused Rust, frontend, and desktop tests cover resolver, mixed plans, bootstrap failure isolation, import rejection, and migration cleanup.
- Functional: live Quick Setup on the desktop/browser fixture completes select → review → result.
- Non-functional: UI lock matches the approved screens.
- Non-functional: `docs/system-architecture.md` describes the shipped 5-layer/3-crate model, not the pre-split god service.

3. Drive live Quick Setup: defaults, uncheck, Start, review copy, skip persistence, terminal progress without a manual refresh.
4. Drive Settings provider detection and a fixture migration review where failed target install cannot start cleanup.
5. Drive portable import mismatch, custom-URL no-probe, and secret-rejection cases.
6. Update evergreen architecture to the shipped crate/module boundaries.
7. After visual/interaction review, record approval, set the UI contract to `locked`, and regenerate the lock. This is the only lock-write step.

- Modify: `docs/system-architecture.md`, `docs/ui-interaction-contract.md` if needed
- Modify: `contracts/ui/ui-contract.lock.json` after `pnpm lock:ui-contract`
- Add/adjust tests beside the owning modules
- Use `.artifacts/` only for verification evidence

## Implementation Steps

1. Run focused resolver, planner, portable-config, and presentation-action tests.
2. Run `pnpm typecheck`, `pnpm test`, and relevant `cargo test` targets.
3. Drive live Quick Setup: defaults, uncheck, Start, review copy, skip persistence.
4. Drive Settings provider detection and a fixture migration review.
5. Drive portable import mismatch and secret-rejection cases.
6. Update evergreen architecture to the shipped crate/module boundaries.
7. Regenerate the UI lock only after the screens match the accepted contract.

## Todo

- [x] Focused automated tests
- [x] Live Quick Setup verification
- [ ] Live provider/import verification
- [x] Update system architecture
- [ ] Lock UI contract

## Success Criteria

- [x] Clean-macOS fixture can install the default profile through one review in the running UI.
- [x] Existing-owner and target-mismatch cases fail closed in tests and UI.
- [x] Evergreen docs match shipped crates, not the old single-core layout.

## Risk Assessment

Do not weaken tests to hide missing mappings. If a default tool still lacks a verified recipe, it must appear Blocked/Guidance, not Installed.
