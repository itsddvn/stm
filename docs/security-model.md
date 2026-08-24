# Security Model

## Protected assets

STM protects user tool-manager state, global Agent Skill trees, MCP client configurations and credential references, lifecycle receipts, signed catalog state, signed product update state, and release signing identities. The webview receives typed inventory and lifecycle commands only; it has no generic shell, filesystem, database, updater-plugin, or privilege capability.

## Trust roots

- Tool execution mappings are compiled policy plus reviewed live manager evidence. Catalog data cannot inject executables or shell strings.
- Skill catalogs use a dedicated detached Ed25519 trust root, monotonic version/expiry checks, immutable Git commit/tree provenance, and bounded staged validation.
- MCP stdio and remote mappings bind executable or endpoint, arguments, clients, capabilities, and credential references. Missing references and unknown mappings fail closed.
- STM product updates use the Tauri updater Minisign public key injected only into protected release builds. Product update plans, operations, receipts, and recovery are separate from tool, skill, and MCP state.

A compromise of the metadata endpoint does not grant the updater signing key. A compromise of a tool or skill catalog key does not grant the product update key.

## Credential and secret handling

Raw MCP tokens, passwords, authorization headers, refresh tokens, private keys, release keys, and administrator credentials are never persisted in SQLite, receipts, diagnostics, logs, screenshots, or plaintext backup files. MCP client-config backups use authenticated XChaCha20-Poly1305 encryption; the encryption key is created and read through macOS Keychain, Windows Credential Manager, or Linux Secret Service.

Release credentials exist only in the protected GitHub `signed-release` environment:

- `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
- `TAURI_UPDATER_PUBLIC_KEY`
- Apple certificate, Apple ID app password, signing identity, and team ID
- Windows PFX, password, and certificate thumbprint

Workflows test presence without printing values. Generated release config has mode 0600 and is never committed or uploaded separately.

## Mutation and recovery invariants

Every mutation requires a fresh immutable plan, exact digest/expiry authorization, immediate evidence revalidation, and authoritative path/executable checks. Tool operations serialize per manager. Skill writes deduplicate canonical physical targets. MCP writes use persistent cross-process locks, all-target preflight, immediate per-target digest revalidation, atomic replacement, encrypted receipts, and digest-safe rollback. Recovery never overwrites user state that differs from the reviewed replacement.

Windows MCP replacement uses `ReplaceFileW` or `MoveFileExW`; it never deletes the live target before replacement. Unix replacement uses same-filesystem rename.

## Product update boundary

The Rust host downloads `latest.json` and `latest.json.sig` from fixed credential-free HTTPS endpoints, verifies the exact manifest bytes with the embedded Minisign public key, validates every artifact URL/signature field, and records a monotonic version-plus-digest acceptance record. Product plans bind the exact URL, target, artifact-signature fingerprint, and signed-manifest digest; the same evidence is reverified after consent. Tauri then verifies the downloaded artifact bytes before installation.

Product consent is single-use and product installation is globally serialized. A durable pending-install record exists before native installation starts. If Windows exits during installation, the next launch reconciles the installed application version into a separate terminal product receipt.

Release promotion is manual after native signatures/notarization, signed manifest and artifact verification, checksum, provenance, SBOM, updater, and fresh-machine smoke gates pass. The previous signed installer remains available for reinstall. Reinstall and rollback guidance preserves user tool-manager state, global skill roots, and MCP client configurations.

## Threat response

- Metadata endpoint compromise: exact manifest authentication and monotonic version/digest state reject modified metadata, same-version drift, and replayed older manifests; stop draft promotion and investigate the endpoint without assuming a key compromise.
- Product signing-key compromise: revoke the protected environment secret, rotate the public key through a separately signed application release, and do not trust manifests or artifacts signed only by the compromised key.
- Skill catalog key compromise: revoke the catalog key through an application trust-root update; product signing remains independent.
- Local path/symlink race or stale consent: operation fails closed and requires a fresh plan.
- Log or diagnostic leakage: treat as a security incident; preserve redacted evidence and rotate any exposed external credential.

Report vulnerabilities privately to the repository owner. Do not include live credentials, private configuration, or personal machine paths in an issue.
