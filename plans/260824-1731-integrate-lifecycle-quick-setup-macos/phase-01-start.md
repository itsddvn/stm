---
phase: 1
title: "Merge foundation"
status: completed
priority: P1
effort: "4h"
dependencies: []
---

# Phase 1: Merge foundation

## Overview

Create the integration branch from the canonical Quick Setup architecture and merge `main` so Git records both histories.

## Requirements

- Functional: retain every tracked addition from both branches.
- Non-functional: resolve architecture conflicts deliberately; no bulk ours/theirs strategy.

## Architecture

`stm-core` owns domain/application/ports. `stm-runtime` owns concrete adapters. `src-tauri` composes both.

## Related Code Files

- Modify: workspace manifests, contracts, crate module roots, plan records
- Add: donor ownership manifest mapping every legacy file and caller to `stm-core`, `stm-runtime`, or `src-tauri`
- Delete: legacy `donor-monolith/` only after every mapped body and caller is ported

## Implementation Steps

1. Merge exact donor commit `51879de84f0d520b3c00cbb92cbf3bccddc9859a` with `--no-ff`.
2. Inventory conflict classes and record every donor file/callsite in the ownership manifest.
3. Resolve contracts and manifests, then complete every mapped move before deleting legacy paths.

## Todo

- [x] Merge exact main donor history
- [x] Resolve workspace and contract foundation
- [x] Complete donor ownership manifest and moves
- [x] Confirm zero legacy crate paths

## Success Criteria

- [x] Index has no unresolved entries and canonical crate topology is intact.

## Risk Assessment

Blind conflict selection can silently drop live Quick Setup or lifecycle recovery. Compile after each ownership boundary.
