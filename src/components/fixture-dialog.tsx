import { useEffect, useRef, type ReactNode } from "react";
import { AppIcon } from "./app-icon";
import { useI18n } from "../lib/i18n";

interface FixtureDialogProps {
  open: boolean;
  title: string;
  description: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
}

export function FixtureDialog({ open, title, description, onClose, children, footer }: FixtureDialogProps) {
  const { t } = useI18n();
  const ref = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  }, [open]);

  return (
    <dialog ref={ref} className="fixture-dialog" onClose={onClose} aria-labelledby="dialog-title" aria-describedby="dialog-description">
      <header>
        <div>
          <h2 id="dialog-title">{title}</h2>
          <p id="dialog-description">{description}</p>
        </div>
        <button className="icon-button" type="button" aria-label={t("common.close")} onClick={onClose}><AppIcon name="close" /></button>
      </header>
      <div className="dialog-body">{children}</div>
      {footer ? <footer>{footer}</footer> : null}
    </dialog>
  );
}
