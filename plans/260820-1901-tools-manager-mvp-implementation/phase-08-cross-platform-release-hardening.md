---
phase: 8
title: "Phase 8: Cross Platform Release Hardening"
status: todo
priority: P1
effort: "3-4 engineer-weeks"
dependencies: [4, 5, 6, 7]
ui_gate: "UI Contract v1.1 approved and locked; reopen Phase 1 before intentional UI change"
external_gate: "Supported OS/architecture matrix and signing/notarization credentials approved"
---

# Phase 8: Cross Platform Release Hardening

## Context Links

- [Plan overview](./plan.md)
- [Approved UI contract phase](./phase-01-mobbin-guided-interface-contract.md)
- [Desktop read-only integration](./phase-04-desktop-read-only-integration.md)
- [Safe tool lifecycle](./phase-05-safe-tool-lifecycle.md)
- [Trusted skill lifecycle](./phase-06-trusted-global-skill-lifecycle.md)
- [MCP server lifecycle](./phase-07-mcp-server-lifecycle.md)
- [Product release strategy](../reports/researcher-2026-08-20-tools-manager-market-and-mvp.md#12-verification-strategy)
- [Tauri updater](https://v2.tauri.app/plugin/updater/)
- [Tauri WebDriver testing](https://v2.tauri.app/develop/tests/webdriver/)

## Overview

Freeze the supported matrix, harden supply-chain and privacy boundaries, produce signed installers and signed in-app updates, and validate upgrade/recovery behavior on representative Windows, macOS, and Linux systems. Preserve UI Contract v1.1 across native webviews and platform differences; release hardening cannot silently redesign the approved interface.

## Key Insights

- Product self-update has a separate trust root, feed, artifacts, history, and rollback behavior from managed tools.
- A platform is supported only when its packaged app, read adapters, enabled mutations, privilege behavior, recovery paths, and approved UI pass the declared matrix.
- Unsupported mappings remain explicit through locked UI states rather than silently degrading.
- Cross-platform visual or interaction differences that require product changes reopen Phase 1 and re-lock affected release work.

## Requirements

- [ ] Verify UI Contract v1.1 lock before work starts and in every release gate.
- [ ] Freeze minimum OS versions and CPU architectures from Phase 2 evidence.
- [ ] Produce reproducible-enough locked builds, SBOM/dependency inventory, signed installers, macOS notarization, and release checksums/attestations.
- [ ] Configure Tauri signed updater artifacts and authenticated versioned endpoint; signatures are mandatory.
- [ ] Bind product self-update logic to the approved settings/consent/progress/failure/recovery UI without reusing tool, skill, or MCP receipts.
- [ ] Test fresh install, app upgrade, interrupted update, rollback/reinstall, corrupted cache, offline, restricted privilege, and manager-missing scenarios.
- [ ] Run security, privacy, accessibility, performance, recovery, cross-platform visual, and smoke gates.
- [ ] Publish exact support/mapping matrix and known limitations; make no unsupported lifecycle claim.
- [ ] Reopen Phase 1 before intentional route, state, copy, token, interaction, responsive, accessibility, or visual-baseline changes.

## Architecture

Release pipeline stages: locked source → UI contract gate → quality/security gates → per-platform build → signing/notarization → artifact verification → disposable-machine UI and lifecycle smoke → signed updater manifest → staged release. Signing keys stay outside the repository and build logs.

Application updater writes only product release state. Tool receipts and skill receipts are never consulted to authorize application update. The updater emits the locked product-update view states and reason codes.

## Related Code Files

- Create: `/Users/itsddvn/projects/tools-managers/.github/workflows/release.yml`, `security.yml`, and platform smoke workflow definitions
- Create: `/Users/itsddvn/projects/tools-managers/scripts/verify-release-artifacts.*` using platform-appropriate repository conventions
- Create: `/Users/itsddvn/projects/tools-managers/docs/deployment-guide.md`, `/Users/itsddvn/projects/tools-managers/docs/security-model.md`, `/Users/itsddvn/projects/tools-managers/docs/supported-platforms.md`
- Modify: `/Users/itsddvn/projects/tools-managers/src-tauri/tauri.conf.json` and updater/capability configuration
- Modify: `/Users/itsddvn/projects/tools-managers/src/features/settings/` and `/Users/itsddvn/projects/tools-managers/src/features/updates/` only to bind approved product-update actions and verified platform defects
- Modify: `/Users/itsddvn/projects/tools-managers/assets/designs/tools-manager-ui/` only through the Phase 1 reopen/version-bump process when approved platform-specific baselines genuinely change
- Modify: `/Users/itsddvn/projects/tools-managers/README.md` with installation, privacy, support, and recovery guidance

## Implementation Steps

1. Verify UI Contract v1.1, interaction fixtures, accessibility checks, and approved critical screenshots before release work starts.
2. Convert Phase 2 matrix evidence into supported minimum OS/architecture policy; remove or mark experimental targets that lack build/smoke ownership.
3. Add locked dependency review, license check, vulnerability scan, secret scan, SBOM, Rust/JS audit, and artifact provenance gates.
4. Configure Windows signing, macOS signing/notarization, and Linux package/AppImage outputs selected by the matrix; document credential injection and rotation.
5. Configure Tauri updater public key, endpoint, signed artifacts, channel/version policy, consent, download verification, restart behavior, and failure recovery through the approved product-update UI.
6. Add platform smoke suites covering scan, manager missing, overlapping skill roots, detect-only denial, one approved managed lifecycle, vendor handoff, privilege denial, diagnostics redaction, and UI contract equivalence.
7. Add upgrade suites from the previous supported application version with existing cache/receipts, schema migration, interrupted update, invalid signature, downgrade, and rollback/reinstall paths.
8. Run threat-model review for catalog compromise, command substitution, path traversal, symlink race, stale consent, privilege boundary, malicious skill tree, update-key compromise, and log leakage.
9. Run performance tests for representative large inventories and skill collections; set measured scan time/memory/file-limit budgets from baseline rather than invented thresholds.
10. Run accessibility and visual review across primary flows and native-dialog boundaries on each supported OS family. Unexpected intentional UI changes trigger the Phase 1 reopen process.
11. Publish support matrix, mapping capabilities, privacy behavior, diagnostics redaction, backup/recovery, security reporting, and release verification instructions.
12. Execute release-candidate checklist on disposable/fresh machines before promoting updater metadata.

## Todo

- [ ] UI Contract v1.1 lock passes on every supported release build.
- [ ] OS/architecture matrix has named CI/build/smoke evidence and owner.
- [ ] Signing/notarization and updater secrets never enter repository or logs.
- [ ] Invalid signature, downgrade, wrong channel, interrupted download, and corrupted artifact are rejected safely.
- [ ] Existing SQLite schema and receipts migrate across app update without data loss.
- [ ] Security/privacy/accessibility/visual findings are fixed or explicitly release-blocking.
- [ ] Release notes distinguish product self-update from managed tool, skill, and MCP lifecycle operations.
- [ ] Fresh-machine and upgrade smoke results include tool, skill, MCP, interaction, and critical screenshot evidence for the approved UI.

## Success Criteria

- [ ] Signed release candidates install and launch on every supported matrix target.
- [ ] Signed application update succeeds from the previous supported version and rejects tampered metadata/artifacts.
- [ ] Report §2.5 acceptance criteria pass on representative fixtures and live systems.
- [ ] No enabled lifecycle mapping lacks platform contract evidence.
- [ ] Published support and privacy docs match tested behavior.
- [ ] Rollback/reinstall guidance restores a usable application without silently deleting user tool, skill, or MCP configuration state.
- [ ] Approved UI routes, states, actions, copy, interactions, accessibility behavior, and critical visual baselines remain equivalent across supported native webviews.

## Risk Assessment

- **Signing credentials unavailable:** produce unsigned internal builds only; do not label public release ready.
- **Matrix too broad:** reduce supported targets explicitly based on evidence; retain read-only/unsupported labels elsewhere.
- **Updater regression:** stage rollout, retain previous signed artifact, and delay manifest promotion until upgrade smoke passes.
- **Cross-platform UI divergence:** reopen Phase 1, verify the revised interface across affected platforms, bump the contract, and propagate changes before release resumes.
- **Flaky desktop E2E:** keep deterministic Rust/renderer contract suites authoritative and use desktop smoke for integration confidence.

## Security Considerations

- Use least-privilege CI identities, protected environments, short-lived credentials where possible, and separate updater signing keys.
- Never expose release private keys to the application, catalog, repository, diagnostics, or tool adapters.
- Treat updater endpoint compromise and signing-key compromise as separate incident procedures.
- The UI contract lock preserves security-critical warnings, explicit consent, and failure/recovery visibility through release hardening.

## Next Steps

After release gates pass, move deferred bundles, catalog suggestions, extra registries, and public CLI into separately approved plans. Any UI change remains subject to Phase 1 reopen and reapproval.