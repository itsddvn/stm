---
phase: 5
title: "Quick Setup And Settings"
status: pending
priority: P1
effort: "3-4d"
dependencies: [4]
---

# Phase 5: Quick Setup And Settings

## Overview

Reopen the UI contract and ship first-launch Quick Setup plus Settings provider preference and migration.

## Requirements

- Functional: first launch opens Quick Setup; Skip never auto-reopens.
- Functional: Dashboard and Tools can reopen Quick Setup.
- Functional: source choice is System recommendations or Import. Import UI may be disabled until Phase 6, but the choice exists.
- Functional: defaults are preselected. User can uncheck or Select all / Clear all.
- Functional: each row shows Install, Update, Installed, Handoff, Guidance, or Blocked.
- Functional: one Start action prepares and reviews the mixed plan.
- Functional: Settings can change provider preference and start reviewed migration.
- Non-functional: Mobbin is reference-only. Do not copy proprietary layout or imagery.

## Architecture

Delivery owns screens. Application owns Quick Setup and provider-preference use cases. No React code compiles manager commands.

First-launch provider screen: Automatic, Prefer Homebrew, Prefer Bun. Detected providers are shown before the choice.

Migration UI lists compatible resources only. Remove old installation is preselected and reviewable.

## Related Code Files

- Modify: `contracts/ui/*`, `docs/ui-interaction-contract.md`, `docs/design-guidelines.md`
- Create: Quick Setup feature module under `src/features/`
- Modify: `src/app/app.tsx`, `src/features/tools/tools-page.tsx`, `src/features/dashboard/dashboard-page.tsx`, `src/features/settings/settings-page.tsx`
- Modify: `src/lib/ipc/runtime-ipc-client.ts`, `src-tauri/src/commands.rs`
- Modify: `contracts/ui/*`, `docs/ui-interaction-contract.md`, `docs/design-guidelines.md`. In this phase bump contract version, set manifest status to `review`, and add every new Quick Setup artifact. Do not write the approved lock here.

## Implementation Steps

1. Add UI contract types for Quick Setup, provider preference, row actions, and migration review.
2. Implement first-launch gate and permanent skip persistence.
1. Add UI contract types for Quick Setup, provider preference, row actions, and migration review. Add new action IDs to the public union.
2. Persist first-launch skip and provider preference in the durable `PreferencesStore`. Restart must not reopen a skipped Quick Setup.
3. Implement recommendation checklist bound to resolver output.
4. Bind Start to mixed lifecycle review. Poll or subscribe until terminal. Surface prepare/start errors as renderable outcomes.
5. Add Settings provider card and a dedicated migration review that uses the typed migration state machine. Cleanup is not startable if target activation failed.
6. Keep copy nontechnical. Put commands in Advanced details.
7. Add fixture scenarios: clean machine, existing npm owner, brew missing, mixed blocked/handoff, expired consent after installer prompt.
- [ ] First-launch Quick Setup
- [ ] Recommendation checklist
- [ ] Provider preference settings
- [ ] Reviewed provider migration
- [ ] Fixture scenarios

## Success Criteria

- [ ] A fixture clean-macOS run shows preselected defaults and one Start action.
- [ ] Skip prevents automatic reopen.
- [ ] Existing npm Codex row stays npm-owned until migration is reviewed.
- [ ] Migration cannot uninstall the old copy before the new copy verifies.

## Risk Assessment

UI Contract v1.1 is locked. Phase 5 may only put the contract into `review`. Phase 7 is the only phase that records approval and writes the lock.
