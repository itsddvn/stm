---
phase: 6
title: "Portable Configuration"
status: completed
priority: P1
effort: "2-3d"
dependencies: [5]
---

# Phase 6: Portable Configuration

## Overview

Add versioned portable setup files and wire them into Quick Setup import plus Settings export/import.

## Requirements

- Functional: export writes one target profile chosen by the user.
- Functional: import on a mismatched platform is blocked with the file target and current target named.
- Functional: import preselects every resource in the file and is additive.
- Functional: export writes one target profile chosen by the user from a fresh scan.
- Functional: import on a mismatched platform is blocked with the file target and current target named.
- Functional: import preselects every resource in the file and is additive.
- Functional: latest compatible versions are resolved on the target using local preference.
- Functional: MCP exports only opaque credential-reference IDs.
- Non-functional: schema rejects secrets, machine paths, commands, receipts, provider-preference authority, oversize files, and deep nesting. Host-owned dialogs only.

Custom resources become Review required. Catalog match is required for automatic recipes.

## Related Code Files

- Create: portable-config schema under `catalog/schemas/` or `contracts/`
- Create: core portable-config service
- Modify: Settings and Quick Setup UI
- Modify: Tauri commands for export/import
- Add fixtures for valid, mismatched-target, custom-resource, and credential-reference files

## Implementation Steps

1. Define a versioned schema with target and resources. Do not import provider preference as authority.
2. Implement export after an authoritative inventory/provider scan.
3. Implement import validation, size/depth limits, and platform-target comparison.
4. Map imported catalog resources through Validator with no network probe of custom URLs.
5. Reject files that include command, script, executable, raw secret, or absolute machine-path fields. Validate credential-reference grammar and scan exported bytes for known secrets.
6. Use native host dialogs. JavaScript never supplies a filesystem path.
7. Add Settings Portable Setup and Quick Setup result export.
## Todo

- [x] Portable config schema
- [x] Export current setup
- [x] Import validation and target check
- [x] Additive preselected import
- [x] Secret/command rejection tests

## Success Criteria

- [x] macOS-exported file cannot run on Windows/Linux.
- [x] Importing a file with a custom URL does not create a managed command.
- [x] MCP secret values never appear in exported bytes.
- [x] Resources already installed as current become no-ops, not reinstalls.

## Risk Assessment

Users may expect exact-version clones. The settled policy is latest compatible. Surface that in the export/import copy so it is not treated as a bug.
