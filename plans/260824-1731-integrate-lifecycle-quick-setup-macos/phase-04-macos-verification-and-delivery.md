---
phase: 4
title: "macOS verification and delivery"
status: completed
priority: P1
effort: "1d"
dependencies: [3]
---

# Phase 4: macOS verification and delivery

## Overview

Prove the integrated code and internal macOS app, then merge and clean the integration workspace.

## Requirements

- Functional: app launches, scans live inventory, prepares real install/update plans without unsolicited mutation, and exercises isolated Skill/MCP plus native portable import/export paths.
- Non-functional: all repository quality, security, release, and contract gates pass. UI manifest remains `review` until separate project-lead approval.

## Architecture

Verification covers core contracts, runtime adapters, Tauri IPC, browser presentation, and the packaged `.app` surface.

## Related Code Files

- Modify only regressions found by gates
- Evidence: copy the final HTML report and referenced screenshots into this plan's tracked `reports/` directory before worktree removal

## Implementation Steps

1. Run Rust and frontend quality gates.
2. Run catalog, security, release, and UI validators.
3. Build an explicit internal macOS app bundle.
4. Launch and exercise live native inventory, Quick Setup review, native portable import/export through host dialogs, and isolated temporary-root Skill/MCP execute-and-recover scenarios.
5. Run code/security review, fix findings, and revalidate.
6. Copy durable evidence into plan reports.
7. Fetch remote `main`; fast-forward only if it still equals the verified donor. If it advanced, merge that exact tip and rerun all gates.
8. Commit, fast-forward `main` to the verified integration tree, push, and remove the temporary worktree.

## Todo

- [x] Full automated gates pass
- [x] Internal macOS app builds and launches
- [x] Native Quick Setup and portable import/export evidence passes
- [x] Isolated packaged Skill/MCP recovery evidence passes
- [x] Review findings resolved
- [x] Durable evidence retained
- [x] Main pushed and worktree removed

## Success Criteria

- [x] `main` contains both verified histories, remote refs match, durable evidence remains, and only the clean main worktree remains. UI locking stays explicitly outside this milestone pending project-lead approval.

## Risk Assessment

No public release claim: signing, notarization, and cross-platform signed-candidate evidence remain outside this milestone.
