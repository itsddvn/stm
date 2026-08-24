# STM UI Interaction Contract

Status: current review contract for browser evidence and desktop runtime wiring.

## Navigation

The seven routes remain Dashboard, Tools, Skills, MCP Servers, Updates, Operation History, and Settings. Visible navigation defaults to Vietnamese and switches persistently to English without changing route IDs or lifecycle semantics.
Quick Setup is a modal workflow, not a route. Desktop first launch opens it until dismissed, and Dashboard, Tools, and Settings can reopen it.

## Runtime boundary

`ToolsManagerIpcClient` is the only UI data, source-analysis, quick-setup, portable-setup, and lifecycle boundary. React sends semantic resource/action identifiers or an opaque source-analysis handle. It does not derive owner, mapping, execution mode, privilege, targets, paths, exact commands, confidence, limitations, digest, expiry, or revalidation state. Prepared plans include an opaque boundary-issued plan ID; execution submits that ID and consent authorization rather than resending webview-held command data.

Browser mode supplies deterministic review evidence. The Tauri runtime adapter invokes typed commands for source analysis, Quick Setup, provider settings, portable import/export, planning, confirmation, execution, status, cancellation, receipts, and authoritative refresh. The UI may label fixture review data, but the contract is not documented as browser-only simulation.

## Lifecycle plan

Lifecycle review leads with a short localized list of tools that STM will install, update, or hand off plus one explicit consent control. Internal plan IDs, mappings, exact executable/argument vectors, versions, affected paths, evidence checks, digests, timestamps, and receipts remain available in a collapsed accessible technical-details disclosure.

Revalidation state is `fresh`, `required`, `expired`, or `evidence_changed`. Consent is enabled only while evidence is fresh and unexpired. Consent remains keyed to the opaque plan ID, digest, expiry, revalidation state, checked time, and ordered checks for the parent and every child; simplifying presentation does not weaken the runtime authorization.

Starting an operation sends the opaque plan ID plus a typed consent authorization containing the reviewed plan digest, plan expiry, and grant time. The trusted runtime resolves and revalidates its stored immutable plan. For mutating desktop runs, reviewed consent is followed by a native host confirmation dialog before execution starts. Progress refresh and cancellation address only the returned operation ID; the UI does not invent a future desktop `complete` command or resend a plan as execution or cancellation policy.

Vendor handoff displays the vendor target and explicitly omits managed command and rollback claims. Detect-only resources remain guidance-only.

A `setup-queue` review is a batch of independent child plans. Each child retains its own identifiers, owner, source, execution mode, exact managed command or vendor handoff, privilege, affected resources, confidence, limitations, digest, expiry, and revalidation. When Homebrew bootstrap is required, the batch prepends a provider-bootstrap child and stages dependent Homebrew children until the provider postcondition recompiles them against fresh evidence. Heterogeneous items are never collapsed into one invented queue command. Partial retry or recovery preserves the failed children’s original semantic item IDs and prepares a fresh filtered batch.

## Quick Setup and provider settings

Quick Setup begins with either system recommendations or a native portable import. Native recommendations use fresh live tool/provider evidence; browser review uses deterministic fixtures. The provider step records `automatic`, `prefer_homebrew`, or `prefer_bun`, but that preference only affects new installs; existing owners stay unchanged.
Selections become a reviewed `setup-queue`, not direct package-manager commands. Missing tools with current-platform supported recipes become Install, outdated manager-owned tools become Update, and the runtime normalizes mappings again before execution.

## Portable setup

Portable import/export is review data, not a migration command.

- Import uses a native desktop open dialog when the Tauri runtime is present.
- Import accepts only target-exact JSON documents with typed `tool`, `skill`, or `mcp` resources.
- Import rejects machine paths, shell/executable/script content, more than 64 KiB, more than 32 resources, and mismatched targets.
- Unknown skills or MCP resources remain review-only and surface through `reviewRequiredIds` instead of inventing a managed install path.
- Export uses a native desktop save dialog, requires a fresh authoritative scan, emits additive desired state only, and carries MCP credential references only as bounded opaque IDs.
- Export omits provider preference, receipts, commands, file-backed references, and raw secrets.
- Settings exposes only the eligible Codex npm → Homebrew migration. The migration review shows exact source/target mappings, prefix-bound target executable, shared-config non-copy, and preselected npm cleanup. Cleanup cannot run until target activation verifies; failed cleanup returns a migration-specific reviewed retry or inspection.

## Execution and result

Lifecycle execution displays a short localized progress or outcome summary. Failures remain visible and actionable; exact per-item results, receipts, operation ID, plan digest, and redacted technical detail live in a collapsed technical-details disclosure.

Retry and recovery result actions are guidance, not direct mutations. Selecting one asks the runtime boundary to prepare a fresh semantic plan, returns to review, clears consent, and requires a new digest/expiry/revalidation-bound consent before execution. Vendor handoff results never offer STM rollback.
Batch progress may resume from persisted child checkpoints. If a child checkpoint cannot be persisted, the current child becomes recoverable and later siblings stay skipped.

## Surface coverage

- Tools: install/update, detect-only guidance, and vendor handoff.
- Quick Setup: first-launch checklist, provider choice, additive import review items, and reviewed `setup-queue` execution.
- Source install: tool, skill, and MCP source analysis followed by a boundary-issued lifecycle plan.
- Updates: independent child plans plus every bulk item result; signed STM update remains a separate trust channel.
- History: receipt inspection and scoped recovery use fresh lifecycle plans.
- Settings: provider preference, portable target selection, native import/export dialogs, and inventory adapter review.
- Skills: install/update, local modification choices, multi-target partial failure, retry, and recovery.
- MCP: reviewed add analyzes an HTTPS endpoint before planning; configure/enable/disable/remove open direct immutable plans for the selected supported bindings, with credential references only.

## Keyboard and dialog behavior

- In-app dialogs close on Escape and return focus to their trigger.
- Portable import/export use native desktop open/save dialogs when available; browser review mode surfaces availability instead of fabricating file selection.
- Dialog bodies scroll within the viewport; sticky headings and actions remain visible.
- All actions use buttons; all navigation uses links.
- Consent and choice labels share their input hit target.
- Dynamic progress and results use polite live regions.

## Change control

`contracts/ui/ui-contract.manifest.json` currently records contract `1.1.0` with status `review`; project-lead approval is null, and diagnostics report `locked: false`.
Moving from review to lock requires manifest status `locked`, approval metadata, regenerated lock digests, and renewed evidence for affected artifacts and viewports.
