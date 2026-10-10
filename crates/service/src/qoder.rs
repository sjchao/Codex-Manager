use chrono::{Days, Local, NaiveDate, SecondsFormat, TimeZone, Utc};
use codexmanager_core::rpc::types::{
    QoderMemberQuotaStat, QoderModelUsageStat, QoderOverviewResult, QoderSettingsResult,
    QoderSettingsView, QoderSharedPackageStat, QoderSyncStatus, QoderSyncTriggerResult,
    QoderTodayTopResult, QoderTopMember, QoderUsageBucket, QoderUsageEventItem,
    QoderUsageEventListResult,
};
use codexmanager_core::storage::{
    now_ts, QoderBucketUsageRow, QoderMember, QoderMemberQuota, QoderSyncState, QoderUsageEvent,
};
use reqwest::blocking::{Client, RequestBuilder};
use reqwest::StatusCode;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::app_settings::{
    get_persisted_app_setting, parse_bool_with_default, save_persisted_app_setting,
    save_persisted_bool_setting, APP_SETTING_QODER_API_KEY_KEY,
    APP_SETTING_QODER_BACKFILL_DAYS_KEY, APP_SETTING_QODER_BASE_URL_KEY,
    APP_SETTING_QODER_ENABLED_KEY, APP_SETTING_QODER_ORGANIZATION_ID_KEY,
};
use crate::storage_helpers::open_storage;

const QODER_DEFAULT_BASE_URL: &str = "https://api.qoder.com.cn";
const QODER_DEFAULT_BACKFILL_DAYS: i64 = 30;
const QODER_MIN_BACKFILL_DAYS: i64 = 1;
const QODER_MAX_BACKFILL_DAYS: i64 = 180;
const QODER_SYNC_INTERVAL_SECS: i64 = 300;
const QODER_SYNC_TICK_SECS: u64 = 60;
const QODER_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const QODER_PAGE_SIZE: usize = 100;
const QODER_MAX_PAGES: usize = 500;
const QODER_INCREMENTAL_OVERLAP_MS: i64 = 10 * 60 * 1000;
// 官方会事后修正已记录的事件（Auto 重归类为具体模型、积分重算），周期性用 48 小时
// 重叠窗口重新拉取并回写，既保证修正能纠正本地数据，又避免每轮同步都全量翻页。
const QODER_REFRESH_OVERLAP_MS: i64 = 48 * 60 * 60 * 1000;
const QODER_REFRESH_OVERLAP_PERIOD: Duration = Duration::from_secs(30 * 60);
// 官方限制单次查询时间跨度不得超过 7 天，这里按 6 天切窗留出安全余量。
const QODER_MAX_WINDOW_MS: i64 = 6 * 24 * 60 * 60 * 1000;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

static QODER_POLLING_STARTED: OnceLock<()> = OnceLock::new();
static QODER_SYNC_RUNNING: AtomicBool = AtomicBool::new(false);
// 串行化同步任务：后台轮询、手动触发与配置保存触发的同步排队执行，避免并发拉取与状态互相覆盖。
static QODER_SYNC_LOCK: Mutex<()> = Mutex::new(());
// 上次使用 48 小时重叠窗口刷新的时间（进程内记录，重启后首轮同步会自动做一次全量回写）。
static QODER_LAST_OVERLAP_REFRESH: Mutex<Option<Instant>> = Mutex::new(None);

#[derive(Debug, Clone)]
pub(crate) struct QoderSettings {
    pub base_url: String,
    pub organization_id: String,
    pub api_key: String,
    pub enabled: bool,
    pub backfill_days: i64,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct QoderSettingsPatch {
    pub base_url: Option<String>,
    pub organization_id: Option<String>,
    pub api_key: Option<String>,
    pub enabled: Option<bool>,
    pub backfill_days: Option<i64>,
}

fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

fn number(value: Option<&Value>) -> Option<f64> {
    value.and_then(|v| {
        v.as_f64()
            .or_else(|| v.as_str()?.trim().parse::<f64>().ok())
    })
}

fn field<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Value> {
    value
        .as_object()
        .and_then(|object| names.iter().find_map(|name| object.get(*name)))
}

fn normalize_millis(value: i64) -> i64 {
    if (0..10_000_000_000).contains(&value) {
        value.saturating_mul(1000)
    } else {
        value
    }
}

fn parse_millis(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    if let Some(raw) = value.as_i64() {
        return Some(normalize_millis(raw));
    }
    if let Some(raw) = value.as_f64() {
        return Some(normalize_millis(raw as i64));
    }
    let raw = value.as_str()?.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(parsed) = raw.parse::<i64>() {
        return Some(normalize_millis(parsed));
    }
    chrono::DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|parsed| parsed.timestamp_millis())
}

fn rfc3339_millis(ms: i64) -> String {
    chrono::DateTime::<Utc>::from_timestamp_millis(ms)
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn local_day_from_ms(ms: i64) -> String {
    Local
        .timestamp_millis_opt(ms)
        .single()
        .unwrap_or_else(Local::now)
        .format("%Y-%m-%d")
        .to_string()
}

fn today_local_day() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn parse_day(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").ok()
}

fn shift_day(day: &str, offset_days: i64) -> String {
    let parsed = parse_day(day).unwrap_or_else(|| Local::now().date_naive());
    if offset_days >= 0 {
        parsed
            .checked_add_days(Days::new(offset_days as u64))
            .unwrap_or(parsed)
            .format("%Y-%m-%d")
            .to_string()
    } else {
        parsed
            .checked_sub_days(Days::new((-offset_days) as u64))
            .unwrap_or(parsed)
            .format("%Y-%m-%d")
            .to_string()
    }
}

fn normalize_qoder_base_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Qoder 服务地址不能为空".to_string());
    }
    let url = reqwest::Url::parse(trimmed).map_err(|_| "Qoder 服务地址无效".to_string())?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err("Qoder 服务地址必须使用 http 或 https".to_string());
    }
    let mut normalized = url.as_str().trim_end_matches('/').to_string();
    if let Some(stripped) = normalized.strip_suffix("/v1") {
        normalized = stripped.trim_end_matches('/').to_string();
    }
    if normalized.is_empty() {
        return Err("Qoder 服务地址无效".to_string());
    }
    Ok(normalized)
}

fn clamp_backfill_days(days: i64) -> i64 {
    days.clamp(QODER_MIN_BACKFILL_DAYS, QODER_MAX_BACKFILL_DAYS)
}

pub(crate) fn current_qoder_settings() -> QoderSettings {
    let base_url = get_persisted_app_setting(APP_SETTING_QODER_BASE_URL_KEY)
        .and_then(|value| normalize_qoder_base_url(&value).ok())
        .unwrap_or_else(|| QODER_DEFAULT_BASE_URL.to_string());
    let organization_id =
        get_persisted_app_setting(APP_SETTING_QODER_ORGANIZATION_ID_KEY).unwrap_or_default();
    let api_key = get_persisted_app_setting(APP_SETTING_QODER_API_KEY_KEY).unwrap_or_default();
    let enabled = get_persisted_app_setting(APP_SETTING_QODER_ENABLED_KEY)
        .map(|value| parse_bool_with_default(&value, false))
        .unwrap_or(false);
    let backfill_days = get_persisted_app_setting(APP_SETTING_QODER_BACKFILL_DAYS_KEY)
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(QODER_DEFAULT_BACKFILL_DAYS);
    QoderSettings {
        base_url,
        organization_id,
        api_key,
        enabled,
        backfill_days: clamp_backfill_days(backfill_days),
    }
}

fn qoder_status_for(settings: &QoderSettings) -> Result<QoderSyncStatus, String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let state = storage
        .get_qoder_sync_state()
        .map_err(|err| format!("读取 Qoder 同步状态失败: {err}"))?
        .unwrap_or_default();
    let (data_from_day, data_to_day) = storage
        .qoder_usage_day_bounds()
        .map_err(|err| format!("读取 Qoder 数据范围失败: {err}"))?;
    Ok(QoderSyncStatus {
        configured: !settings.organization_id.is_empty() && !settings.api_key.is_empty(),
        enabled: settings.enabled,
        running: QODER_SYNC_RUNNING.load(Ordering::SeqCst),
        last_sync_at: state.last_sync_at,
        last_sync_status: state.last_sync_status,
        last_sync_error: state.last_sync_error,
        last_event_at: state.last_event_at,
        covered_until: state.covered_until,
        backfill_from: state.backfill_from,
        event_count: storage
            .count_qoder_usage_events()
            .map_err(|err| format!("统计 Qoder 事件失败: {err}"))?,
        member_count: storage
            .count_qoder_members()
            .map_err(|err| format!("统计 Qoder 成员失败: {err}"))?,
        data_from_day,
        data_to_day,
    })
}

pub(crate) fn qoder_settings_result() -> Result<QoderSettingsResult, String> {
    let settings = current_qoder_settings();
    let status = qoder_status_for(&settings)?;
    Ok(QoderSettingsResult {
        settings: QoderSettingsView {
            base_url: settings.base_url,
            organization_id: settings.organization_id,
            has_api_key: !settings.api_key.is_empty(),
            enabled: settings.enabled,
            backfill_days: settings.backfill_days,
        },
        status,
    })
}

pub(crate) fn qoder_status() -> Result<QoderSyncStatus, String> {
    let settings = current_qoder_settings();
    qoder_status_for(&settings)
}

pub(crate) fn save_qoder_settings(
    patch: QoderSettingsPatch,
) -> Result<QoderSettingsResult, String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let current = current_qoder_settings();
    let mut next = current.clone();
    if let Some(base_url) = patch.base_url {
        next.base_url = normalize_qoder_base_url(&base_url)?;
    }
    if let Some(organization_id) = patch.organization_id {
        next.organization_id = organization_id.trim().to_string();
    }
    if let Some(api_key) = patch.api_key {
        let api_key = api_key.trim();
        if !api_key.is_empty() {
            next.api_key = api_key.to_string();
        }
    }
    if let Some(enabled) = patch.enabled {
        next.enabled = enabled;
    }
    if let Some(backfill_days) = patch.backfill_days {
        next.backfill_days = clamp_backfill_days(backfill_days);
    }
    if next.enabled && (next.organization_id.is_empty() || next.api_key.is_empty()) {
        return Err("启用 Qoder 同步前请先填写组织 ID 与 API Key".to_string());
    }
    if !current.organization_id.is_empty() && current.organization_id != next.organization_id {
        storage
            .clear_qoder_data()
            .map_err(|err| format!("清理旧组织 Qoder 数据失败: {err}"))?;
    }
    save_persisted_app_setting(APP_SETTING_QODER_BASE_URL_KEY, Some(&next.base_url))?;
    save_persisted_app_setting(
        APP_SETTING_QODER_ORGANIZATION_ID_KEY,
        Some(&next.organization_id),
    )?;
    save_persisted_app_setting(APP_SETTING_QODER_API_KEY_KEY, Some(&next.api_key))?;
    save_persisted_bool_setting(APP_SETTING_QODER_ENABLED_KEY, next.enabled)?;
    save_persisted_app_setting(
        APP_SETTING_QODER_BACKFILL_DAYS_KEY,
        Some(&next.backfill_days.to_string()),
    )?;
    if next.enabled {
        spawn_qoder_sync("settings-save");
    }
    qoder_settings_result()
}

fn request_json(_client: &Client, builder: RequestBuilder) -> Result<Value, String> {
    let response = builder.send().map_err(|err| format!("Qoder 请求失败: {err}"))?;
    let status = response.status();
    let payload = response
        .json::<Value>()
        .map_err(|err| format!("Qoder 返回数据不是有效 JSON: {err}"))?;
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return Err("Qoder 鉴权失败，请检查组织 API Key 与组织 ID 是否匹配".to_string());
    }
    if !status.is_success() {
        let detail = text(field(&payload, &["message", "msg", "error", "code"]));
        return Err(match detail {
            Some(detail) => format!("Qoder 返回 HTTP {}：{detail}", status.as_u16()),
            None => format!("Qoder 返回 HTTP {}", status.as_u16()),
        });
    }
    Ok(payload)
}

fn members_url(settings: &QoderSettings) -> String {
    format!(
        "{}/v1/organizations/{}/members",
        settings.base_url,
        settings.organization_id
    )
}

fn usage_events_url(settings: &QoderSettings) -> String {
    format!(
        "{}/v1/organizations/{}/usage-events",
        settings.base_url,
        settings.organization_id
    )
}

fn parse_member(item: &Value, synced_at: i64) -> Option<QoderMember> {
    let member_id = text(field(item, &["id", "memberId", "member_id"]))?;
    Some(QoderMember {
        member_id,
        user_id: text(field(item, &["userId", "user_id"])),
        name: text(field(item, &["name", "nickName", "nickname"])),
        email: text(field(item, &["email"])),
        role: text(field(item, &["role"])),
        status: text(field(item, &["status"])),
        joined_at: text(field(item, &["joinedAt", "joined_at"])),
        deleted_at: text(field(item, &["deletedAt", "deleted_at"])),
        synced_at,
        plan_used: None,
        plan_limit: None,
        resource_package_used: None,
        resource_package_limit: None,
        total_used: None,
        total_limit: None,
        shared_used: None,
        shared_limit: None,
        quota_unit: None,
        quota_status: None,
        last_reset_at: None,
        next_reset_at: None,
        quota_synced_at: None,
    })
}

fn parse_usage_event(item: &Value, synced_at: i64) -> Option<(QoderUsageEvent, i64)> {
    let event_id = text(field(item, &["eventId", "event_id", "id"]))?;
    let occurred_at = parse_millis(field(item, &["timestamp", "beginAt", "begin_at", "finishAt", "finish_at"]))?;
    let credits = number(field(item, &["credits", "credit", "cost"])).unwrap_or(0.0);
    let cost = number(field(item, &["cost"]));
    let day = local_day_from_ms(occurred_at);
    Some((
        QoderUsageEvent {
            event_id,
            member_id: text(field(item, &["memberId", "member_id"])),
            user_id: text(field(item, &["userId", "user_id"])),
            user_email: text(field(item, &["userEmail", "user_email", "email"])),
            source: text(field(item, &["source"])),
            operation: text(field(item, &["operation"])),
            model_tier: text(field(item, &["modelTier", "model_tier"])),
            credits,
            cost,
            occurred_at,
            day,
            synced_at,
        },
        occurred_at,
    ))
}

fn fetch_members(
    client: &Client,
    settings: &QoderSettings,
    synced_at: i64,
) -> Result<Vec<QoderMember>, String> {
    let url = members_url(settings);
    let mut members = Vec::new();
    let mut next_token: Option<String> = None;
    for _ in 0..QODER_MAX_PAGES {
        let mut request = client
            .get(url.as_str())
            .bearer_auth(settings.api_key.as_str())
            .query(&[("maxResults", QODER_PAGE_SIZE.to_string())]);
        if let Some(token) = next_token.as_deref() {
            request = request.query(&[("nextToken", token)]);
        }
        let payload = request_json(client, request)?;
        if let Some(items) = payload.get("members").and_then(Value::as_array) {
            for item in items {
                if let Some(member) = parse_member(item, synced_at) {
                    members.push(member);
                }
            }
        }
        next_token = text(payload.get("nextToken"));
        if next_token.is_none() {
            break;
        }
    }
    Ok(members)
}

fn quota_summary(block: Option<&Value>) -> (Option<f64>, Option<f64>) {
    let Some(summary) = block.and_then(|value| field(value, &["quotaSummary", "quota_summary"]))
    else {
        return (None, None);
    };
    (
        number(field(summary, &["usedValue", "used_value"])),
        number(field(summary, &["limitValue", "limit_value"])),
    )
}

fn quota_unit(block: Option<&Value>) -> Option<String> {
    block
        .and_then(|value| field(value, &["quotaSummary", "quota_summary"]))
        .and_then(|summary| text(field(summary, &["unit"])))
}

fn parse_member_quota(item: &Value, synced_at: i64) -> Option<QoderMemberQuota> {
    let member_id = text(field(item, &["memberId", "member_id", "id"]))?;
    let plan_block = field(item, &["planQuota", "plan_quota"]);
    let resource_block = field(item, &["resourcePackageQuota", "resource_package_quota"]);
    let total_block = field(item, &["totalQuota", "total_quota"]);
    let shared_block = field(item, &["sharedQuota", "shared_quota"]);
    let (plan_used, plan_limit) = quota_summary(plan_block);
    let (resource_used, resource_limit) = quota_summary(resource_block);
    let (total_used, total_limit) = quota_summary(total_block);
    let (shared_used, shared_limit) = quota_summary(shared_block);
    Some(QoderMemberQuota {
        member_id,
        plan_used,
        plan_limit,
        resource_package_used: resource_used,
        resource_package_limit: resource_limit,
        total_used,
        total_limit,
        shared_used,
        shared_limit,
        quota_unit: quota_unit(total_block)
            .or_else(|| quota_unit(plan_block))
            .or_else(|| quota_unit(resource_block)),
        quota_status: text(field(item, &["status"])),
        last_reset_at: text(field(item, &["lastResetAt", "last_reset_at"])),
        next_reset_at: text(field(item, &["nextResetAt", "next_reset_at"])),
        synced_at,
    })
}

const QODER_QUOTA_BATCH_SIZE: usize = 50;

fn fetch_member_quotas(
    client: &Client,
    settings: &QoderSettings,
    member_ids: &[String],
    synced_at: i64,
) -> Result<Vec<QoderMemberQuota>, String> {
    if member_ids.is_empty() {
        return Ok(Vec::new());
    }
    let url = format!(
        "{}/v1/organizations/{}/members/batchGetQuota",
        settings.base_url, settings.organization_id
    );
    let mut quotas = Vec::new();
    for chunk in member_ids.chunks(QODER_QUOTA_BATCH_SIZE) {
        let request = client
            .post(url.as_str())
            .bearer_auth(settings.api_key.as_str())
            .json(&serde_json::json!({ "memberIds": chunk }));
        let payload = request_json(client, request)?;
        if let Some(items) = payload.get("quotas").and_then(Value::as_array) {
            for item in items {
                if let Some(quota) = parse_member_quota(item, synced_at) {
                    quotas.push(quota);
                }
            }
        }
    }
    Ok(quotas)
}

fn resource_packages_url(settings: &QoderSettings) -> String {
    format!(
        "{}/v1/organizations/{}/resource-packages",
        settings.base_url, settings.organization_id
    )
}

/// 汇总组织当前生效（active）的共享资源包额度：返回 (总量, 剩余量)。
fn fetch_resource_packages(
    client: &Client,
    settings: &QoderSettings,
) -> Result<(f64, f64), String> {
    let url = resource_packages_url(settings);
    let mut limit_total = 0.0_f64;
    let mut remaining_total = 0.0_f64;
    let mut next_token: Option<String> = None;
    for _ in 0..QODER_MAX_PAGES {
        let mut request = client
            .get(url.as_str())
            .bearer_auth(settings.api_key.as_str())
            .query(&[
                ("status", "active".to_string()),
                ("maxResults", QODER_PAGE_SIZE.to_string()),
            ]);
        if let Some(token) = next_token.as_deref() {
            request = request.query(&[("nextToken", token)]);
        }
        let payload = request_json(client, request)?;
        if let Some(items) = payload.get("resourcePackages").and_then(Value::as_array) {
            for item in items {
                let limit = number(field(item, &["limitValue", "limit_value"])).unwrap_or(0.0);
                let used = number(field(item, &["usedValue", "used_value"])).unwrap_or(0.0);
                let remaining = number(field(item, &["remainingValue", "remaining_value"]))
                    .unwrap_or_else(|| (limit - used).max(0.0));
                limit_total += limit.max(0.0);
                remaining_total += remaining.max(0.0);
            }
        }
        next_token = text(payload.get("nextToken"));
        if next_token.is_none() {
            break;
        }
    }
    Ok((limit_total, remaining_total))
}

fn fetch_usage_events(
    client: &Client,
    settings: &QoderSettings,
    start_ms: i64,
    end_ms: i64,
    synced_at: i64,
) -> Result<(Vec<QoderUsageEvent>, Option<i64>), String> {
    let url = usage_events_url(settings);
    let mut events = Vec::new();
    let mut max_event_at: Option<i64> = None;
    let mut next_token: Option<String> = None;
    for _ in 0..QODER_MAX_PAGES {
        let mut request = client
            .get(url.as_str())
            .bearer_auth(settings.api_key.as_str())
            .query(&[
                ("startDate", rfc3339_millis(start_ms)),
                ("endDate", rfc3339_millis(end_ms)),
                ("maxResults", QODER_PAGE_SIZE.to_string()),
            ]);
        if let Some(token) = next_token.as_deref() {
            request = request.query(&[("nextToken", token)]);
        }
        let payload = request_json(client, request)?;
        if let Some(items) = payload.get("usages").and_then(Value::as_array) {
            for item in items {
                if let Some((event, occurred_at)) = parse_usage_event(item, synced_at) {
                    max_event_at =
                        Some(max_event_at.map_or(occurred_at, |current| current.max(occurred_at)));
                    events.push(event);
                }
            }
        }
        next_token = text(payload.get("nextToken"));
        if next_token.is_none() {
            break;
        }
    }
    Ok((events, max_event_at))
}

fn sync_start_ms(
    covered_until: Option<i64>,
    backfill_from: i64,
    now_ms: i64,
    overlap_ms: i64,
) -> i64 {
    match covered_until {
        Some(covered) if covered > backfill_from => covered
            .saturating_sub(overlap_ms)
            .max(backfill_from),
        _ => backfill_from,
    }
    .min(now_ms)
}

fn overlap_refresh_due(last_refresh: Option<Instant>) -> bool {
    last_refresh
        .map(|at| at.elapsed() >= QODER_REFRESH_OVERLAP_PERIOD)
        .unwrap_or(true)
}

/// 常规同步回退 10 分钟做增量；每 30 分钟改用 48 小时窗口重新拉取，让官方修正回写。
fn current_overlap_ms() -> i64 {
    let mut last_refresh = QODER_LAST_OVERLAP_REFRESH
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if overlap_refresh_due(*last_refresh) {
        *last_refresh = Some(Instant::now());
        QODER_REFRESH_OVERLAP_MS
    } else {
        QODER_INCREMENTAL_OVERLAP_MS
    }
}

fn run_qoder_sync(settings: &QoderSettings) -> Result<(), String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let client = crate::gateway::fresh_upstream_client_with_timeout(QODER_REQUEST_TIMEOUT);
    let synced_at = now_ts();

    let members = fetch_members(&client, settings, synced_at)?;
    storage
        .upsert_qoder_members(&members)
        .map_err(|err| format!("保存 Qoder 成员失败: {err}"))?;

    // 额度快照属于辅助信息，拉取失败不阻断用量事件同步。
    let member_ids = members
        .iter()
        .map(|member| member.member_id.clone())
        .collect::<Vec<_>>();
    match fetch_member_quotas(&client, settings, &member_ids, synced_at) {
        Ok(quotas) => {
            storage
                .update_qoder_member_quotas(&quotas)
                .map_err(|err| format!("保存 Qoder 成员额度失败: {err}"))?;
        }
        Err(error) => log::warn!("event=qoder_quota_sync_failed err={}", error),
    }

    // 共享资源包汇总同样属于辅助信息，失败时沿用上一轮结果。
    let shared_package = match fetch_resource_packages(&client, settings) {
        Ok(totals) => Some(totals),
        Err(error) => {
            log::warn!("event=qoder_shared_package_sync_failed err={}", error);
            None
        }
    };

    let state = storage
        .get_qoder_sync_state()
        .map_err(|err| format!("读取 Qoder 同步进度失败: {err}"))?
        .unwrap_or_default();
    let now_ms = Utc::now().timestamp_millis();
    let desired_from = now_ms.saturating_sub(settings.backfill_days.saturating_mul(DAY_MS));
    let effective_from = match state.backfill_from {
        Some(stored) => stored.min(desired_from),
        None => desired_from,
    };
    let mut cursor = sync_start_ms(
        state.covered_until,
        effective_from,
        now_ms,
        current_overlap_ms(),
    );
    let mut max_event_at = state.last_event_at;
    let mut written_total = 0usize;
    while cursor < now_ms {
        let window_end = cursor.saturating_add(QODER_MAX_WINDOW_MS).min(now_ms);
        let (events, window_max) =
            fetch_usage_events(&client, settings, cursor, window_end, synced_at)?;
        written_total += storage
            .upsert_qoder_usage_events(&events)
            .map_err(|err| format!("保存 Qoder 用量事件失败: {err}"))?;
        if let Some(window_max) = window_max {
            max_event_at = Some(max_event_at.map_or(window_max, |current| current.max(window_max)));
        }
        cursor = window_end;
    }

    storage
        .save_qoder_sync_state(&QoderSyncState {
            last_sync_at: Some(now_ts()),
            last_sync_status: Some("success".to_string()),
            last_sync_error: None,
            covered_until: Some(now_ms),
            backfill_from: Some(effective_from),
            last_event_at: max_event_at,
            updated_at: now_ts(),
            shared_package_limit: shared_package
                .map(|(limit, _)| limit)
                .or(state.shared_package_limit),
            shared_package_remaining: shared_package
                .map(|(_, remaining)| remaining)
                .or(state.shared_package_remaining),
            shared_package_synced_at: match shared_package {
                Some(_) => Some(now_ts()),
                None => state.shared_package_synced_at,
            },
        })
        .map_err(|err| format!("保存 Qoder 同步状态失败: {err}"))?;
    log::info!(
        "event=qoder_sync_ok members={} written_events={}",
        members.len(),
        written_total
    );
    Ok(())
}

fn record_qoder_sync_failure(error: &str) {
    let Some(storage) = open_storage() else {
        return;
    };
    let state = storage.get_qoder_sync_state().ok().flatten().unwrap_or_default();
    let _ = storage.save_qoder_sync_state(&QoderSyncState {
        last_sync_at: Some(now_ts()),
        last_sync_status: Some("failed".to_string()),
        last_sync_error: Some(error.to_string()),
        covered_until: state.covered_until,
        backfill_from: state.backfill_from,
        last_event_at: state.last_event_at,
        updated_at: now_ts(),
        shared_package_limit: state.shared_package_limit,
        shared_package_remaining: state.shared_package_remaining,
        shared_package_synced_at: state.shared_package_synced_at,
    });
}

struct SyncRunningGuard;

impl Drop for SyncRunningGuard {
    fn drop(&mut self) {
        QODER_SYNC_RUNNING.store(false, Ordering::SeqCst);
    }
}

pub(crate) fn sync_qoder_now() -> Result<(), String> {
    let _lock = QODER_SYNC_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let settings = current_qoder_settings();
    if !settings.enabled {
        return Err("Qoder 同步未启用".to_string());
    }
    if settings.organization_id.is_empty() || settings.api_key.is_empty() {
        return Err("Qoder 组织 ID 或 API Key 未配置".to_string());
    }
    QODER_SYNC_RUNNING.store(true, Ordering::SeqCst);
    let _running = SyncRunningGuard;
    match run_qoder_sync(&settings) {
        Ok(()) => Ok(()),
        Err(error) => {
            record_qoder_sync_failure(&error);
            log::warn!("event=qoder_sync_failed err={}", error);
            Err(error)
        }
    }
}

fn spawn_qoder_sync(trigger: &str) -> bool {
    let trigger = trigger.to_string();
    thread::Builder::new()
        .name("qoder-usage-sync".to_string())
        .spawn(move || {
            if let Err(error) = sync_qoder_now() {
                log::warn!("event=qoder_sync_skipped trigger={} err={}", trigger, error);
            }
        })
        .is_ok()
}

pub(crate) fn trigger_qoder_sync() -> Result<QoderSyncTriggerResult, String> {
    if QODER_SYNC_RUNNING.load(Ordering::SeqCst) {
        return Ok(QoderSyncTriggerResult { started: false });
    }
    Ok(QoderSyncTriggerResult {
        started: spawn_qoder_sync("manual"),
    })
}

fn schedule_qoder_sync_if_due() {
    let settings = current_qoder_settings();
    if !settings.enabled || settings.organization_id.is_empty() || settings.api_key.is_empty() {
        return;
    }
    let Some(storage) = open_storage() else {
        return;
    };
    let last_sync_at = storage
        .get_qoder_sync_state()
        .ok()
        .flatten()
        .and_then(|state| state.last_sync_at);
    let due = last_sync_at
        .map(|at| now_ts().saturating_sub(at) >= QODER_SYNC_INTERVAL_SECS)
        .unwrap_or(true);
    if due && !QODER_SYNC_RUNNING.load(Ordering::SeqCst) {
        spawn_qoder_sync("polling");
    }
}

pub(crate) fn ensure_qoder_polling() {
    QODER_POLLING_STARTED.get_or_init(|| {
        let _ = thread::Builder::new()
            .name("qoder-usage-polling".to_string())
            .spawn(|| loop {
                schedule_qoder_sync_if_due();
                thread::sleep(Duration::from_secs(QODER_SYNC_TICK_SECS));
            });
    });
}

type MemberMetaIndex<'a> = (HashMap<&'a str, &'a QoderMember>, HashMap<&'a str, &'a QoderMember>);

fn member_meta_index(members: &[QoderMember]) -> MemberMetaIndex<'_> {
    let mut by_member_id = HashMap::new();
    let mut by_user_id = HashMap::new();
    for member in members {
        by_member_id.insert(member.member_id.as_str(), member);
        if let Some(user_id) = member.user_id.as_deref() {
            by_user_id.insert(user_id, member);
        }
    }
    (by_member_id, by_user_id)
}

fn resolve_member_display(
    member_id: &str,
    user_id: Option<&str>,
    fallback_email: Option<&str>,
    index: &MemberMetaIndex<'_>,
) -> (Option<String>, Option<String>) {
    let meta: Option<&QoderMember> = index
        .0
        .get(member_id)
        .map(|member| *member)
        .or_else(|| user_id.and_then(|id| index.1.get(id)).map(|member| *member));
    let name = meta
        .and_then(|member| member.name.clone())
        .or_else(|| fallback_email.map(str::to_string));
    let email = meta
        .and_then(|member| member.email.clone())
        .or_else(|| fallback_email.map(str::to_string));
    let name = match name {
        Some(value) if !value.is_empty() => Some(value),
        _ if member_id.is_empty() => Some("未知成员".to_string()),
        other => other,
    };
    (name, email)
}

fn normalize_day_range(
    start_day: Option<String>,
    end_day: Option<String>,
) -> Result<(String, String), String> {
    let today = today_local_day();
    let end = end_day
        .filter(|value| parse_day(value).is_some())
        .unwrap_or_else(|| today.clone());
    let start = start_day
        .filter(|value| parse_day(value).is_some())
        .unwrap_or_else(|| shift_day(&end, -6));
    if start > end {
        return Err("开始日期不能晚于结束日期".to_string());
    }
    Ok((start, end))
}

fn day_label(day: &str) -> String {
    if day.len() == 10 {
        day[5..].to_string()
    } else {
        day.to_string()
    }
}

const QODER_BUCKET_FILL_LIMIT: usize = 400;

/// 生成完整时间桶序列并补零；`half_hour` 为 true 时按半小时分桶（单日区间）。
fn build_bucket_series(
    rows: Vec<QoderBucketUsageRow>,
    start_day: &str,
    end_day: &str,
    half_hour: bool,
) -> Vec<QoderUsageBucket> {
    let mut by_bucket: HashMap<String, (f64, i64, Vec<QoderModelUsageStat>)> = HashMap::new();
    for row in rows {
        let entry = by_bucket
            .entry(row.bucket)
            .or_insert_with(|| (0.0, 0, Vec::new()));
        entry.0 += row.credits;
        entry.1 += row.events;
        entry.2.push(QoderModelUsageStat {
            model_tier: row.model_tier.unwrap_or_default(),
            credits: row.credits,
            events: row.events,
        });
    }

    let mut keys: Vec<(String, String)> = Vec::new();
    if half_hour {
        for hour in 0..24 {
            for minute in [0, 30] {
                keys.push((
                    format!("{start_day}T{hour:02}:{minute:02}"),
                    format!("{hour:02}:{minute:02}"),
                ));
            }
        }
    } else {
        let end = parse_day(end_day);
        let mut cursor = parse_day(start_day);
        while let (Some(current), Some(end)) = (cursor, end) {
            if current > end || keys.len() >= QODER_BUCKET_FILL_LIMIT {
                break;
            }
            let day = current.format("%Y-%m-%d").to_string();
            keys.push((day.clone(), day_label(&day)));
            cursor = current.checked_add_days(Days::new(1));
        }
    }

    keys.into_iter()
        .map(|(key, label)| {
            let (credits, events, mut models) = by_bucket
                .remove(&key)
                .unwrap_or_else(|| (0.0, 0, Vec::new()));
            models.sort_by(|left, right| {
                right
                    .credits
                    .partial_cmp(&left.credits)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            QoderUsageBucket {
                key,
                label,
                credits,
                events,
                models,
            }
        })
        .collect()
}

pub(crate) fn qoder_overview(
    start_day: Option<String>,
    end_day: Option<String>,
    member_id: Option<String>,
) -> Result<QoderOverviewResult, String> {
    let (start, end) = normalize_day_range(start_day, end_day)?;
    let half_hour = start == end;
    let member_filter = member_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let (total_credits, total_events) = storage
        .qoder_usage_totals(&start, &end, member_filter.as_deref())
        .map_err(|err| format!("统计 Qoder 积分失败: {err}"))?;
    let active_members = storage
        .qoder_active_member_count(&start, &end)
        .map_err(|err| format!("统计 Qoder 活跃成员失败: {err}"))?;
    let member_count = storage
        .count_qoder_members()
        .map_err(|err| format!("统计 Qoder 成员失败: {err}"))?;
    let models = storage
        .qoder_model_usage(&start, &end, member_filter.as_deref())
        .map_err(|err| format!("统计 Qoder 模型用量失败: {err}"))?
        .into_iter()
        .map(|row| QoderModelUsageStat {
            model_tier: row.model_tier.unwrap_or_default(),
            credits: row.credits,
            events: row.events,
        })
        .collect::<Vec<_>>();
    let bucket_rows = storage
        .qoder_usage_buckets(&start, &end, half_hour, member_filter.as_deref())
        .map_err(|err| format!("统计 Qoder 分桶用量失败: {err}"))?;
    let buckets = build_bucket_series(bucket_rows, &start, &end, half_hour);
    let sync_state = storage
        .get_qoder_sync_state()
        .map_err(|err| format!("读取 Qoder 同步进度失败: {err}"))?;
    let shared_package = sync_state.and_then(|state| {
        Some(QoderSharedPackageStat {
            limit: state.shared_package_limit?,
            remaining: state.shared_package_remaining?,
            synced_at: state.shared_package_synced_at,
        })
    });
    let members = storage
        .list_qoder_members()
        .map_err(|err| format!("读取 Qoder 成员失败: {err}"))?
        .into_iter()
        .map(|member| QoderMemberQuotaStat {
            member_id: member.member_id,
            name: member.name.or_else(|| member.email.clone()),
            email: member.email,
            status: member.status,
            plan_used: member.plan_used,
            plan_limit: member.plan_limit,
            resource_used: member.resource_package_used,
            resource_limit: member.resource_package_limit,
            total_used: member.total_used,
            total_limit: member.total_limit,
            shared_used: member.shared_used,
            shared_limit: member.shared_limit,
            quota_unit: member.quota_unit,
            quota_status: member.quota_status,
            next_reset_at: member.next_reset_at,
            quota_synced_at: member.quota_synced_at,
        })
        .collect();

    Ok(QoderOverviewResult {
        start_day: start,
        end_day: end,
        bucket_unit: if half_hour { "half_hour" } else { "day" }.to_string(),
        total_credits,
        total_events,
        active_members,
        member_count,
        models,
        buckets,
        members,
        shared_package,
    })
}

pub(crate) fn qoder_events(
    start_day: Option<String>,
    end_day: Option<String>,
    member_id: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
) -> Result<QoderUsageEventListResult, String> {
    let (start, end) = normalize_day_range(start_day, end_day)?;
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(20).clamp(1, 200);
    let member_filter = member_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let (rows, total) = storage
        .qoder_usage_events_page(
            &start,
            &end,
            member_filter.as_deref(),
            page_size,
            (page - 1).saturating_mul(page_size),
        )
        .map_err(|err| format!("查询 Qoder 用量明细失败: {err}"))?;
    let members_meta = storage
        .list_qoder_members()
        .map_err(|err| format!("读取 Qoder 成员失败: {err}"))?;
    let index = member_meta_index(&members_meta);
    let items = rows
        .into_iter()
        .map(|row| {
            let (name, email) = resolve_member_display(
                row.member_id.as_deref().unwrap_or(""),
                row.user_id.as_deref(),
                row.user_email.as_deref(),
                &index,
            );
            QoderUsageEventItem {
                event_id: row.event_id,
                occurred_at: row.occurred_at,
                day: row.day,
                member_id: row.member_id,
                name,
                email,
                model_tier: row.model_tier.unwrap_or_default(),
                source: row.source,
                operation: row.operation,
                credits: row.credits,
                cost: row.cost,
            }
        })
        .collect();
    Ok(QoderUsageEventListResult {
        items,
        total,
        page,
        page_size,
    })
}

pub(crate) fn qoder_today_top(limit: Option<i64>) -> Result<QoderTodayTopResult, String> {
    let day = today_local_day();
    let limit = limit.unwrap_or(10).clamp(1, 50) as usize;
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let (total_credits, total_events) = storage
        .qoder_usage_totals(&day, &day, None)
        .map_err(|err| format!("统计 Qoder 今日积分失败: {err}"))?;
    let member_rows = storage
        .qoder_member_usage(&day, &day)
        .map_err(|err| format!("统计 Qoder 今日成员用量失败: {err}"))?;
    let members_meta = storage
        .list_qoder_members()
        .map_err(|err| format!("读取 Qoder 成员失败: {err}"))?;
    let index = member_meta_index(&members_meta);
    let items = member_rows
        .into_iter()
        .filter(|row| row.credits > 0.0)
        .take(limit)
        .map(|row| {
            let (name, email) = resolve_member_display(
                &row.member_id,
                row.user_id.as_deref(),
                row.user_email.as_deref(),
                &index,
            );
            QoderTopMember {
                member_id: row.member_id,
                name,
                email,
                credits: row.credits,
                events: row.events,
            }
        })
        .collect();
    Ok(QoderTodayTopResult {
        day,
        total_credits,
        total_events,
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        build_bucket_series, local_day_from_ms, normalize_qoder_base_url, overlap_refresh_due,
        parse_usage_event, rfc3339_millis, shift_day, sync_start_ms, QODER_INCREMENTAL_OVERLAP_MS,
        QODER_REFRESH_OVERLAP_PERIOD,
    };
    use codexmanager_core::storage::QoderBucketUsageRow;
    use std::time::{Duration, Instant};

    #[test]
    fn bucket_series_fills_gaps_and_sorts_models() {
        let rows = vec![
            QoderBucketUsageRow {
                bucket: "2026-10-10".to_string(),
                model_tier: Some("ultimate".to_string()),
                credits: 2.0,
                events: 2,
            },
            QoderBucketUsageRow {
                bucket: "2026-10-10".to_string(),
                model_tier: Some("standard".to_string()),
                credits: 1.0,
                events: 1,
            },
            QoderBucketUsageRow {
                bucket: "2026-10-12".to_string(),
                model_tier: None,
                credits: 0.5,
                events: 1,
            },
        ];
        let series = build_bucket_series(rows, "2026-10-10", "2026-10-12", false);
        assert_eq!(series.len(), 3);
        assert_eq!(series[0].label, "10-10");
        assert_eq!(series[0].credits, 3.0);
        assert_eq!(series[0].models[0].model_tier, "ultimate");
        assert_eq!(series[1].credits, 0.0);
        assert!(series[1].models.is_empty());
        assert_eq!(series[2].models[0].model_tier, "");
    }

    #[test]
    fn half_hour_bucket_series_has_48_slots() {
        let series = build_bucket_series(Vec::new(), "2026-10-10", "2026-10-10", true);
        assert_eq!(series.len(), 48);
        assert_eq!(series[0].key, "2026-10-10T00:00");
        assert_eq!(series[1].label, "00:30");
        assert_eq!(series[47].key, "2026-10-10T23:30");
    }

    #[test]
    fn base_url_normalization_handles_v1_suffix_and_bad_scheme() {
        assert_eq!(
            normalize_qoder_base_url(" https://api.qoder.com.cn/ ").expect("normalized"),
            "https://api.qoder.com.cn"
        );
        assert_eq!(
            normalize_qoder_base_url("https://api.qoder.com.cn/v1").expect("normalized"),
            "https://api.qoder.com.cn"
        );
        assert!(normalize_qoder_base_url("ftp://api.qoder.com.cn").is_err());
        assert!(normalize_qoder_base_url("").is_err());
    }

    #[test]
    fn usage_event_parsing_falls_back_across_fields() {
        let synced_at = 1_700_000_000;
        let item = serde_json::json!({
            "eventId": "evt_1",
            "timestamp": 1_700_000_000_000i64,
            "memberId": "mem_1",
            "userEmail": "a@example.com",
            "operation": "Agent",
            "modelTier": "ultimate",
            "credits": "1.25"
        });
        let (event, occurred_at) = parse_usage_event(&item, synced_at).expect("parsed");
        assert_eq!(event.event_id, "evt_1");
        assert_eq!(event.credits, 1.25);
        assert_eq!(event.model_tier.as_deref(), Some("ultimate"));
        assert_eq!(occurred_at, 1_700_000_000_000);
        assert_eq!(event.day, local_day_from_ms(1_700_000_000_000));

        let seconds = serde_json::json!({
            "eventId": "evt_2",
            "timestamp": 1_700_000_000i64,
        });
        let (event, occurred_at) = parse_usage_event(&seconds, synced_at).expect("parsed");
        assert_eq!(occurred_at, 1_700_000_000_000);
        assert_eq!(event.credits, 0.0);

        assert!(parse_usage_event(&serde_json::json!({ "timestamp": 1 }), synced_at).is_none());
    }

    #[test]
    fn sync_window_prefers_incremental_then_backfill() {
        let backfill = 1_000_000i64;
        assert_eq!(
            sync_start_ms(None, backfill, 2_000_000, QODER_INCREMENTAL_OVERLAP_MS),
            backfill
        );
        // 已有覆盖点时从覆盖点回退一小段重叠窗口做增量
        assert_eq!(
            sync_start_ms(
                Some(1_900_000),
                backfill,
                2_000_000,
                QODER_INCREMENTAL_OVERLAP_MS
            ),
            1_900_000 - QODER_INCREMENTAL_OVERLAP_MS
        );
        // 覆盖率区间早于回填边界时，从回填边界重新补齐
        assert_eq!(
            sync_start_ms(Some(1_100_000), backfill, 2_000_000, QODER_INCREMENTAL_OVERLAP_MS),
            backfill
        );
        // 时钟偏移导致覆盖点在将来时，最多只取到当前时间
        assert_eq!(
            sync_start_ms(Some(9_000_000), backfill, 2_000_000, QODER_INCREMENTAL_OVERLAP_MS),
            2_000_000
        );
    }

    #[test]
    fn overlap_refresh_runs_first_time_and_periodically() {
        assert!(overlap_refresh_due(None));
        assert!(overlap_refresh_due(Some(
            Instant::now() - QODER_REFRESH_OVERLAP_PERIOD
        )));
        assert!(overlap_refresh_due(Some(
            Instant::now() - QODER_REFRESH_OVERLAP_PERIOD - Duration::from_secs(1)
        )));
        assert!(!overlap_refresh_due(Some(Instant::now())));
    }

    #[test]
    fn day_helpers_cover_shift_and_format() {
        assert_eq!(shift_day("2026-03-01", -1), "2026-02-28");
        assert_eq!(shift_day("2026-01-31", 1), "2026-02-01");
        assert!(rfc3339_millis(1_700_000_000_000).starts_with("2023-11-14T"));
    }
}
