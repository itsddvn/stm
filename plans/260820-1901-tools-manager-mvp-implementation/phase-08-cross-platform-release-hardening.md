---
phase: 8
title: "Phase 8: Cross Platform Release Hardening"
status: blocked
priority: P1
effort: "3-4 engineer-weeks"
dependencies: [4, 5, 6, 7]
ui_gate: "UI Contract v1.1 approved and locked; reopen Phase 1 before intentional UI change"
external_gate: "Matrix frozen; blocked on protected signing/notarization credentials and signed fresh-machine release evidence"
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

- [x] Verify UI Contract v1.1 lock before work starts and in every release gate.
- [x] Freeze minimum OS versions and CPU architectures from Phase 2 evidence.
- [ ] Produce signed installers and complete macOS notarization; locked builds, SBOM, dependency inventory, checksums, provenance, and protected workflow are implemented.
- [x] Configure Tauri signed updater artifacts and authenticated versioned endpoint; protected release credentials remain required to build them.
- [x] Bind product self-update logic to the approved settings/consent/progress/failure/recovery UI without reusing tool, skill, or MCP receipts.
- [ ] Test signed fresh install, app upgrade, interrupted update, rollback/reinstall, corrupted cache, offline, restricted privilege, and manager-missing scenarios on every stable target.
- [ ] Complete cross-platform security, privacy, accessibility, performance, recovery, visual, and smoke gates; local and contract gates pass.
- [x] Publish exact support/mapping matrix and known limitations; make no unsupported lifecycle claim.
- [x] Preserve locked route, state, copy, token, interaction, responsive, accessibility, and visual contracts.

## Architecture

Release pipeline stages: locked source → UI contract gate → quality/security gates → per-platform build → signing/notarization → artifact verification → disposable-machine UI and lifecycle smoke → signed updater manifest → staged release. Signing keys stay outside the repository and build logs.

Application updater writes only product release state. Tool receipts and skill receipts are never consulted to authorize application update. The updater emits the locked product-update view states and reason codes.

## Related Code Files

- Create: `.github/workflows/release.yml` and `security.yml` for protected signed drafts, dependency/security gates, SBOM, provenance, and artifact retention.
- Create: release config, contract, secret, tooling, and artifact verification scripts following repository TypeScript/Node conventions.
- Create: `docs/deployment-guide.md`, `docs/security-model.md`, and `docs/supported-platforms.md`.
- Modify: `src-tauri/tauri.conf.json`, release-only Tauri config, updater dependency, and typed Rust product-update modules.
- Keep approved Settings/Updates UI; bind its existing product action through the typed Tauri lifecycle command route.
- Preserve design assets and visual baselines; no intentional UI change was required.
- Modify: `README.md` with release commands, privacy, support, and recovery guidance.

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

- [ ] UI Contract v1.1 lock passes on every configured release build; manifest remains `review` pending project-lead approval.
- [x] OS/architecture matrix has named CI/build/smoke runners and stable versus experimental ownership.
- [x] Signing/notarization and updater secrets never enter repository or logs.
- [ ] Invalid signature, downgrade, wrong channel, interrupted download, and corrupted signed artifact rejection pass against real draft artifacts.
- [x] Existing SQLite schema and receipts pass local migration/regression tests; signed previous-version upgrade evidence remains pending.
- [ ] Cross-platform security/privacy/accessibility/visual findings are fixed or explicitly release-blocking after signed candidate review.
- [x] Release guidance distinguishes product self-update from managed tool, skill, and MCP lifecycle operations.
- [ ] Fresh-machine and upgrade smoke results include tool, skill, MCP, interaction, and critical screenshot evidence for every stable target.

## Success Criteria

- [ ] Signed release candidates install and launch on every supported matrix target.
- [ ] Signed application update succeeds from the previous supported version and rejects tampered metadata/artifacts.
- [ ] Report §2.5 acceptance criteria pass on representative signed fixtures and live systems.
- [x] No enabled lifecycle mapping lacks platform contract evidence.
- [x] Published support and privacy docs match tested local behavior and explicitly mark external gates.
- [x] Rollback/reinstall guidance restores a usable application without silently deleting user tool, skill, or MCP configuration state.
- [ ] Approved UI routes, states, actions, copy, interactions, accessibility behavior, and critical visual baselines are equivalent across supported native webviews.

## Local Completion Evidence

- Stable matrix: macOS arm64/x64, Windows x64, and Ubuntu/glibc x64; Windows/Linux ARM64 remain experimental.
- Signed updater boundary: exact `latest.json` Minisign authentication, monotonic metadata version/digest, full target/URL/manifest/artifact fingerprint consent and revalidation, single-use global execution exclusion, Windows pending-install restart reconciliation, separate durable product receipts, and internal-build fail-closed behavior.
- Automation: exact annotated-tag/main/version provenance, full-SHA-pinned actions, exact-commit quality/security gates before step-scoped secrets, native and updater signing, signed aggregate manifest verification, dependency review, pnpm/cargo audit, CodeQL, secret patterns, SBOM, provenance attestations, streaming checksums, and stable-matrix contract checks.
- Local verification: UI Contract v1.1, release contract/tooling/secret checks, frontend lint/typecheck/tests, desktop integration, Rust format/clippy/tests, internal Tauri release build, generated release-config build, and packaged launch pass.
- Runtime UI: signed product fixture shows exact independent plan and recoverable terminal state without tool/skill/MCP receipt reuse.
- Independent focused correctness and security re-reviews pass with no remaining Blocker/Important or Critical/High finding in the implemented release paths.
- Apple signed-candidate evidence: release tag `v0.1.0` at `1aa3bde`; GitHub run `32792779967` completed both macOS arm64 and x64 jobs after required environment approval. Each job built signed DMG and updater archives, separately notarized/stapled the final DMG, passed local artifact/checksum verification, `codesign --strict`, Gatekeeper assessment, provenance attestation, and artifact upload. Downloaded draft assets independently matched GitHub SHA-256, both DMGs returned `source=Notarized Developer ID` and passed stapler validation, bounded verification covered 10 artifacts, and four updater signatures verified across `darwin-aarch64` and `darwin-x86_64`; the exact final `latest.json` received a separately verified signed envelope.
- Remaining blockers: Windows signing credentials/candidate, Linux/Windows matrix completion, project-lead UI lock approval, and a real previous-version upgrade path. `v0.1.0` is the first signed baseline, so same-version reinstall/restart with schema version 5 was used instead of a nonexistent previous signed release.

## Risk Assessment

- **Missing platform credentials or evidence:** Apple credentials are configured; Windows signing and remaining public matrix evidence stay release-blocking. Do not label the draft fully public-release ready.
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

Complete Windows signing and the Linux/Windows candidate matrix, obtain project-lead UI Contract approval, run cross-platform fresh-machine checks, and use `v0.1.0` as the signed baseline for the next version's real updater/upgrade smoke. Promote updater metadata last.