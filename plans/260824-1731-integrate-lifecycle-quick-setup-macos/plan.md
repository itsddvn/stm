---
title: "Integrate lifecycle Quick Setup macOS"
description: "Unify the canonical stm-core/stm-runtime Quick Setup branch with Skill, MCP, updater, and release work from main, then deliver a verified internal macOS app."
status: completed
priority: P1
effort: "multi-day"
tags: [integration, macos, quick-setup, lifecycle]
created: 2026-08-24
---

# Integrate lifecycle Quick Setup macOS

## Overview

Use `feat/quick-setup-capabilities` as the architecture base. Merge `main` deliberately, retain the canonical `stm-core`/`stm-runtime` dependency direction, and produce one tested branch containing live Quick Setup, Skill lifecycle, MCP lifecycle, product updater, and release contracts.

## Outcome

One branch and internal macOS `.app` containing every accepted capability. No `donor monolith` compatibility crate, duplicate runtime implementation, fixture-backed native action, or unverified conflict resolution remains.

## Constraints

- Preserve `stm-core` as policy/application code and `stm-runtime` as concrete SQLite, HTTP, process, keyring, filesystem, and OS adapters.
- Preserve Vietnamese default, persistent English switching, concise review, native confirmation, and live install/update behavior.
- Preserve Skill/MCP receipt, backup, recovery, and release verification contracts from `main`.
- Never execute unsolicited package mutations during verification.

## Non-goals

- Public signing, notarization, Windows/Linux release publication, or updater promotion.
- Locking the UI contract before native portable-import evidence and project-lead approval.
- Backward aliases for `donor monolith`.

## Phases

| # | Phase | Status | Dependency |
|---|---|---|---|
| 1 | [Merge foundation](./phase-01-start.md) | Completed | None |
| 2 | [Runtime and lifecycle integration](./phase-02-runtime-and-lifecycle-integration.md) | Completed | 1 |
| 3 | [Desktop and localization integration](./phase-03-desktop-and-localization-integration.md) | Completed | 2 |
| 4 | [macOS verification and delivery](./phase-04-macos-verification-and-delivery.md) | Completed | 3 |

## Dependencies

- Source branch: `origin/feat/quick-setup-capabilities`.
- Donor branch: `main` at `51879de`.
- Existing Quick Setup and MVP plans remain historical capability authorities until this integration passes.

## Success Criteria

- [x] Merge commit contains both branch histories and zero unresolved conflicts.
- [x] Canonical crate names and dependency direction remain enforced.
- [x] Rust, frontend, desktop integration, security, release, and contract gates pass.
- [x] Internal macOS `.app` builds and launches.
- [x] Native live inventory and Quick Setup review pass without mutating packages.
- [x] Integrated branch merges into `main`, pushes successfully, and temporary worktree is removed.
