"use client";

import { useMemo } from "react";
import { cn } from "@/lib/utils";
import { QoderMemberQuota } from "@/types";

export const QUOTA_BASELINE = 3000;
export const QUOTA_WARN_THRESHOLD = 2400;

function pad2(value: number): string {
  return String(value).padStart(2, "0");
}

export function formatQuota(value: number | null | undefined): string {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return "-";
  }
  return value.toLocaleString("zh-CN", {
    minimumFractionDigits: 0,
    maximumFractionDigits: 2,
  });
}

function formatResetAt(value: string | null): string {
  if (!value) {
    return "-";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return `${date.getFullYear()}-${pad2(date.getMonth() + 1)}-${pad2(
    date.getDate()
  )} ${pad2(date.getHours())}:${pad2(date.getMinutes())}`;
}

export function memberLabel(
  member:
    | QoderMemberQuota
    | { name?: string | null; email?: string | null; memberId?: string }
): string {
  return (
    String(member.name || "").trim() ||
    String(member.email || "").trim() ||
    String(member.memberId || "") ||
    "未知成员"
  );
}

/** 过滤不占席位（没有任何配额信息）的成员。 */
export function hasSeatQuota(member: QoderMemberQuota): boolean {
  return (
    member.planUsed != null ||
    member.planLimit != null ||
    member.resourceUsed != null ||
    member.resourceLimit != null ||
    member.totalUsed != null ||
    member.totalLimit != null ||
    member.sharedUsed != null ||
    member.sharedLimit != null
  );
}

function quotaBarColor(value: number): string {
  if (value > QUOTA_BASELINE) {
    return "#ef4444";
  }
  if (value > QUOTA_WARN_THRESHOLD) {
    return "#f59e0b";
  }
  return "#10b981";
}

function hasQuotaValue(used: number | null, limit: number | null): boolean {
  return used != null || limit != null;
}

/**
 * 组织控制台里只有「共享资源包」一种额度，接口的 resourcePackageQuota 与 sharedQuota
 * 是同一概念的不同字段，这里合并取数：优先有上限的一方，其次有任意数据的一方。
 */
function pickSharedPackageQuota(member: QoderMemberQuota): {
  used: number | null;
  limit: number | null;
} {
  const pairs = [
    { used: member.sharedUsed, limit: member.sharedLimit },
    { used: member.resourceUsed, limit: member.resourceLimit },
  ];
  const withLimit = pairs.find(
    (pair) => typeof pair.limit === "number" && pair.limit > 0
  );
  if (withLimit) {
    return withLimit;
  }
  const withData = pairs.find((pair) => hasQuotaValue(pair.used, pair.limit));
  return withData ?? { used: null, limit: null };
}

/** 当前周期已用总积分：套餐额度、资源包、共享资源包各来源已用量之和。 */
function memberPeriodCredits(member: QoderMemberQuota): number {
  const values = [member.planUsed, member.resourceUsed, member.sharedUsed].filter(
    (value): value is number => typeof value === "number" && Number.isFinite(value)
  );
  if (values.length > 0) {
    return values.reduce((sum, value) => sum + value, 0);
  }
  if (typeof member.totalUsed === "number" && Number.isFinite(member.totalUsed)) {
    return member.totalUsed;
  }
  return 0;
}

export function MemberQuotaBarChart({
  members,
  emptyText = "暂无占席位的成员数据",
}: {
  members: QoderMemberQuota[];
  emptyText?: string;
}) {
  const items = useMemo(
    () =>
      members
        .filter(hasSeatQuota)
        .map((member) => ({ member, value: memberPeriodCredits(member) }))
        .sort((left, right) => right.value - left.value),
    [members]
  );

  if (items.length === 0) {
    return (
      <div className="flex h-48 items-center justify-center text-xs text-muted-foreground">
        {emptyText}
      </div>
    );
  }

  const maxValue = items.reduce((max, item) => Math.max(max, item.value), 0);
  const scaleMax = Math.max(QUOTA_BASELINE, maxValue) * 1.12;
  const baselineBottom = (QUOTA_BASELINE / scaleMax) * 100;

  return (
    <div>
      <div className="relative h-60">
        <div
          className="absolute inset-x-0 border-t border-dashed border-amber-500/80"
          style={{ bottom: `${baselineBottom}%` }}
        />
        <span
          className="absolute right-0 -translate-y-full text-[9px] text-amber-600"
          style={{ bottom: `${baselineBottom}%` }}
        >
          基准线 {QUOTA_BASELINE}
        </span>
        <div className="flex h-full items-stretch gap-1.5">
          {items.map(({ member, value }) => {
            const pct = value <= 0 ? 0 : (value / scaleMax) * 100;
            const color = quotaBarColor(value);
            const sharedPackage = pickSharedPackageQuota(member);
            const unit = member.quotaUnit ? ` ${member.quotaUnit}` : "";
            const tooltipLines = [
              memberLabel(member),
              `本周期已用总积分：${formatQuota(value)}${unit}`,
            ];
            if (hasQuotaValue(member.planUsed, member.planLimit)) {
              tooltipLines.push(
                `套餐额度：${formatQuota(member.planUsed)} / ${formatQuota(
                  member.planLimit
                )}`
              );
            }
            if (hasQuotaValue(sharedPackage.used, sharedPackage.limit)) {
              tooltipLines.push(
                `共享资源包：${formatQuota(sharedPackage.used)} / ${formatQuota(
                  sharedPackage.limit
                )}`
              );
            }
            tooltipLines.push(`下次重置：${formatResetAt(member.nextResetAt)}`);
            const tooltip = tooltipLines.join("\n");
            return (
              <div
                key={member.memberId}
                className="relative h-full min-w-0 flex-1"
                title={tooltip}
              >
                <div
                  className={cn(
                    "absolute bottom-0 left-0 right-0 rounded-t",
                    value <= 0 && "bg-muted"
                  )}
                  style={{
                    height: value <= 0 ? "2px" : `${pct}%`,
                    backgroundColor: value > 0 ? color : undefined,
                  }}
                />
                <span
                  className="absolute left-1/2 -translate-x-1/2 whitespace-nowrap font-mono text-[9px] font-semibold"
                  style={{ bottom: `calc(${pct}% + 4px)`, color }}
                >
                  {formatQuota(value)}
                </span>
              </div>
            );
          })}
        </div>
      </div>
      <div className="mt-1 flex gap-1.5">
        {items.map(({ member }) => (
          <div key={member.memberId} className="min-w-0 flex-1 text-center">
            <span
              className="block truncate text-[9px] text-muted-foreground"
              title={memberLabel(member)}
            >
              {memberLabel(member)}
            </span>
          </div>
        ))}
      </div>
      <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1 text-[10px] text-muted-foreground">
        <span className="flex items-center gap-1">
          <span className="h-2 w-2 rounded-sm bg-emerald-500" />≤{" "}
          {QUOTA_WARN_THRESHOLD}
        </span>
        <span className="flex items-center gap-1">
          <span className="h-2 w-2 rounded-sm bg-amber-500" />
          {QUOTA_WARN_THRESHOLD} ~ {QUOTA_BASELINE}
        </span>
        <span className="flex items-center gap-1">
          <span className="h-2 w-2 rounded-sm bg-red-500" />＞ {QUOTA_BASELINE}
        </span>
        <span>纵轴为成员当前周期内已用总积分（套餐 + 资源包 + 共享资源包），按从大到小排序</span>
      </div>
    </div>
  );
}
