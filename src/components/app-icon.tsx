import {
  ArrowClockwise,
  ArrowCounterClockwise,
  ArrowSquareOut,
  CheckCircle,
  ClockCounterClockwise,
  Cube,
  DownloadSimple,
  Gauge,
  GearSix,
  HardDrives,
  Info,
  MagnifyingGlass,
  Package,
  LinkSimple,
  Play,
  Plus,
  ShieldWarning,
  SlidersHorizontal,
  PlugsConnected,
  TerminalWindow,
  Toolbox,
  Warning,
  X,
  XCircle,
} from "@phosphor-icons/react";

export const icons = {
  dashboard: Gauge,
  tools: Toolbox,
  skills: Cube,
  mcp: PlugsConnected,
  updates: DownloadSimple,
  history: ClockCounterClockwise,
  settings: GearSix,
  refresh: ArrowClockwise,
  search: MagnifyingGlass,
  filter: SlidersHorizontal,
  package: Package,
  terminal: TerminalWindow,
  manager: HardDrives,
  warning: Warning,
  privilege: ShieldWarning,
  success: CheckCircle,
  failure: XCircle,
  close: X,
  info: Info,
  link: LinkSimple,
  add: Plus,
  external: ArrowSquareOut,
  run: Play,
  rollback: ArrowCounterClockwise,
} as const;

export type IconName = keyof typeof icons;

export function AppIcon({ name, size = 20, weight = "regular" }: { name: IconName; size?: number; weight?: "regular" | "bold" | "fill" }) {
  const Icon = icons[name];
  return <Icon aria-hidden="true" size={size} weight={weight} />;
}
