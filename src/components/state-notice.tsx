import type { ReasonCode } from "../../contracts/ui/state-contract";
import { reasonCopy } from "../lib/copy";
import { AppIcon } from "./app-icon";

export function StateNotice({ reasonCode }: { reasonCode?: ReasonCode }) {
  if (!reasonCode) return null;
  const copy = reasonCopy[reasonCode];
  const isFailure = reasonCode.includes("failed") || reasonCode.includes("blocked");
  return (
    <section className={`state-notice ${isFailure ? "state-danger" : ""}`} aria-live="polite">
      <AppIcon name={isFailure ? "failure" : "info"} />
      <div>
        <strong>{copy.title}</strong>
        <p>{copy.detail}</p>
      </div>
    </section>
  );
}
