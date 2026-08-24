---
phase: 3
title: "Desktop and localization integration"
status: completed
priority: P1
effort: "1d"
dependencies: [2]
---

# Phase 3: Desktop and localization integration

## Overview

Compose all integrated services in Tauri while preserving bilingual, concise lifecycle presentation and product-update separation.

## Requirements

- Functional: Quick Setup, Skill, MCP, history, settings, and typed product-update availability work through IPC.
- Non-functional: Vietnamese remains default; English persistence and collapsed technical details remain intact. Internal builds must not register or expose the updater plugin.

## Architecture

Tauri owns platform dialogs. Product-update plugin registration is gated by protected release configuration; internal builds return typed unavailable state. React owns presentation only; all execution authority remains behind typed commands and opaque plan IDs.

## Related Code Files

- Modify: `src-tauri/src/`, permissions, generated schemas, Tauri manifests
- Modify: `src/`, `contracts/ui/`, localization catalog and tests

## Implementation Steps

1. Combine Quick Setup provider bootstrap with mode-gated product-update dispatch.
2. Compose Skill/MCP/runtime services in `AppState`.
3. Merge dialog permissions and release-only updater registration.
4. Preserve localized error, duplicate-start prevention, and concise review/result UI.
5. Regenerate owned schemas from the resolved application.

## Todo

- [x] Integrate Tauri state and commands
- [x] Gate updater plugin to protected release mode
- [x] Integrate localized lifecycle UI
- [x] Regenerate permissions and schemas

## Success Criteria

- [x] Frontend typecheck and desktop integration tests pass with all capabilities exposed, and an internal-build test proves updater registration remains disabled.

## Risk Assessment

Generated schemas are outputs, not conflict authorities. Regenerate them only after source permissions and plugins are correct.
