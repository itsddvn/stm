---
phase: 6
title: "Phase 6: Trusted Global Skill Lifecycle"
status: done
priority: P1
effort: "4-5 engineer-weeks"
dependencies: [4, 5]
ui_gate: "UI Contract v1.1 approved and locked; reopen Phase 1 before intentional UI change"
external_gate: "Approved 2026-08-21: public STM catalog, project-lead review, detached Ed25519 signatures, pinned trust roots, monotonic expiring snapshots, and fixed HTTPS updates"
---

# Phase 6: Trusted Global Skill Lifecycle

## Context Links

- [Plan overview](./plan.md)
- [Approved UI contract phase](./phase-01-mobbin-guided-interface-contract.md)
- [Read-only core](./phase-03-read-only-core.md)
- [Implemented planning and consent substrate](./phase-05-safe-tool-lifecycle.md)
- [Skills lifecycle contract](../reports/researcher-2026-08-20-tools-manager-market-and-mvp.md#8-skills-manager-lifecycle)
- [Agent Skills specification](https://agentskills.io/specification)
- [Claude Code skills](https://code.claude.com/docs/en/skills)
- [Codex skills](https://developers.openai.com/codex/skills/)

## Overview

Analyze pasted HTTPS repository or skill-path URLs, then install and update only sources that resolve to trusted, catalog-listed global AI Agent Skills using pinned Git provenance, staged validation, the implemented immutable planning/consent substrate, receipt-backed atomic replacement, conflict detection, and rollback. Activate the approved source-review/install/update/diff/conflict/progress/result/recovery UI without redesign. This phase does not begin until the catalog trust gate is approved.

## Key Insights

- UI Contract v1.1 defines skill install, update, diff, risk, local-modification conflict, partial failure, rollback, and recovery states.
- Skill frontmatter version is optional and not authoritative; resolved Git commit + directory digest own update identity.
- One physical installation may serve several logical clients.
- Skill content is active supply-chain input: inspect and copy, never execute during management.
- Phase 5 owns the shared immutable plan/digest/expiry/consent substrate; this phase extends it for skill targets and file materialization rather than duplicating authorization logic.

## Trust Gate Decision

- **Publisher:** `itsddvn/stm-skill-catalog` is the public metadata distribution repository. The STM application repository retains the schemas, verifier, bundled fallback, and publisher tooling.
- **Review:** catalog changes use pull requests with schema, provenance, tree-policy, digest, and duplicate-identity checks. Project-lead approval is required before an offline signing key may publish the stable snapshot; unsigned branch content is never trusted by the app.
- **Authentication:** the stable channel contains exact-byte `manifest.json`, detached `manifest.sig.json`, and digest-bound `catalog.json`. Signatures use Ed25519 and a key ID resolved only through trust roots compiled into a signed STM application release.
- **Freshness:** the signed manifest binds schema version, monotonically increasing catalog version, creation/expiry timestamps, channel, payload SHA-256, and payload byte length. STM rejects unknown/revoked keys, invalid signatures, hash/length mismatch, expiry, downgrade, and same-version content changes.
- **Updates:** STM fetches only fixed HTTPS paths under the dedicated catalog origin with bounded size/time/redirect policy. A last-known-good bundled or persisted snapshot remains available when the network or remote catalog fails.
- **Rotation:** a signed STM release introduces the next public key before use; a dual-sign transition overlaps active and next keys. Remote catalog content cannot add trust roots. Emergency revocation requires an STM application trust-root update.
- **Scope:** MVP sources are public GitHub repositories and immutable commits. Private repository credentials and additional Git hosts remain deferred.

## Requirements

- [x] Verify UI Contract v1.1 lock and the external trust gate before work starts and in every CI run.
- [x] Analyze pasted HTTPS skill source URLs before planning; normalize repository/subpath identity, reject embedded credentials and unsupported hosts/schemes, and require a match to authenticated catalog provenance before managed installation.
- [x] Authenticate and version the selected skill catalog; reject downgrade, invalid signature/authentication, unknown publisher, or malformed snapshot.
- [x] Resolve repository, subpath, approved ref, immutable commit, expected digest, license, compatibility, and risk metadata.
- [x] Stage content in an app-private temporary directory; enforce file count/size/path/symlink/type limits and never execute files.
- [x] Install only into selected approved global client targets; never project-local roots.
- [x] Record per-target receipt, file manifest, digest, provenance, client binding, and previous managed revision.
- [x] Detect local modification and block overwrite until the developer selects an approved explicit conflict action.
- [x] Preview source/revision/risk/file diff through the approved UI and require immutable consent before atomic write.
- [x] Roll back completed targets when a selected multi-target operation partially fails, or clearly preserve/report split state when rollback also fails.
- [x] Activate approved skill lifecycle UI controls through typed IPC and reason codes without changing locked routes, states, copy, interactions, or baselines.

## Architecture

```mermaid
flowchart TD
    CAT[Authenticated catalog] --> RESOLVE[Resolve ref to commit]
    RESOLVE --> FETCH[Private staging fetch]
    FETCH --> VALIDATE[Manifest/path/size/symlink validation]
    VALIDATE --> DIFF[Approved target diff and risk view]
    DIFF --> CONSENT[Shared immutable consent]
    CONSENT --> RECHECK[Recheck local digest]
    RECHECK --> WRITE[Atomic per-target replacement]
    WRITE --> RECEIPT[Commit receipts after verification]
    WRITE --> ROLLBACK[Approved rollback/recovery state]
```

Physical target identity is canonical path + installation identity. Logical clients bind to it many-to-many; write planning deduplicates physical targets before consent. Application DTOs extend the Phase 5 operation contract with skill provenance, file diff, and per-target materialization evidence.

## Related Code Files

- Create: `catalog/skills/stable/`, catalog/signature schemas, publisher tooling, and the compiled trust-root configuration.
- Create: `crates/tools-manager-core/src/skill_catalog.rs` and `skill_lifecycle/` for authenticated catalog activation, immutable Git resolution, staged validation, materialization, conflict handling, receipts, and rollback.
- Create: `tests/fixtures/skill-lifecycle/` for valid, malicious, conflict, partial-failure, and recovery evidence.
- Modify: `lifecycle/skill_planner.rs`, `skill_source.rs`, and `service.rs` to add skill-specific immutable plan evidence without weakening Phase 5 guarantees.
- Modify: SQLite storage for authenticated catalog state, receipts, previous revisions, partial results, and recovery.
- Modify: `src/features/skills/`, `updates/`, and `history/` only to bind approved actions and verified defect fixes.
- Modify: `src-tauri/src/commands.rs` and command permissions for typed skill plan, consent, materialization, and status IPC.

## Implementation Steps

1. Verify UI Contract v1.1 plus the approved skill source-analysis and lifecycle fixtures, interaction tests, and visual baselines. Confirm the publisher/review/authentication decision is documented and testable.
2. Convert the approved trust decision into catalog trust-root configuration, schema, activation, downgrade prevention, and rotation/revocation procedure.
3. Extend the shared bounded source-analysis service for skill repository/subpath URLs. Produce provenance, requested ref, target-client compatibility, risk, and catalog-match evidence without materialization or execution.
4. Implement Git source resolver with URL allowlist, subpath containment, ref-to-commit resolution, bounded fetch, immutable commit checkout, and digest verification only after the source-review gate passes.
5. Implement staged tree validation: required `SKILL.md`, YAML/frontmatter, canonical identity, file manifest, size/count/depth, symlink escape, binary/script flags, and license/compatibility metadata.
6. Extend the shared operation planner for logical clients, canonical physical roots, project-location rejection, write deduplication, provenance, file diff, and name/source/path conflicts.
7. Implement receipt-backed install using sibling staging + atomic rename where supported; define platform fallback and cleanup behavior when atomic directory replacement is unavailable.
8. Implement update comparison using trusted resolved commit and digest; frontmatter version remains display-only.
9. Implement local-modification detection and approved conflict choices: keep local, export diff, restore managed, or side-by-side only where the target client supports it.
10. Bind file-level diff/risk preview and immutable consent to the approved UI with source/repository/subpath/target/revision/digest preconditions.
11. Implement multi-target write orchestration, verification, receipt commit, partial failure reporting, rollback, and recovery from interrupted staging.
12. Activate approved source-review/install/update/conflict/rollback flows and ensure external skills remain inspect-only; do not structurally redesign the interface.
13. Add malicious repository, deleted ref, network loss, target changed after preview, duplicate root, partial write, and rollback-failure tests.

## Todo

- [x] UI Contract v1.1 lock and external trust gate pass before and after lifecycle activation.
- [x] Resolver always records requested ref and immutable resolved commit.
- [x] Pasted source URLs cannot reach materialization unless normalized provenance matches an authenticated trusted catalog entry.
- [x] Untrusted URL, mutable-only identity, digest mismatch, traversal, and escaping symlink are blocked.
- [x] External and locally modified skills cannot be overwritten by default.
- [x] Overlapping Codex/Claude/AgentKit roots produce one physical write.
- [x] Every selected target has independent result and receipt state.
- [x] Interrupted/failed update retains a usable previous managed revision or explicit recovery artifact.
- [x] Approved skill lifecycle interaction and screenshot baselines pass with real operation state fixtures.

## Success Criteria

- [x] Trusted managed skill installs to one or more global clients after approved preview and consent.
- [x] Developer can paste a reviewed trusted skill URL, inspect source/provenance/targets/risk, and reach the same immutable plan as catalog selection; source changes invalidate consent.
- [x] Repeated install of the same commit/digest is no-op.
- [x] Trusted update previews exact revision and file diff; local change blocks overwrite.
- [x] Project-local roots and unmanaged external skills never reach materialization.
- [x] No skill script, binary, hook, or instruction executes during scan/install/update/rollback.
- [x] Partial multi-target failure is visible, recoverable, and never falsely reported as complete.
- [x] Shared immutable consent guarantees from Phase 5 remain intact for skill operations.
- [x] No locked skill route, state, warning, copy, interaction, or visual baseline changed without reopening Phase 1.

## Risk Assessment

- **Signing key or publisher compromise:** reject signatures outside compiled trust roots, enforce monotonic expiry-bound snapshots, retain last-known-good state, and revoke through a signed STM application update.
- **Backend semantics do not fit approved skill UI:** fix DTO/reason-code mapping or reopen Phase 1 with evidence; never bypass the lock.
- **Git/source unavailable:** retain installed content and receipt; show source unavailable.
- **Non-atomic filesystem behavior:** use verified backup/replace sequence and document reduced confidence per platform.
- **Shared physical root:** deduplicate before plan generation to prevent duplicate replacement.

## Security Considerations

- No repository credentials in receipts/logs; use OS credential facilities only if private sources are later approved.
- Fetch into private bounded staging outside managed roots; cleanup is best-effort and verified on next startup.
- Catalog trust never implies skill content is harmless; the locked UI must continue surfacing scripts, binaries, symlinks, requirements, and diffs.
- Consent digest verification remains in Rust; frontend state is not authorization.

## Next Steps

Phase 6 is complete. Phase 7 reused its authenticated-source, immutable-consent, receipt, conflict, and recovery substrate without reopening the approved interface.