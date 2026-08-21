import { useEffect, useMemo, useState } from "react";
import type { AppViewModel, UpdateViewModel } from "../../../contracts/ui/view-model-contract";
import { ActionDisabledReason } from "../../components/action-disabled-reason";
import { AppIcon } from "../../components/app-icon";
import { EmptyState } from "../../components/empty-state";
import { LoadingTable } from "../../components/loading-table";
import { PageHeader } from "../../components/page-header";
import { StateNotice } from "../../components/state-notice";
import { UpdateReviewDialog } from "./update-review-dialog";

export function UpdatesPage({ view }: { view: AppViewModel }) {
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const [dialogItems, setDialogItems] = useState<UpdateViewModel[]>([]);
  useEffect(() => setSelected(new Set()), [view]);
  const selectedItems = useMemo(() => view.updates.filter((item) => selected.has(item.id)), [selected, view.updates]);
  const productUpdate = view.updates.find((item) => item.resourceType === "product");
  const managedUpdates = view.updates.filter((item) => item.resourceType !== "product");
  const toggle = (item: UpdateViewModel) => {
    if (!item.selectionAction?.enabled) return;
    setSelected((current) => {
      const next = new Set(current);
      next.has(item.id) ? next.delete(item.id) : next.add(item.id);
      return next;
    });
  };

  return (
    <>
      <PageHeader title="Update Review" description="Available changes stay unselected until source, owner, target, and risk are reviewed." actions={<button className="primary-button" type="button" disabled={selectedItems.length === 0} onClick={() => setDialogItems(selectedItems)}>Review Selected ({selectedItems.length})</button>} />
      <StateNotice reasonCode={view.surface.reasonCode} />
      {view.surface.loadState === "loading" ? <LoadingTable /> : view.updates.length === 0 ? <EmptyState title="No updates available" detail="The current fixture inventory has no newer trusted targets." /> : (
        <div className="updates-layout">
          <section className="updates-table" aria-label="Tool and skill updates">
            <div className="section-heading"><div><h2>Tool & Skill Queue</h2><p>Separate planners, shared review surface</p></div></div>
            <div className="table-header"><span>Select</span><span>Resource</span><span>Version</span><span>Authority</span><span>Risk boundary</span></div>
            {managedUpdates.map((item) => {
              const reasonId = `update-action-${item.id}`;
              return <label className={`update-row ${!item.selectionAction?.enabled ? "update-row-disabled" : ""}`} key={item.id}><span className="checkbox-cell"><input type="checkbox" checked={selected.has(item.id)} disabled={!item.selectionAction?.enabled} aria-describedby={item.selectionAction?.disabledReasonCode ? reasonId : undefined} onChange={() => toggle(item)} /><span className="sr-only">{item.selectionAction?.label ?? `Select ${item.name}`}</span></span><span className="update-name"><span className="resource-glyph"><AppIcon name={item.resourceType === "skill" ? "skills" : "tools"} /></span><span><strong>{item.name}</strong><small>{item.resourceType}</small></span></span><span className="version-change"><span className="mono-data">{item.current}</span><span aria-hidden="true">→</span><span className="mono-data">{item.target}</span></span><span>{item.executionMode.replaceAll("_", " ")}</span><span className="update-risk-copy"><span>{item.risk}</span><ActionDisabledReason compact id={reasonId} reasonCode={item.selectionAction?.disabledReasonCode} /></span></label>;
            })}
          </section>
          {productUpdate ? <aside className="product-update-panel"><div className="product-update-title"><span className="resource-glyph large"><AppIcon name="settings" size={24} /></span><div><small>Separate trust channel</small><h2>STM Update</h2></div></div><div className="version-block"><span className="mono-data">{productUpdate.current}</span><span aria-hidden="true">→</span><strong className="mono-data">{productUpdate.target}</strong></div><p>Signed artifact, authenticated endpoint, platform package, and independent recovery state.</p><button className="secondary-button" type="button" disabled={!productUpdate.reviewAction?.enabled} onClick={() => setDialogItems([productUpdate])}>{productUpdate.reviewAction?.label ?? "Review Product Update"}</button></aside> : null}
        </div>
      )}
      <UpdateReviewDialog items={dialogItems} open={dialogItems.length > 0} onClose={() => setDialogItems([])} />
    </>
  );
}
