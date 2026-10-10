"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import {
  keepPreviousData,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import {
  CalendarClock,
  Coins,
  RefreshCw,
  Settings2,
} from "lucide-react";
import { toast } from "sonner";
import {
  hasSeatQuota,
  memberLabel,
} from "@/components/qoder/member-quota-bar-chart";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { qoderClient } from "@/lib/api/qoder-client";
import { cn } from "@/lib/utils";
import { formatTsFromSeconds } from "@/lib/utils/usage";
import { useAppStore } from "@/lib/store/useAppStore";
import { useDesktopPageActive } from "@/hooks/useDesktopPageActive";
import { useDeferredDesktopActivation } from "@/hooks/useDeferredDesktopActivation";
import { usePageTransitionReady } from "@/hooks/usePageTransitionReady";
import { useRuntimeCapabilities } from "@/hooks/useRuntimeCapabilities";
import {
  QoderModelUsage,
  QoderSyncStatus,
  QoderUsageBucket,
} from "@/types";

const DEFAULT_QODER_BASE_URL = "https://api.qoder.com.cn";
const EVENTS_PAGE_SIZE = 20;
const ALL_MEMBERS_FILTER = "all";

const MODEL_COLORS = [
  "#0ea5e9",
  "#8b5cf6",
  "#10b981",
  "#f59e0b",
  "#f43f5e",
  "#06b6d4",
  "#6366f1",
  "#84cc16",
  "#ec4899",
  "#14b8a6",
];

type RangePreset = "today" | "yesterday" | "dayBefore" | "7d" | "30d" | "custom";

const RANGE_PRESETS: Array<{ id: RangePreset; label: string }> = [
  { id: "today", label: "今日" },
  { id: "yesterday", label: "昨天" },
  { id: "dayBefore", label: "前天" },
  { id: "7d", label: "近 7 天" },
  { id: "30d", label: "近 30 天" },
  { id: "custom", label: "自定义" },
];

function pad2(value: number): string {
  return String(value).padStart(2, "0");
}

function formatDay(date: Date): string {
  return `${date.getFullYear()}-${pad2(date.getMonth() + 1)}-${pad2(date.getDate())}`;
}

function shiftDay(day: string, offsetDays: number): string {
  const [year, month, date] = day.split("-").map(Number);
  const value = new Date(year, (month || 1) - 1, date || 1);
  value.setDate(value.getDate() + offsetDays);
  return formatDay(value);
}

function buildPresetRange(
  preset: RangePreset,
  customStart: string,
  customEnd: string
): { startDay: string; endDay: string } {
  const today = formatDay(new Date());
  switch (preset) {
    case "today":
      return { startDay: today, endDay: today };
    case "yesterday": {
      const yesterday = shiftDay(today, -1);
      return { startDay: yesterday, endDay: yesterday };
    }
    case "dayBefore": {
      const dayBefore = shiftDay(today, -2);
      return { startDay: dayBefore, endDay: dayBefore };
    }
    case "7d":
      return { startDay: shiftDay(today, -6), endDay: today };
    case "30d":
      return { startDay: shiftDay(today, -29), endDay: today };
    case "custom":
    default:
      return { startDay: customStart || today, endDay: customEnd || today };
  }
}

function formatCredits(value: number | null | undefined): string {
  const normalized =
    typeof value === "number" && Number.isFinite(value) ? value : 0;
  return normalized.toLocaleString("zh-CN", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
}

function formatEventTime(timestamp: number): string {
  if (!Number.isFinite(timestamp) || timestamp <= 0) {
    return "-";
  }
  const date = new Date(timestamp);
  return `${pad2(date.getMonth() + 1)}-${pad2(date.getDate())} ${pad2(
    date.getHours()
  )}:${pad2(date.getMinutes())}:${pad2(date.getSeconds())}`;
}

function modelLabel(modelTier: string | null | undefined): string {
  const value = String(modelTier || "").trim();
  return value || "未标注模型";
}

function describeStatus(status: QoderSyncStatus | undefined): {
  label: string;
  variant: "default" | "secondary" | "destructive" | "outline";
} {
  if (!status) {
    return { label: "读取中", variant: "secondary" };
  }
  if (!status.configured) {
    return { label: "未配置", variant: "secondary" };
  }
  if (!status.enabled) {
    return { label: "已暂停", variant: "secondary" };
  }
  if (status.running) {
    return { label: "同步中", variant: "default" };
  }
  if (status.lastSyncStatus === "failed") {
    return { label: "同步失败", variant: "destructive" };
  }
  if (status.lastSyncStatus === "success") {
    return { label: "自动同步中 · 每 5 分钟", variant: "outline" };
  }
  return { label: "等待首次同步", variant: "secondary" };
}

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

function StackedUsageChart({
  buckets,
  models,
  halfHour,
}: {
  buckets: QoderUsageBucket[];
  models: QoderModelUsage[];
  halfHour: boolean;
}) {
  const colors = useMemo(() => {
    const map = new Map<string, string>();
    models.forEach((model, index) => {
      map.set(model.modelTier, MODEL_COLORS[index % MODEL_COLORS.length]);
    });
    return map;
  }, [models]);

  const maxTotal = useMemo(
    () =>
      buckets.reduce(
        (max, bucket) =>
          Math.max(
            max,
            bucket.models.reduce(
              (sum, model) => sum + Math.max(0, model.credits),
              0
            )
          ),
        0
      ),
    [buckets]
  );

  if (buckets.length === 0 || maxTotal <= 0) {
    return (
      <div className="flex h-56 items-center justify-center text-xs text-muted-foreground">
        该时间段暂无积分消耗
      </div>
    );
  }

  const labelStep = Math.max(1, Math.ceil(buckets.length / 8));

  return (
    <div>
      <div className="flex h-56 items-stretch gap-[2px]">
        {buckets.map((bucket) => {
          const tooltipLines = [
            `${bucket.label} · 合计 ${formatCredits(bucket.credits)} 积分 · ${
              bucket.events
            } 次调用`,
            ...bucket.models.map(
              (model) =>
                `${modelLabel(model.modelTier)}：${formatCredits(
                  model.credits
                )}（${model.events} 次）`
            ),
          ];
          return (
            <div
              key={bucket.key}
              className="flex h-full flex-1 flex-col justify-end gap-px"
              title={tooltipLines.join("\n")}
            >
              {bucket.models.map((model) => {
                if (model.credits <= 0) {
                  return null;
                }
                const height = (model.credits / maxTotal) * 100;
                return (
                  <div
                    key={model.modelTier || "unknown"}
                    className="w-full rounded-[2px] transition-opacity hover:opacity-80"
                    style={{
                      height: `${Math.max(1, height)}%`,
                      backgroundColor:
                        colors.get(model.modelTier) || MODEL_COLORS[0],
                    }}
                  />
                );
              })}
            </div>
          );
        })}
      </div>
      <div className="mt-1 flex gap-[2px]">
        {buckets.map((bucket, index) => (
          <div
            key={bucket.key}
            className="min-w-0 flex-1 text-center text-[9px] text-muted-foreground"
          >
            {halfHour ? (
              // 半小时粒度：每个整点都显示刻度（如 9点、16点）
              bucket.label.endsWith(":00") ? (
                <span className="block whitespace-nowrap">
                  {parseInt(bucket.label.slice(0, 2), 10)}点
                </span>
              ) : null
            ) : index % labelStep === 0 ? (
              <span className="block truncate">{bucket.label}</span>
            ) : null}
          </div>
        ))}
      </div>
      <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1.5">
        {models.map((model, index) => (
          <span
            key={model.modelTier || `model-${index}`}
            className="flex items-center gap-1.5 text-xs"
          >
            <span
              className="h-2.5 w-2.5 shrink-0 rounded-sm"
              style={{
                backgroundColor: colors.get(model.modelTier) || MODEL_COLORS[0],
              }}
            />
            <span className="text-muted-foreground">
              {modelLabel(model.modelTier)}
            </span>
            <span className="font-mono font-semibold">
              {formatCredits(model.credits)}
            </span>
          </span>
        ))}
      </div>
    </div>
  );
}

export default function QoderPage() {
  const queryClient = useQueryClient();
  const serviceStatus = useAppStore((state) => state.serviceStatus);
  const { canAccessManagementRpc } = useRuntimeCapabilities();
  const isServiceReady = canAccessManagementRpc && serviceStatus.connected;
  const isPageActive = useDesktopPageActive("/qoder/");
  const isQueryEnabled = useDeferredDesktopActivation(isServiceReady);

  const today = formatDay(new Date());
  const [preset, setPreset] = useState<RangePreset>("today");
  const [customStart, setCustomStart] = useState(() => shiftDay(today, -6));
  const [customEnd, setCustomEnd] = useState(today);
  const [configOpen, setConfigOpen] = useState(false);
  const [memberFilter, setMemberFilter] = useState(ALL_MEMBERS_FILTER);
  const [eventsPage, setEventsPage] = useState(1);
  const [formEnabled, setFormEnabled] = useState(false);
  const [formBaseUrl, setFormBaseUrl] = useState(DEFAULT_QODER_BASE_URL);
  const [formOrgId, setFormOrgId] = useState("");
  const [formApiKey, setFormApiKey] = useState("");
  const [formBackfillDays, setFormBackfillDays] = useState("30");

  const range = useMemo(
    () => buildPresetRange(preset, customStart, customEnd),
    [preset, customStart, customEnd]
  );

  const settingsQuery = useQuery({
    queryKey: ["qoder-settings", serviceStatus.addr || null],
    queryFn: () => qoderClient.getSettings(),
    enabled: isQueryEnabled,
    retry: 1,
    refetchInterval: (query) =>
      query.state.data?.status.running ? 2_500 : 60_000,
  });

  const settings = settingsQuery.data?.settings;
  const status = settingsQuery.data?.status;
  const syncRunning = status?.running ?? false;
  const isConfigured = status?.configured ?? false;

  const overviewQuery = useQuery({
    queryKey: [
      "qoder-overview",
      range.startDay,
      range.endDay,
      memberFilter,
      serviceStatus.addr || null,
    ],
    queryFn: () =>
      qoderClient.getOverview({
        ...range,
        memberId: memberFilter === ALL_MEMBERS_FILTER ? null : memberFilter,
      }),
    enabled: isQueryEnabled && isConfigured,
    retry: 1,
    // 切换成员筛选时保留上一份数据，避免成员下拉项短暂卸载导致选中值被重置
    placeholderData: keepPreviousData,
    refetchInterval: syncRunning ? 5_000 : 60_000,
  });

  const eventsQuery = useQuery({
    queryKey: [
      "qoder-events",
      range.startDay,
      range.endDay,
      memberFilter,
      eventsPage,
      serviceStatus.addr || null,
    ],
    queryFn: () =>
      qoderClient.getEvents({
        startDay: range.startDay,
        endDay: range.endDay,
        memberId: memberFilter === ALL_MEMBERS_FILTER ? null : memberFilter,
        page: eventsPage,
        pageSize: EVENTS_PAGE_SIZE,
      }),
    enabled: isQueryEnabled && isConfigured,
    retry: 1,
    placeholderData: keepPreviousData,
    refetchInterval: syncRunning ? 5_000 : 60_000,
  });

  const isLoading =
    isQueryEnabled &&
    (settingsQuery.isPending || (isConfigured && overviewQuery.isPending));
  usePageTransitionReady("/qoder/", !isServiceReady || !isLoading);

  const wasRunningRef = useRef(false);
  useEffect(() => {
    if (wasRunningRef.current && !syncRunning) {
      void queryClient.invalidateQueries({ queryKey: ["qoder-overview"] });
      void queryClient.invalidateQueries({ queryKey: ["qoder-events"] });
      void queryClient.invalidateQueries({ queryKey: ["qoder-today-top"] });
    }
    wasRunningRef.current = syncRunning;
  }, [syncRunning, queryClient]);

  const openConfig = () => {
    setFormEnabled(settings?.enabled ?? false);
    setFormBaseUrl(settings?.baseUrl || DEFAULT_QODER_BASE_URL);
    setFormOrgId(settings?.organizationId || "");
    setFormApiKey("");
    setFormBackfillDays(String(settings?.backfillDays ?? 30));
    setConfigOpen(true);
  };

  const saveSettings = useMutation({
    mutationFn: async () => {
      const backfillDays = Number(formBackfillDays);
      if (
        !Number.isFinite(backfillDays) ||
        backfillDays < 1 ||
        backfillDays > 180
      ) {
        throw new Error("回填天数需在 1-180 之间");
      }
      return qoderClient.saveSettings({
        baseUrl: formBaseUrl.trim() || undefined,
        organizationId: formOrgId.trim(),
        apiKey: formApiKey.trim() || undefined,
        enabled: formEnabled,
        backfillDays: Math.round(backfillDays),
      });
    },
    onSuccess: async () => {
      toast.success(
        formEnabled ? "Qoder 配置已保存，已触发同步" : "Qoder 配置已保存"
      );
      setConfigOpen(false);
      await queryClient.invalidateQueries({ queryKey: ["qoder-settings"] });
      await queryClient.invalidateQueries({ queryKey: ["qoder-overview"] });
      await queryClient.invalidateQueries({ queryKey: ["qoder-events"] });
      await queryClient.invalidateQueries({ queryKey: ["qoder-today-top"] });
    },
    onError: (error: unknown) => {
      toast.error(error instanceof Error ? error.message : String(error));
    },
  });

  const syncNow = useMutation({
    mutationFn: () => qoderClient.sync(),
    onSuccess: async (result) => {
      if (result.started) {
        toast.success("已开始同步 Qoder 用量");
      } else {
        toast.info("同步已在进行中");
      }
      await queryClient.invalidateQueries({ queryKey: ["qoder-settings"] });
    },
    onError: (error: unknown) => {
      toast.error(error instanceof Error ? error.message : String(error));
    },
  });

  const members = useMemo(() => overviewQuery.data?.members || [], [overviewQuery.data]);
  const visibleMembers = useMemo(() => members.filter(hasSeatQuota), [members]);
  const models = useMemo(() => overviewQuery.data?.models || [], [overviewQuery.data]);
  const buckets = useMemo(() => overviewQuery.data?.buckets || [], [overviewQuery.data]);
  const events = eventsQuery.data?.items || [];
  const eventsTotal = eventsQuery.data?.total ?? 0;
  const totalPages = Math.max(1, Math.ceil(eventsTotal / EVENTS_PAGE_SIZE));

  const memberFilterOptions = useMemo(() => {
    if (
      memberFilter !== ALL_MEMBERS_FILTER &&
      !visibleMembers.some((member) => member.memberId === memberFilter)
    ) {
      const selected = members.find((member) => member.memberId === memberFilter);
      if (selected) {
        return [selected, ...visibleMembers];
      }
    }
    return visibleMembers;
  }, [visibleMembers, members, memberFilter]);

  const memberNameById = useMemo(() => {
    const map = new Map<string, string>();
    for (const member of memberFilterOptions) {
      map.set(member.memberId, memberLabel(member));
    }
    return map;
  }, [memberFilterOptions]);

  const memberFilterLabel = (value: unknown): string => {
    const normalized = String(value ?? "");
    if (!normalized || normalized === ALL_MEMBERS_FILTER) {
      return "全部成员";
    }
    return memberNameById.get(normalized) || "未知成员";
  };

  const rangePresetsControl = (
    <div className="flex flex-wrap items-center gap-2">
      {RANGE_PRESETS.map((item) => (
        <Button
          key={item.id}
          size="sm"
          variant={preset === item.id ? "default" : "outline"}
          onClick={() => {
            setPreset(item.id);
            setEventsPage(1);
          }}
        >
          {item.label}
        </Button>
      ))}
      {preset === "custom" ? (
        <div className="flex items-center gap-2">
          <Input
            type="date"
            className="h-8 w-[150px]"
            value={customStart}
            max={customEnd || today}
            onChange={(event) => {
              const value = event.target.value;
              setCustomStart(value);
              setEventsPage(1);
              if (value && customEnd && value > customEnd) {
                setCustomEnd(value);
              }
            }}
          />
          <span className="text-xs text-muted-foreground">至</span>
          <Input
            type="date"
            className="h-8 w-[150px]"
            value={customEnd}
            min={customStart}
            max={today}
            onChange={(event) => {
              const value = event.target.value;
              setCustomEnd(value);
              setEventsPage(1);
              if (value && customStart && value < customStart) {
                setCustomStart(value);
              }
            }}
          />
        </div>
      ) : null}
    </div>
  );

  const memberFilterSelect = (
    <Select
      value={memberFilter}
      onValueChange={(value) => {
        // 忽略下拉在选项缺失时抛出的空值事件，避免选中被重置回“全部成员”
        if (typeof value !== "string" || !value) {
          return;
        }
        setMemberFilter(value);
        setEventsPage(1);
      }}
    >
      <SelectTrigger className="h-8 w-44">
        <SelectValue>{(value: unknown) => memberFilterLabel(value)}</SelectValue>
      </SelectTrigger>
      <SelectContent>
        <SelectItem value={ALL_MEMBERS_FILTER}>全部成员</SelectItem>
        {memberFilterOptions.map((member) => (
          <SelectItem key={member.memberId} value={member.memberId}>
            {memberLabel(member)}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );

  const statusInfo = describeStatus(status);
  const isHalfHour = overviewQuery.data?.bucketUnit === "half_hour";
  const bucketUnitLabel = isHalfHour ? "半小时粒度" : "按天粒度";
  const showOverviewLoading =
    isLoading || (isConfigured && overviewQuery.isPending);

  return (
    <div className="space-y-6 animate-in fade-in duration-700">
      <Card className="glass-card border-none shadow-sm">
        <CardContent className="space-y-2 pt-6">
          <div className="flex flex-wrap items-center gap-2">
            {memberFilterSelect}
            {rangePresetsControl}
            <div className="ml-auto flex flex-wrap items-center gap-2">
              <Badge variant={statusInfo.variant}>{statusInfo.label}</Badge>
              <span className="hidden text-[10px] text-muted-foreground md:inline">
                最后同步 {formatTsFromSeconds(status?.lastSyncAt, "尚未同步")}
              </span>
              <Button
                variant="outline"
                size="sm"
                className="gap-1.5"
                onClick={openConfig}
              >
                <Settings2 className="h-3.5 w-3.5" />
                配置
              </Button>
              <Button
                size="sm"
                className="gap-1.5"
                disabled={!isConfigured || syncRunning || syncNow.isPending}
                onClick={() => syncNow.mutate()}
              >
                <RefreshCw
                  className={cn("h-3.5 w-3.5", syncRunning && "animate-spin")}
                />
                立即同步
              </Button>
            </div>
          </div>
          <p className="text-[10px] text-muted-foreground">
            已同步 {status?.eventCount ?? 0} 条用量事件 · {status?.memberCount ?? 0}{" "}
            名成员
            {status?.dataFromDay && status.dataToDay
              ? ` · 数据范围 ${status.dataFromDay} ~ ${status.dataToDay}`
              : ""}
            {status?.enabled ? " · 每 5 分钟自动增量同步" : ""}
          </p>
        </CardContent>
      </Card>

      {!settingsQuery.isPending && !isConfigured ? (
        <Card className="glass-card border-none shadow-sm">
          <CardContent className="flex flex-wrap items-center justify-between gap-3 pt-6 text-sm text-muted-foreground">
            <span>
              尚未配置 Qoder 团队同步：需要组织 ID 与组织 API Key（在 Qoder
              控制台创建），保存后每 5 分钟自动增量同步。
            </span>
            <Button size="sm" className="gap-2" onClick={openConfig}>
              <Settings2 className="h-3.5 w-3.5" />
              立即配置
            </Button>
          </CardContent>
        </Card>
      ) : null}

      {status?.lastSyncError ? (
        <Card className="glass-card border-none shadow-sm">
          <CardContent className="break-all pt-6 text-xs text-destructive">
            同步失败：{status.lastSyncError}
          </CardContent>
        </Card>
      ) : null}

      <Card className="glass-card border-none shadow-md">
        <CardHeader className="flex flex-row items-center justify-between gap-3 space-y-0">
          <CardTitle className="text-base font-semibold">
            模型积分消耗趋势
          </CardTitle>
          <div className="flex items-center gap-2">
            <span className="text-[10px] text-muted-foreground">
              {bucketUnitLabel}
            </span>
            <CalendarClock className="h-4 w-4 text-sky-500" />
          </div>
        </CardHeader>
        <CardContent>
          {showOverviewLoading ? (
            <ChartSkeleton />
          ) : (
            <>
              <StackedUsageChart
                buckets={buckets}
                models={models}
                halfHour={isHalfHour}
              />
              <p className="mt-3 text-[10px] text-muted-foreground">
                合计 {formatCredits(overviewQuery.data?.totalCredits ?? 0)} 积分 ·{" "}
                {overviewQuery.data?.totalEvents ?? 0} 次调用
                {memberFilter === ALL_MEMBERS_FILTER
                  ? ` · 活跃 ${overviewQuery.data?.activeMembers ?? 0} 人`
                  : ""}
              </p>
            </>
          )}
        </CardContent>
      </Card>

      <Card className="glass-card border-none shadow-md">
        <CardHeader className="flex flex-row items-center justify-between gap-3 space-y-0">
          <CardTitle className="text-base font-semibold">调用明细</CardTitle>
          <Coins className="h-4 w-4 text-amber-500" />
        </CardHeader>
        <CardContent>
          {showOverviewLoading || (isConfigured && eventsQuery.isPending) ? (
            <div className="space-y-3">
              {Array.from({ length: 6 }).map((_, index) => (
                <Skeleton key={index} className="h-9 w-full rounded-lg" />
              ))}
            </div>
          ) : events.length === 0 ? (
            <div className="flex h-40 items-center justify-center text-xs text-muted-foreground">
              {!isConfigured
                ? "完成配置后将自动同步并展示每次模型调用明细"
                : "该时间段暂无调用记录"}
            </div>
          ) : (
            <>
              <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead className="whitespace-nowrap">时间</TableHead>
                      <TableHead>成员</TableHead>
                      <TableHead>模型</TableHead>
                      <TableHead>操作</TableHead>
                      <TableHead>来源</TableHead>
                      <TableHead className="text-right">积分</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {events.map((event) => (
                      <TableRow key={event.eventId}>
                        <TableCell className="whitespace-nowrap font-mono text-xs text-muted-foreground">
                          {formatEventTime(event.occurredAt)}
                        </TableCell>
                        <TableCell>
                          <p
                            className="max-w-[220px] truncate text-sm"
                            title={event.name || event.email || "未知成员"}
                          >
                            {event.name || event.email || "未知成员"}
                          </p>
                        </TableCell>
                        <TableCell>
                          <Badge
                            variant="secondary"
                            className="max-w-[180px] truncate text-[10px]"
                          >
                            {modelLabel(event.modelTier)}
                          </Badge>
                        </TableCell>
                        <TableCell className="text-xs text-muted-foreground">
                          {event.operation || "-"}
                        </TableCell>
                        <TableCell className="text-xs text-muted-foreground">
                          {event.source || "-"}
                        </TableCell>
                        <TableCell
                          className={cn(
                            "text-right font-mono text-sm font-semibold",
                            event.credits < 0 && "text-destructive"
                          )}
                        >
                          {formatCredits(event.credits)}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              <div className="mt-3 flex flex-wrap items-center justify-between gap-3">
                <p className="text-[10px] text-muted-foreground">
                  共 {eventsTotal} 条调用记录 · 第 {eventsPage} / {totalPages} 页
                </p>
                <div className="flex items-center gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={eventsPage <= 1}
                    onClick={() =>
                      setEventsPage((current) => Math.max(1, current - 1))
                    }
                  >
                    上一页
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={eventsPage >= totalPages}
                    onClick={() => setEventsPage((current) => current + 1)}
                  >
                    下一页
                  </Button>
                </div>
              </div>
            </>
          )}
        </CardContent>
      </Card>

      <Dialog open={configOpen && isPageActive} onOpenChange={setConfigOpen}>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>Qoder 团队同步配置</DialogTitle>
            <DialogDescription>
              在 Qoder 控制台创建组织 API Key 后填入，保存后每 5 分钟自动增量同步用量事件（按服务所在机器的本地时区归档）。
            </DialogDescription>
          </DialogHeader>
          <div className="grid gap-4 py-2">
            <div className="flex items-center justify-between rounded-lg border p-3">
              <div>
                <p className="text-sm font-medium">启用自动同步</p>
                <p className="text-xs text-muted-foreground">
                  关闭后停止后台同步，已同步数据保留
                </p>
              </div>
              <Switch checked={formEnabled} onCheckedChange={setFormEnabled} />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="qoder-base-url">服务地址</Label>
              <Input
                id="qoder-base-url"
                value={formBaseUrl}
                onChange={(event) => setFormBaseUrl(event.target.value)}
                placeholder={DEFAULT_QODER_BASE_URL}
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="qoder-org-id">组织 ID</Label>
              <Input
                id="qoder-org-id"
                value={formOrgId}
                onChange={(event) => setFormOrgId(event.target.value)}
                placeholder="org_xxxxxxxx"
              />
              <p className="text-xs text-muted-foreground">
                更换组织 ID 会清空已同步的旧组织数据
              </p>
            </div>
            <div className="grid gap-2">
              <Label htmlFor="qoder-api-key">组织 API Key</Label>
              <Input
                id="qoder-api-key"
                type="password"
                autoComplete="off"
                value={formApiKey}
                onChange={(event) => setFormApiKey(event.target.value)}
                placeholder={
                  settings?.hasApiKey ? "已配置，留空则保持不变" : "粘贴 API Key"
                }
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="qoder-backfill-days">首次回填天数</Label>
              <Input
                id="qoder-backfill-days"
                type="number"
                min={1}
                max={180}
                value={formBackfillDays}
                onChange={(event) => setFormBackfillDays(event.target.value)}
              />
              <p className="text-xs text-muted-foreground">
                首次同步向前回填的历史天数（1-180），仅影响历史数据补齐
              </p>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfigOpen(false)}>
              取消
            </Button>
            <Button
              onClick={() => saveSettings.mutate()}
              disabled={saveSettings.isPending}
            >
              {saveSettings.isPending ? "保存中..." : "保存"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
