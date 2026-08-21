import type { LifecyclePlan } from "../../contracts/ui/lifecycle-contract";
import { AppIcon } from "./app-icon";
import { isLifecycleConsentEligible } from "./use-lifecycle-operation";
import { isFixtureRuntime } from "../lib/ipc/runtime-ipc-client";

export function LifecyclePlanReview({
  plan,
  consented,
  onConsentChange,
}: {
  plan: LifecyclePlan;
  consented: boolean;
  onConsentChange: (checked: boolean) => void;
}) {
  const managedExecution = plan.execution.mode === "managed_execute" || plan.execution.mode === "signed_product_update" ? plan.execution : null;
  const consentEligible = isLifecycleConsentEligible(plan);
  const affected = [
    ...plan.affectedRecords.map((value) => ({ type: "Record", value })),
    ...plan.affectedPaths.map((value) => ({ type: "Path", value })),
  ];

  return (
    <div className="lifecycle-review">
      {isFixtureRuntime() ? <div className="simulation-banner"><AppIcon name="info" /><div><strong>Deterministic simulation</strong><p>This review uses fixture evidence through the desktop-ready lifecycle boundary. This run will not change the system.</p></div></div> : null}
      {plan.execution.mode === "vendor_handoff" ? (
        <div className="handoff-boundary"><AppIcon name="external" /><div><strong>Vendor-owned execution</strong><p>STM records the reviewed handoff to {plan.execution.handoffTarget}. No rollback capability is claimed.</p></div></div>
      ) : null}
      <dl className="plan-grid lifecycle-plan-grid">
        <PlanField label="Plan ID" value={plan.planId} mono />
        <PlanField label="Canonical ID" value={plan.canonicalId} mono />
        <PlanField label="Mapping ID" value={plan.mappingId} mono />
        <PlanField label="Resource ID" value={plan.resourceId} mono />
        <PlanField label="Owner" value={plan.owner} />
        <PlanField label="Source" value={plan.source} mono />
        <PlanField label="Privilege" value={plan.privilege.replaceAll("_", " ")} />
        <PlanField label="Current version" value={plan.currentVersion} mono />
        <PlanField label="Target version" value={plan.targetVersion} mono />
        <PlanField label="Confidence" value={plan.confidence} />
        <PlanField label="Expires" value={plan.expiresAt} mono />
      </dl>
      {plan.execution.mode === "batch" ? <BatchPlanList items={plan.execution.items} /> : null}
      {managedExecution ? (
        <section className="lifecycle-command" aria-label="Exact managed command">
          <h3>Exact managed command</h3>
          <dl>
            <div><dt>Executable</dt><dd className="mono-data">{managedExecution.executable}</dd></div>
            <div><dt>Argument vector</dt><dd><ol className="argv-list">{managedExecution.argv.map((argument, index) => <li key={`${argument}-${index}`}><span>{index}</span><code>{argument}</code></li>)}</ol></dd></div>
          </dl>
        </section>
      ) : null}
      <div className="lifecycle-evidence-columns">
        <EvidenceList title="Affected records and paths" items={affected.map((item) => `${item.type}: ${item.value}`)} empty="No persistent record or path is affected by this handoff." />
        <EvidenceList title="Limitations" items={plan.limitations} />
        <EvidenceList title="Revalidation before execution" items={plan.revalidation.checks} />
      </div>
      <dl className="revalidation-state">
        <div><dt>Revalidation state</dt><dd>{plan.revalidation.state.replaceAll("_", " ")}</dd></div>
        <div><dt>Evidence checked</dt><dd className="mono-data">{plan.revalidation.checkedAt}</dd></div>
      </dl>
      <div className="digest-panel">
        <span>Consent evidence digest</span>
        <code>{plan.digest}</code>
        <small>Expires {plan.expiresAt}. Changed evidence, digest, or expiry clears consent and requires review again.</small>
      </div>
      {plan.execution.mode !== "detect_only" ? (
        <label className="consent-control">
          <input type="checkbox" checked={consented} disabled={!consentEligible} onChange={(event) => onConsentChange(event.target.checked)} />
          <span><strong>Consent to this digest until its expiry</strong><small>I reviewed the exact owner, source, command or handoff, targets, limitations, digest, and expiry.</small></span>
        </label>
      ) : null}
      {!consentEligible && plan.execution.mode !== "detect_only" ? <div className="warning-callout"><AppIcon name="warning" /><div><strong>Fresh review required</strong><p>Consent is unavailable because evidence is {plan.revalidation.state.replaceAll("_", " ")} or the plan has expired.</p></div></div> : null}
    </div>
  );
}

function BatchPlanList({ items }: { items: Extract<LifecyclePlan, { execution: { mode: "batch" } }>["execution"]["items"] }) {
  return (
    <section className="batch-plan-list">
      <h3>Independent item plans</h3>
      {items.map((item) => {
        const managed = item.execution.mode === "managed_execute" || item.execution.mode === "signed_product_update" ? item.execution : null;
        return (
          <article key={`${item.resourceId}:${item.digest}`}>
            <header><strong>{item.canonicalId}</strong><span>{item.execution.mode.replaceAll("_", " ")}</span></header>
            <dl className="plan-grid">
              <PlanField label="Plan ID" value={item.planId} mono /><PlanField label="Mapping ID" value={item.mappingId} mono /><PlanField label="Resource ID" value={item.resourceId} mono />
              <PlanField label="Owner" value={item.owner} /><PlanField label="Source" value={item.source} mono />
              <PlanField label="Current" value={item.currentVersion} mono /><PlanField label="Target" value={item.targetVersion} mono />
              <PlanField label="Privilege" value={item.privilege.replaceAll("_", " ")} /><PlanField label="Confidence" value={item.confidence} />
            </dl>
            {managed ? <div className="batch-command"><span>Executable</span><code>{managed.executable}</code><span>argv</span><code>{JSON.stringify(managed.argv)}</code></div> : item.execution.mode === "vendor_handoff" ? <div className="handoff-boundary"><AppIcon name="external" /><div><strong>Handoff to {item.execution.handoffTarget}</strong><p>No rollback capability is claimed for this vendor-owned item.</p></div></div> : null}
            <div className="batch-evidence"><p><strong>Affected:</strong> {[...item.affectedRecords, ...item.affectedPaths].join(" · ") || "No managed paths"}</p><p><strong>Limitations:</strong> {item.limitations.join(" · ")}</p></div>
            <div className="digest-panel"><span>Item digest</span><code>{item.digest}</code><small>Expires {item.expiresAt} · revalidation {item.revalidation.state.replaceAll("_", " ")} at {item.revalidation.checkedAt}</small></div>
          </article>
        );
      })}
    </section>
  );
}

function PlanField({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return <div><dt>{label}</dt><dd className={mono ? "mono-data" : undefined}>{value}</dd></div>;
}

function EvidenceList({ title, items, empty = "None reported." }: { title: string; items: string[]; empty?: string }) {
  return <section><h3>{title}</h3>{items.length ? <ul>{items.map((item) => <li key={item}>{item}</li>)}</ul> : <p>{empty}</p>}</section>;
}
