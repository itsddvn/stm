import { AppIcon } from "./app-icon";

interface SearchFilterBarProps {
  query: string;
  onQueryChange: (value: string) => void;
  filter: string;
  onFilterChange: (value: string) => void;
  options: { value: string; label: string }[];
  label: string;
}

export function SearchFilterBar({ query, onQueryChange, filter, onFilterChange, options, label }: SearchFilterBarProps) {
  return (
    <div className="search-filter-bar">
      <label className="search-control">
        <span className="sr-only">Search {label}</span>
        <AppIcon name="search" size={18} />
        <input name={`${label}-search`} type="search" value={query} autoComplete="off" onChange={(event) => onQueryChange(event.target.value)} placeholder={`Search ${label}…`} />
      </label>
      <label className="filter-control">
        <span className="sr-only">Filter {label}</span>
        <AppIcon name="filter" size={18} />
        <select name={`${label}-filter`} value={filter} onChange={(event) => onFilterChange(event.target.value)}>
          {options.map((option) => <option value={option.value} key={option.value}>{option.label}</option>)}
        </select>
      </label>
    </div>
  );
}
