import { useEffect, useRef, useState } from "react";
import type { ScenarioId } from "../../contracts/ui/state-contract";
import type { AppViewModel } from "../../contracts/ui/view-model-contract";
import { createDesktopRuntimeController } from "./desktop-runtime-controller";
import { runtimeIpcClient } from "../lib/ipc/runtime-ipc-client";

export function useFixtureView(scenario: ScenarioId) {
  const [view, setView] = useState<AppViewModel | null>(null);
  const controllerRef = useRef<ReturnType<typeof createDesktopRuntimeController> | null>(null);
  const [runtimeClient] = useState(() => runtimeIpcClient);
  const desktopMode = runtimeClient.isDesktop();

  useEffect(() => {
    if (!desktopMode) {
      return;
    }

    const controller = createDesktopRuntimeController({
      client: runtimeClient,
      onView: setView,
    });
    controllerRef.current = controller;
    void controller.start();
    return () => {
      controller.dispose();
      controllerRef.current = null;
    };
  }, [desktopMode, runtimeClient]);

  useEffect(() => {
    if (!desktopMode) return;
    const refreshAfterLifecycle = () => {
      void controllerRef.current?.refresh();
    };
    window.addEventListener("stm:lifecycle-settled", refreshAfterLifecycle);
    return () => window.removeEventListener("stm:lifecycle-settled", refreshAfterLifecycle);
  }, [desktopMode]);

  useEffect(() => {
    if (desktopMode) {
      return;
    }

    let active = true;
    void runtimeClient.getAppView(scenario).then((next) => {
      if (active) setView(next);
    });
    return () => {
      active = false;
    };
  }, [desktopMode, runtimeClient, scenario]);

  return view;
}
