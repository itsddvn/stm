import type { ActionDisabledReasonCode } from "../../contracts/ui/action-contract";
import { actionDisabledReasonCopy } from "../lib/copy";
import { AppIcon } from "./app-icon";

interface ActionDisabledReasonProps {
  compact?: boolean;
  id?: string;
  reasonCode?: ActionDisabledReasonCode;
}

export function ActionDisabledReason({
  compact = false,
  id,
  reasonCode,
}: ActionDisabledReasonProps) {
  if (!reasonCode) return null;
  const copy = actionDisabledReasonCopy[reasonCode];

  return (
    <div
      id={id}
      className={`action-disabled-reason ${compact ? "action-disabled-reason-compact" : ""}`}
    >
      <AppIcon name="warning" size={compact ? 14 : 16} />
      <span>
        <strong>{copy.title}</strong>
        <small>{copy.detail}</small>
      </span>
    </div>
  );
}
