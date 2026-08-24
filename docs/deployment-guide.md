# Deployment Guide

STM public releases are signed draft candidates first. This repository can build unsigned internal binaries, but an unsigned artifact is never public-release ready and never receives updater metadata.

## Local internal build

```text
pnpm install --frozen-lockfile
pnpm verify:release-contract
pnpm verify:ui-contract
pnpm lint
pnpm typecheck
pnpm test
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm tauri:build
```

The base `src-tauri/tauri.conf.json` keeps bundling and release updater artifacts disabled. This prevents a developer build from looking like a signed release.

## Protected release configuration

`src-tauri/tauri.release.conf.json` defines release-only CSP, bundles, updater artifacts, and the fixed stable endpoint. `pnpm release:config <output>` requires `TAURI_UPDATER_PUBLIC_KEY`, injects it into a mode-0600 generated config, and fails without a valid value. Windows also requires `WINDOWS_CERTIFICATE_THUMBPRINT`.

The generated file belongs under `target/release-config/`; never commit it. The updater private key remains in `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` for Tauri artifact signing.

## GitHub release candidate

1. Create and push an annotated `v<semver>` tag after version references and release notes are reviewed.
2. Configure required secrets in the protected `signed-release` environment. Require human approval for environment access.
3. Run `.github/workflows/release.yml` by tag or manual dispatch.
4. The workflow verifies release/UI/quality/security gates on the exact peeled tag commit, builds the stable matrix, signs/notarizes native installers, creates updater signatures, signs the exact aggregate `latest.json`, cryptographically verifies the manifest and every local artifact/signature pair, attests bundles, and retains a draft release plus CI artifacts.
5. Do not publish the draft yet. Run fresh-machine checks on every stable matrix row:
   - fresh install and launch;
   - all seven locked routes and critical viewport checks;
   - inventory and diagnostics redaction;
   - manager-missing and offline states;
   - one supported managed tool lifecycle and vendor handoff;
   - skill and MCP review/consent/mutation/recovery;
   - previous-version update to the candidate, restart, state migration, invalid-signature rejection, and interrupted-download recovery.
6. Download the draft assets and run both `stm-release-verifier` (exact Minisign verification for `latest.json.sig` and updater artifacts) and `pnpm verify:release-artifacts <bundle-root> <version> latest.json` (bounded structure/checksums). Compare `release-checksums.json` with CI evidence and verify native Authenticode/codesign/notarization results.
7. Promote the draft manually only when every stable row passes. Publish the authenticated `latest.json` and `latest.json.sig` pair last so clients cannot discover an unapproved candidate.

## Version and channel policy

Stable releases use annotated SemVer tags reachable from protected `main` and the fixed endpoint `https://github.com/itsddvn/stm/releases/latest/download/latest.json`. STM authenticates the exact manifest, enforces monotonic version/digest state, revalidates the consented target/URL/signature fingerprints, and then lets Tauri verify artifact bytes before installation. Downgrades, replayed or drifting metadata, wrong-target artifacts, missing/invalid signatures, corrupt downloads, HTTP URLs, and credentialed URLs are rejected.

A beta or alternate channel requires a separately approved plan, endpoint, signing policy, and UI decision. It must not reuse stable metadata accidentally.

## Rollback and reinstall

Product rollback is reinstall of the previous signed installer; STM never authorizes product rollback from tool, skill, or MCP receipts.

1. Quit STM and preserve its application data directory.
2. Download the previous signed installer from the release history and verify its published checksum/signature.
3. Reinstall the application. Do not delete SQLite, global Agent Skill roots, tool-manager state, or MCP client configurations.
4. Launch STM and run diagnostics plus a read-only inventory refresh.
5. If a schema migration blocks downgrade, reinstall the current signed version and restore the last-good application snapshot. Never manually edit lifecycle receipts to force a downgrade.

Corrupt updater downloads may be deleted from the operating system's temporary cache; user inventory/configuration state is not an updater cache and must remain untouched.

## Credential rotation

Apple and Windows signing certificate rotation occurs in the protected environment without changing application trust semantics. Updater signing-key rotation requires an application release signed by the old trusted key that embeds the new public key, followed by protected-secret rotation. If the old key is compromised, stop updater publication and distribute the trust-root replacement through independently verified signed installers.
