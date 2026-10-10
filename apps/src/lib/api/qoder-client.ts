import { invoke, withAddr } from "./transport";
import {
  QoderMemberQuota,
  QoderModelUsage,
  QoderOverview,
  QoderSettings,
  QoderSettingsResult,
  QoderSharedPackage,
  QoderSyncStatus,
  QoderSyncTriggerResult,
  QoderTodayTop,
  QoderTopMember,
  QoderUsageBucket,
  QoderUsageEventItem,
  QoderUsageEventList,
} from "../../types";

function asObject(payload: unknown): Record<string, unknown> {
  if (payload && typeof payload === "object" && !Array.isArray(payload)) {
    return payload as Record<string, unknown>;
  }
  return {};
}

function asArray<T = unknown>(payload: unknown): T[] {
  return Array.isArray(payload) ? (payload as T[]) : [];
}

function asString(value: unknown, fallback = ""): string {
  if (typeof value === "string") {
    return value;
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  return fallback;
}

function asBoolean(value: unknown, fallback = false): boolean {
  if (typeof value === "boolean") {
    return value;
  }
  if (typeof value === "number") {
    return value !== 0;
  }
  if (typeof value === "string") {
    const normalized = value.trim().toLowerCase();
    if (["1", "true", "yes", "on"].includes(normalized)) return true;
    if (["0", "false", "no", "off", ""].includes(normalized)) return false;
  }
  return fallback;
}

function toNullableNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string" && value.trim()) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

function asNumber(value: unknown, fallback = 0): number {
  return toNullableNumber(value) ?? fallback;
}

function asInteger(value: unknown, fallback = 0): number {
  return Math.trunc(asNumber(value, fallback));
}

function normalizeModelUsage(item: unknown): QoderModelUsage {
  const source = asObject(item);
  return {
    modelTier: asString(source.modelTier ?? source.model_tier) || "",
    credits: asNumber(source.credits),
    events: asInteger(source.events),
  };
}

function normalizeUsageBucket(item: unknown): QoderUsageBucket {
  const source = asObject(item);
  return {
    key: asString(source.key),
    label: asString(source.label),
    credits: asNumber(source.credits),
    events: asInteger(source.events),
    models: asArray(source.models).map(normalizeModelUsage),
  };
}

function normalizeMemberQuota(item: unknown): QoderMemberQuota {
  const source = asObject(item);
  return {
    memberId: asString(source.memberId ?? source.member_id),
    name: asString(source.name) || null,
    email: asString(source.email) || null,
    status: asString(source.status) || null,
    planUsed: toNullableNumber(source.planUsed ?? source.plan_used),
    planLimit: toNullableNumber(source.planLimit ?? source.plan_limit),
    resourceUsed: toNullableNumber(source.resourceUsed ?? source.resource_used),
    resourceLimit: toNullableNumber(source.resourceLimit ?? source.resource_limit),
    totalUsed: toNullableNumber(source.totalUsed ?? source.total_used),
    totalLimit: toNullableNumber(source.totalLimit ?? source.total_limit),
    sharedUsed: toNullableNumber(source.sharedUsed ?? source.shared_used),
    sharedLimit: toNullableNumber(source.sharedLimit ?? source.shared_limit),
    quotaUnit: asString(source.quotaUnit ?? source.quota_unit) || null,
    quotaStatus: asString(source.quotaStatus ?? source.quota_status) || null,
    nextResetAt: asString(source.nextResetAt ?? source.next_reset_at) || null,
    quotaSyncedAt: toNullableNumber(source.quotaSyncedAt ?? source.quota_synced_at),
  };
}

function normalizeUsageEvent(item: unknown): QoderUsageEventItem {
  const source = asObject(item);
  return {
    eventId: asString(source.eventId ?? source.event_id),
    occurredAt: asInteger(source.occurredAt ?? source.occurred_at),
    day: asString(source.day),
    memberId: asString(source.memberId ?? source.member_id) || null,
    name: asString(source.name) || null,
    email: asString(source.email) || null,
    modelTier: asString(source.modelTier ?? source.model_tier) || "",
    source: asString(source.source) || null,
    operation: asString(source.operation) || null,
    credits: asNumber(source.credits),
    cost: toNullableNumber(source.cost),
  };
}

export function normalizeQoderSettings(payload: unknown): QoderSettings {
  const source = asObject(payload);
  return {
    baseUrl: asString(source.baseUrl ?? source.base_url),
    organizationId: asString(source.organizationId ?? source.organization_id),
    hasApiKey: asBoolean(source.hasApiKey ?? source.has_api_key),
    enabled: asBoolean(source.enabled),
    backfillDays: asInteger(source.backfillDays ?? source.backfill_days, 30),
  };
}

export function normalizeQoderSyncStatus(payload: unknown): QoderSyncStatus {
  const source = asObject(payload);
  return {
    configured: asBoolean(source.configured),
    enabled: asBoolean(source.enabled),
    running: asBoolean(source.running),
    lastSyncAt: toNullableNumber(source.lastSyncAt ?? source.last_sync_at),
    lastSyncStatus: asString(source.lastSyncStatus ?? source.last_sync_status) || null,
    lastSyncError: asString(source.lastSyncError ?? source.last_sync_error) || null,
    lastEventAt: toNullableNumber(source.lastEventAt ?? source.last_event_at),
    coveredUntil: toNullableNumber(source.coveredUntil ?? source.covered_until),
    backfillFrom: toNullableNumber(source.backfillFrom ?? source.backfill_from),
    eventCount: asInteger(source.eventCount ?? source.event_count),
    memberCount: asInteger(source.memberCount ?? source.member_count),
    dataFromDay: asString(source.dataFromDay ?? source.data_from_day) || null,
    dataToDay: asString(source.dataToDay ?? source.data_to_day) || null,
  };
}

function normalizeQoderSettingsResult(payload: unknown): QoderSettingsResult {
  const source = asObject(payload);
  return {
    settings: normalizeQoderSettings(source.settings),
    status: normalizeQoderSyncStatus(source.status),
  };
}

function normalizeSharedPackage(payload: unknown): QoderSharedPackage | null {
  const source = payload ? asObject(payload) : null;
  if (!source) {
    return null;
  }
  return {
    limit: asNumber(source.limit),
    remaining: asNumber(source.remaining),
    syncedAt: toNullableNumber(source.syncedAt ?? source.synced_at),
  };
}

function normalizeQoderOverview(payload: unknown): QoderOverview {
  const source = asObject(payload);
  return {
    startDay: asString(source.startDay ?? source.start_day),
    endDay: asString(source.endDay ?? source.end_day),
    bucketUnit: asString(source.bucketUnit ?? source.bucket_unit) || "day",
    totalCredits: asNumber(source.totalCredits ?? source.total_credits),
    totalEvents: asInteger(source.totalEvents ?? source.total_events),
    activeMembers: asInteger(source.activeMembers ?? source.active_members),
    memberCount: asInteger(source.memberCount ?? source.member_count),
    models: asArray(source.models).map(normalizeModelUsage),
    buckets: asArray(source.buckets).map(normalizeUsageBucket),
    members: asArray(source.members).map(normalizeMemberQuota),
    sharedPackage: normalizeSharedPackage(
      source.sharedPackage ?? source.shared_package
    ),
  };
}

function normalizeQoderUsageEventList(payload: unknown): QoderUsageEventList {
  const source = asObject(payload);
  return {
    items: asArray(source.items).map(normalizeUsageEvent),
    total: asInteger(source.total),
    page: asInteger(source.page, 1),
    pageSize: asInteger(source.pageSize ?? source.page_size, 20),
  };
}

function normalizeQoderTodayTop(payload: unknown): QoderTodayTop {
  const source = asObject(payload);
  return {
    day: asString(source.day),
    totalCredits: asNumber(source.totalCredits ?? source.total_credits),
    totalEvents: asInteger(source.totalEvents ?? source.total_events),
    items: asArray(source.items).map(normalizeTopMember),
  };
}

function normalizeTopMember(item: unknown): QoderTopMember {
  const source = asObject(item);
  return {
    memberId: asString(source.memberId ?? source.member_id),
    name: asString(source.name) || null,
    email: asString(source.email) || null,
    credits: asNumber(source.credits),
    events: asInteger(source.events),
  };
}

export interface QoderSettingsPatch {
  baseUrl?: string;
  organizationId?: string;
  apiKey?: string;
  enabled?: boolean;
  backfillDays?: number;
}

export const qoderClient = {
  async getSettings(): Promise<QoderSettingsResult> {
    const result = await invoke<unknown>("service_qoder_settings_get", withAddr());
    return normalizeQoderSettingsResult(result);
  },
  async saveSettings(patch: QoderSettingsPatch): Promise<QoderSettingsResult> {
    const result = await invoke<unknown>(
      "service_qoder_settings_set",
      withAddr({
        baseUrl: patch.baseUrl ?? null,
        organizationId: patch.organizationId ?? null,
        apiKey: patch.apiKey ?? null,
        enabled: typeof patch.enabled === "boolean" ? patch.enabled : null,
        backfillDays:
          typeof patch.backfillDays === "number" ? patch.backfillDays : null,
      }),
      { timeoutMs: 60_000, retries: 0 }
    );
    return normalizeQoderSettingsResult(result);
  },
  async getStatus(): Promise<QoderSyncStatus> {
    const result = await invoke<unknown>("service_qoder_status", withAddr());
    return normalizeQoderSyncStatus(result);
  },
  async sync(): Promise<QoderSyncTriggerResult> {
    const result = await invoke<unknown>(
      "service_qoder_sync",
      withAddr(),
      { timeoutMs: 30_000, retries: 0 }
    );
    const source = asObject(result);
    return { started: asBoolean(source.started) };
  },
  async getOverview(
    params: { startDay?: string; endDay?: string; memberId?: string | null } = {}
  ): Promise<QoderOverview> {
    const result = await invoke<unknown>(
      "service_qoder_overview",
      withAddr({
        startDay: params.startDay ?? null,
        endDay: params.endDay ?? null,
        memberId: params.memberId ?? null,
      }),
      { timeoutMs: 60_000, retries: 0 }
    );
    return normalizeQoderOverview(result);
  },
  async getEvents(
    params: {
      startDay?: string;
      endDay?: string;
      memberId?: string | null;
      page?: number;
      pageSize?: number;
    } = {}
  ): Promise<QoderUsageEventList> {
    const result = await invoke<unknown>(
      "service_qoder_events",
      withAddr({
        startDay: params.startDay ?? null,
        endDay: params.endDay ?? null,
        memberId: params.memberId ?? null,
        page: typeof params.page === "number" ? params.page : null,
        pageSize: typeof params.pageSize === "number" ? params.pageSize : null,
      }),
      { timeoutMs: 60_000, retries: 0 }
    );
    return normalizeQoderUsageEventList(result);
  },
  async getTodayTop(limit = 10): Promise<QoderTodayTop> {
    const result = await invoke<unknown>(
      "service_qoder_today_top",
      withAddr({ limit }),
      { timeoutMs: 30_000, retries: 0 }
    );
    return normalizeQoderTodayTop(result);
  },
};
