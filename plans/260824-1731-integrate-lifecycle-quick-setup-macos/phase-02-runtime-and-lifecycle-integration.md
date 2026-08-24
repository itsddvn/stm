---
phase: 2
title: "Runtime and lifecycle integration"
status: completed
priority: P1
effort: "2d"
dependencies: [1]
---

# Phase 2: Runtime and lifecycle integration

## Overview

Port Skill, MCP, storage, keyring, HTTP, and updater lifecycle behavior into the split architecture without returning concrete infrastructure to `stm-core`.

## Requirements

- Functional: preserve immutable plans, consent, revalidation, receipts, backups, recovery, and partial-failure behavior.
- Non-functional: concrete SQLite/filesystem/network/process/keyring implementations remain in `stm-runtime`.

## Architecture

Add typed core ports and domain records where donor code currently depends on concrete stores or host functions. Implement those ports in runtime and inject them through `LifecycleService`/application composition.

## Related Code Files

- Modify: `crates/stm-core/src/{ports,lifecycle,mcp,skills,storage}`
- Modify: `crates/stm-runtime/src/` and `crates/stm-runtime/migrations/`
- Modify: `Cargo.toml`, crate manifests, `Cargo.lock`

## Implementation Steps

1. Move migrations and concrete stores to runtime.
2. Split Skill catalog/source/materialization policy from runtime effects.
3. Split MCP planning/policy from config, backup, health, and keyring effects.
4. Integrate updater/release verification without coupling tool lifecycle.
5. Add named regression coverage for authorization expiry before each mutating child, fresh provider identity after bootstrap, single-use consent, pre-spawn durable receipts, per-child checkpoint/postcondition journals, restart recovery, partial failure, and digest-safe rollback.

## Todo

- [x] Integrate storage and migrations
- [x] Integrate Skill lifecycle through ports
- [x] Integrate MCP lifecycle through ports
- [x] Integrate product updater contracts

## Success Criteria

- [x] `cargo check --workspace` passes and core has no concrete infrastructure dependency regression.
- [x] Focused lifecycle safety tests cover and pass every invariant named in step 5 before desktop composition begins.

## Risk Assessment

The donor branch assumes a monolithic crate. Copying modules unchanged would violate the accepted architecture and create dependency cycles.
