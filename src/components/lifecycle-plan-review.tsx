import type { LifecyclePlan } from "../../contracts/ui/lifecycle-contract";
import { useI18n, type MessageKey } from "../lib/i18n";
import { isFixtureRuntime } from "../lib/ipc/runtime-ipc-client";
import { AppIcon } from "./app-icon";
import { isLifecycleConsentEligible } from "./use-lifecycle-operation";

export function LifecyclePlanReview({
  plan,
  consented,
  onConsentChange,
}: {
  plan: LifecyclePlan;
  consented: boolean;
  onConsentChange: (checked: boolean) => void;
}) {
  const { t } = useI18n();
  const consentEligible = isLifecycleConsentEligible(plan);
  const items = plan.execution.mode === "batch" ? plan.execution.items : [plan];
  const visibleItems = items.filter((item) => !item.canonicalId.startsWith("provider:"));
  const summaryItems = visibleItems.length ? visibleItems : items;

  return (
    <div className="lifecycle-review">
      {isFixtureRuntime() ? (
        <div className="simulation-banner"><AppIcon name="info" /><div><strong>{t("runtime.fixture")}</strong><p>{t("review.simulation")}</p></div></div>
      ) : null}
      <section className="simple-plan-summary">
        <h3>{t("review.title")}</h3>
        <p>{t("review.count", { count: summaryItems.length })}</p>
        <ul>
          {summaryItems.map((item) => (
            <li key={`${item.resourceId}:${item.digest}`}>
              <AppIcon name={actionIcon(item)} />
              <span>{t(actionKey(item), { name: displayName(item.resourceId) })}</span>
            </li>
          ))}
        </ul>
        {items.some((item) => item.execution.mode === "vendor_handoff") ? <p>{t("review.vendor")}</p> : null}
      </section>

      {plan.execution.mode !== "detect_only" ? (
        <label className="consent-control">
          <input type="checkbox" checked={consented} disabled={!consentEligible} onChange={(event) => onConsentChange(event.target.checked)} />
          <span><strong>{t("review.confirm")}</strong><small>{t("review.confirmDetail")}</small></span>
        </label>
      ) : null}
      {!consentEligible && plan.execution.mode !== "detect_only" ? (
        <div className="warning-callout"><AppIcon name="warning" /><div><strong>{t("review.expired")}</strong></div></div>
      ) : null}

      <details className="advanced-details">
        <summary>{t("review.advanced")}</summary>
        <TechnicalPlan plan={plan} />
        {plan.execution.mode === "batch" ? (
          <div className="batch-plan-list">
            {plan.execution.items.map((item) => <TechnicalPlan plan={item} key={`${item.resourceId}:${item.digest}`} />)}
          </div>
        ) : null}
      </details>
    </div>
  );
}

function TechnicalPlan({ plan }: { plan: LifecyclePlan }) {
  const { t } = useI18n();
  const execution = plan.execution;
  const managed = execution.mode === "managed_execute"
    || execution.mode === "signed_product_update"
    || execution.mode === "native_installer"
    || execution.mode === "archive_installer"
    ? execution
    : null;
  return (
    <article className="technical-plan">
      <strong>{displayName(plan.resourceId)}</strong>
      <dl className="plan-grid lifecycle-plan-grid">
        <PlanField label={t("technical.plan")} value={plan.planId} mono />
        <PlanField label={t("technical.mapping")} value={plan.mappingId} mono />
        <PlanField label={t("technical.owner")} value={plan.owner} />
        <PlanField label={t("technical.current")} value={plan.currentVersion} mono />
        <PlanField label={t("technical.target")} value={plan.targetVersion} mono />
        <PlanField label={t("technical.expires")} value={plan.expiresAt} mono />
      </dl>
      {managed ? <div className="batch-command"><span>{t("technical.executable")}</span><code>{managed.executable}</code><span>{t("technical.arguments")}</span><code>{JSON.stringify(managed.argv)}</code></div> : null}
      <div className="digest-panel"><span>{t("technical.digest")}</span><code>{plan.digest}</code></div>
    </article>
  );
}

function actionKey(plan: LifecyclePlan): MessageKey {
  if (plan.execution.mode === "vendor_handoff") return "review.handoff";
  if (plan.request.action === "update") return "review.update";
  if (plan.request.action === "install" || plan.request.action === "bootstrap") return "review.install";
  return "review.other";
}

function actionIcon(plan: LifecyclePlan) {
  if (plan.execution.mode === "vendor_handoff") return "external" as const;
  if (plan.request.action === "update") return "updates" as const;
  return "tools" as const;
}

function displayName(resourceId: string) {
  return resourceId
    .replace(/^update-/, "")
    .split("-")
    .map((part) => part ? `${part[0].toUpperCase()}${part.slice(1)}` : part)
    .join(" ");
}

function PlanField({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return <div><dt>{label}</dt><dd className={mono ? "mono-data" : undefined}>{value}</dd></div>;
}
