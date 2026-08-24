---
phase: 7
title: "Phase 7: MCP Server Lifecycle"
status: done
priority: P1
effort: "3-5 engineer-weeks"
dependencies: [4, 5, 6]
ui_gate: "UI Contract v1.1 approved and locked; reopen Phase 1 before intentional UI change"
external_gate: "Approved: Codex, Claude Code, and Cursor schemas; signed local mapping policy; environment/OS credential references"
---

# Phase 7: MCP Server Lifecycle

## Context Links

- [Plan overview](./plan.md)
- [Approved UI contract phase](./phase-01-mobbin-guided-interface-contract.md)
- [Desktop read-only integration](./phase-04-desktop-read-only-integration.md)
- [Safe tool lifecycle](./phase-05-safe-tool-lifecycle.md)
- [Trusted global skill lifecycle](./phase-06-trusted-global-skill-lifecycle.md)
- [Model Context Protocol specification](https://modelcontextprotocol.io/specification/latest)

## Overview

Implement first-class MCP server inventory and reviewed lifecycle management for supported Codex, Claude Code, and Cursor global configurations. Reuse the immutable plan, explicit consent, receipt, partial-failure, and recovery substrate from tool and skill lifecycle work without treating client-specific MCP configuration as a generic package install.

## Key Insights

- One logical MCP server may be configured in several clients with different transports, capability grants, credential references, and health states.
- Remote URLs, package identifiers, arguments, environment-variable names, and capability declarations are untrusted inputs until normalized and policy-checked.
- STM stores credential references only. Secret values remain in OS credential facilities, environment configuration, or the owning client's approved auth flow.
- Adding, updating, enabling, disabling, or removing an MCP server is a configuration mutation with an immutable reviewed plan and per-client outcomes.
- Arbitrary shell strings are never accepted. Local stdio launch plans come from trusted declarative mappings with an executable and typed argument array.

## Requirements

- [x] Verify UI Contract v1.1 before implementing MCP behavior or binding real data.
- [x] Discover supported global MCP configuration locations without traversing project roots.
- [x] Normalize logical server identity across Codex, Claude Code, and Cursor while retaining each client binding.
- [x] Model stdio and Streamable HTTP transports, capability declarations, auth kind, health, trust, and per-client state.
- [x] Analyze pasted HTTPS source URLs without executing remote content or trusting catalog claims.
- [x] Build immutable add, update, enable, disable, remove, retry, and rollback plans from approved mappings only.
- [x] Require explicit consent bound to server identity, transport, endpoint or executable, arguments, targets, capabilities, credential-reference names, and expected config digest.
- [x] Apply client configuration changes atomically when supported; otherwise use verified encrypted backup/replace/recovery sequences and report per-client outcomes.
- [x] Never persist raw tokens, passwords, OAuth refresh tokens, private keys, or copied environment values in STM storage, receipts, diagnostics, logs, or plaintext backup artifacts.
- [x] Preserve UI Contract v1.1 routes, actions, copy, interaction hierarchy, accessibility behavior, and visual language; verified defects were corrected and the lock was regenerated.

## Architecture

The MCP lifecycle pipeline is: discover client configuration → parse bounded schema → normalize logical server and client bindings → resolve trusted mapping or reviewed remote source → build immutable plan → revalidate current digest → collect consent → apply per-client configuration → health-check without invoking arbitrary tools → persist redacted receipts and recovery data → emit locked UI view models.

Credential fields contain reference metadata only, such as an environment-variable name or OS credential handle. Health checks use protocol initialization and capability listing with bounded timeouts; they never call domain tools during installation verification.

## Related Code Files

- Create: `catalog/mcp/approved.json` and `catalog/schemas/mcp-catalog.schema.json` for versioned approved mappings, client support, capabilities, and credential references.
- Create: `crates/tools-manager-core/src/mcp/` for bounded discovery, policy, health, authenticated backup encryption, mutation, receipts, and recovery.
- Modify: `crates/tools-manager-core/src/domain/mcp.rs`, `lifecycle/mcp_planner.rs`, `lifecycle/service.rs`, `lifecycle/command.rs`, and SQLite storage for client-specific bindings and shared immutable lifecycle behavior.
- Modify: `tests/fixtures/mcp/` and focused Rust tests for malformed input, secret handling, concurrent mutation, interruption, partial failure, health, and rollback.
- Modify: `src/features/mcp/`, shared lifecycle dialog/hook code, and the UI contract lock only for real-data binding and verified direct-plan defects.
- Modify: `src-tauri/src/commands/` for scoped MCP inventory and lifecycle commands.
- Modify: `.github/workflows/quality.yml` and `platform-contracts.yml` for catalog, client configuration, and cross-platform recovery contracts.

## Implementation Steps

1. Freeze supported client versions, global MCP configuration roots, schemas, and precedence rules from Phase 2 evidence.
2. Define normalized server identity, client binding, transport, capability, auth-reference, health, trust, plan, receipt, and failure contracts matching UI Contract v1.1.
3. Implement read-only parsers for supported global client configurations with path bounding, size limits, duplicate detection, and redaction.
4. Implement canonical deduplication while retaining client-specific transport, target, and enabled state.
5. Implement HTTPS source analysis and trusted mapping resolution without executing downloaded content or accepting catalog-provided commands.
6. Implement declarative stdio mappings with explicit executable and argument arrays; reject shell strings, traversal, symlink escapes, and unknown executables.
7. Build immutable add, update, enable, disable, and remove plans that include current config digests, target clients, capabilities, credential references, and rollback evidence.
8. Revalidate configuration digest and policy immediately before mutation; require fresh consent after any change.
9. Apply per-client config changes with atomic replacement or verified backup/replace, then run bounded protocol initialization and capability discovery.
10. Persist redacted receipts and per-client outcomes; surface retry, keep-partial, rollback, and restore actions through the approved UI.
11. Add concurrency, interruption, malformed config, duplicate identity, unreachable server, auth-reference missing, partial-client failure, and rollback tests.
12. Run packaged desktop smoke for MCP inventory, reviewed add, consent invalidation, partial failure, recovery, diagnostics redaction, and zero raw-secret persistence.

## Todo

- [x] Supported MCP client configuration schemas and roots are frozen with fixtures.
- [x] Logical MCP servers deduplicate without losing per-client bindings.
- [x] URL, transport, capability, credential-reference, and config-digest fields are plan-bound.
- [x] No arbitrary shell string or catalog-provided executable reaches process spawn.
- [x] Raw credentials never enter STM persistence, receipts, diagnostics, logs, or plaintext backups.
- [x] Add, update, enable, disable, remove, partial failure, retry, keep-partial, and rollback are covered.
- [x] MCP health checks initialize the protocol without invoking domain tools.
- [x] Packaged desktop and browser-fixture behavior match locked MCP UI routes, states, actions, and copy.

## Success Criteria

- [x] Supported global MCP configurations inventory consistently across Codex, Claude Code, and Cursor.
- [x] Reviewed remote and trusted stdio server plans require explicit, digest-bound consent before any config change.
- [x] Unsupported transports, untrusted commands, malformed URLs, unknown capabilities, and missing credential references fail closed with actionable reasons.
- [x] Multi-client partial failure preserves successful bindings and offers explicit retry, keep-partial, or rollback choices.
- [x] Removal and rollback restore valid client configuration without deleting unrelated MCP entries.
- [x] Logs, receipts, diagnostics, screenshots, SQLite, and backup artifacts contain no raw MCP credential values.
- [x] The packaged app and fixture browser match UI Contract v1.1 without an intentional interface change.

## Completion Evidence

- Rust: 96 core tests pass; MCP-focused coverage includes approved stdio and remote mappings, canonical cross-client identity, client-specific entry/auth serialization, bounded discovery, protocol initialization, encrypted/tamper-evident backups, immediate per-target revalidation, concurrent stale-plan exclusion, partial failure, retry argument preservation, keep-partial dispatch, Windows atomic replacement, and rollback/recovery.
- Frontend: lint, typecheck, 16 interaction-contract tests, UI Contract v1.1 verification, approved MCP catalog verification, production build, and release Tauri build pass.
- Runtime: direct MCP actions open immutable plans without redundant source analysis; consent-gated fixture execution reaches `Simulation success`.
- Runtime core: isolated real-service disable writes an `STMMCP01` encrypted backup with no plaintext marker, then receipt-backed rollback decrypts the backup, restores exact configuration, and removes the artifact.
- Post-verification: `.artifacts/report/20260821-140455-phase-seven-mcp-safety/report.html`.

## Risk Assessment

- **Client schema drift:** version adapters and fail closed on unknown structures instead of rewriting them.
- **Command injection:** trusted declarative mappings only; executable and arguments remain separate typed fields.
- **Secret leakage:** store reference names or OS handles only and redact configuration values before persistence or diagnostics.
- **Cross-client identity collision:** keep canonical identity separate from each client binding and source digest.
- **Non-atomic client writes:** use verified backups, bounded replace sequences, and explicit reduced-confidence recovery states.
- **Remote server impersonation:** require HTTPS, normalized origin, reviewed capability metadata, and explicit trust state; never infer trust from reachability.

## Security Considerations

- MCP servers can expose powerful tools and data. Capability display, trust state, target clients, auth references, and transport are consent-critical fields.
- Never invoke an MCP domain tool while analyzing, installing, or health-checking a server.
- Strip or redact environment values, authorization headers, query secrets, home paths, and client-specific tokens from logs and receipts.
- Revalidate file ownership, symlink resolution, configuration digest, endpoint origin, and mapping identity immediately before mutation.

## Next Steps

Proceed to Phase 8 after its supported OS/architecture matrix and signing/notarization credentials are approved. Any future intentional interface change still reopens Phase 1.
