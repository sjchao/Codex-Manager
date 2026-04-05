"use client";

import {
  Calendar,
  Clock,
  Database,
  type LucideIcon,
  RefreshCw,
  Zap,
} from "lucide-react";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button, buttonVariants } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { cn } from "@/lib/utils";
import {
  formatTsFromSeconds,
  getExtraUsageDisplayRows,
  getUsageDisplayBuckets,
  isPrimaryWindowOnlyUsage,
  isSecondaryWindowOnlyUsage,
} from "@/lib/utils/usage";
import { Account } from "@/types";

interface UsageModalProps {
  account: Account | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onRefresh: (id: string) => void;
  isRefreshing: boolean;
}

interface UsageDetailRowProps {
  label: string;
  remainPercent: number | null;
  resetsAt: number | null | undefined;
  icon: LucideIcon;
  tone: "green" | "blue" | "amber";
  caption?: string;
  emptyText?: string;
  emptyResetText?: string;
}

/**
 * 函数 `UsageDetailRow`
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
function UsageDetailRow({
  label,
  remainPercent,
  resetsAt,
  icon: Icon,
  tone,
  caption,
  emptyText = "--",
  emptyResetText = "未知",
}: UsageDetailRowProps) {
  const value = remainPercent ?? 0;
  const toneClasses = {
    blue: {
      icon: "bg-blue-500/10 text-blue-500",
      track: "bg-blue-500/20",
      indicator: "bg-blue-500",
    },
    green: {
      icon: "bg-green-500/10 text-green-500",
      track: "bg-green-500/20",
      indicator: "bg-green-500",
    },
    amber: {
      icon: "bg-amber-500/10 text-amber-500",
      track: "bg-amber-500/20",
      indicator: "bg-amber-500",
    },
  } as const;
  const palette = toneClasses[tone];

  return (
    <div className="space-y-2 rounded-xl border border-primary/5 bg-background/40 px-3 py-3">
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          <div className={cn("rounded-lg p-1.5", palette.icon)}>
            <Icon className="h-3.5 w-3.5" />
          </div>
          <div className="min-w-0 space-y-0.5">
            <span className="block truncate font-medium">{label}</span>
            {caption ? (
              <span className="block text-[10px] text-muted-foreground">{caption}</span>
            ) : null}
          </div>
        </div>
        <div className="shrink-0 text-right">
          <span className="text-base font-semibold">
            {remainPercent == null ? emptyText : `${value}%`}
          </span>
          <span className="ml-1 text-xs text-muted-foreground">
            {remainPercent == null ? "" : "剩余"}
          </span>
        </div>
      </div>

      <Progress
        value={value}
        trackClassName={palette.track}
        indicatorClassName={palette.indicator}
      />

      <div className="flex items-center justify-between gap-3 text-[10px] text-muted-foreground">
        <span className="shrink-0">
          已使用 {remainPercent == null ? "--" : `${Math.max(0, 100 - value)}%`}
        </span>
        <span className="flex min-w-0 items-center justify-end gap-1 text-right">
          <Clock className="h-2.5 w-2.5" />
          重置时间: {formatTsFromSeconds(resetsAt, emptyResetText)}
        </span>
      </div>
    </div>
  );
}

export default function UsageModal({
  account,
  open,
  onOpenChange,
  onRefresh,
  isRefreshing,
}: UsageModalProps) {
  if (!account) return null;
  const primaryWindowOnly = isPrimaryWindowOnlyUsage(account.usage);
  const secondaryWindowOnly = isSecondaryWindowOnlyUsage(account.usage);
  const usageBuckets = getUsageDisplayBuckets(account.usage);
  const extraUsageRows = getExtraUsageDisplayRows(account.usage);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="glass-card border-none p-6 sm:max-w-[450px]">
        <DialogHeader>
          <div className="mb-2 flex items-center gap-3">
            <div className="rounded-full bg-primary/10 p-2 text-primary">
              <Database className="h-5 w-5" />
            </div>
            <DialogTitle>用量详情</DialogTitle>
          </div>
          <DialogDescription className="font-medium text-foreground/80">
            账号: {account.name} ({account.id.slice(0, 8)}...)
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-4 py-4">
          <div className="space-y-3 rounded-2xl border border-primary/5 bg-accent/10 p-4">
            <div className="space-y-1">
              <p className="text-sm font-semibold">额度窗口</p>
              <p className="text-[11px] text-muted-foreground">
                标准 5 小时、7 天周期，以及像 Code Review / Spark 这类专属额度都会在这里按单列依次显示。
              </p>
            </div>

            <div className="space-y-2">
              <UsageDetailRow
                label="5小时额度"
                remainPercent={usageBuckets.primaryRemainPercent}
                resetsAt={usageBuckets.primaryResetsAt}
                icon={Clock}
                tone="green"
                caption="标准模型窗口"
                emptyText={secondaryWindowOnly ? "未提供" : "--"}
                emptyResetText={secondaryWindowOnly ? "未提供" : "未知"}
              />

              <UsageDetailRow
                label="7天周期额度"
                remainPercent={usageBuckets.secondaryRemainPercent}
                resetsAt={usageBuckets.secondaryResetsAt}
                icon={Calendar}
                tone="blue"
                caption="长周期窗口"
                emptyText={primaryWindowOnly ? "未提供" : "--"}
                emptyResetText={primaryWindowOnly ? "未提供" : "未知"}
              />

              {extraUsageRows.map((item) => (
                <UsageDetailRow
                  key={item.id}
                  label={item.label}
                  remainPercent={item.remainPercent}
                  resetsAt={item.resetsAt}
                  icon={Zap}
                  tone="amber"
                  caption={item.windowLabel}
                  emptyText="--"
                  emptyResetText="未知"
                />
              ))}
            </div>
          </div>

          <div className="text-center">
            <p className="text-[10px] italic text-muted-foreground">
              数据捕获于: {formatTsFromSeconds(account.lastRefreshAt, "未知时间")}
            </p>
          </div>
        </div>

        <DialogFooter>
          <DialogClose
            className={buttonVariants({ variant: "ghost" })}
            type="button"
          >
            关闭
          </DialogClose>
          <Button onClick={() => onRefresh(account.id)} disabled={isRefreshing} className="gap-2">
            <RefreshCw className={cn("h-4 w-4", isRefreshing && "animate-spin")} />
            {isRefreshing ? "正在刷新..." : "立即刷新"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
