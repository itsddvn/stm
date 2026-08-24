# Red Team — Quick Setup / remaining plan

Date: 2026-08-22
Scope: worktree `tools-managers-quick-setup-capabilities`
Personas: adversary, supply-chain, insider, infra
Mode: audit only. No fix.

Threat model: local-first Tauri app. Attacker can write user-writable paths, hand the user a setup JSON, or drive IPC from the webview. Not a multi-tenant web API.

## Verdict

**Do not “finish everything.”** Current Quick Setup is not a safe install surface. `get_quick_setup` can spawn binaries without consent. Client `mappingId` can change owner without the migration machine. Portable import does not use the rust validator. UI contract is still marked locked after we rewrote the lock.

Cook of remaining P4 bootstrap (Homebrew `.pkg` / Bun download) on this base would multiply supply-chain risk.

## Coverage

Personas: Adversary[x] Supply-chain[x] Insider[x] Infra[x]
STRIDE: S[x] T[x] R[~] I[x] D[~] E[x]
OWASP: A01[x] A03[x] A04[x] A05[x] A08[x] A10[-]

## Findings

| # | Sev | Persona | STRIDE / OWASP | Location | Proof |
|---|-----|---------|----------------|----------|-------|
| 1 | Critical | Infra / Adversary | EoP / A03 | `providers.rs:29-32,95-101`; `app.tsx:23-33`; `commands.rs:167-173` | `detect_named` runs `Command::new(path).arg("--version")` **before** `classify_trust`. `get_quick_setup` is allowlisted as read. `App` calls it on first view load, so **startup** can execute `~/.bun/bin/{brew,npm,node,bun}` with no dialog and no consent. |
| 2 | Critical | Infra | Spoof / A08 | `providers.rs:41-59,82-88` | Symlink in `/opt/homebrew/bin` or `~/.bun/bin` is accepted if the **target** is any executable. Trust is classified on the **link path**, not the canonical target. `~/.bun/bin` is user-writable and approved for every name (`brew`, `npm`, `node`, `bun`). Missing official brew → `~/.bun/bin/brew` is trusted Homebrew. |
| 3 | High | Adversary | Tamper / A04 | `planner.rs:377-388,617-624` | Typed children copy `desired_action` and `mapping_id` from the webview. `requested_mapping` accepts any catalog mapping string. No ownership preflight. Client can prepare `codex-cli` + `homebrew:codex` and get a consentable install plan that switches owner. Plan requires the 8-step migration machine (phase 5). Not implemented. Review UI shows mapping/owner (`lifecycle-plan-review.tsx:31-33`) only if the user reads the grid. |
| 4 | High | Adversary | Tamper / A04 | `quick-setup-dialog.tsx:59-75` | Import is `JSON.parse` in the renderer. Rust `PortableSetupDocument::validate_bytes` (64 KiB, `deny_unknown_fields`, credential grammar) is never called. `command` / `script` / extra fields are ignored, not rejected. Target check is hardcoded `macos_arm64` and allows any `macos*` silently. Unbounded `file.text()`. |
| 5 | High | Supply-chain | Integrity / A08 | `contracts/ui/ui-contract.manifest.json:3-6` + regenerated `ui-contract.lock.json` | Manifest still `status: "locked"` with project-lead approval `2026-08-21`. Lock digests were rewritten in-session to match drifted artifacts. Phase 5 required `review`; phase 7 is the only lock writer. Locked contract is no longer an integrity signal. |
| 6 | High | Plan / Supply-chain | Insecure design / A04 | `plan.md` bootstrap § + stub `capabilities/installer.rs` | Remaining “finish all” work would download Homebrew `.pkg` / Bun binary. Installer/updater are empty wrappers. No pin/digest/Team ID code exists. Implementing that on findings 1–3 is remote-code-as-a-feature. |
| 7 | Medium | Insider | Disclosure / A01 | `src-tauri/src/state.rs:51` `preferences.rs:23-35` | Preferences live at `<repo>/target/stm-runtime/stm-preferences.json`. World-writable in a normal workspace. Invalid JSON silently becomes defaults (`unwrap_or_default`). Dismiss / preference can be reset or forged. |
| 8 | Medium | Adversary | Tamper | `planner.rs:423-436` | `setup-queue` + unknown `itemIds` defaults `action` to `"install"`. Unknown id is not hard-fail; it proceeds to catalog lookup. Combined with client children this is a second install path. |
| 9 | Medium | Insider | Repudiation / A09 | export `quick-setup-dialog.tsx:82-87` | Export does not rescan, does not byte-scan secrets, does not restrict MCP credential refs. Plan phase 6 required all three. Today it dumps selected row actions only — low secret risk, but the Settings/MCP path is unspecified and will be wrong if naively extended. |
| 10 | Low | Spec | A04 | `capabilities/installer.rs`, `updater.rs`, `optimizer.rs` | Capability modules do not install/update. Success criteria “supported tools install from one review” is unmet. Fixture banner says simulation (`lifecycle-plan-review.tsx:24`). Shipping this as done is a product lie, not RCE. |
| 11 | Medium | Adversary | Tamper / A04 | `planner.rs:353-388`; `lifecycle.rs:30-39`; `service.rs:691-717` | Typed `children` silently win over `itemIds`. Child `dependsOn` is dropped. Batch runs sequentially with no DAG. Caller can disagree the two lists and skip declared parents. |
| 12 | Medium | Supply-chain | Tamper / A08 | `catalog/mod.rs:283-306` | Catalog reject list is exact keys `command\|args\|executable\|shell`. `script` and case variants are ignored (`deny_unknown_fields` absent). They do not execute today; they can hide in a “locked” catalog. |

Rejected / not real in this threat model:
- JWT / session / IDOR web patterns — no remote users.
- Hardcoded cloud secrets — grep clean on `src/`, `crates/`, `src-tauri/`.
- Import executing file-supplied argv — import only toggles existing row ids; no command sink on that path **today**.

## Do not implement next

1. Homebrew `.pkg` / Bun download.
2. Silent owner migration.
3. Crate-split + install semantics in the same change (plan order).

## Fix order if asked

1. Probe only after trust of **canonical** path; never spawn from `~/.bun/bin` for `brew`/`npm`/`node`.
2. Ignore client `mappingId` unless it matches inventory owner or an explicit reviewed migration intent.
3. Route import through `validate_bytes` + `validate_portable_document`.
4. Move UI contract to `review` or stop claiming lock.
5. Then, and only then, P4 bootstrap with pinned artifacts.


## Re-audit after remediations

Fixed in-tree:
- #1 startup spawn: `App` uses `getSetupPreferences` only; provider discovery no longer executes provider binaries.
- #2 bun-home brew/npm spoof: `~/.bun/bin` only trusts canonical `bun`/`bunx`; it never grants trust to brew/npm/node names.
- #3 client mappingId: accepted only if it matches current installed owner mapping.
- #4 import: `validatePortableSetupText` + rust `validate_bytes`.
- #5 lock lie: manifest `status: review`, approval null. Verifier skips digest unless locked.
- #7 prefs path: `~/Library/Application Support/stm` (override `STM_DATA_DIR`), file mode 0600.
- #8 unknown setup-queue ids: malformed input.
- #9 export: regex secret-shape reject.
- #11 dependsOn: rejected until DAG exists; children/itemIds must agree.
- #12 catalog denylist includes `script` and is case-insensitive.

Still open outside this reviewed slice:
- Bun bootstrap has no verified catalog recipe requiring it yet.
- Optimizer apply UX remains an explicit non-goal.

Provider discovery no longer executes provider binaries. Homebrew planning requires a package receipt; managed probes run only through bounded reviewed identities.

## Homebrew bootstrap review

- Official `Homebrew.pkg` 6.0.18: URL, SHA-256, Team ID `927JGANW46`, package ID `sh.brew.homebrew`.
- Per-hop HTTPS allowlist, 200 MiB cap, 180s preparation timeout, private cache, hostile entry cleanup.
- Exact Apple Installer.app binding; native operation has no STM cancel/timeout and holds manager lock until close.
- Postcondition requires exact receipt version, fresh install-time, receipt ownership of `bin/brew`, and a bounded identity-bound `brew --version`.
- Dependents bind a static recipe fingerprint, compile only after provider postcondition, check parent expiry, and persist each child checkpoint.
- Native batch evidence and preparation errors are user-visible.

Accepted threat boundary, explicitly selected by the user:
- Malware/process already executing with the same UID is outside scope.
- Same-UID provider/artifact tampering and the final revalidation-to-kernel-open micro-window are accepted residuals.
- Remote supply-chain, compromised-webview authorization, fake signer/digest, handler hijack, stale receipt, hidden child, and checkpoint-loss paths remain in scope.

Final gates: tester PASS; code review 10/10; security 0 Critical / 0 High / 0 Medium excluding the accepted residual.
**Status:** DONE
**Summary:** Quick Setup and Homebrew bootstrap security findings remediated; same-UID tampering explicitly excluded from the product threat model.
