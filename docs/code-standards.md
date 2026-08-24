# STM Code Standards

Status: backend standards established in Phase 2 on 2026-08-20.

## General

- Keep the React UI contract authoritative. Backend code adapts to locked DTOs; it does not renegotiate them.
- Keep the Rust core independent from Tauri.
- Prefer focused modules over speculative abstractions. Split by stable contract or boundary, not by novelty.
- Use ASCII by default.

## Security

- No shell strings. Process execution always uses allowlisted executables with explicit arg arrays.
- Reject project-local skill roots and path escapes after normalization plus canonicalization.
- Never store raw MCP secret values. Persist only redacted references or state.
- Treat source URLs as untrusted input. Strip fragments and queries before recording them.

## Data contracts

- Domain contracts live in `crates/stm-core/src/domain/`.
- UI-facing DTOs live in `crates/stm-core/src/application/dto.rs`.
- JSON schemas under `catalog/schemas/` must describe the durable serialized contract, not transient implementation detail.
- Keep Phase 2 schema filenames stable because CI and feasibility evidence refer to them directly.

## Testing

- Prefer deterministic fixtures for cross-platform feasibility work.
- Use narrow unit tests first: parser, root guard, process policy, and SQLite migration.
- Manager fixtures must encode explicit non-success states through metadata instead of shelling out.
- MCP fixtures must cover duplicate logical bindings, disabled client bindings, malformed entries, unsupported schemas, and redacted auth references.
- Shared CI never mutates a persistent host package manager. Phase 5 uses deterministic state/command contracts plus cross-platform core tests; real Homebrew formula/cask, npm, APT, DNF, and WinGet mutation tests run only in disposable CI environments and refuse to run without `STM_DISPOSABLE_LIFECYCLE=1`.

## Desktop host

- Tauri commands stay narrow, typed, and compiled into explicit named application allowlists.
- The host may coordinate state and command registration only; business logic belongs in the core.
- Capabilities are deny-by-default and list only the exact commands exposed to the webview.
