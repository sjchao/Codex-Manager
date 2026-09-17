"use client";

import { useMemo } from "react";
import {
  BarChart3,
  BrainCircuit,
  DollarSign,
  Gauge,
  KeyRound,
  RefreshCw,
  Wallet,
  Zap,
  type LucideIcon,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { useDashboardStats } from "@/hooks/useDashboardStats";
import { usePageTransitionReady } from "@/hooks/usePageTransitionReady";
import { cn } from "@/lib/utils";
import { formatCompactNumber, formatTsFromSeconds } from "@/lib/utils/usage";

interface MetricCardProps {
  title: string;
  value: string;
  sub: string;
  icon: LucideIcon;
  color: string;
}

interface BarItem {
  label: string;
  ratio: number;
  display: string;
  hint?: string;
}

const BAR_TONES = {
  primary: "bg-primary",
  emerald: "bg-emerald-500",
  sky: "bg-sky-500",
  violet: "bg-violet-500",
  amber: "bg-amber-500",
} as const;

type BarTone = keyof typeof BAR_TONES;

const TOP_KEY_LIMIT = 10;
const MODEL_LIMIT = 12;

/**
 * 函数 `formatCompactTokenAmount`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - value: 参数 value
 *
 * # 返回
 * 返回函数执行结果
 */
function formatCompactTokenAmount(value: number | null | undefined): string {
  const normalized =
    typeof value === "number" && Number.isFinite(value) ? Math.max(0, value) : 0;
  if (normalized < 1000) {
    return normalized.toLocaleString("zh-CN", {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    });
  }
  return formatCompactNumber(normalized, "0.00", 2, true);
}

/**
 * 函数 `formatUsd`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - value: 参数 value
 *
 * # 返回
 * 返回函数执行结果
 */
function formatUsd(value: number | null | undefined): string {
  const normalized =
    typeof value === "number" && Number.isFinite(value) ? Math.max(0, value) : 0;
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(normalized);
}

/**
 * 函数 `formatPreciseUsd`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - value: 参数 value
 *
 * # 返回
 * 返回函数执行结果
 */
function formatPreciseUsd(value: number | null | undefined): string {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return "--";
  }
  const abs = Math.abs(value);
  const digits = abs >= 100 ? 2 : abs >= 1 ? 4 : 6;
  return `$${value.toFixed(digits)}`;
}

/**
 * 函数 `formatCacheHitRate`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - value: 参数 value
 *
 * # 返回
 * 返回函数执行结果
 */
function formatCacheHitRate(value: number | null | undefined): string {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return "-";
  }
  return `${(value * 100).toFixed(1)}%`;
}

/**
 * 函数 `formatSub2ApiAccountLabel`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - accountName: 参数 accountName
 * - accountEmail: 参数 accountEmail
 *
 * # 返回
 * 返回函数执行结果
 */
function formatSub2ApiAccountLabel(
  accountName: string | null,
  accountEmail: string | null
): string {
  const name = String(accountName || "").trim();
  const email = String(accountEmail || "").trim();
  if (!name) {
    return email || "-";
  }
  if (!email || email === name) {
    return name;
  }
  return `${name} · ${email}`;
}

/**
 * 函数 `MetricCard`
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
function MetricCard({ title, value, sub, icon: Icon, color }: MetricCardProps) {
  return (
    <Card className="glass-card overflow-hidden border-none shadow-md backdrop-blur-md transition-all hover:scale-[1.02]">
      <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
        <CardTitle className="text-sm font-medium">{title}</CardTitle>
        <Icon className={cn("h-4 w-4", color)} />
      </CardHeader>
      <CardContent>
        <div className="text-2xl font-bold">{value}</div>
        <p className="mt-1 text-[10px] text-muted-foreground">{sub}</p>
      </CardContent>
    </Card>
  );
}

/**
 * 函数 `BarList`
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
function BarList({
  items,
  tone,
  emptyText,
  showRank = false,
}: {
  items: BarItem[];
  tone: BarTone;
  emptyText: string;
  showRank?: boolean;
}) {
  if (items.length === 0) {
    return (
      <div className="flex h-40 items-center justify-center text-xs text-muted-foreground">
        {emptyText}
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {items.map((item, index) => {
        const width =
          item.ratio <= 0 ? 0 : Math.max(3, Math.min(100, item.ratio * 100));
        return (
          <div key={`${item.label}-${index}`} className="space-y-1">
            <div className="flex items-baseline justify-between gap-3 text-xs">
              <span className="flex min-w-0 flex-1 items-baseline gap-1.5">
                {showRank ? (
                  <span className="shrink-0 font-mono text-[10px] text-muted-foreground">
                    {index + 1}
                  </span>
                ) : null}
                <span className="min-w-0 truncate" title={item.label}>
                  {item.label}
                </span>
              </span>
              <span
                className="shrink-0 font-mono font-semibold"
                title={item.hint || item.label}
              >
                {item.display}
              </span>
            </div>
            <div className="h-2 w-full overflow-hidden rounded-full bg-muted/60">
              <div
                className={cn("h-full rounded-full transition-all", BAR_TONES[tone])}
                style={{ width: `${width}%` }}
              />
            </div>
          </div>
        );
      })}
    </div>
  );
}

/**
 * 函数 `ChartSkeleton`
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
function ChartSkeleton() {
  return (
    <div className="space-y-4">
      {Array.from({ length: 4 }).map((_, index) => (
        <div key={index} className="space-y-2">
          <Skeleton className="h-3 w-full" />
          <Skeleton className="h-2 w-3/4" />
        </div>
      ))}
    </div>
  );
}

export default function DashboardPage() {
  const {
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
  } = useDashboardStats();
  usePageTransitionReady("/", !isServiceReady || !isLoading);

  const modelItems = useMemo<BarItem[]>(() => {
    const items = modelTokenUsage
      .filter((item) => item.totalTokens > 0)
      .slice(0, MODEL_LIMIT);
    const maxTokens = items.reduce(
      (max, item) => Math.max(max, item.totalTokens),
      0
    );
    return items.map((item) => ({
      label: item.model,
      ratio: maxTokens > 0 ? item.totalTokens / maxTokens : 0,
      display: formatCompactTokenAmount(item.totalTokens),
      hint: `${item.requestCount} 次请求 · 输入 ${formatCompactTokenAmount(
        item.inputTokens
      )} · 缓存 ${formatCompactTokenAmount(
        item.cachedInputTokens
      )} · 输出 ${formatCompactTokenAmount(item.outputTokens)}`,
    }));
  }, [modelTokenUsage]);

  const supplierItems = useMemo<BarItem[]>(() => {
    return aggregateApis
      .filter(
        (api) =>
          api.todayCacheHitRate != null &&
          api.todayCacheHitRate > 0 &&
          api.todayInputTokens > 0
      )
      .sort(
        (left, right) =>
          (right.todayCacheHitRate || 0) - (left.todayCacheHitRate || 0)
      )
      .map((api) => ({
        label: String(api.supplierName || "").trim() || api.url,
        ratio: Math.max(0, Math.min(1, api.todayCacheHitRate || 0)),
        display: formatCacheHitRate(api.todayCacheHitRate),
        hint: `输入 ${formatCompactTokenAmount(
          api.todayInputTokens
        )} · 缓存 ${formatCompactTokenAmount(api.todayCachedInputTokens)}`,
      }));
  }, [aggregateApis]);

  const topCostKeyItems = useMemo<BarItem[]>(() => {
    const items = apiKeyUsageStats
      .filter((item) => item.todayActualCostUsd > 0)
      .sort((left, right) => right.todayActualCostUsd - left.todayActualCostUsd)
      .slice(0, TOP_KEY_LIMIT);
    const maxCost = items.reduce(
      (max, item) => Math.max(max, item.todayActualCostUsd),
      0
    );
    return items.map((item) => ({
      label: apiKeyNames[item.keyId] || item.keyId,
      ratio: maxCost > 0 ? item.todayActualCostUsd / maxCost : 0,
      display: formatUsd(item.todayActualCostUsd),
    }));
  }, [apiKeyUsageStats, apiKeyNames]);

  const topTokenKeyItems = useMemo<BarItem[]>(() => {
    const items = apiKeyUsageStats
      .filter((item) => item.todayTokens > 0)
      .sort((left, right) => right.todayTokens - left.todayTokens)
      .slice(0, TOP_KEY_LIMIT);
    const maxTokens = items.reduce(
      (max, item) => Math.max(max, item.todayTokens),
      0
    );
    return items.map((item) => ({
      label: apiKeyNames[item.keyId] || item.keyId,
      ratio: maxTokens > 0 ? item.todayTokens / maxTokens : 0,
      display: formatCompactTokenAmount(item.todayTokens),
    }));
  }, [apiKeyUsageStats, apiKeyNames]);

  const topDeepseekKeyItems = useMemo<BarItem[]>(() => {
    const items = apiKeyUsageStats
      .filter((item) => item.todayDeepseekTokens > 0)
      .sort((left, right) => right.todayDeepseekTokens - left.todayDeepseekTokens)
      .slice(0, TOP_KEY_LIMIT);
    const maxTokens = items.reduce(
      (max, item) => Math.max(max, item.todayDeepseekTokens),
      0
    );
    return items.map((item) => ({
      label: apiKeyNames[item.keyId] || item.keyId,
      ratio: maxTokens > 0 ? item.todayDeepseekTokens / maxTokens : 0,
      display: formatCompactTokenAmount(item.todayDeepseekTokens),
    }));
  }, [apiKeyUsageStats, apiKeyNames]);

  return (
    <div className="space-y-6 animate-in fade-in duration-700">
      {!isServiceReady ? (
        <Card className="glass-card border-none shadow-sm">
          <CardContent className="pt-6 text-sm text-muted-foreground">
            服务未连接，仪表盘数据暂不可用；连接恢复后会自动继续加载。
          </CardContent>
        </Card>
      ) : null}

      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
        {isLoading ? (
          Array.from({ length: 3 }).map((_, index) => (
            <Skeleton key={index} className="h-32 w-full rounded-2xl" />
          ))
        ) : (
          <>
            <MetricCard
              title="今日 Token"
              value={formatCompactTokenAmount(totals.todayTokens)}
              sub={`历史累计 ${formatCompactTokenAmount(totals.totalTokens)}`}
              icon={Zap}
              color="text-amber-500"
            />
            <MetricCard
              title="今日 DeepSeek Token"
              value={formatCompactTokenAmount(totals.todayDeepseekTokens)}
              sub={`历史累计 ${formatCompactTokenAmount(
                totals.totalDeepseekTokens
              )}`}
              icon={BrainCircuit}
              color="text-violet-500"
            />
            <MetricCard
              title="今日消费"
              value={formatUsd(sub2ApiTotals.todayActualCost)}
              sub={`历史累计 ${formatUsd(totals.totalCostUsd)}`}
              icon={DollarSign}
              color="text-emerald-500"
            />
          </>
        )}
      </div>

      <div className="grid gap-6 md:grid-cols-2 lg:grid-cols-3">
        <Card className="glass-card border-none shadow-md">
          <CardHeader className="flex flex-row items-center justify-between space-y-0">
            <CardTitle className="text-base font-semibold">Sub2API 账户</CardTitle>
            <div className="flex items-center gap-1">
              <Wallet className="h-4 w-4 text-emerald-500" />
              <Button
                variant="ghost"
                size="icon-sm"
                title="同步全部账户"
                onClick={() => refreshSub2ApiAccounts.mutate()}
                disabled={
                  !isServiceReady ||
                  sub2ApiAccounts.length === 0 ||
                  refreshSub2ApiAccounts.isPending
                }
              >
                <RefreshCw
                  className={
                    refreshSub2ApiAccounts.isPending
                      ? "h-4 w-4 animate-spin"
                      : "h-4 w-4"
                  }
                />
              </Button>
            </div>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <div className="space-y-3">
                <Skeleton className="h-16 w-full rounded-xl" />
                <Skeleton className="h-16 w-full rounded-xl" />
              </div>
            ) : sub2ApiAccounts.length === 0 ? (
              <div className="flex h-32 items-center justify-center text-xs text-muted-foreground">
                暂无 Sub2API 账户，可在聚合 API 页面添加
              </div>
            ) : (
              <div className="space-y-3">
                {sub2ApiAccounts.map((account) => {
                  const accountLabel = formatSub2ApiAccountLabel(
                    account.accountName,
                    account.accountEmail
                  );
                  return (
                    <div
                      key={account.id}
                      className="flex items-center justify-between gap-3 rounded-xl bg-accent/20 p-3"
                    >
                      <div className="min-w-0">
                        <p
                          className="truncate text-xs font-medium"
                          title={account.baseUrl}
                        >
                          {account.baseUrl}
                        </p>
                        <p
                          className="truncate text-[10px] text-muted-foreground"
                          title={accountLabel}
                        >
                          {accountLabel}
                        </p>
                      </div>
                      <div className="shrink-0 space-y-0.5 text-right">
                        <p className="font-mono text-sm font-semibold">
                          <span className="mr-1 font-sans text-[10px] font-normal text-muted-foreground">
                            余额
                          </span>
                          {formatPreciseUsd(account.balance)}
                        </p>
                        <p className="font-mono text-sm font-semibold">
                          <span className="mr-1 font-sans text-[10px] font-normal text-muted-foreground">
                            当日
                          </span>
                          {formatPreciseUsd(account.todayActualCost)}
                        </p>
                      </div>
                    </div>
                  );
                })}
                {sub2ApiTotals.lastSyncAt ? (
                  <p className="text-[10px] text-muted-foreground">
                    最后同步{" "}
                    {formatTsFromSeconds(sub2ApiTotals.lastSyncAt, "未知")}
                  </p>
                ) : null}
              </div>
            )}
          </CardContent>
        </Card>

        <Card className="glass-card border-none shadow-md">
          <CardHeader className="flex flex-row items-center justify-between space-y-0">
            <CardTitle className="text-base font-semibold">
              今日各模型 Token 消耗
            </CardTitle>
            <BarChart3 className="h-4 w-4 text-violet-500" />
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <ChartSkeleton />
            ) : (
              <BarList
                items={modelItems}
                tone="violet"
                emptyText="今日暂无模型调用记录"
              />
            )}
          </CardContent>
        </Card>

        <Card className="glass-card border-none shadow-md">
          <CardHeader className="flex flex-row items-center justify-between space-y-0">
            <CardTitle className="text-base font-semibold">
              今日供应商缓存命中率
            </CardTitle>
            <Gauge className="h-4 w-4 text-sky-500" />
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <ChartSkeleton />
            ) : (
              <BarList
                items={supplierItems}
                tone="sky"
                emptyText="今日暂无可用缓存命中数据"
              />
            )}
          </CardContent>
        </Card>
      </div>

      <div className="grid gap-6 md:grid-cols-2 lg:grid-cols-3">
        <Card className="glass-card border-none shadow-md">
          <CardHeader className="flex flex-row items-center justify-between space-y-0">
            <CardTitle className="text-base font-semibold">
              今日消费 Top 10
            </CardTitle>
            <DollarSign className="h-4 w-4 text-emerald-500" />
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <ChartSkeleton />
            ) : (
              <BarList
                items={topCostKeyItems}
                tone="emerald"
                emptyText="今日暂无消费记录"
                showRank
              />
            )}
          </CardContent>
        </Card>

        <Card className="glass-card border-none shadow-md">
          <CardHeader className="flex flex-row items-center justify-between space-y-0">
            <CardTitle className="text-base font-semibold">
              今日 Token Top 10
            </CardTitle>
            <Zap className="h-4 w-4 text-amber-500" />
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <ChartSkeleton />
            ) : (
              <BarList
                items={topTokenKeyItems}
                tone="amber"
                emptyText="今日暂无 Token 消耗"
                showRank
              />
            )}
          </CardContent>
        </Card>

        <Card className="glass-card border-none shadow-md">
          <CardHeader className="flex flex-row items-center justify-between space-y-0">
            <CardTitle className="text-base font-semibold">
              今日 DeepSeek 用量 Top 10
            </CardTitle>
            <KeyRound className="h-4 w-4 text-violet-500" />
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <ChartSkeleton />
            ) : (
              <BarList
                items={topDeepseekKeyItems}
                tone="violet"
                emptyText="今日暂无 DeepSeek 用量"
                showRank
              />
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
