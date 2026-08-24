---
title: "STM integration completed"
date: 2026-08-24 19:44
status: completed
component: "quick-setup / lifecycle / runtime integration"
commit: "93cc8c8"
plan: "plans/260824-1731-integrate-lifecycle-quick-setup-macos/plan.md"
pm_report: "plans/260824-1731-integrate-lifecycle-quick-setup-macos/reports/pm-2026-08-24-integration-status.md"
durable_evidence: "plans/260824-1731-integrate-lifecycle-quick-setup-macos/reports/integration-evidence/report.html"
agentwiki: "skipped: CLI unavailable"
---

# STM integration completed

## Context

This closes the internal macOS STM integration milestone documented in `plans/260824-1731-integrate-lifecycle-quick-setup-macos/plan.md`. The work started from `feat/quick-setup-capabilities` and treated the Quick Setup architecture as canonical, then pulled main-branch Skill lifecycle, MCP lifecycle, updater, and release-safe behavior through the `stm-core` ports and `stm-runtime` adapter boundary instead of backsliding into a monolith.

## What happened

Chronologically, the branch was merged onto the Quick Setup base, the runtime/lifecycle work was ported into the canonical crate split, desktop/localization behavior was preserved, and the internal macOS app was built and launched. Verification then closed the loop: Rust format and Clippy passed, Rust workspace tests landed at 149 passed with 6 ignored, frontend landed at 23 UI tests plus 3 desktop integration tests passed, and the native current-host Quick Setup Install/Update smoke passed without mutating packages. The retained smoke evidence shows `native_quick_setup_tests::native_quick_setup_uses_live_host_evidence ... ok` and also captured the live warning `"Cursor config uses unsupported schema root"` without turning that warning into fake success theater. Final commit `93cc8c8` did the boring but necessary cleanup: mark the plan completed, add the smoke log, and sync the durable evidence link from `.log` to `.txt` so the record is not rotten.

## Reflection

The painful part is how easy this integration could have turned into architectural vandalism. It would have been faster in the moment to jam main-branch behavior straight into the Tauri surface or rebuild runtime logic in place, and that would have left the repo as a liar. Instead, the branch paid the integration tax properly: seven review findings were fixed, staging cleanup was finished, and the app now reflects one coherent architecture instead of two competing stories. Relief is deserved here, but it is the tired kind: the work passed because the boundaries held, not because the code got lucky.

## Decisions

- Quick Setup architecture is the canonical base for integrated STM behavior.
- Main-branch Skill, MCP, updater, and release work stays only where it belongs: policy and ports in `stm-core`, concrete effects in `stm-runtime`, composition in Tauri.
- Internal app verification is authoritative for this milestone; no package mutation was allowed during smoke verification.
- UI Contract lock remains external and stays in `review` pending project-lead approval.
- Public signing, notarization, signed cross-platform candidates, and updater promotion remain external release gates, not hidden inside this milestone.

## Next

- Project lead: decide when UI Contract can leave `review`.
- Release owner: handle signing, notarization, and public candidate promotion outside this internal macOS milestone.
- Follow-up maintainer: keep using the Quick Setup-first architecture; do not reintroduce shortcut runtime logic into the desktop layer.
- If portable import/export needs visual proof on macOS, revisit host dialog automation separately; this milestone already has policy and IPC coverage.
