export const routes = [
  { id: "dashboard", path: "/", labelKey: "nav.dashboard" },
  { id: "tools", path: "/tools", labelKey: "nav.tools" },
  { id: "skills", path: "/skills", labelKey: "nav.skills" },
  { id: "mcp", path: "/mcp", labelKey: "nav.mcp" },
  { id: "updates", path: "/updates", labelKey: "nav.updates" },
  { id: "history", path: "/history", labelKey: "nav.history" },
  { id: "settings", path: "/settings", labelKey: "nav.settings" },
] as const;

export type RouteId = (typeof routes)[number]["id"];
export type RoutePath = (typeof routes)[number]["path"];
