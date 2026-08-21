export function LoadingTable() {
  return (
    <div className="loading-table" aria-live="polite" aria-busy="true">
      <span>Loading fixture data…</span>
      {Array.from({ length: 6 }, (_, index) => <div className="skeleton-row" key={index} />)}
    </div>
  );
}
