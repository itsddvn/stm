import { AppIcon } from "./app-icon";

export function EmptyState({ title, detail }: { title: string; detail: string }) {
  return (
    <section className="empty-state">
      <AppIcon name="package" size={28} />
      <h2>{title}</h2>
      <p>{detail}</p>
    </section>
  );
}
