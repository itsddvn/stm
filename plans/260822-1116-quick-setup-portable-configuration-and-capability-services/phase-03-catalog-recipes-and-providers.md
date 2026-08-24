---
phase: 3
title: "Catalog Recipes And Providers"
status: pending
priority: P1
effort: "3-4d"
dependencies: [2]
---

# Phase 3: Catalog Recipes And Providers

## Overview

Replace ad-hoc mappings with typed install recipes and provider detection. No user-facing Quick Setup yet.

## Requirements

- Functional: catalog schema accepts typed recipe steps and rejects shell strings.
- Functional: provider scan reports Homebrew, Bun, and Node.js/npm path, version, and owned resources.
- Functional: resolver returns one recipe per resource using the settled macOS order.
- Functional: Orca, Oh My Pi, and Codex metadata match current official sources.
- Non-functional: unsupported or untested recipes cannot become `managed_execute`.

## Architecture

Catalog/Profile kernel owns recipe documents. Validator consumes them read-only. Resolver input is resource + target profile + detected providers + user preference + existing owner.

Fix stale sources before any execution mapping is marked supported:

- Orca: `onorca.dev` / `stablyai/orca`. Preferred macOS recipe is Homebrew cask `stablyai/orca/orca`, fallback signed DMG.
- Oh My Pi: `can1357/oh-my-pi`. Preferred macOS recipe is Homebrew tap formula, fallback npm/Bun package `@oh-my-pi/pi-coding-agent`.
- Codex: preferred Homebrew cask `codex`, fallback npm `@openai/codex`.
- GitHub CLI: add Homebrew formula mapping only after package identity is verified. Until then it stays candidate/blocked.

Add Docker Engine mappings for Ubuntu/Fedora. Arch remains guidance-only.

Replace the ten-tool invariant with an explicit platform-profile document keyed by stable target. Catalog membership, `recommended`, and default preselection are different fields. Defaults: common Git/AgentKit/Codex/cloudflared; macOS adds OrbStack/Orca/cmux; Windows adds Docker Desktop; Linux adds Docker Engine. Oh My Pi and Grok are optional and unchecked.

## Related Code Files
- Modify: `catalog/tools/recommended.json`, `catalog/tools/candidates.json`, `catalog/schemas/`
- Modify: `crates/tools-manager-core/src/catalog/mod.rs` including both ten-count sites
- Modify: `crates/tools-manager-core/src/application/service.rs` ten-count tests
- Modify: `scripts/verify-phase-three-core.mjs` length and exact-ID checks
- Create: provider detection, `ProviderInventory` snapshot, and `PreferencesStore` port
- Create: runtime provider probes that authenticate approved roots/receipts/signatures
- Modify: `crates/tools-manager-core/src/lifecycle/evidence.rs` to compare tap/publisher provenance

## Implementation Steps

1. Add recipe schema and reject unknown step types plus any command/script string fields. Keep provider-specific package-id/version deny rules.
2. Correct Orca and Oh My Pi identity/source fields.
3. Add Codex Homebrew cask recipe and keep npm fallback.
4. Add platform-profile defaults and migrate all six ten-tool enforcement points.
5. Implement authenticated provider probes and a `ProviderInventory` generation bound into plan digest.
6. Implement a durable `PreferencesStore` for provider preference and first-launch dismissal.
7. Implement resolver unit tests for existing-owner, Homebrew-first, Bun-first, and skip-unsupported cases.
8. Add Linux Docker Engine recipes for APT/DNF only.
## Todo

- [ ] Add typed recipe schema
- [ ] Fix Orca and Oh My Pi sources
- [ ] Add Codex Homebrew cask recipe
- [ ] Detect Homebrew, Bun, npm/Node
- [ ] Implement provider resolver
- [ ] Replace ten-id recommended invariant

## Success Criteria

- [ ] Catalog validation fails on shell/script fields.
- [ ] Resolver never chooses Bun for a desktop app.
- [ ] Existing npm-owned Codex stays npm-owned.
- [ ] Clean macOS Automatic preference chooses Homebrew for Codex when brew exists or will be bootstrapped.

## Risk Assessment

Official package IDs can drift. Pin identities from official docs and fail closed if live manager evidence does not match.
