# Tools Manager — Product Report Audit

**Version:** 0.2.0
**Date:** 2026-08-20
**Status:** Final
**Source:** `researcher-2026-08-20-tools-manager-market-and-mvp.md` v0.3.1 reviewed; fixes applied in v0.4.0
**Owner:** Project lead

---

## 1. Review decision

The report has a sound product direction and preserves the confirmed user decisions. It needs two contract corrections and four clarifications before becoming the implementation-plan authority.

Do not change the selected Recommended tools, multi-group classification, desktop-first direction, global-only skill scope, or consent-first update policy. The required changes refine readiness and lifecycle semantics rather than reverse product scope.

## 2. Findings at a glance

| ID | Severity | Finding | Disposition |
|---|---|---|---|
| F1 | Major | Tool-level `ready` hides mapping-level readiness | Applied in source report v0.4.0 |
| F2 | Major | Vendor/self-updater execution contract conflicts with manager-only lifecycle | Applied in source report v0.4.0 |
| F3 | Medium | Overlapping global skill roots are not normalized before scanning | Applied in source report v0.4.0 |
| F4 | Medium | Ownership and privilege rules lack platform-specific boundaries | Applied in source report v0.4.0 |
| F5 | Medium | Application self-update is mixed with managed-tool update concerns | Applied in source report v0.4.0 |
| F6 | Medium | Delivery phases are too broad for safe execution | Applied in source report v0.4.0 and implementation plan |

## 3. Detailed findings

### F1 — Split canonical verification from mapping readiness

**Evidence:** The report defines `catalog_status = ready` when one safe lifecycle path exists, then assigns that status to all ten Recommended tools even when support varies by platform and owner (`source:255–272`). Catalog validation and acceptance criteria are mapping-specific (`source:97–109`, `source:327–332`).

**Risk:** A global Ready label can be mistaken for permission to install, update, or uninstall on an unverified OS, architecture, distribution, or ownership path.

**Required correction:**

- Keep `recommended` as the user-confirmed, tool-level curation flag.
- Replace the overloaded Ready meaning with two axes:
  - canonical catalog status: `verified`, `candidate`, or `blocked`;
  - mapping lifecycle status: `ready`, `detect_only`, `handoff_only`, `unsupported`, or `blocked`.
- Gate every lifecycle action by the selected mapping, detected owner, and platform—not by `recommended` or canonical verification.
- Describe the ten selected tools as recommended and canonically verified; do not claim every lifecycle mapping is ready until its contract tests pass.

### F2 — Make updater execution modes explicit

**Evidence:** The product contract says lifecycle runs through trusted owning managers (`source:20–25`, `source:140–147`). Update detection allows vendor/self-updater handoff (`source:348–355`), while every adapter is required to expose install/update/uninstall plan methods (`source:519–525`). Direct-release support is deferred (`source:645–655`). Several Recommended tools currently depend on a vendor updater, self-update command, or vendor release (`source:262–270`).

**Risk:** Implementers cannot tell whether the application executes an update, opens the owner's updater, or only reports availability. That ambiguity affects adapter APIs, consent screens, privilege handling, receipts, rollback claims, and acceptance tests.

**Required correction:** Define one execution mode per platform mapping:

| Mode | MVP behavior |
|---|---|
| `managed_execute` | The app builds and executes a typed operation through WinGet, Homebrew, APT/dpkg, DNF/RPM, or Pacman |
| `vendor_handoff` | The app verifies state and opens or invokes the owner's supported updater flow; the app does not claim transactional execution or rollback |
| `detect_only` | The app reports installed and available versions, then provides safe remediation guidance |

Direct vendor asset download, archive extraction, binary replacement, and app-owned uninstall receipts remain outside MVP.

Ownership must be resolved per installation. For example, macOS system Git is system-owned and must not be updated or removed through Homebrew unless the detected binary is actually Homebrew-owned.

### F3 — Deduplicate overlapping skill roots

**Evidence:** The report models one canonical skill across multiple client targets (`source:383–403`) but does not state how adapters behave when two configured client roots resolve to the same physical directory.

**Risk:** Codex, Claude Code, and AgentKit-compatible setups may expose shared or symlinked global locations. Scanning or writing the same physical tree twice can create duplicate identities, misleading target counts, conflicts, or repeated mutations.

**Required correction:** Resolve approved roots to canonical paths, deduplicate physical scan targets, and retain a many-to-many binding from physical installation to logical clients. Symlink escape policy remains enforced after canonicalization.

### F4 — Define ownership and elevation boundaries

**Evidence:** The report requires an unprivileged application and exact-operation elevation (`source:67–79`, `source:645–650`) but does not define how Windows UAC, macOS authorization, or Linux privilege escalation is brokered.

**Risk:** Privilege handling can leak into adapters or encourage unsafe shell/sudo execution. System-managed installations may also be misclassified as package-manager owned.

**Required correction:**

- Add `system_owned` as an ownership outcome with detection-only lifecycle unless an authoritative supported owner exists.
- Keep scanning, catalog refresh, and update checks unprivileged.
- Define typed, per-operation elevation contracts during the feasibility phase.
- Do not install a persistent privileged helper in MVP unless a platform spike proves it necessary and the threat model is revised.
- Never collect, store, or pipe administrator passwords.

### F5 — Separate product self-update from tool updates

**Evidence:** Release hardening mentions packaging and upgrade testing (`source:601–608`) but the architecture only defines managed tool and skill lifecycle.

**Risk:** Updating Tools Manager itself has a different trust root, signing policy, release feed, and rollback behavior from updating catalog tools.

**Required correction:** Add a separate application-update boundary. Tauri application updates require signed artifacts, platform-specific bundles, an authenticated endpoint, explicit user consent, and release rollback tests. This channel must not reuse catalog tool adapters or receipts.

### F6 — Convert broad phases into vertical slices

**Evidence:** The current read-only foundation phase combines catalog/schema work, all tool and skill discovery, persistence, reconciliation, four major UI surfaces, and update detection (`source:563–571`). The tool lifecycle phase combines operation security, two manager families plus three Linux families, desktop updater handoff, privilege handling, cancellation, and logging (`source:573–581`).

**Risk:** Large phases obscure dependency order and delay the first demonstrable, testable product slice.

**Required correction for the implementation plan:**

1. Foundation contracts and platform feasibility.
2. Read-only core: catalog, mapping state, fixtures, inventory reconciliation.
3. Desktop read-only vertical slice for the ten Recommended tools and global skill discovery.
4. Safe tool lifecycle, promoted mapping-by-mapping from `detect_only` to executable or handoff modes.
5. Trusted global skill lifecycle after publisher and catalog authentication decisions are resolved.
6. Cross-platform packaging, product self-update, security, and release hardening.

Each phase must have a narrow acceptance gate and must not imply mutation support for mappings that only passed discovery tests.

## 4. Verified strengths

- Independent desktop application is a defensible boundary; UniGetUI remains a benchmark/backend candidate rather than a suitable domain foundation.
- One primary tool kind plus multiple functional groups correctly separates product shape from discovery taxonomy.
- `recommended` as an independent curation flag preserves the user-confirmed catalog without forcing installation.
- Global-only skill discovery is explicit and excludes project-local traversal.
- Receipt-backed skill provenance, local modification detection, preview, consent, and atomic replacement form a coherent safety model.
- Rust core behind Tauri keeps a future diagnostic CLI possible without making CLI a second MVP product surface.

## 5. Plan readiness

**Decision:** Ready for implementation planning after approved corrections.

F1–F5 are incorporated in source report v0.4.0. F6 shapes the implementation plan at `plans/260820-1901-tools-manager-mvp-implementation/`. The two existing unresolved product decisions do not block foundation and read-only phases, but they remain explicit gates:

- trusted skill catalog publisher/review/authentication blocks managed skill installation and updates;
- supported OS versions and CPU architectures block the final release matrix, not initial contract work.

Applied source-report version: **0.4.0**. The change adds durable mapping-state and lifecycle-execution contracts while preserving confirmed product decisions.

## 6. Proposed document changes

| Document | Change |
|---|---|
| Source report §1–2 | Clarify tool lifecycle as execute, handoff, or detect-only per mapping |
| Source report §5–7 | Split canonical status from mapping lifecycle status; add system ownership |
| Source report §8–9 | Add physical-root deduplication and privilege broker boundary |
| Source report §11–13 | Separate application self-update and revise delivery gates |
| New implementation plan | Use vertical slices and mapping-level promotion criteria |

## 7. Unresolved questions

1. Which publisher/review workflow and authenticated distribution mechanism will own the trusted skill catalog before managed skill lifecycle work begins?
2. Which OS versions and CPU architectures constitute the supported release matrix?

---

## Change Log

| Version | Date | Author | Change |
|---|---|---|---|
| 0.1.0 | 2026-08-20 | Reviewer | Audit product contracts, lifecycle semantics, skill-root handling, privilege boundary, self-update, and plan readiness. |
| 0.2.0 | 2026-08-20 | Codex | Record user approval, mark all six findings applied in source report v0.4.0, and link the implementation plan. |
