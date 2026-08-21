import { useEffect, useState } from "react";
import { routes, type RouteId, type RoutePath } from "../../contracts/ui/route-contract";

function readPath(): RoutePath {
  const path = window.location.hash.replace(/^#/, "") || "/";
  return (routes.some((route) => route.path === path) ? path : "/") as RoutePath;
}

export function useHashRoute() {
  const [path, setPath] = useState<RoutePath>(readPath);

  useEffect(() => {
    const update = () => setPath(readPath());
    window.addEventListener("hashchange", update);
    return () => window.removeEventListener("hashchange", update);
  }, []);

  const route = routes.find((candidate) => candidate.path === path)!;
  return { path, routeId: route.id as RouteId };
}
