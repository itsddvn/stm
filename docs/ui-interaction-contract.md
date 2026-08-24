# STM UI Interaction Contract

Status: UI Contract v1.1 approved and locked on 2026-08-21 after the required lifecycle runtime and viewport verification.

## Navigation

The seven hash-addressable routes remain Dashboard, Tools, Skills, MCP Servers, Updates, Operation History, and Settings. Navigation, heading focus, master-detail layouts, and approved industrial visual language remain unchanged.

## Runtime boundary

`ToolsManagerIpcClient` is the only UI data, source-analysis, and lifecycle boundary. React sends semantic resource/action identifiers or an opaque source-analysis handle. It does not derive owner, mapping, execution mode, privilege, targets, paths, exact commands, confidence, limitations, digest, expiry, or revalidation state. Prepared plans include an opaque boundary-issued plan ID; execution submits that ID and consent authorization rather than resending webview-held command data.

The deterministic fixture client supplies review evidence and simulation results in browser mode. The Tauri runtime adapter invokes the implemented typed Rust lifecycle boundary for source analysis, planning, consent, execution, status, cancellation, receipts, and authoritative refresh; only browser-mode copy identifies a run as simulation.

## Lifecycle plan

Every lifecycle review displays:

- immutable plan, canonical, mapping, and resource IDs;
- authoritative owner and source;
- exact executable plus ordered argument vector for managed execution;
- current and target versions;
- privilege boundary;
- affected records and paths;
- confidence and limitations;
- evidence digest, expiry, typed revalidation state, checked time, and checks.

Revalidation state is `fresh`, `required`, `expired`, or `evidence_changed`. Consent is enabled only while evidence is fresh and unexpired. Consent is keyed to the opaque plan ID, digest, expiry, revalidation state, checked time, and ordered checks for the parent and every child; any change clears consent.

Starting an operation sends the opaque plan ID plus a typed consent authorization containing the reviewed plan digest, plan expiry, and grant time. The trusted runtime resolves and revalidates its stored immutable plan. Progress refresh and cancellation address only the returned operation ID; the UI does not invent a future desktop `complete` command or resend a plan as execution or cancellation policy.

Vendor handoff displays the vendor target and explicitly omits managed command and rollback claims. Detect-only resources remain guidance-only.

Bulk review is a batch of independent child plans. Each child retains its own identifiers, owner, source, execution mode, exact managed command or vendor handoff, privilege, affected resources, confidence, limitations, digest, expiry, and revalidation. Heterogeneous items are never collapsed into one invented queue command. Partial retry or recovery preserves the failed children’s original semantic item IDs and prepares a fresh filtered batch.

## Execution and result

Lifecycle execution displays progress, step count, cancellation availability, and a Cancel Operation control while cancellation is valid. Refresh and cancel responses remain progress states until the runtime reports a terminal result; queued selections alone never produce a completed result. Terminal states display every item result, item receipt, overall receipt, operation ID, plan digest, and redacted detail.

Retry and recovery result actions are guidance, not direct mutations. Selecting one asks the runtime boundary to prepare a fresh semantic plan, returns to review, clears consent, and requires a new digest/expiry/revalidation-bound consent before execution. Vendor handoff results never offer STM rollback.

## Surface coverage

- Tools: install/update, detect-only guidance, and vendor handoff.
- Source install: tool, skill, and MCP source analysis followed by a boundary-issued lifecycle plan.
- Updates: independent child plans plus every bulk item result; signed STM update remains a separate trust channel.
- History: receipt inspection and scoped recovery use fresh lifecycle plans.
- Skills: install/update, local modification choices, multi-target partial failure, retry, and recovery.
- MCP: reviewed add analyzes an HTTPS endpoint before planning; configure/enable/disable/remove open direct immutable plans for the selected supported bindings, with credential references only.

## Keyboard and dialog behavior

- Native dialogs close on Escape and return focus to their trigger.
- Dialog bodies scroll within the viewport; sticky headings and actions remain visible.
- All actions use buttons; all navigation uses links.
- Consent and choice labels share their input hit target.
- Dynamic progress and results use polite live regions.

## Change control

`contracts/ui/ui-contract.manifest.json` is `locked` at `1.1.0`, records project-lead approval, and is verified against the regenerated v1.1 lock. Any artifact drift now fails verification.

The required v1.1 lifecycle viewport matrix is locked at 1024x720, 1280x800, and 1440x900. Any further intentional UI contract change must explicitly reopen Phase 1, bump the version, repeat runtime and viewport verification, obtain project-lead approval, and regenerate the lock before dependent implementation resumes.
