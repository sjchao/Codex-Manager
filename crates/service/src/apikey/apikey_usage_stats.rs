use std::collections::HashMap;

use chrono::{Duration, Local, LocalResult, TimeZone};
use codexmanager_core::rpc::types::ApiKeyUsageStatSummary;

use super::deepseek_pricing::estimate_deepseek_cost_cny;
use crate::storage_helpers::{open_storage, StorageHandle};

pub(crate) fn local_day_bounds_ts() -> Result<(i64, i64), String> {
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
    let deepseek_costs = read_deepseek_cost_by_key(&storage, start_ts, end_ts)?;

    Ok(items
        .into_iter()
        .map(|item| {
            let today_deepseek_cost_cny = deepseek_costs.get(&item.key_id).copied().unwrap_or(0.0);
            ApiKeyUsageStatSummary {
                key_id: item.key_id,
                today_tokens: item.today_tokens.max(0),
                total_tokens: item.total_tokens.max(0),
                today_actual_cost_usd: item.today_actual_cost_usd.max(0.0),
                actual_cost_usd: item.actual_cost_usd.max(0.0),
                today_deepseek_tokens: item.today_deepseek_tokens.max(0),
                total_deepseek_tokens: item.total_deepseek_tokens.max(0),
                today_deepseek_cost_cny: today_deepseek_cost_cny.max(0.0),
            }
        })
        .collect())
}

/// 按请求发生时间逐条估算当天 DeepSeek 费用，并按平台密钥汇总（单位：元）。
fn read_deepseek_cost_by_key(
    storage: &StorageHandle,
    start_ts: i64,
    end_ts: i64,
) -> Result<HashMap<String, f64>, String> {
    let rows = storage
        .list_deepseek_request_token_stats_between(start_ts, end_ts)
        .map_err(|err| format!("list deepseek token stats failed: {err}"))?;

    let mut costs: HashMap<String, f64> = HashMap::new();
    for row in rows {
        let Some(key_id) = row
            .key_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let cost = estimate_deepseek_cost_cny(
            row.model.as_deref().unwrap_or_default(),
            row.created_at,
            row.input_tokens.unwrap_or(0),
            row.cached_input_tokens.unwrap_or(0),
            row.output_tokens.unwrap_or(0),
        );
        if cost > 0.0 {
            *costs.entry(key_id.to_string()).or_insert(0.0) += cost;
        }
    }
    Ok(costs)
}
