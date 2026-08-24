import { useLayoutEffect, useMemo, useState, type FormEvent } from "react";
import type { McpPresentationAction } from "../../contracts/ui/action-contract";
import type { LifecyclePlanRequest } from "../../contracts/ui/lifecycle-contract";
import type { SourceAnalysisViewModel, SourceKind } from "../../contracts/ui/view-model-contract";
import { isFixtureRuntime, runtimeIpcClient } from "../lib/ipc/runtime-ipc-client";
import { useI18n, type MessageKey } from "../lib/i18n";
import { AppIcon } from "./app-icon";
import { FixtureDialog } from "./fixture-dialog";
import { LifecycleExecutionState } from "./lifecycle-execution-state";
import { LifecyclePlanReview } from "./lifecycle-plan-review";
import { useLifecycleOperation } from "./use-lifecycle-operation";

const sourceCopy: Record<SourceKind, { title: MessageKey; description: MessageKey; placeholder: string }> = {
  tool: { title: "source.tool.title", description: "source.tool.description", placeholder: "https://github.com/openai/codex" },
  skill: { title: "source.skill.title", description: "source.skill.description", placeholder: "https://github.com/agentkit/skills/tree/main/frontend-design" },
  mcp: { title: "source.mcp.title", description: "source.mcp.description", placeholder: "https://mcp.sentry.dev/mcp" },
};

const directMcpFallback = {
  title: "source.mcp.direct.title",
  description: "source.mcp.direct.description",
  start: "source.authorizeStart",
} satisfies { title: MessageKey; description: MessageKey; start: MessageKey };

const directMcpCopy = {
  configure: {
    title: "source.mcp.direct.configure.title",
    description: "source.mcp.direct.configure.description",
    start: "source.mcp.direct.configure.start",
  },
  enable: {
    title: "source.mcp.direct.enable.title",
    description: "source.mcp.direct.enable.description",
    start: "source.mcp.direct.enable.start",
  },
  disable: {
    title: "source.mcp.direct.disable.title",
    description: "source.mcp.direct.disable.description",
    start: "source.mcp.direct.disable.start",
  },
  remove: {
    title: "source.mcp.direct.remove.title",
    description: "source.mcp.direct.remove.description",
    start: "source.mcp.direct.remove.start",
  },
} satisfies Record<string, { title: MessageKey; description: MessageKey; start: MessageKey }>;

function getDirectMcpCopy(request?: LifecyclePlanRequest) {
  if (!request || request.resourceKind !== "mcp") return null;
  return directMcpCopy[request.action as keyof typeof directMcpCopy] ?? directMcpFallback;
}

export function SourceInstallDialog({ kind, open, onClose, initialUrl = "", title, mcpAction, mcpServerId, directRequest }: {
  kind: SourceKind;
  open: boolean;
  onClose: () => void;
  initialUrl?: string;
  title?: string;
  mcpAction?: McpPresentationAction;
  mcpServerId?: string;
  directRequest?: LifecyclePlanRequest;
}) {
  const { t } = useI18n();
  const [sourceStage, setSourceStage] = useState<"input" | "review">(directRequest ? "review" : "input");
  const [url, setUrl] = useState("");
  const [analysis, setAnalysis] = useState<SourceAnalysisViewModel | null>(null);
  const [analyzing, setAnalyzing] = useState(false);
  const copy = sourceCopy[kind];
  const directCopy = getDirectMcpCopy(directRequest);
  const request = useMemo(
    () => directRequest ?? (analysis?.status === "review_ready" ? buildSourceLifecycleRequest(analysis, mcpAction, mcpServerId) : null),
    [analysis, directRequest, mcpAction, mcpServerId],
  );
  const lifecycle = useLifecycleOperation(request, open && sourceStage === "review");
  const lifecycleReady = directRequest !== undefined || analysis?.status === "review_ready";

  useLayoutEffect(() => {
    if (!open) return;
    setSourceStage(directRequest ? "review" : "input");
    setUrl(initialUrl);
    setAnalysis(null);
    setAnalyzing(false);
  }, [directRequest, initialUrl, kind, open]);

  async function analyze(event: FormEvent) {
    event.preventDefault();
    setAnalyzing(true);
    const next = await runtimeIpcClient.analyzeSource(kind, url);
    setUrl(next.normalizedUrl ?? next.submittedUrl ?? "");
    setAnalysis(next);
    setSourceStage("review");
    setAnalyzing(false);
  }

  const footer = sourceStage === "input" ? (
    <>
      <button className="secondary-button" type="button" onClick={onClose}>{t("common.cancel")}</button>
      <button className="primary-button" type="submit" form={`${kind}-source-form`} disabled={analyzing || !url.trim()}>
        <AppIcon name="search" />{analyzing ? t("source.analyzing") : t("source.analyze")}
      </button>
    </>
  ) : analysis?.status === "blocked" ? (
    <>
      <button className="secondary-button" type="button" onClick={() => setSourceStage("input")}>{t("common.back")}</button>
      <button className="primary-button" type="button" onClick={onClose}>{t("common.close")}</button>
    </>
  ) : (
    <>
      <button className="secondary-button" type="button" onClick={lifecycle.stage === "review" && !directRequest ? () => setSourceStage("input") : onClose}>
        {lifecycle.stage === "review" && !directRequest ? t("common.back") : t("common.close")}
      </button>
      {lifecycle.stage === "review" ? (
        <button className="primary-button" type="button" disabled={lifecycle.starting || !lifecycle.consented || !lifecycle.consentEligible} onClick={() => void lifecycle.start()}>
          <AppIcon name="run" />{t(directCopy?.start ?? "source.authorizeStart")}
        </button>
      ) : null}
      {lifecycle.stage === "progress" && lifecycle.result?.canCancel ? <button className="secondary-button" type="button" onClick={() => void lifecycle.cancel()}>{t("common.cancelOperation")}</button> : null}
      {lifecycle.stage === "progress" ? <button className="primary-button" type="button" onClick={() => void lifecycle.refreshStatus()}>{t("common.refreshStatus")}</button> : null}
    </>
  );

  return (
    <FixtureDialog
      open={open}
      onClose={onClose}
      title={directCopy ? t(directCopy.title) : title ?? t(copy.title)}
      description={directCopy ? t(directCopy.description) : t(copy.description)}
      footer={footer}
    >
      {sourceStage === "input" ? (
        <form id={`${kind}-source-form`} className="source-form" onSubmit={analyze}>
          <label htmlFor={`${kind}-source-url`}>{t("source.urlLabel")}</label>
          <div className="source-url-control"><AppIcon name="link" /><input id={`${kind}-source-url`} type="url" inputMode="url" autoComplete="url" value={url} onChange={(event) => setUrl(event.target.value)} placeholder={copy.placeholder} required /></div>
          {isFixtureRuntime() ? <div className="simulation-banner"><AppIcon name="info" /><div><strong>{t("source.fixtureTitle")}</strong><p>{t("source.fixtureDetail")}</p></div></div> : null}
        </form>
      ) : (
        <>
          {analysis ? <SourceAnalysisSummary analysis={analysis} /> : null}
          {lifecycleReady && lifecycle.prepareError ? <div className="warning-callout"><AppIcon name="warning" /><div><strong>{t("source.lifecycle.prepareFailed")}</strong><p>{lifecycle.prepareError}</p><button className="secondary-button" type="button" onClick={lifecycle.retryPrepare}>{t("setup.retry")}</button></div></div> : null}
          {lifecycleReady && lifecycle.executionError ? <div className="warning-callout"><AppIcon name="warning" /><div><strong>{t("error.operation", { message: lifecycle.executionError })}</strong></div></div> : null}
          {lifecycleReady && !lifecycle.prepareError && lifecycle.stage === "loading" ? <p className="dialog-loading">{t("source.lifecycle.preparing")}</p> : null}
          {lifecycleReady && lifecycle.stage === "review" && lifecycle.plan ? <LifecyclePlanReview plan={lifecycle.plan} consented={lifecycle.consented} onConsentChange={lifecycle.setConsented} /> : null}
          {lifecycleReady && (lifecycle.stage === "progress" || lifecycle.stage === "result") && lifecycle.plan && lifecycle.result ? (
            <LifecycleExecutionState
              plan={lifecycle.plan}
              result={lifecycle.result}
              onReviewFollowUp={(action) => {
                if (action.planRequest.action === "reanalyze-source") {
                  setAnalysis(null);
                  setSourceStage("input");
                  return;
                }
                void lifecycle.reviewFollowUp(action);
              }}
            />
          ) : null}
        </>
      )}
    </FixtureDialog>
  );
}

function SourceAnalysisSummary({ analysis }: { analysis: SourceAnalysisViewModel }) {
  const { t } = useI18n();
  return (
    <section className="source-analysis-summary">
      <div className={analysis.status === "blocked" ? "warning-callout" : "info-callout"}><AppIcon name={analysis.status === "blocked" ? "warning" : "info"} /><div><strong>{analysis.status === "blocked" ? t("source.blocked") : t("source.complete")}</strong><p>{analysis.normalizedUrl ?? analysis.submittedUrl ?? t("source.noValidUrl")}</p></div></div>
      <dl className="plan-grid"><div><dt>{t("source.detected")}</dt><dd>{analysis.detectedName}</dd></div><div><dt>{t("source.publisher")}</dt><dd>{analysis.publisher}</dd></div><div><dt>{t("source.target")}</dt><dd>{analysis.target}</dd></div><div><dt>{t("source.trust")}</dt><dd>{analysis.trust.replaceAll("_", " ")}</dd></div></dl>
      <div className="source-review-columns"><section><h3>{t("source.riskFlags")}</h3><ul>{analysis.riskFlags.map((flag) => <li key={flag}>{flag}</li>)}</ul></section><section><h3>{t("source.reviewNotes")}</h3><ul>{analysis.notes.map((note) => <li key={note}>{note}</li>)}</ul></section></div>
    </section>
  );
}

function buildSourceLifecycleRequest(analysis: SourceAnalysisViewModel, action?: McpPresentationAction, serverId?: string): LifecyclePlanRequest {
  const actionName = action?.id.split(".").at(-1) ?? "install";
  return {
    ...analysis.lifecycleRequest,
    action: actionName,
    resourceId: serverId ?? analysis.lifecycleRequest.resourceId,
  };
}
