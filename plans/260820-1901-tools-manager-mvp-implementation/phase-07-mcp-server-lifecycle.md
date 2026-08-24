---
phase: 7
title: "Phase 7: MCP Server Lifecycle"
status: todo
priority: P1
effort: "3-5 engineer-weeks"
dependencies: [4, 5, 6]
ui_gate: "UI Contract v1.1 approved and locked; reopen Phase 1 before intentional UI change"
external_gate: "Supported MCP client schemas, trust policy, and credential-reference mechanism approved"
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

Implement first-class MCP server inventory and reviewed lifecycle management for supported Codex, Claude Code, and AgentKit global configurations. Reuse the immutable plan, explicit consent, receipt, partial-failure, and recovery substrate from tool and skill lifecycle work without treating client-specific MCP configuration as a generic package install.

## Key Insights

- One logical MCP server may be configured in several clients with different transports, capability grants, credential references, and health states.
- Remote URLs, package identifiers, arguments, environment-variable names, and capability declarations are untrusted inputs until normalized and policy-checked.
- STM stores credential references only. Secret values remain in OS credential facilities, environment configuration, or the owning client's approved auth flow.
- Adding, updating, enabling, disabling, or removing an MCP server is a configuration mutation with an immutable reviewed plan and per-client outcomes.
- Arbitrary shell strings are never accepted. Local stdio launch plans come from trusted declarative mappings with an executable and typed argument array.

## Requirements

- [ ] Verify UI Contract v1.1 before implementing MCP behavior or binding real data.
- [ ] Discover supported global MCP configuration locations without traversing project roots.
- [ ] Normalize logical server identity across Codex, Claude Code, and AgentKit while retaining each client binding.
- [ ] Model stdio and Streamable HTTP transports, capability declarations, auth kind, health, trust, and per-client state.
- [ ] Analyze pasted HTTPS source URLs without executing remote content or trusting catalog claims.
- [ ] Build immutable add, update, enable, disable, remove, retry, and rollback plans from approved mappings only.
- [ ] Require explicit consent bound to server identity, transport, endpoint or executable, arguments, targets, capabilities, credential-reference names, and expected config digest.
- [ ] Apply client configuration changes atomically when supported; otherwise use verified backup/replace/recovery sequences and report per-client outcomes.
- [ ] Never persist raw tokens, passwords, OAuth refresh tokens, private keys, or copied environment values in STM storage, receipts, diagnostics, or logs.
- [ ] Reopen Phase 1 before any intentional MCP route, state, action, copy, interaction, responsive, accessibility, or visual change.

## Architecture

The MCP lifecycle pipeline is: discover client configuration → parse bounded schema → normalize logical server and client bindings → resolve trusted mapping or reviewed remote source → build immutable plan → revalidate current digest → collect consent → apply per-client configuration → health-check without invoking arbitrary tools → persist redacted receipts and recovery data → emit locked UI view models.

Credential fields contain reference metadata only, such as an environment-variable name or OS credential handle. Health checks use protocol initialization and capability listing with bounded timeouts; they never call domain tools during installation verification.

## Related Code Files

- Create: `/Users/itsddvn/projects/tools-managers/catalog/schemas/mcp-server.schema.json` and approved MCP mapping data
- Create: `/Users/itsddvn/projects/tools-managers/crates/stm-core/src/mcp/` for discovery, normalization, planning, policy, health, receipts, and recovery
- Create: `/Users/itsddvn/projects/tools-managers/crates/stm-core/src/adapters/mcp-clients/` for Codex, Claude Code, and AgentKit configuration adapters
- Create: `/Users/itsddvn/projects/tools-managers/tests/fixtures/mcp/` with redacted stdio, Streamable HTTP, conflict, partial-failure, and recovery fixtures
- Modify: `/Users/itsddvn/projects/tools-managers/crates/stm-core/src/planning/`, `policy/`, `storage/`, and `operations/` to reuse shared immutable plan and receipt behavior
- Modify: `/Users/itsddvn/projects/tools-managers/src/features/mcp/` only for approved real-data binding and verified defects
- Modify: `/Users/itsddvn/projects/tools-managers/src-tauri/src/commands/` for scoped MCP inventory and lifecycle commands
- Modify: `/Users/itsddvn/projects/tools-managers/.github/workflows/platform-contracts.yml` for client configuration and recovery tests

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

- [ ] Supported MCP client configuration schemas and roots are frozen with fixtures.
- [ ] Logical MCP servers deduplicate without losing per-client bindings.
- [ ] URL, transport, capability, credential-reference, and config-digest fields are plan-bound.
- [ ] No arbitrary shell string or catalog-provided executable reaches process spawn.
- [ ] Raw credentials never enter STM persistence, receipts, diagnostics, or logs.
- [ ] Add, update, enable, disable, remove, partial failure, retry, and rollback are covered.
- [ ] MCP health checks initialize the protocol without invoking domain tools.
- [ ] Packaged desktop behavior matches locked MCP UI routes, states, actions, and copy.

## Success Criteria

- [ ] Supported global MCP configurations inventory consistently across Codex, Claude Code, and AgentKit.
- [ ] Reviewed remote and trusted stdio server plans require explicit, digest-bound consent before any config change.
- [ ] Unsupported transports, untrusted commands, malformed URLs, unknown capabilities, and missing credential references fail closed with actionable reasons.
- [ ] Multi-client partial failure preserves successful bindings and offers explicit retry, keep-partial, or rollback choices.
- [ ] Removal and rollback restore valid client configuration without deleting unrelated MCP entries.
- [ ] Logs, receipts, diagnostics, screenshots, and SQLite contain no raw MCP credential values.
- [ ] The packaged app matches UI Contract v1.1 without reopening Phase 1.

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

Proceed to Phase 8 only after supported MCP client inventory and lifecycle behavior pass locked UI, security, recovery, and packaged smoke gates. Reopen Phase 1 before any intentional interface change.
