use codexmanager_core::rpc::types::{
    RequestTokenUsageByModel, RequestTokenUsageByModelListResult,
};

use crate::{apikey_usage_stats::local_day_bounds_ts, storage_helpers::open_storage};

/// 函数 `read_requestlog_model_usage`
///
/// 作者: gaohongshun
///
/// 时间: 2026-09-17
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn read_requestlog_model_usage() -> Result<RequestTokenUsageByModelListResult, String>
{
    let storage = open_storage().ok_or_else(|| "open storage failed".to_string())?;
    let (start_ts, end_ts) = local_day_bounds_ts()?;
    let items = storage
        .summarize_request_token_stats_by_model(start_ts, end_ts)
        .map_err(|err| format!("summarize request token stats by model failed: {err}"))?;

    Ok(RequestTokenUsageByModelListResult {
        items: items
            .into_iter()
            .map(|item| RequestTokenUsageByModel {
                model: item.model,
                request_count: item.request_count.max(0),
                input_tokens: item.input_tokens.max(0),
                cached_input_tokens: item.cached_input_tokens.max(0),
                output_tokens: item.output_tokens.max(0),
                total_tokens: item.total_tokens.max(0),
            })
            .collect(),
    })
}
