---
phase: 6
title: "Phase 6: Cross Platform Release Hardening"
status: todo
priority: P1
effort: "3-4 engineer-weeks"
dependencies: [3, 4, 5]
external_gate: "Supported OS/architecture matrix and signing/notarization credentials approved"
---

# Phase 6: Cross Platform Release Hardening

## Context Links

- [Plan overview](./plan.md)
- [Product release strategy](../reports/researcher-2026-08-20-tools-manager-market-and-mvp.md#12-verification-strategy)
- [Tauri updater](https://v2.tauri.app/plugin/updater/)
- [Tauri WebDriver testing](https://v2.tauri.app/develop/tests/webdriver/)

## Overview

Freeze the supported matrix, harden supply-chain and privacy boundaries, produce signed installers and signed in-app updates, and validate upgrade/recovery behavior on representative Windows, macOS, and Linux systems.

## Key Insights

- Product self-update has a separate trust root, feed, artifacts, history, and rollback behavior from managed tools.
- A platform is supported only when its packaged app, read adapters, enabled mutations, privilege behavior, and recovery paths pass the declared matrix.
- Unsupported mappings must remain explicit in UI and catalog rather than silently degrading.

## Requirements

- [ ] Freeze minimum OS versions and CPU architectures from Phase 1 evidence.
- [ ] Produce reproducible-enough locked builds, SBOM/dependency inventory, signed installers, macOS notarization, and release checksums/attestations.
- [ ] Configure Tauri signed updater artifacts and authenticated versioned endpoint; signatures are mandatory.
- [ ] Test fresh install, app upgrade, interrupted update, rollback/reinstall, corrupted cache, offline, restricted privilege, and manager-missing scenarios.
- [ ] Run security, privacy, accessibility, performance, recovery, and cross-platform smoke gates.
- [ ] Publish exact support/mapping matrix and known limitations; no unsupported lifecycle claim.

## Architecture

Release pipeline stages: locked source → quality/security gates → per-platform build → signing/notarization → artifact verification → disposable-machine smoke → signed updater manifest → staged release. Signing keys stay outside repository and build logs.

Application updater writes only product release state. Tool receipts and skill receipts are never consulted to authorize application update.

## Related Code Files

- Create: `/Users/itsddvn/projects/tools-managers/.github/workflows/release.yml`, `security.yml`, and platform smoke workflow definitions
- Create: `/Users/itsddvn/projects/tools-managers/scripts/verify-release-artifacts.*` using platform-appropriate repository conventions
- Create: `/Users/itsddvn/projects/tools-managers/docs/deployment-guide.md`, `/Users/itsddvn/projects/tools-managers/docs/security-model.md`, `/Users/itsddvn/projects/tools-managers/docs/supported-platforms.md`
- Modify: `/Users/itsddvn/projects/tools-managers/src-tauri/tauri.conf.json` and updater/capability configuration
- Modify: `/Users/itsddvn/projects/tools-managers/src/features/settings/` and `/Users/itsddvn/projects/tools-managers/src/features/updates/` for product-update UX
- Modify: `/Users/itsddvn/projects/tools-managers/README.md` with installation, privacy, support, and recovery guidance

## Implementation Steps

1. Convert Phase 1 matrix evidence into supported minimum OS/architecture policy; remove or mark experimental targets that lack build/smoke ownership.
2. Add locked dependency review, license check, vulnerability scan, secret scan, SBOM, Rust/JS audit, and artifact provenance gates.
3. Configure Windows signing, macOS signing/notarization, and Linux package/AppImage outputs selected by the matrix; document credential injection and rotation.
4. Configure Tauri updater public key, endpoint, signed artifacts, channel/version policy, consent UI, download verification, restart behavior, and failure recovery.
5. Add platform smoke suites covering scan, manager missing, overlapping skill roots, detect-only denial, one approved managed lifecycle, vendor handoff, privilege denial, and diagnostics redaction.
6. Add upgrade suites from the previous supported application version with existing cache/receipts, schema migration, interrupted update, invalid signature, downgrade, and rollback/reinstall paths.
7. Run threat-model review for catalog compromise, command substitution, path traversal, symlink race, stale consent, privilege boundary, malicious skill tree, update-key compromise, and log leakage.
8. Run performance tests for representative large inventories and skill collections; set measured scan time/memory/file-limit budgets from baseline rather than invented thresholds.
9. Run accessibility review across primary flows and native-dialog boundaries on each supported OS family.
10. Publish support matrix, mapping capabilities, privacy behavior, diagnostics redaction, backup/recovery, security reporting, and release verification instructions.
11. Execute release-candidate checklist on disposable/fresh machines before promoting updater metadata.

## Todo

- [ ] OS/architecture matrix has named CI/build/smoke evidence and owner.
- [ ] Signing/notarization and updater secrets never enter repository or logs.
- [ ] Invalid signature, downgrade, wrong channel, interrupted download, and corrupted artifact are rejected safely.
- [ ] Existing SQLite schema and receipts migrate across app update without data loss.
- [ ] Security/privacy/accessibility findings are fixed or explicitly release-blocking.
- [ ] Release notes distinguish product self-update from managed tool/skill updates.
- [ ] Fresh-machine and upgrade smoke results are attached to the release candidate.

## Success Criteria

- [ ] Signed release candidates install and launch on every supported matrix target.
- [ ] Signed application update succeeds from the previous supported version and rejects tampered metadata/artifacts.
- [ ] Report §2.5 acceptance criteria pass on representative fixtures and live systems.
- [ ] No enabled lifecycle mapping lacks platform contract evidence.
- [ ] Published support and privacy docs match tested behavior.
- [ ] Rollback/reinstall guidance restores a usable application without silently deleting user tool/skill state.

## Risk Assessment

- **Signing credentials unavailable:** produce unsigned internal builds only; do not label public release ready.
- **Matrix too broad:** reduce supported targets explicitly based on evidence; retain read-only/unsupported labels elsewhere.
- **Updater regression:** stage rollout, retain previous signed artifact, and delay manifest promotion until upgrade smoke passes.
- **Flaky desktop E2E:** keep deterministic Rust/renderer contract suites authoritative and use desktop smoke for integration confidence.

## Security Considerations

- Use least-privilege CI identities, protected environments, short-lived credentials where possible, and separate updater signing keys.
- Never expose release private keys to the application, catalog, repository, diagnostics, or tool adapters.
- Treat updater endpoint compromise and signing-key compromise as separate incident procedures.

## Next Steps

After release gates pass, move deferred bundles, URL analysis, catalog suggestions, extra registries, and public CLI into separately approved plans.
