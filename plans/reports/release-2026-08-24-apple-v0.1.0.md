---
title: "Apple signed candidate v0.1.0"
date: 2026-08-24
status: completed
scope: macOS arm64 and x64
---

# Apple Signed Candidate v0.1.0

## Summary

| Item | Result |
|---|---|
| Tag | `v0.1.0` at `1aa3bde` |
| GitHub run | `32792779967` |
| Environment gate | `signed-release`, required reviewer approved |
| macOS arm64 | Passed |
| macOS x64 | Passed |
| Draft release | `https://github.com/itsddvn/stm/releases/tag/untagged-997fab978c65d86a0e5f` |
| Publication | Draft only; not promoted |

## Build and Trust Evidence

- Release contract, lint, typecheck, frontend tests, desktop integration, Rust format/Clippy/tests, dependency audit, cargo-audit 0.22.2, secret scan, SBOM, and offline CodeQL result gate passed.
- Both Apple jobs imported the protected Developer ID certificate, verified the Tauri updater public key, and generated release-only configuration.
- Both jobs built `.app`, `.dmg`, `.app.tar.gz`, and updater signatures.
- The contained apps passed strict codesign and Gatekeeper with `source=Notarized Developer ID`.
- Final DMGs were separately submitted to Apple notary service, stapled, re-uploaded, and passed `spctl --type open` plus stapler validation.
- Both jobs passed local bounded artifact/checksum verification and provenance attestation.

## Independent Download Verification

Downloaded draft assets were checked outside CI:

- `STM_0.1.0_aarch64.dmg`: `sha256:57a189e97df21efd4c9507790f8c624df6e7fe7d611b07e151b64c3d9cb69ce6`
- `STM_0.1.0_x64.dmg`: `sha256:19f7d62b76894df978dc1b10dec7dd8320e1f2b4fb16aa8b112bda1f75834eea`
- `latest.json`: `sha256:3c70f65b1721dba9fbc274ef3cb42bb91f21546e30e08b2e7a9994f495566ef7`
- `latest.json.sig`: `sha256:e963e73372ef7d04f0a2f0febb079f5133da9dec6ef4b27e8604499ae00a9c97`
- GitHub API digests matched downloaded bytes.
- Bounded verifier accepted 10 Apple artifacts for version `0.1.0`.
- `stm-release-verifier` authenticated the exact signed `latest.json` and verified four updater signatures across `darwin-aarch64` and `darwin-x86_64`.

## Install and Restart Smoke

- Mounted the notarized arm64 DMG and copied `STM.app` to a temporary install location.
- App launched from the copied signed bundle with isolated `STM_DATA_DIR`.
- SQLite initialized at schema `user_version=5`.
- Same signed app relaunched against the same data successfully.
- `v0.1.0` is the first signed release; a real previous-version updater smoke is not possible until the next signed version. This same-version reinstall/restart establishes the baseline.

## Remaining Release Gates

- Draft remains unpublished.
- Windows certificate and Windows signed candidate are not configured.
- Linux/Windows matrix and full aggregate updater workflow remain incomplete.
- UI Contract remains `review` pending project-lead approval.
- Cross-platform fresh-machine and visual/accessibility evidence remain required before a full stable release claim.

## Unresolved Questions

None for the Apple-only signed candidate milestone.
