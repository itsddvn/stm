---
phase: 2
title: "Core Runtime Split"
status: completed
priority: P1
effort: "2-3d"
dependencies: [1]
---

# Phase 2: Core Runtime Split

## Overview

Split infrastructure out of `stm-core` and introduce capability module boundaries. Behavior stays the same.

## Requirements

- Functional: desktop inventory, plan, start, status, and cancel still work through the same Tauri commands.
- Functional: core compiles without Tauri, rusqlite, keyring, reqwest, libc, or windows-sys as direct dependencies.
- Non-functional: no install-semantics change in this phase.
- Non-functional: existing lifecycle tests keep passing after the move.

`stm-runtime` implements ports: manager adapters, process supervisor, source probe, SQLite, keyring, filesystem, HTTP. Adding this crate makes four workspace members including existing `release-verifier`.

`stm-desktop` constructs one `Arc<LifecycleCoordinator>` / `LifecycleService` and injects that same instance into every capability and into `PhaseThreeApplicationService`. Keep the existing facade and its 20 desktop callers. Do not invent a second application service in this phase.

`stm-runtime` implements ports: manager adapters, process supervisor, source probe, SQLite, keyring, filesystem, HTTP.

`stm-desktop` constructs `ApplicationServices` with runtime impls.

Add empty-but-typed capability modules:

```text
crates/stm-core/src/capabilities/{installer,updater,validator,optimizer}/
```

Do not implement new recipes here. Facades may forward to current `LifecycleService` until Phase 4.

- Create: `crates/stm-runtime/`
- Modify: `Cargo.toml`, `crates/stm-core/Cargo.toml`, `crates/stm-core/src/lib.rs`
- Move: adapters, storage, process supervisor, source probe, and other IO impls from core into runtime
- Modify: `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`, `src-tauri/src/state.rs`
- Keep unchanged: `src-tauri/src/commands.rs` call sites and `PhaseThreeApplicationService` public methods
- Keep: product updater in `src-tauri/src/product_update.rs`

## Implementation Steps

1. Add the runtime crate and workspace membership.
2. Extract port traits that core already needs but currently calls as concrete types.
3. Move IO implementations into runtime.
4. Add capability module skeletons that all receive the shared coordinator.
5. Keep `PhaseThreeApplicationService` as the desktop facade; inject the shared coordinator behind it.
6. Add a composition test that two capability handles cannot run the same manager concurrently.
7. Run existing core and desktop tests. Do not add product features.
## Todo

- [x] Create `stm-runtime`
- [x] Remove infrastructure deps from core
- [x] Add capability module skeletons
- [x] Update desktop composition root
- [x] Keep existing tests green

## Success Criteria

- [x] `cargo test --workspace` passes.
- [x] Core package manifest has no OS/SQLite/keyring/HTTP impl deps.
- [x] Tauri commands still map to the same user-visible behavior.

## Risk Assessment

A wide move can break relative `include_str!` catalog paths and fixture workspace roots. Keep catalog/fixture path resolution explicit and test it first.
