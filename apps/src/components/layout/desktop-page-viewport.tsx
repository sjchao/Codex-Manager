"use client";

import { type ComponentType, useEffect, useMemo, useState } from "react";
import { usePathname } from "next/navigation";
import { useAppStore } from "@/lib/store/useAppStore";
import { cn } from "@/lib/utils";
import { ROOT_PAGE_PATHS, type RootPagePath } from "@/lib/routes/root-page-paths";
import { normalizeRoutePath } from "@/lib/utils/static-routes";
import DashboardPage from "@/app/page";
import AccountsPage from "@/app/accounts/page";
import AggregateApiPage from "@/app/aggregate-api/page";
import ApiKeysPage from "@/app/apikeys/page";
import PluginsPage from "@/app/plugins/page";
import QoderPage from "@/app/qoder/page";
import LogsPage from "@/app/logs/page";
import SettingsPage from "@/app/settings/page";

const PAGE_COMPONENTS: Record<RootPagePath, ComponentType> = {
  "/": DashboardPage,
  "/accounts": AccountsPage,
  "/aggregate-api": AggregateApiPage,
  "/apikeys": ApiKeysPage,
  "/plugins": PluginsPage,
  "/qoder": QoderPage,
  "/logs": LogsPage,
  "/settings": SettingsPage,
};

/**
 * 函数 `DesktopPageViewport`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - params: 参数 params
 *
 * # 返回
 * 返回函数执行结果
 */
export function DesktopPageViewport({ children }: { children: React.ReactNode }) {
  const pathname = normalizeRoutePath(usePathname());
  const pendingRoutePath = useAppStore((state) => state.pendingRoutePath);
  const runtimeCapabilities = useAppStore((state) => state.runtimeCapabilities);
  const [visitedRoutes, setVisitedRoutes] = useState<RootPagePath[]>([]);
  const keepAliveEnabled = runtimeCapabilities?.mode === "desktop-tauri";
  const normalizedPendingRoutePath = pendingRoutePath
    ? normalizeRoutePath(pendingRoutePath)
    : "";
  const effectivePathname =
    keepAliveEnabled && normalizedPendingRoutePath
      ? normalizedPendingRoutePath
      : pathname;

  const activeRootRoute = useMemo(() => {
    const matchedRoute = ROOT_PAGE_PATHS.find((item) => item === effectivePathname);
    return matchedRoute ?? null;
  }, [effectivePathname]);

  useEffect(() => {
    if (!keepAliveEnabled || !activeRootRoute) {
      return;
    }
    if (typeof window === "undefined") {
      return;
    }
    const frameId = window.requestAnimationFrame(() => {
      setVisitedRoutes((current) =>
        current.includes(activeRootRoute) ? current : [...current, activeRootRoute],
      );
    });
    return () => {
      window.cancelAnimationFrame(frameId);
    };
  }, [activeRootRoute, keepAliveEnabled]);

  if (!keepAliveEnabled || !activeRootRoute) {
    return <>{children}</>;
  }

  return (
    <div className="relative min-h-full">
      {visitedRoutes.map((routePath) => {
        const PageComponent = PAGE_COMPONENTS[routePath];
        const isActive = routePath === activeRootRoute;

        return (
          <section
            key={routePath}
            data-route-path={routePath}
            aria-hidden={!isActive}
            className={cn(!isActive && "hidden")}
          >
            <PageComponent />
          </section>
        );
      })}
    </div>
  );
}
