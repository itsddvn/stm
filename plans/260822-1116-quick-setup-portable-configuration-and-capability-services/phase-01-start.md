---
phase: 1
title: "Locked Architecture Contract"
status: completed
priority: P1
effort: "done"
dependencies: []
---

# Phase 1: Locked Architecture Contract

## Overview

Architecture and product decisions are accepted. Do not reopen them during implementation unless new evidence contradicts a verified invariant.

## Requirements

- [x] Five layers and three crates are approved.
- [x] Installer, updater, validator, and optimizer are in-process capabilities.
- [x] Catalog/profile, inventory/evidence, and lifecycle/operations kernels have one owner each.
- [x] No capability can bypass validation, consent, or receipt persistence.

## Architecture

See `plan.md` Settled Decisions. That section is authoritative.

## Related Code Files

- Current owners: `docs/system-architecture.md`, `crates/stm-core/src/application/service.rs`, `crates/stm-core/src/lifecycle/service.rs`, `src-tauri/src/lib.rs`.
- Target owners are defined in later phases.

## Implementation Steps

1. Treat `plan.md` as the implementation contract.
2. Start implementation at Phase 2.

## Success Criteria

- [x] Process model is in-process capabilities, not daemons.
- [x] Packaging is `stm-core`, `stm-runtime`, and `stm-desktop`.
- [x] Provider, recipe, Quick Setup, and portable-config decisions are recorded without interview noise.

## Risk Assessment

Reopening settled provider or packaging choices mid-implementation will stall delivery. New evidence only.
