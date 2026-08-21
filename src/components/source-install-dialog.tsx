import { useLayoutEffect, useMemo, useState, type FormEvent } from "react";
import type { McpPresentationAction } from "../../contracts/ui/action-contract";
import type { LifecyclePlanRequest } from "../../contracts/ui/lifecycle-contract";
import type { SourceAnalysisViewModel, SourceKind } from "../../contracts/ui/view-model-contract";
import { isFixtureRuntime, runtimeIpcClient } from "../lib/ipc/runtime-ipc-client";
import { AppIcon } from "./app-icon";
import { FixtureDialog } from "./fixture-dialog";
import { LifecycleExecutionState } from "./lifecycle-execution-state";
import { LifecyclePlanReview } from "./lifecycle-plan-review";
import { useLifecycleOperation } from "./use-lifecycle-operation";

const sourceCopy: Record<SourceKind, { title: string; description: string; placeholder: string }> = {
  tool: { title: "Install Tool from Link", description: "Analyze source identity, then review the complete managed plan.", placeholder: "https://github.com/openai/codex" },
  skill: { title: "Install Skill from Link", description: "Analyze repository provenance, risks, and targets before lifecycle review.", placeholder: "https://github.com/agentkit/skills/tree/main/frontend-design" },
  mcp: { title: "Add MCP Server", description: "Analyze endpoint provenance before reviewing global client configuration.", placeholder: "https://mcp.sentry.dev/mcp" },
};

export function SourceInstallDialog({ kind, open, onClose, initialUrl = "", title, mcpAction, mcpServerId }: {
  kind: SourceKind;
  open: boolean;
  onClose: () => void;
  initialUrl?: string;
  title?: string;
  mcpAction?: McpPresentationAction;
  mcpServerId?: string;
}) {
  const [sourceStage, setSourceStage] = useState<"input" | "review">("input");
  const [url, setUrl] = useState("");
  const [analysis, setAnalysis] = useState<SourceAnalysisViewModel | null>(null);
  const [analyzing, setAnalyzing] = useState(false);
  const copy = sourceCopy[kind];
  const request = useMemo(() => analysis?.status === "review_ready" ? buildSourceLifecycleRequest(analysis, mcpAction, mcpServerId) : null, [analysis, mcpAction, mcpServerId]);
  const lifecycle = useLifecycleOperation(request, open && sourceStage === "review");

  useLayoutEffect(() => {
    if (!open) return;
    setSourceStage("input");
    setUrl(initialUrl);
    setAnalysis(null);
    setAnalyzing(false);
  }, [initialUrl, kind, open]);

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
    <><button className="secondary-button" type="button" onClick={onClose}>Cancel</button><button className="primary-button" type="submit" form={`${kind}-source-form`} disabled={analyzing || !url.trim()}><AppIcon name="search" />{analyzing ? "Analyzing…" : "Analyze Source"}</button></>
  ) : analysis?.status === "blocked" ? (
    <><button className="secondary-button" type="button" onClick={() => setSourceStage("input")}>Back</button><button className="primary-button" type="button" onClick={onClose}>Close</button></>
  ) : (
    <>
      <button className="secondary-button" type="button" onClick={lifecycle.stage === "review" ? () => setSourceStage("input") : onClose}>{lifecycle.stage === "review" ? "Back" : "Close"}</button>
      {lifecycle.stage === "review" ? <button className="primary-button" type="button" disabled={!lifecycle.consented || !lifecycle.consentEligible} onClick={() => void lifecycle.start()}><AppIcon name="run" />Authorize &amp; Start</button> : null}
      {lifecycle.stage === "progress" && lifecycle.result?.canCancel ? <button className="secondary-button" type="button" onClick={() => void lifecycle.cancel()}>Cancel Operation</button> : null}
      {lifecycle.stage === "progress" ? <button className="primary-button" type="button" onClick={() => void lifecycle.refreshStatus()}>Refresh Status</button> : null}
    </>
  );

  return (
    <FixtureDialog open={open} onClose={onClose} title={title ?? copy.title} description={copy.description} footer={footer}>
      {sourceStage === "input" ? (
        <form id={`${kind}-source-form`} className="source-form" onSubmit={analyze}>
          <label htmlFor={`${kind}-source-url`}>HTTPS source URL</label>
          <div className="source-url-control"><AppIcon name="link" /><input id={`${kind}-source-url`} type="url" inputMode="url" autoComplete="url" value={url} onChange={(event) => setUrl(event.target.value)} placeholder={copy.placeholder} required /></div>
          {isFixtureRuntime() ? <div className="simulation-banner"><AppIcon name="info" /><div><strong>Deterministic source simulation</strong><p>This mode validates fixture input through the typed runtime boundary. This run does not fetch or modify the system.</p></div></div> : null}
        </form>
      ) : analysis ? (
        <>
          <SourceAnalysisSummary analysis={analysis} />
          {analysis.status === "review_ready" && lifecycle.stage === "loading" ? <p className="dialog-loading">Preparing digest-bound lifecycle evidence…</p> : null}
          {analysis.status === "review_ready" && lifecycle.stage === "review" && lifecycle.plan ? <LifecyclePlanReview plan={lifecycle.plan} consented={lifecycle.consented} onConsentChange={lifecycle.setConsented} /> : null}
          {analysis.status === "review_ready" && (lifecycle.stage === "progress" || lifecycle.stage === "result") && lifecycle.plan && lifecycle.result ? (
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
      ) : null}
    </FixtureDialog>
  );
}

function SourceAnalysisSummary({ analysis }: { analysis: SourceAnalysisViewModel }) {
  return (
    <section className="source-analysis-summary">
      <div className={analysis.status === "blocked" ? "warning-callout" : "info-callout"}><AppIcon name={analysis.status === "blocked" ? "warning" : "info"} /><div><strong>{analysis.status === "blocked" ? "Source blocked" : "Source analysis complete"}</strong><p>{analysis.normalizedUrl ?? analysis.submittedUrl ?? "No valid URL supplied"}</p></div></div>
      <dl className="plan-grid"><div><dt>Detected</dt><dd>{analysis.detectedName}</dd></div><div><dt>Publisher</dt><dd>{analysis.publisher}</dd></div><div><dt>Target</dt><dd>{analysis.target}</dd></div><div><dt>Trust</dt><dd>{analysis.trust.replaceAll("_", " ")}</dd></div></dl>
      <div className="source-review-columns"><section><h3>Risk flags</h3><ul>{analysis.riskFlags.map((flag) => <li key={flag}>{flag}</li>)}</ul></section><section><h3>Review notes</h3><ul>{analysis.notes.map((note) => <li key={note}>{note}</li>)}</ul></section></div>
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
