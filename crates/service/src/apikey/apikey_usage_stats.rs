use chrono::{Duration, Local, LocalResult, TimeZone};
use codexmanager_core::rpc::types::ApiKeyUsageStatSummary;

use crate::storage_helpers::open_storage;

fn local_day_bounds_ts() -> Result<(i64, i64), String> {
    let now = Local::now();
    let today = now.date_naive();
    let start_naive = today
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| "build local start-of-day failed".to_string())?;
    let tomorrow_naive = (today + Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| "build local end-of-day failed".to_string())?;

    let start = match Local.from_local_datetime(&start_naive) {
        LocalResult::Single(value) => value.timestamp(),
        LocalResult::Ambiguous(a, b) => a.timestamp().min(b.timestamp()),
        LocalResult::None => now.timestamp(),
    };
    let end = match Local.from_local_datetime(&tomorrow_naive) {
        LocalResult::Single(value) => value.timestamp(),
        LocalResult::Ambiguous(a, b) => a.timestamp().max(b.timestamp()),
        LocalResult::None => start + 24 * 60 * 60,
    };
    Ok((start, end.max(start)))
}

/// 函数 `read_api_key_usage_stats`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn read_api_key_usage_stats() -> Result<Vec<ApiKeyUsageStatSummary>, String> {
    let storage = open_storage().ok_or_else(|| "open storage failed".to_string())?;
    let (start_ts, end_ts) = local_day_bounds_ts()?;
    let items = storage
        .summarize_request_token_stats_by_key(start_ts, end_ts)
        .map_err(|err| format!("summarize api key token stats failed: {err}"))?;

    Ok(items
        .into_iter()
        .map(|item| ApiKeyUsageStatSummary {
            key_id: item.key_id,
            today_tokens: item.today_tokens.max(0),
            total_tokens: item.total_tokens.max(0),
            today_estimated_cost_usd: item.today_estimated_cost_usd.max(0.0),
            estimated_cost_usd: item.estimated_cost_usd.max(0.0),
        })
        .collect())
}
