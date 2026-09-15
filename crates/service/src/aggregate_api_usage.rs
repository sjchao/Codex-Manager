use chrono::{Duration as ChronoDuration, Local, TimeZone};
use codexmanager_core::rpc::types::{
    AggregateApiPlatformKeyUsageDailySummary, AggregateApiUsageDailySummary,
    AggregateApiUsageSummaryResult,
};
use codexmanager_core::storage::{now_ts, AggregateApi, AggregateApiDailyUsage, Storage};
use reqwest::blocking::{Client, RequestBuilder};
use reqwest::StatusCode;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::aggregate_api::AGGREGATE_API_AUTH_APIKEY;
use crate::storage_helpers::open_storage;

const SUB2API_USAGE_PAGE_SIZE: usize = 1000;
const SUB2API_USAGE_MAX_PAGES: usize = 100;
// 中文注释：面板按明文 key 精确匹配拿到 api_key_id 后，用量列表只拉这把 key 的记录，避免全账号翻页。
const SUB2API_KEYS_PAGE_SIZE: usize = 1000;
const SUB2API_KEYS_MAX_PAGES: usize = 5;
const SUB2API_TOKEN_REFRESH_AHEAD_SECS: i64 = 300;
const SUB2API_USAGE_POLL_INTERVAL: Duration = Duration::from_secs(60);
// 中文注释：10 分钟一轮整轮同步：账号汇总、每日用量与全量翻页补齐积压。
const SUB2API_FULL_SYNC_INTERVAL: Duration = Duration::from_secs(600);

static SUB2API_USAGE_POLLING_STARTED: OnceLock<()> = OnceLock::new();

#[derive(Debug)]
enum UsageHttpError {
    Unauthorized,
    Failed(String),
}

#[derive(Clone, Copy)]
enum UsageTokenOwner<'a> {
    AggregateApi(&'a str),
    Sub2ApiAccount(&'a str),
}

fn normalized_non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_token_expires_at(value: Option<i64>) -> Option<i64> {
    value.map(|value| {
        if value > 1_000_000_000_000 {
            value / 1_000
        } else {
            value
        }
    })
}

fn sub2api_origin_from_url(raw_url: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(raw_url.trim())
        .map_err(|_| "aggregate api URL is invalid for Sub2API usage sync".to_string())?;
    let origin = url.origin().ascii_serialization();
    if origin == "null" {
        Err("aggregate api URL must use HTTP or HTTPS for Sub2API usage sync".to_string())
    } else {
        Ok(origin)
    }
}

fn is_success_payload(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return true;
    };
    if let Some(code) = object.get("code") {
        return matches!(code.as_i64(), Some(0)) || matches!(code.as_str(), Some("0"));
    }
    !matches!(object.get("success").and_then(Value::as_bool), Some(false))
}

fn response_data(value: &Value) -> &Value {
    value.get("data").unwrap_or(value)
}

fn read_number(value: Option<&Value>) -> Option<f64> {
    value.and_then(|value| match value {
        Value::Number(number) => number.as_f64(),
        Value::String(raw) => raw.trim().parse::<f64>().ok(),
        _ => None,
    })
}

fn read_i64(value: Option<&Value>) -> Option<i64> {
    value.and_then(|value| match value {
        Value::Number(number) => number.as_i64(),
        Value::String(raw) => raw.trim().parse::<i64>().ok(),
        _ => None,
    })
}

fn read_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn field<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Value> {
    let object = value.as_object()?;
    names.iter().find_map(|name| object.get(*name))
}

fn array_field<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Vec<Value>> {
    field(value, names).and_then(Value::as_array)
}

fn usage_items(value: &Value) -> Vec<&Value> {
    let data = response_data(value);
    if let Some(items) = data.as_array() {
        return items.iter().collect();
    }
    array_field(data, &["items", "records", "list", "usage"]).map_or_else(Vec::new, |items| {
        items.iter().collect()
    })
}

fn request_total(value: &Value) -> Option<usize> {
    let data = response_data(value);
    read_i64(field(data, &["total", "total_count", "totalCount"]))
        .and_then(|value| usize::try_from(value).ok())
}

static USAGE_API_KEY_IDS: OnceLock<Mutex<HashMap<(String, String), i64>>> = OnceLock::new();

fn cached_usage_api_key_id(origin: &str, api_key: &str) -> Option<i64> {
    USAGE_API_KEY_IDS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .and_then(|cache| cache.get(&(origin.to_string(), api_key.to_string())).copied())
}

fn store_usage_api_key_id(origin: &str, api_key: &str, api_key_id: i64) {
    if let Ok(mut cache) = USAGE_API_KEY_IDS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        cache.insert((origin.to_string(), api_key.to_string()), api_key_id);
    }
}

fn call_json(builder: RequestBuilder) -> Result<Value, UsageHttpError> {
    let response = builder
        .send()
        .map_err(|err| UsageHttpError::Failed(format!("request failed: {err}")))?;
    let status = response.status();
    if status == StatusCode::UNAUTHORIZED {
        return Err(UsageHttpError::Unauthorized);
    }
    if !status.is_success() {
        return Err(UsageHttpError::Failed(format!(
            "endpoint returned HTTP {}",
            status.as_u16()
        )));
    }
    let payload = response
        .json::<Value>()
        .map_err(|err| UsageHttpError::Failed(format!("invalid JSON response: {err}")))?;
    if !is_success_payload(&payload) {
        return Err(UsageHttpError::Failed(
            "endpoint returned an unsuccessful Sub2API result".to_string(),
        ));
    }
    Ok(payload)
}

fn error_text(error: UsageHttpError) -> String {
    match error {
        UsageHttpError::Unauthorized => "endpoint returned HTTP 401".to_string(),
        UsageHttpError::Failed(message) => message,
    }
}

fn refresh_access_token(
    storage: &Storage,
    client: &Client,
    origin: &str,
    token_owner: UsageTokenOwner<'_>,
    auth_token: &mut String,
    refresh_token: &mut Option<String>,
    token_expires_at: &mut Option<i64>,
) -> Result<(), String> {
    let refresh = refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Sub2API access token expired but refresh token is missing".to_string())?;
    let endpoint = format!("{origin}/api/v1/auth/refresh");
    let payload = call_json(client.post(endpoint).json(&serde_json::json!({
        "refresh_token": refresh,
    })))
    .map_err(error_text)?;
    let data = response_data(&payload);
    let next_access_token = read_string(field(data, &["access_token", "accessToken"]))
        .ok_or_else(|| "Sub2API refresh response did not include access_token".to_string())?;
    let next_refresh_token = read_string(field(data, &["refresh_token", "refreshToken"]));
    let expires_in = read_i64(field(data, &["expires_in", "expiresIn"]));
    *auth_token = next_access_token;
    if next_refresh_token.is_some() {
        *refresh_token = next_refresh_token;
    }
    *token_expires_at = expires_in
        .map(|seconds| now_ts().saturating_add(seconds.max(0)))
        .or_else(|| super::sub2api::jwt_expires_at(auth_token.as_str()));
    match token_owner {
        UsageTokenOwner::AggregateApi(api_id) => storage
            .update_aggregate_api_usage_tokens(
                api_id,
                auth_token.as_str(),
                refresh_token.as_deref(),
                *token_expires_at,
            ),
        UsageTokenOwner::Sub2ApiAccount(account_id) => storage
            .update_sub2api_account_tokens(
                account_id,
                auth_token.as_str(),
                refresh_token.as_deref(),
                *token_expires_at,
            ),
    }
    .map_err(|err| format!("persist refreshed Sub2API token failed: {err}"))
}

fn fetch_daily_usage(
    client: &Client,
    origin: &str,
    api_key: &str,
) -> Result<Value, String> {
    call_json(
        client
            .get(format!("{origin}/v1/usage"))
            .bearer_auth(api_key.trim()),
    )
    .map_err(error_text)
}

fn persist_daily_usage(
    storage: &Storage,
    api_id: &str,
    payload: &Value,
    today: &str,
) -> Result<(), String> {
    let data = response_data(payload);
    let usage = data.get("usage").unwrap_or(data);
    let synced_at = now_ts();
    let mut persisted_today = false;
    if let Some(days) = array_field(usage, &["daily_usage", "dailyUsage", "days"]) {
        for day in days {
            let Some(usage_date) = read_string(field(day, &["date", "usage_date", "usageDate"])) else {
                continue;
            };
            let actual_cost = read_number(field(day, &["actual_cost", "actualCost"]))
                .unwrap_or(0.0);
            let total_cost = read_number(field(day, &["total_cost", "totalCost"]));
            let request_count = read_i64(field(day, &["request_count", "requestCount", "count"]));
            storage
                .upsert_aggregate_api_daily_usage(&AggregateApiDailyUsage {
                    aggregate_api_id: api_id.to_string(),
                    usage_date: usage_date.clone(),
                    actual_cost,
                    total_cost,
                    request_count,
                    synced_at,
                })
                .map_err(|err| format!("persist Sub2API daily usage failed: {err}"))?;
            persisted_today |= usage_date == today;
        }
    }
    if !persisted_today {
        if let Some(today_usage) = usage.get("today").or_else(|| Some(usage)) {
            // 中文注释：面板首页统计（dashboard/stats）把当日数据平铺在 today_* 字段上，
            // 其它形态的响应则用 actual_cost/total_cost/request_count，这里两种都兼容。
            let actual_cost = read_number(field(
                today_usage,
                &[
                    "today_actual_cost",
                    "todayActualCost",
                    "actual_cost",
                    "actualCost",
                ],
            ))
            .unwrap_or(0.0);
            let total_cost = read_number(field(
                today_usage,
                &["today_cost", "todayCost", "total_cost", "totalCost"],
            ));
            let request_count = read_i64(field(
                today_usage,
                &[
                    "today_requests",
                    "todayRequests",
                    "request_count",
                    "requestCount",
                    "count",
                ],
            ));
            storage
                .upsert_aggregate_api_daily_usage(&AggregateApiDailyUsage {
                    aggregate_api_id: api_id.to_string(),
                    usage_date: today.to_string(),
                    actual_cost,
                    total_cost,
                    request_count,
                    synced_at,
                })
                .map_err(|err| format!("persist Sub2API current daily usage failed: {err}"))?;
        }
    }
    Ok(())
}

fn find_api_key_id(payload: &Value, api_key: &str) -> Option<i64> {
    usage_items(payload).into_iter().find_map(|item| {
        if read_string(field(item, &["key"]))? != api_key {
            return None;
        }
        read_i64(field(item, &["id", "key_id", "keyId"]))
    })
}

fn resolve_usage_api_key_id(
    storage: &Storage,
    client: &Client,
    origin: &str,
    aggregate_api_id: &str,
    api_key: &str,
    token_owner: UsageTokenOwner<'_>,
    auth_token: &mut String,
    refresh_token: &mut Option<String>,
    token_expires_at: &mut Option<i64>,
) -> Option<i64> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return None;
    }
    if let Some(api_key_id) = cached_usage_api_key_id(origin, api_key) {
        return Some(api_key_id);
    }
    let mut refreshed_after_401 = false;
    let mut page = 1usize;
    while page <= SUB2API_KEYS_MAX_PAGES {
        let result = call_json(
            client
                .get(format!("{origin}/api/v1/keys"))
                .bearer_auth(auth_token.as_str())
                .query(&[
                    ("page", page.to_string()),
                    ("page_size", SUB2API_KEYS_PAGE_SIZE.to_string()),
                ]),
        );
        match result {
            Ok(payload) => {
                if let Some(api_key_id) = find_api_key_id(&payload, api_key) {
                    store_usage_api_key_id(origin, api_key, api_key_id);
                    return Some(api_key_id);
                }
                if usage_items(&payload).len() < SUB2API_KEYS_PAGE_SIZE {
                    return None;
                }
                page += 1;
            }
            Err(UsageHttpError::Unauthorized) if !refreshed_after_401 => {
                refreshed_after_401 = true;
                if let Err(error) = refresh_access_token(
                    storage,
                    client,
                    origin,
                    token_owner,
                    auth_token,
                    refresh_token,
                    token_expires_at,
                ) {
                    log::warn!(
                        "event=sub2api_usage_key_lookup_failed aggregate_api_id={} err={}",
                        aggregate_api_id,
                        error
                    );
                    return None;
                }
            }
            Err(error) => {
                log::warn!(
                    "event=sub2api_usage_key_lookup_failed aggregate_api_id={} err={}",
                    aggregate_api_id,
                    error_text(error)
                );
                return None;
            }
        }
    }
    None
}

fn fetch_usage_page(
    client: &Client,
    origin: &str,
    auth_token: &str,
    page: usize,
    start_date: &str,
    end_date: &str,
    api_key_id: Option<i64>,
) -> Result<Value, UsageHttpError> {
    let mut query = vec![
        ("page", page.to_string()),
        ("page_size", SUB2API_USAGE_PAGE_SIZE.to_string()),
        ("start_date", start_date.to_string()),
        ("end_date", end_date.to_string()),
    ];
    if let Some(api_key_id) = api_key_id {
        query.push(("api_key_id", api_key_id.to_string()));
    }
    call_json(
        client
            .get(format!("{origin}/api/v1/usage"))
            .bearer_auth(auth_token)
            .query(&query),
    )
}

#[allow(clippy::too_many_arguments)]
fn sync_request_usage(
    storage: &Storage,
    client: &Client,
    origin: &str,
    aggregate_api_id: &str,
    api_key: Option<&str>,
    token_owner: UsageTokenOwner<'_>,
    auth_token: &mut String,
    refresh_token: &mut Option<String>,
    token_expires_at: &mut Option<i64>,
    full_sync: bool,
) -> Result<(), String> {
    let near_expiry = token_expires_at.is_some_and(|expires_at| {
        expires_at <= now_ts().saturating_add(SUB2API_TOKEN_REFRESH_AHEAD_SECS)
    }) || (token_expires_at.is_none()
        && refresh_token.as_deref().is_some_and(|value| !value.trim().is_empty()));
    if near_expiry {
        refresh_access_token(
            storage,
            client,
            origin,
            token_owner,
            auth_token,
            refresh_token,
            token_expires_at,
        )?;
    }

    let api_key_id = api_key.and_then(|api_key| {
        resolve_usage_api_key_id(
            storage,
            client,
            origin,
            aggregate_api_id,
            api_key,
            token_owner,
            auth_token,
            refresh_token,
            token_expires_at,
        )
    });

    // 中文注释：列表按 created_at 倒序，第 1 页就是最新记录；每分钟只拉第 1 页，
    // 无论历史积压多少，新记录都能在一轮内回填。整轮（每 10 分钟）再翻页补齐积压与迟到账单。
    let today = Local::now().date_naive();
    let start_date = (today - ChronoDuration::days(1)).format("%F").to_string();
    let end_date = today.format("%F").to_string();
    sync_usage_pages(
        storage,
        client,
        origin,
        aggregate_api_id,
        token_owner,
        auth_token,
        refresh_token,
        token_expires_at,
        start_date.as_str(),
        end_date.as_str(),
        api_key_id,
        full_sync,
    )
}

#[allow(clippy::too_many_arguments)]
fn sync_usage_pages(
    storage: &Storage,
    client: &Client,
    origin: &str,
    aggregate_api_id: &str,
    token_owner: UsageTokenOwner<'_>,
    auth_token: &mut String,
    refresh_token: &mut Option<String>,
    token_expires_at: &mut Option<i64>,
    start_date: &str,
    end_date: &str,
    api_key_id: Option<i64>,
    full_sync: bool,
) -> Result<(), String> {
    let mut api_key_id = api_key_id;
    let max_pages = if full_sync { SUB2API_USAGE_MAX_PAGES } else { 1 };
    let mut refreshed_after_401 = false;
    let mut processed = 0usize;
    for page in 1..=max_pages {
        let payload = match fetch_usage_page(
            client,
            origin,
            auth_token.as_str(),
            page,
            start_date,
            end_date,
            api_key_id,
        ) {
            Ok(payload) => payload,
            Err(UsageHttpError::Unauthorized) if !refreshed_after_401 => {
                refreshed_after_401 = true;
                refresh_access_token(
                    storage,
                    client,
                    origin,
                    token_owner,
                    auth_token,
                    refresh_token,
                    token_expires_at,
                )?;
                fetch_usage_page(
                    client,
                    origin,
                    auth_token.as_str(),
                    page,
                    start_date,
                    end_date,
                    api_key_id,
                )
                .map_err(error_text)?
            }
            Err(UsageHttpError::Failed(message)) if api_key_id.is_some() => {
                // 中文注释：key 过滤不被面板接受（如跨账号 key）时退回全量拉取，保证回填不中断。
                log::warn!(
                    "event=sub2api_usage_filter_dropped aggregate_api_id={} err={}",
                    aggregate_api_id,
                    message
                );
                api_key_id = None;
                fetch_usage_page(
                    client,
                    origin,
                    auth_token.as_str(),
                    page,
                    start_date,
                    end_date,
                    api_key_id,
                )
                .map_err(error_text)?
            }
            Err(error) => return Err(error_text(error)),
        };
        let items = usage_items(&payload);
        let item_count = items.len();
        for item in items {
            let Some(request_id) = read_string(field(item, &["request_id", "requestId"])) else {
                continue;
            };
            let trace_id = request_id
                .strip_prefix("client:")
                .unwrap_or(request_id.as_str())
                .trim();
            if trace_id.is_empty() {
                continue;
            }
            storage
                .update_request_log_sub2api_usage_by_trace_id(
                    aggregate_api_id,
                    trace_id,
                    read_number(field(item, &["actual_cost", "actualCost"])),
                    read_number(field(item, &["total_cost", "totalCost"])),
                    read_i64(field(item, &["duration_ms", "durationMs"])),
                    read_i64(field(item, &["first_token_ms", "firstTokenMs"])),
                    now_ts(),
                )
                .map_err(|err| format!("update request log Sub2API usage failed: {err}"))?;
        }
        processed = processed.saturating_add(item_count);
        let total = request_total(&payload);
        // 中文注释：不足一页或已覆盖 total，说明窗口内没有更多记录。
        if item_count < SUB2API_USAGE_PAGE_SIZE || total.is_some_and(|total| processed >= total) {
            return Ok(());
        }
    }
    if full_sync {
        return Err(format!(
            "Sub2API usage records exceed the sync cap of {SUB2API_USAGE_MAX_PAGES} pages"
        ));
    }
    Ok(())
}

fn sync_one_aggregate_api_usage(
    storage: &Storage,
    api: &AggregateApi,
    full_sync: bool,
) -> Result<(), String> {
    if !api.auth_type.eq_ignore_ascii_case(AGGREGATE_API_AUTH_APIKEY) {
        return Err("Sub2API usage sync only supports API key aggregate APIs".to_string());
    }
    let bound_account = api.sub2api_account_id.as_deref().and_then(|id| storage.find_sub2api_account_by_id(id).ok().flatten());
    let credential = match bound_account.as_ref() {
        Some(account) => (account.auth_token.clone(), account.refresh_token.clone(), account.token_expires_at, UsageTokenOwner::Sub2ApiAccount(account.id.as_str())),
        None => {
            let credential = storage.find_aggregate_api_usage_credential_by_id(api.id.as_str()).map_err(|err| format!("read Sub2API usage credential failed: {err}"))?.ok_or_else(|| "Sub2API usage credential is not configured".to_string())?;
            (credential.auth_token, credential.refresh_token, credential.token_expires_at, UsageTokenOwner::AggregateApi(api.id.as_str()))
        }
    };
    let api_key = storage.find_aggregate_api_secret_by_id(api.id.as_str()).ok().flatten();
    let origin = sub2api_origin_from_url(bound_account.as_ref().map(|a| a.base_url.as_str()).unwrap_or(api.url.as_str()))?;
    let client = crate::gateway::fresh_upstream_client_with_timeout(Duration::from_secs(30));
    let today = Local::now().date_naive().format("%F").to_string();
    let mut auth_token = credential.0;
    let mut refresh_token = credential.1;
    let mut token_expires_at = credential.2;

    let daily_result = if full_sync {
        match api_key.as_deref() {
            Some(api_key) if bound_account.is_none() => fetch_daily_usage(&client, origin.as_str(), api_key).and_then(|payload| persist_daily_usage(storage, api.id.as_str(), &payload, today.as_str())),
            // 中文注释：面板的 /api/v1/usage/stats 不认 period=day，会退化成近 7 天口径；改取面板首页统计，
            // 它的 today_actual_cost/today_cost/today_requests 才是当日数据（账号汇总用的也是这个接口）。
            _ => call_json(client.get(format!("{origin}/api/v1/usage/dashboard/stats")).bearer_auth(auth_token.as_str())).map_err(error_text).and_then(|payload| persist_daily_usage(storage, api.id.as_str(), &payload, today.as_str())),
        }
    } else {
        Ok(())
    };
    let request_result = sync_request_usage(
        storage,
        &client,
        origin.as_str(),
        api.id.as_str(),
        api_key.as_deref(),
        credential.3,
        &mut auth_token,
        &mut refresh_token,
        &mut token_expires_at,
        full_sync,
    );

    match (daily_result, request_result) {
        (Ok(()), Ok(())) => {
            storage
                .update_aggregate_api_usage_sync_status(api.id.as_str(), "success", None)
                .map_err(|err| format!("update Sub2API sync status failed: {err}"))?;
            Ok(())
        }
        (daily_result, request_result) => {
            let mut errors = Vec::new();
            if let Err(error) = daily_result {
                errors.push(format!("daily cost: {error}"));
            }
            if let Err(error) = request_result {
                errors.push(format!("request usage: {error}"));
            }
            let status = if errors.len() == 2 { "failed" } else { "partial" };
            let message = errors.join("; ");
            storage
                .update_aggregate_api_usage_sync_status(
                    api.id.as_str(),
                    status,
                    Some(message.as_str()),
                )
                .map_err(|err| format!("update Sub2API sync status failed: {err}"))?;
            Err(message)
        }
    }
}

pub(crate) fn save_aggregate_api_usage_credentials(
    api_id: &str,
    auth_token: Option<String>,
    refresh_token: Option<String>,
    token_expires_at: Option<i64>,
) -> Result<(), String> {
    let api_id = api_id.trim();
    if api_id.is_empty() {
        return Err("aggregate api id required".to_string());
    }
    let auth_token = normalized_non_empty(auth_token)
        .ok_or_else(|| "Sub2API auth token is required".to_string())?;
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let api = storage
        .find_aggregate_api_by_id(api_id)
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "aggregate api not found".to_string())?;
    if !api.auth_type.eq_ignore_ascii_case(AGGREGATE_API_AUTH_APIKEY) {
        return Err("Sub2API usage sync only supports API key aggregate APIs".to_string());
    }
    storage
        .upsert_aggregate_api_usage_credential(
            api_id,
            auth_token.as_str(),
            normalized_non_empty(refresh_token).as_deref(),
            normalize_token_expires_at(token_expires_at),
        )
        .map_err(|err| format!("persist Sub2API usage credential failed: {err}"))
}

pub(crate) fn read_aggregate_api_usage_summary() -> Result<AggregateApiUsageSummaryResult, String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let today = Local::now().date_naive();
    let usage_date = today.format("%F").to_string();
    let statuses = storage
        .list_aggregate_api_usage_sync_statuses()
        .map_err(|err| format!("list Sub2API usage sync status failed: {err}"))?
        .into_iter()
        .map(|status| (status.aggregate_api_id.clone(), status))
        .collect::<HashMap<_, _>>();
    let sub2api_accounts = storage
        .list_sub2api_account_summaries()
        .map_err(|err| format!("list Sub2API accounts failed: {err}"))?
        .into_iter()
        .map(|account| (account.id.clone(), account))
        .collect::<HashMap<_, _>>();
    let mut total_actual_cost = 0.0;
    let mut counted_sub2api_account_ids = std::collections::HashSet::new();
    let mut items = Vec::new();
    for api in storage
        .list_aggregate_apis()
        .map_err(|err| format!("list aggregate APIs failed: {err}"))?
    {
        let legacy_status = statuses.get(api.id.as_str());
        let bound_account = api
            .sub2api_account_id
            .as_deref()
            .and_then(|id| sub2api_accounts.get(id));
        if legacy_status.is_none() && bound_account.is_none() {
            continue;
        }
        let daily = storage
            .find_aggregate_api_daily_usage(api.id.as_str(), usage_date.as_str())
            .map_err(|err| format!("read Sub2API daily usage failed: {err}"))?;
        let actual_cost = daily.as_ref().map(|item| item.actual_cost).unwrap_or(0.0);
        let should_add_to_total = api
            .sub2api_account_id
            .as_deref()
            .map(|account_id| counted_sub2api_account_ids.insert(account_id.to_string()))
            .unwrap_or(true);
        if should_add_to_total {
            total_actual_cost += actual_cost;
        }
        items.push(AggregateApiUsageDailySummary {
            aggregate_api_id: api.id,
            supplier_name: api.supplier_name,
            url: api.url,
            usage_date: usage_date.clone(),
            actual_cost,
            total_cost: daily.as_ref().and_then(|item| item.total_cost),
            request_count: daily.as_ref().and_then(|item| item.request_count),
            synced_at: daily.as_ref().map(|item| item.synced_at),
            configured: legacy_status.is_some_and(|status| status.configured) || bound_account.is_some(),
            last_sync_at: legacy_status
                .and_then(|status| status.last_sync_at)
                .or_else(|| bound_account.and_then(|account| account.last_sync_at)),
            last_sync_status: legacy_status
                .and_then(|status| status.last_sync_status.clone())
                .or_else(|| bound_account.and_then(|account| account.last_sync_status.clone())),
            last_sync_error: legacy_status
                .and_then(|status| status.last_sync_error.clone())
                .or_else(|| bound_account.and_then(|account| account.last_sync_error.clone())),
        });
    }
    let start_at = Local
        .from_local_datetime(&today.and_hms_opt(0, 0, 0).expect("midnight should be valid"))
        .earliest()
        .expect("local midnight should resolve")
        .timestamp();
    let end_at = start_at.saturating_add(ChronoDuration::days(1).num_seconds());
    let api_keys_by_id = storage
        .list_api_keys()
        .map_err(|err| format!("list platform keys failed: {err}"))?
        .into_iter()
        .map(|key| (key.id.clone(), key))
        .collect::<HashMap<_, _>>();
    let platform_key_items = storage
        .summarize_request_log_upstream_actual_cost_by_key(start_at, end_at)
        .map_err(|err| format!("summarize platform key actual cost failed: {err}"))?
        .into_iter()
        .map(|item| {
            let key = api_keys_by_id.get(item.key_id.as_str());
            AggregateApiPlatformKeyUsageDailySummary {
                key_id: item.key_id,
                key_name: key.and_then(|key| key.name.clone()),
                group_name: key.and_then(|key| key.group_name.clone()),
                actual_cost: item.actual_cost,
                request_count: item.request_count,
            }
        })
        .collect::<Vec<_>>();
    let mapped_platform_key_actual_cost = platform_key_items
        .iter()
        .map(|item| item.actual_cost)
        .sum();
    Ok(AggregateApiUsageSummaryResult {
        usage_date,
        total_actual_cost,
        mapped_platform_key_actual_cost,
        items,
        platform_key_items,
    })
}

pub(crate) fn sync_all_aggregate_api_usage() -> Result<AggregateApiUsageSummaryResult, String> {
    let _ = crate::sub2api::sync_all_sub2api_accounts();
    sync_aggregate_api_usage(true)
}

fn sync_aggregate_api_usage(
    full_sync: bool,
) -> Result<AggregateApiUsageSummaryResult, String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let configured_ids = storage
        .list_aggregate_api_usage_sync_statuses()
        .map_err(|err| format!("list Sub2API usage sync status failed: {err}"))?
        .into_iter()
        .map(|item| item.aggregate_api_id)
        .collect::<std::collections::HashSet<_>>();
    let apis = storage
        .list_aggregate_apis()
        .map_err(|err| format!("list aggregate APIs failed: {err}"))?;
    for api in apis.into_iter().filter(|api| {
        configured_ids.contains(api.id.as_str()) || api.sub2api_account_id.is_some()
    }) {
        if let Err(error) = sync_one_aggregate_api_usage(&storage, &api, full_sync) {
            log::warn!(
                "event=sub2api_usage_sync_failed aggregate_api_id={} err={}",
                api.id,
                error
            );
        }
    }
    drop(storage);
    read_aggregate_api_usage_summary()
}

pub(crate) fn ensure_aggregate_api_usage_polling() {
    SUB2API_USAGE_POLLING_STARTED.get_or_init(|| {
        let _ = thread::Builder::new()
            .name("sub2api-usage-polling".to_string())
            .spawn(|| {
                let mut last_full_sync: Option<Instant> = None;
                loop {
                    let full_sync_due = last_full_sync
                        .map(|at| at.elapsed() >= SUB2API_FULL_SYNC_INTERVAL)
                        .unwrap_or(true);
                    if full_sync_due {
                        last_full_sync = Some(Instant::now());
                        let _ = crate::sub2api::sync_all_sub2api_accounts();
                    }
                    // 中文注释：每分钟只拉最新一页逐条回填；账号汇总、每日用量与翻页补齐随 10 分钟整轮执行。
                    if let Err(error) = sync_aggregate_api_usage(full_sync_due) {
                        log::warn!("event=sub2api_usage_polling_failed err={}", error);
                    }
                    thread::sleep(SUB2API_USAGE_POLL_INTERVAL);
                }
            });
    });
}

#[cfg(test)]
mod tests {
    use super::{
        find_api_key_id, is_success_payload, normalize_token_expires_at, persist_daily_usage,
        sub2api_origin_from_url, usage_items,
    };

    #[test]
    fn usage_origin_drops_upstream_v1_path() {
        assert_eq!(
            sub2api_origin_from_url("https://sub2api.example:8443/v1").expect("origin"),
            "https://sub2api.example:8443"
        );
    }

    #[test]
    fn api_key_id_resolves_from_plaintext_key() {
        let payload = serde_json::json!({
            "code": 0,
            "data": {
                "items": [
                    { "id": 100, "key": "sk_other" },
                    { "id": 101, "key": "sk_target" }
                ],
                "total": 2
            }
        });
        assert_eq!(find_api_key_id(&payload, "sk_target"), Some(101));
        assert_eq!(find_api_key_id(&payload, "sk_missing"), None);
    }

    #[test]
    fn token_expiry_accepts_milliseconds() {
        assert_eq!(normalize_token_expires_at(Some(1_700_000_000_000)), Some(1_700_000_000));
    }

    #[test]
    fn code_zero_is_success_but_other_codes_are_not() {
        assert!(is_success_payload(&serde_json::json!({ "code": 0 })));
        assert!(!is_success_payload(&serde_json::json!({ "code": 401 })));
    }

    #[test]
    fn daily_usage_prefers_dashboard_today_fields() {
        use codexmanager_core::storage::{AggregateApi, Storage};

        let storage = Storage::open_in_memory().expect("open in memory");
        storage.init().expect("init schema");
        storage
            .insert_aggregate_api(&AggregateApi {
                id: "agg-dashboard".to_string(),
                provider_type: "codex".to_string(),
                supported_models: Vec::new(),
                supplier_name: None,
                sort: 0,
                weight: 0,
                url: "https://sub2api.example".to_string(),
                auth_type: "apikey".to_string(),
                auth_params_json: None,
                action: None,
                status: "active".to_string(),
                created_at: 0,
                updated_at: 0,
                last_test_at: None,
                last_test_status: None,
                last_test_error: None,
                sub2api_account_id: None,
            })
            .expect("insert supplier");
        let payload = serde_json::json!({
            "code": 0,
            "data": {
                "total_cost": 102773.12,
                "total_actual_cost": 12677.57,
                "today_cost": 210.86,
                "today_actual_cost": 18.77,
                "today_requests": 1314
            }
        });
        persist_daily_usage(&storage, "agg-dashboard", &payload, "2026-09-15")
            .expect("persist daily usage");
        let row = storage
            .find_aggregate_api_daily_usage("agg-dashboard", "2026-09-15")
            .expect("read daily usage")
            .expect("daily usage row");
        assert!((row.actual_cost - 18.77).abs() < f64::EPSILON);
        assert_eq!(row.total_cost, Some(210.86));
        assert_eq!(row.request_count, Some(1314));
    }
}
