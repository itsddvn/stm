import { useEffect, useMemo, useState } from "react";
import type { AppViewModel, UpdateViewModel } from "../../../contracts/ui/view-model-contract";
import { ActionDisabledReason } from "../../components/action-disabled-reason";
import { AppIcon } from "../../components/app-icon";
import { EmptyState } from "../../components/empty-state";
import { LoadingTable } from "../../components/loading-table";
import { PageHeader } from "../../components/page-header";
import { StateNotice } from "../../components/state-notice";
import { useI18n, type MessageKey } from "../../lib/i18n";
import { UpdateReviewDialog } from "./update-review-dialog";

const resourceKeys: Record<UpdateViewModel["resourceType"], MessageKey> = {
  tool: "updates.tool",
  skill: "updates.skill",
  product: "updates.product",
};

export function UpdatesPage({ view }: { view: AppViewModel }) {
  const { t } = useI18n();
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const [dialogItems, setDialogItems] = useState<UpdateViewModel[]>([]);
  useEffect(() => setSelected(new Set()), [view]);
  const selectedItems = useMemo(
    () => view.updates.filter((item) => selected.has(item.id)),
    [selected, view.updates],
  );
  const productUpdate = view.updates.find((item) => item.resourceType === "product");
  const managedUpdates = view.updates.filter((item) => item.resourceType !== "product");

  function toggle(item: UpdateViewModel) {
    if (!item.selectionAction?.enabled) return;
    setSelected((current) => {
      const next = new Set(current);
      next.has(item.id) ? next.delete(item.id) : next.add(item.id);
      return next;
    });
  }

  return (
    <>
      <PageHeader
        title={t("page.updates.title")}
        description={t("page.updates.description")}
        actions={<button className="primary-button" type="button" disabled={selectedItems.length === 0} onClick={() => setDialogItems(selectedItems)}>{t("common.reviewSelected", { count: selectedItems.length })}</button>}
      />
      <StateNotice reasonCode={view.surface.reasonCode} />
      {view.surface.loadState === "loading" ? <LoadingTable /> : view.updates.length === 0 ? (
        <EmptyState title={t("updates.empty")} detail={t("updates.emptyDetail")} />
      ) : (
        <div className="updates-layout">
          <section className="updates-table" aria-label={t("page.updates.title")}>
            <div className="section-heading"><div><h2>{t("updates.queue")}</h2><p>{t("updates.queueDetail")}</p></div></div>
            <div className="table-header"><span>{t("updates.select")}</span><span>{t("updates.resource")}</span><span>{t("updates.version")}</span><span>{t("updates.owner")}</span><span>{t("updates.risk")}</span></div>
            {managedUpdates.map((item) => {
              const reasonId = `update-action-${item.id}`;
              return (
                <label className={`update-row ${!item.selectionAction?.enabled ? "update-row-disabled" : ""}`} key={item.id}>
                  <span className="checkbox-cell"><input type="checkbox" checked={selected.has(item.id)} disabled={!item.selectionAction?.enabled} aria-describedby={item.selectionAction?.disabledReasonCode ? reasonId : undefined} onChange={() => toggle(item)} /></span>
                  <span className="update-name"><span className="resource-glyph"><AppIcon name={item.resourceType === "skill" ? "skills" : "tools"} /></span><span><strong>{item.name}</strong><small>{t(resourceKeys[item.resourceType])}</small></span></span>
                  <span className="version-change"><span className="mono-data">{item.current}</span><span aria-hidden="true">→</span><span className="mono-data">{item.target}</span></span>
                  <span>{t(item.executionMode === "managed_execute" ? "mode.managed_execute" : item.executionMode === "vendor_handoff" ? "mode.vendor_handoff" : "mode.detect_only")}</span>
                  <span className="update-risk-copy"><small>{t(item.executionMode === "managed_execute" ? "updates.riskManaged" : item.executionMode === "vendor_handoff" ? "updates.riskVendor" : "updates.riskReadonly")}</small><ActionDisabledReason compact id={reasonId} reasonCode={item.selectionAction?.disabledReasonCode} /></span>
                </label>
              );
            })}
          </section>
          {productUpdate ? (
            <aside className="product-update-panel">
              <div className="product-update-title"><span className="resource-glyph large"><AppIcon name="settings" size={24} /></span><div><small>{t("updates.product")}</small><h2>STM</h2></div></div>
              <div className="version-block"><span className="mono-data">{productUpdate.current}</span><span aria-hidden="true">→</span><strong className="mono-data">{productUpdate.target}</strong></div>
              <button className="secondary-button" type="button" disabled={!productUpdate.reviewAction?.enabled} onClick={() => setDialogItems([productUpdate])}>{t("common.reviewSelected", { count: 1 })}</button>
            </aside>
          ) : null}
        </div>
      )}
      <UpdateReviewDialog items={dialogItems} open={dialogItems.length > 0} onClose={() => setDialogItems([])} />
    </>
  );
}
