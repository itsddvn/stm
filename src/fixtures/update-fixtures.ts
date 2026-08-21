import type { UpdateViewModel } from "../../contracts/ui/view-model-contract";
import { withUpdatePresentationActions } from "./presentation-action-fixtures";

type UpdateFixtureSeed = {
  conflictResolutionRequired?: boolean;
  viewModel: Omit<UpdateViewModel, "reviewAction" | "selectionAction">;
};

const updateFixtureSeeds: UpdateFixtureSeed[] = [
  { viewModel: { id: "update-orca", resourceType: "tool", name: "Orca", current: "0.9.4", target: "0.10.1", executionMode: "vendor_handoff", selected: false, risk: "Vendor updater controls execution" } },
  { viewModel: { id: "update-docker", resourceType: "tool", name: "Docker Desktop", current: "4.44.2", target: "4.45.0", executionMode: "vendor_handoff", selected: false, risk: "Privilege may be requested by vendor" } },
  { viewModel: { id: "update-codex", resourceType: "tool", name: "Codex CLI", current: "0.31.0", target: "0.32.1", executionMode: "managed_execute", selected: false, risk: "User-scoped npm package" } },
  { viewModel: { id: "update-frontend-design", resourceType: "skill", name: "Frontend Design", current: "7f84c21", target: "c9e9f31", executionMode: "managed_execute", selected: false, risk: "2 files changed, includes scripts" } },
  {
    conflictResolutionRequired: true,
    viewModel: { id: "update-release-pilot", resourceType: "skill", name: "Release Pilot", current: "d24b80c + local", target: "f91a6bc", executionMode: "managed_execute", selected: false, risk: "Blocked by local modification" },
  },
  { viewModel: { id: "update-product", resourceType: "product", name: "STM", current: "0.1.0", target: "0.2.0", executionMode: "signed_product_update", selected: false, risk: "Separate signed product channel" } },
];

export const updateFixtures: UpdateViewModel[] = updateFixtureSeeds.map((seed) =>
  withUpdatePresentationActions(seed.viewModel, {
    conflictResolutionRequired: seed.conflictResolutionRequired,
  }),
);
