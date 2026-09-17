"use client";

import { useEffect, useMemo, useRef } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { accountClient } from "@/lib/api/account-client";
import { serviceClient } from "@/lib/api/service-client";
import { useDeferredDesktopActivation } from "@/hooks/useDeferredDesktopActivation";
import {
  buildStartupSnapshotQueryKey,
  hasStartupSnapshotSignal,
  STARTUP_SNAPSHOT_REQUEST_LOG_LIMIT,
  STARTUP_SNAPSHOT_STALE_TIME,
  STARTUP_SNAPSHOT_WARMUP_INTERVAL_MS,
  STARTUP_SNAPSHOT_WARMUP_TIMEOUT_MS,
} from "@/lib/api/startup-snapshot";
import { useAppStore } from "@/lib/store/useAppStore";

const DASHBOARD_REFETCH_INTERVAL_MS = 60_000;

function isTimestampToday(value: number | null | undefined): boolean {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return false;
  }
  const current = new Date();
  const timestamp = new Date(value * 1000);
  return (
    current.getFullYear() === timestamp.getFullYear() &&
    current.getMonth() === timestamp.getMonth() &&
    current.getDate() === timestamp.getDate()
  );
}

/**
 * 函数 `useDashboardStats`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * 无
 *
 * # 返回
 * 返回函数执行结果
 */
export function useDashboardStats() {
  const queryClient = useQueryClient();
  const serviceStatus = useAppStore((state) => state.serviceStatus);
  const isServiceReady = serviceStatus.connected;
  const isSnapshotQueryEnabled = useDeferredDesktopActivation(isServiceReady);
  const warmupStartedAtRef = useRef<number | null>(null);

  useEffect(() => {
    if (!isServiceReady) {
      warmupStartedAtRef.current = null;
      return;
    }
    warmupStartedAtRef.current = Date.now();
  }, [isServiceReady, serviceStatus.addr]);

  const snapshotQuery = useQuery({
    queryKey: buildStartupSnapshotQueryKey(
      serviceStatus.addr,
      STARTUP_SNAPSHOT_REQUEST_LOG_LIMIT
    ),
    queryFn: () =>
      serviceClient.getStartupSnapshot({
        requestLogLimit: STARTUP_SNAPSHOT_REQUEST_LOG_LIMIT,
      }),
    enabled: isSnapshotQueryEnabled,
    retry: 1,
    staleTime: STARTUP_SNAPSHOT_STALE_TIME,
    refetchInterval: (query) => {
      if (!isServiceReady) return false;
      const startedAt = warmupStartedAtRef.current;
      if (startedAt == null) return false;
      if (Date.now() - startedAt >= STARTUP_SNAPSHOT_WARMUP_TIMEOUT_MS) {
        warmupStartedAtRef.current = null;
        return false;
      }

      const snapshot = query.state.data;
      if (!snapshot || snapshot.accounts.length === 0) {
        return false;
      }

      return hasStartupSnapshotSignal(snapshot)
        ? false
        : STARTUP_SNAPSHOT_WARMUP_INTERVAL_MS;
    },
    refetchIntervalInBackground: true,
  });

  const apiKeyUsageStatsQuery = useQuery({
    queryKey: ["apikey-usage-stats", serviceStatus.addr || null],
    queryFn: () => accountClient.listApiKeyUsageStats(),
    enabled: isSnapshotQueryEnabled,
    retry: 1,
    refetchInterval: DASHBOARD_REFETCH_INTERVAL_MS,
  });
  const aggregateApisQuery = useQuery({
    queryKey: ["aggregate-apis"],
    queryFn: () => accountClient.listAggregateApis(),
    enabled: isSnapshotQueryEnabled,
    retry: 1,
    refetchInterval: DASHBOARD_REFETCH_INTERVAL_MS,
  });
  const sub2ApiAccountsQuery = useQuery({
    queryKey: ["sub2api-accounts"],
    queryFn: () => accountClient.listSub2Api(),
    enabled: isSnapshotQueryEnabled,
    retry: 1,
    refetchInterval: DASHBOARD_REFETCH_INTERVAL_MS,
  });
  const modelTokenUsageQuery = useQuery({
    queryKey: ["requestlog-model-usage", serviceStatus.addr || null],
    queryFn: () => serviceClient.getModelTokenUsage(),
    enabled: isSnapshotQueryEnabled,
    retry: 1,
    refetchInterval: DASHBOARD_REFETCH_INTERVAL_MS,
  });

  const refreshSub2ApiAccounts = useMutation({
    mutationFn: () => accountClient.syncAllSub2Api(),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["sub2api-accounts"] });
    },
    onError: (error: unknown) => {
      toast.error(error instanceof Error ? error.message : String(error));
    },
  });

  const data = snapshotQuery.data;
  const accounts = data?.accounts || [];
  const hasStartupSignal = hasStartupSnapshotSignal(data);
  const shouldWarmupPoll =
    isServiceReady &&
    accounts.length > 0 &&
    !hasStartupSignal &&
    snapshotQuery.isFetching;
  const hasSnapshotData = Boolean(data);

  const apiKeyUsageStats = useMemo(
    () => apiKeyUsageStatsQuery.data || [],
    [apiKeyUsageStatsQuery.data]
  );
  const aggregateApis = useMemo(
    () => aggregateApisQuery.data || [],
    [aggregateApisQuery.data]
  );
  const sub2ApiAccounts = useMemo(
    () => sub2ApiAccountsQuery.data || [],
    [sub2ApiAccountsQuery.data]
  );
  const modelTokenUsage = useMemo(
    () => modelTokenUsageQuery.data || [],
    [modelTokenUsageQuery.data]
  );

  const apiKeyNames = useMemo(() => {
    const names: Record<string, string> = {};
    for (const apiKey of data?.apiKeys || []) {
      const id = String(apiKey.id || "").trim();
      if (!id) continue;
      names[id] = String(apiKey.name || "").trim() || id;
    }
    return names;
  }, [data?.apiKeys]);

  const totals = useMemo(() => {
    let todayTokens = 0;
    let totalTokens = 0;
    let todayDeepseekTokens = 0;
    let totalDeepseekTokens = 0;
    let totalCostUsd = 0;
    for (const item of apiKeyUsageStats) {
      todayTokens += Math.max(0, item.todayTokens || 0);
      totalTokens += Math.max(0, item.totalTokens || 0);
      todayDeepseekTokens += Math.max(0, item.todayDeepseekTokens || 0);
      totalDeepseekTokens += Math.max(0, item.totalDeepseekTokens || 0);
      totalCostUsd += Math.max(0, item.actualCostUsd || 0);
    }
    return {
      todayTokens,
      totalTokens,
      todayDeepseekTokens,
      totalDeepseekTokens,
      totalCostUsd,
    };
  }, [apiKeyUsageStats]);

  const sub2ApiTotals = useMemo(() => {
    let todayActualCost = 0;
    let lastSyncAt: number | null = null;
    for (const account of sub2ApiAccounts) {
      if (
        typeof account.lastSyncAt === "number" &&
        Number.isFinite(account.lastSyncAt) &&
        (lastSyncAt == null || account.lastSyncAt > lastSyncAt)
      ) {
        lastSyncAt = account.lastSyncAt;
      }
      if (!isTimestampToday(account.lastSyncAt)) {
        continue;
      }
      if (
        typeof account.todayActualCost === "number" &&
        Number.isFinite(account.todayActualCost)
      ) {
        todayActualCost += Math.max(0, account.todayActualCost);
      }
    }
    return { todayActualCost, lastSyncAt };
  }, [sub2ApiAccounts]);

  const isLoading =
    (!isServiceReady && !hasSnapshotData) ||
    (!isSnapshotQueryEnabled && !data) ||
    snapshotQuery.isPending ||
    shouldWarmupPoll ||
    (isSnapshotQueryEnabled &&
      (apiKeyUsageStatsQuery.isPending ||
        aggregateApisQuery.isPending ||
        sub2ApiAccountsQuery.isPending ||
        modelTokenUsageQuery.isPending));

  return {
    totals,
    apiKeyUsageStats,
    apiKeyNames,
    aggregateApis,
    sub2ApiAccounts,
    sub2ApiTotals,
    modelTokenUsage,
    refreshSub2ApiAccounts,
    isLoading,
    isServiceReady,
  };
}
