"use client";

import { useMemo } from "react";
import { isTauriRuntime } from "@/lib/api/transport";
import { useAppStore } from "@/lib/store/useAppStore";
import { RuntimeCapabilities, RuntimeMode } from "@/types";

type RuntimeCapabilityView = {
  runtimeCapabilities: RuntimeCapabilities | null;
  mode: RuntimeMode;
  isDesktopRuntime: boolean;
  isUnsupportedWebRuntime: boolean;
  canManageService: boolean;
  canSelfUpdate: boolean;
  canCloseToTray: boolean;
  canOpenLocalDir: boolean;
  canUseBrowserFileImport: boolean;
  canUseBrowserDownloadExport: boolean;
};

export function useRuntimeCapabilities(): RuntimeCapabilityView {
  const runtimeCapabilities = useAppStore((state) => state.runtimeCapabilities);

  return useMemo(() => {
    const desktopFallback = isTauriRuntime();
    const isDesktopRuntime =
      runtimeCapabilities?.mode === "desktop-tauri" ||
      (!runtimeCapabilities && desktopFallback);
    const mode: RuntimeMode = runtimeCapabilities?.mode ??
      (isDesktopRuntime ? "desktop-tauri" : "unsupported-web");

    return {
      runtimeCapabilities,
      mode,
      isDesktopRuntime,
      isUnsupportedWebRuntime: mode === "unsupported-web",
      canManageService: runtimeCapabilities?.canManageService ?? isDesktopRuntime,
      canSelfUpdate: runtimeCapabilities?.canSelfUpdate ?? isDesktopRuntime,
      canCloseToTray: runtimeCapabilities?.canCloseToTray ?? false,
      canOpenLocalDir: runtimeCapabilities?.canOpenLocalDir ?? isDesktopRuntime,
      canUseBrowserFileImport:
        runtimeCapabilities?.canUseBrowserFileImport ?? !isDesktopRuntime,
      canUseBrowserDownloadExport:
        runtimeCapabilities?.canUseBrowserDownloadExport ?? !isDesktopRuntime,
    };
  }, [runtimeCapabilities]);
}
