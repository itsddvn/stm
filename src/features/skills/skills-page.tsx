import { useMemo, useState } from "react";
import type { AppViewModel, SkillViewModel } from "../../../contracts/ui/view-model-contract";
import { AppIcon } from "../../components/app-icon";
import { EmptyState } from "../../components/empty-state";
import { LoadingTable } from "../../components/loading-table";
import { PageHeader } from "../../components/page-header";
import { SearchFilterBar } from "../../components/search-filter-bar";
import { SourceInstallDialog } from "../../components/source-install-dialog";
import { StateNotice } from "../../components/state-notice";
import { StatusBadge } from "../../components/status-badge";
import { SkillDetailPanel } from "./skill-detail-panel";
import { SkillReviewDialog } from "./skill-review-dialog";

export function SkillsPage({ view }: { view: AppViewModel }) {
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState("all");
  const [selectedId, setSelectedId] = useState(view.skills[0]?.id ?? "");
  const [reviewSkill, setReviewSkill] = useState<SkillViewModel | null>(null);
  const [sourceDialogOpen, setSourceDialogOpen] = useState(false);
  const filtered = useMemo(() => view.skills.filter((skill) => (filter === "all" || skill.state === filter) && `${skill.name} ${skill.purposes.join(" ")}`.toLowerCase().includes(query.toLowerCase())), [filter, query, view.skills]);
  const selected = filtered.find((skill) => skill.id === selectedId) ?? filtered[0];

  return (
    <>
      <PageHeader title="Global Skills" description="Canonical skill identity, provenance, content state, and physical client targets." actions={<><button className="secondary-button" type="button"><AppIcon name="refresh" />Rescan Fixtures</button><button className="primary-button" type="button" onClick={() => setSourceDialogOpen(true)}><AppIcon name="link" />Install from Link</button></>} />
      <StateNotice reasonCode={view.surface.reasonCode} />
      <SearchFilterBar label="skills" query={query} onQueryChange={setQuery} filter={filter} onFilterChange={setFilter} options={[{ value: "all", label: "All skill states" }, { value: "managed_current", label: "Current" }, { value: "managed_update_available", label: "Update available" }, { value: "modified", label: "Modified" }, { value: "conflict", label: "Conflict" }, { value: "external", label: "External" }]} />
      {view.surface.loadState === "loading" ? <LoadingTable /> : filtered.length === 0 ? <EmptyState title="No skills match" detail="Clear filters or choose another fixture scenario." /> : (
        <div className="master-detail-layout">
          <div className="resource-list" aria-label="Global skills">
            {filtered.map((skill) => <button className={`resource-row ${skill.id === selected?.id ? "selected" : ""}`} type="button" onClick={() => setSelectedId(skill.id)} key={skill.id}><span className="resource-glyph"><AppIcon name="skills" /></span><span className="resource-summary"><strong>{skill.name}</strong><small>{skill.targets.map((target) => target.client).join(" · ")}</small></span><span className="revision-cell"><small>Revision</small><strong className="mono-data">{skill.revision.split(" · ")[0]}</strong></span><StatusBadge state={skill.state} /></button>)}
          </div>
          {selected ? <SkillDetailPanel skill={selected} onReview={() => setReviewSkill(selected)} /> : null}
        </div>
      )}
      {reviewSkill ? <SkillReviewDialog skill={reviewSkill} open onClose={() => setReviewSkill(null)} /> : null}
      {sourceDialogOpen ? <SourceInstallDialog kind="skill" open onClose={() => setSourceDialogOpen(false)} /> : null}
    </>
  );
}
