use crate::gateway::error_log::GatewayErrorLogInput;
use crate::gateway::ModelType;
use codexmanager_core::storage::{now_ts, RequestLog, RequestTokenStat, Storage};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RequestLogUsage {
    pub input_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub reasoning_output_tokens: Option<i64>,
    pub first_response_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AggregateApiAttemptFailure {
    pub aggregate_api_id: Option<String>,
    pub supplier_name: Option<String>,
    pub status_code: Option<i64>,
    pub error: String,
}

fn cleanup_uncatalogued_image_results(image_results_json: Option<&str>) {
    let Some(image_results_json) = image_results_json.filter(|value| !value.trim().is_empty()) else {
        return;
    };
    let db_path = match std::env::var("CODEXMANAGER_DB_PATH") {
        Ok(path) => path,
        Err(_) => {
            log::warn!("event=request_log_image_cleanup_skipped reason=database_path_missing");
            return;
        }
    };
    if let Err(err) = crate::requestlog::image_assets::clear_image_results(
        Path::new(&db_path),
        [Some(image_results_json.to_string())],
    ) {
        log::warn!(
            "event=request_log_image_cleanup_failed reason=request_log_write_failed err={}",
            err
        );
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RequestLogTraceContext<'a> {
    pub trace_id: Option<&'a str>,
    pub original_path: Option<&'a str>,
    pub adapted_path: Option<&'a str>,
    pub request_type: Option<&'a str>,
    pub model_type: Option<ModelType>,
    pub image_count: Option<i64>,
    pub image_size: Option<&'a str>,
    pub image_results_json: Option<&'a str>,
    pub service_tier: Option<&'a str>,
    pub effective_service_tier: Option<&'a str>,
    pub queue_wait_ms: Option<u128>,
    pub response_adapter: Option<super::ResponseAdapter>,
    pub aggregate_api_supplier_name: Option<&'a str>,
    pub aggregate_api_url: Option<&'a str>,
    pub aggregate_api_id: Option<&'a str>,
    pub upstream_client_request_id: Option<&'a str>,
    pub attempted_aggregate_api_ids: Option<&'a [String]>,
    pub aggregate_api_attempt_failures: Option<&'a [AggregateApiAttemptFailure]>,
}

/// 函数 `normalize_token`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - value: 参数 value
///
/// # 返回
/// 返回函数执行结果
fn normalize_token(value: Option<i64>) -> Option<i64> {
    value.map(|v| v.max(0))
}

/// 函数 `normalize_duration_ms`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - value: 参数 value
///
/// # 返回
/// 返回函数执行结果
fn normalize_duration_ms(value: Option<u128>) -> Option<i64> {
    value.map(|duration| duration.min(i64::MAX as u128) as i64)
}

/// 函数 `is_inference_path`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - path: 参数 path
///
/// # 返回
/// 返回函数执行结果
fn is_inference_path(path: &str) -> bool {
    path.starts_with("/v1/responses")
        || path.starts_with("/v1/chat/completions")
        || path.starts_with("/v1/messages")
}

fn should_write_gateway_error_fallback(status_code: Option<u16>, error: Option<&str>) -> bool {
    let Some(status_code) = status_code else {
        return false;
    };
    if !matches!(status_code, 401 | 403 | 429) {
        return false;
    }
    let Some(error) = error.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };
    let normalized = error.to_ascii_lowercase();
    normalized.contains("cloudflare")
        || normalized.contains("cf_ray=")
        || normalized.contains("cf-ray")
        || normalized.contains("challenge")
        || normalized.contains("just a moment")
        || normalized.contains("usage_limit_reached")
}

/// 函数 `response_adapter_label`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - value: 参数 value
///
/// # 返回
/// 返回函数执行结果
fn response_adapter_label(value: super::ResponseAdapter) -> &'static str {
    match value {
        super::ResponseAdapter::Passthrough => "Passthrough",
        super::ResponseAdapter::AnthropicJson => "AnthropicJson",
        super::ResponseAdapter::AnthropicSse => "AnthropicSse",
        super::ResponseAdapter::GeminiJson => "GeminiJson",
        super::ResponseAdapter::GeminiSse => "GeminiSse",
        super::ResponseAdapter::GeminiCliJson => "GeminiCliJson",
        super::ResponseAdapter::GeminiCliSse => "GeminiCliSse",
        super::ResponseAdapter::OpenAIChatCompletionsJson => "OpenAIChatCompletionsJson",
        super::ResponseAdapter::OpenAIChatCompletionsSse => "OpenAIChatCompletionsSse",
        super::ResponseAdapter::OpenAICompletionsJson => "OpenAICompletionsJson",
        super::ResponseAdapter::OpenAICompletionsSse => "OpenAICompletionsSse",
    }
}

/// 函数 `write_request_log`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 无
pub(crate) fn write_request_log(
    storage: &Storage,
    trace_context: RequestLogTraceContext<'_>,
    key_id: Option<&str>,
    account_id: Option<&str>,
    request_path: &str,
    method: &str,
    model: Option<&str>,
    reasoning_effort: Option<&str>,
    upstream_url: Option<&str>,
    status_code: Option<u16>,
    usage: RequestLogUsage,
    error: Option<&str>,
    duration_ms: Option<u128>,
) {
    write_request_log_with_attempts(
        storage,
        trace_context,
        key_id,
        account_id,
        request_path,
        method,
        model,
        reasoning_effort,
        upstream_url,
        status_code,
        usage,
        error,
        duration_ms,
        None,
    );
}

/// 函数 `write_request_log_with_attempts`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 无
#[allow(clippy::too_many_arguments)]
pub(crate) fn write_request_log_with_attempts(
    storage: &Storage,
    trace_context: RequestLogTraceContext<'_>,
    key_id: Option<&str>,
    account_id: Option<&str>,
    request_path: &str,
    method: &str,
    model: Option<&str>,
    reasoning_effort: Option<&str>,
    upstream_url: Option<&str>,
    status_code: Option<u16>,
    usage: RequestLogUsage,
    error: Option<&str>,
    duration_ms: Option<u128>,
    attempted_account_ids: Option<&[String]>,
) {
    let original_path = trace_context.original_path.unwrap_or(request_path);
    let adapted_path = trace_context.adapted_path.unwrap_or(request_path);
    let initial_account_id = attempted_account_ids
        .and_then(|items| items.first())
        .map(String::as_str);
    let attempted_account_ids_json = attempted_account_ids
        .filter(|items| !items.is_empty())
        .and_then(|items| serde_json::to_string(items).ok());
    let initial_aggregate_api_id = trace_context
        .attempted_aggregate_api_ids
        .and_then(|items| items.first())
        .map(String::as_str);
    let attempted_aggregate_api_ids_json = trace_context
        .attempted_aggregate_api_ids
        .filter(|items| !items.is_empty())
        .and_then(|items| serde_json::to_string(items).ok());
    let aggregate_api_attempt_failures_json = trace_context
        .aggregate_api_attempt_failures
        .filter(|items| !items.is_empty())
        .and_then(|items| serde_json::to_string(items).ok());
    let input_tokens = normalize_token(usage.input_tokens);
    let cached_input_tokens = normalize_token(usage.cached_input_tokens);
    let output_tokens = normalize_token(usage.output_tokens);
    let total_tokens = normalize_token(usage.total_tokens);
    let reasoning_output_tokens = normalize_token(usage.reasoning_output_tokens);
    let duration_ms = normalize_duration_ms(duration_ms);
    let first_response_ms = usage.first_response_ms.map(|value| value.max(0));
    let queue_wait_ms = normalize_duration_ms(trace_context.queue_wait_ms);
    let created_at = now_ts();
    let request_type = trace_context
        .request_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("http");
    let model_type = trace_context.model_type.unwrap_or(ModelType::Text);
    let (image_count, image_size) = if model_type == ModelType::Image {
        (
            Some(trace_context.image_count.unwrap_or(1).max(1)),
            trace_context
                .image_size
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        )
    } else {
        (None, None)
    };
    let service_tier = trace_context
        .service_tier
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let effective_service_tier = trace_context
        .effective_service_tier
        .map(str::trim)
        .filter(|value| !value.is_empty());
    super::trace_log::log_failed_request(
        created_at,
        trace_context.trace_id,
        key_id,
        account_id,
        method,
        request_path,
        Some(original_path),
        Some(adapted_path),
        Some(request_type),
        model,
        reasoning_effort,
        service_tier,
        upstream_url,
        status_code,
        error,
        duration_ms,
    );
    let success = status_code
        .map(|status| (200..300).contains(&status))
        .unwrap_or(false);
    let input_zero_or_missing = input_tokens.unwrap_or(0) == 0;
    let cached_zero_or_missing = cached_input_tokens.unwrap_or(0) == 0;
    let output_zero_or_missing = output_tokens.unwrap_or(0) == 0;
    let total_zero_or_missing = total_tokens.unwrap_or(0) == 0;
    let reasoning_zero_or_missing = reasoning_output_tokens.unwrap_or(0) == 0;
    if success
        && is_inference_path(request_path)
        && input_zero_or_missing
        && cached_zero_or_missing
        && output_zero_or_missing
        && total_zero_or_missing
        && reasoning_zero_or_missing
    {
        log::warn!(
            "event=gateway_token_usage_missing path={} status={} account_id={} key_id={} model={}",
            request_path,
            status_code.unwrap_or(0),
            account_id.unwrap_or("-"),
            key_id.unwrap_or("-"),
            model.unwrap_or("-"),
        );
    }
    // 记录请求最终结果（而非内部重试明细），保证 UI 一次请求只展示一条记录。
    let (request_log_id, token_stat_error) = match storage.insert_request_log_with_token_stat(
        &RequestLog {
            trace_id: trace_context.trace_id.map(|v| v.to_string()),
            key_id: key_id.map(|v| v.to_string()),
            account_id: account_id.map(|v| v.to_string()),
            initial_account_id: initial_account_id.map(str::to_string),
            attempted_account_ids_json,
            initial_aggregate_api_id: initial_aggregate_api_id.map(str::to_string),
            aggregate_api_id: trace_context.aggregate_api_id.map(str::to_string),
            attempted_aggregate_api_ids_json,
            aggregate_api_attempt_failures_json,
            request_path: request_path.to_string(),
            original_path: Some(original_path.to_string()),
            adapted_path: Some(adapted_path.to_string()),
            method: method.to_string(),
            request_type: Some(request_type.to_string()),
            model: model.map(|v| v.to_string()),
            model_type: Some(model_type.as_str().to_string()),
            image_count,
            image_size,
            image_results_json: trace_context.image_results_json.map(str::to_string),
            reasoning_effort: reasoning_effort.map(|v| v.to_string()),
            service_tier: service_tier.map(str::to_string),
            effective_service_tier: effective_service_tier.map(str::to_string),
            response_adapter: trace_context
                .response_adapter
                .map(response_adapter_label)
                .map(str::to_string),
            upstream_url: upstream_url.map(|v| v.to_string()),
            aggregate_api_supplier_name: trace_context
                .aggregate_api_supplier_name
                .map(str::to_string),
            aggregate_api_url: trace_context.aggregate_api_url.map(str::to_string),
            status_code: status_code.map(|v| i64::from(v)),
            duration_ms,
            first_response_ms,
            queue_wait_ms,
            upstream_actual_cost: None,
            upstream_total_cost: None,
            upstream_duration_ms: None,
            upstream_first_response_ms: None,
            upstream_usage_synced_at: None,
            upstream_client_request_id: trace_context
                .upstream_client_request_id
                .map(str::to_string),
            input_tokens: None,
            cached_input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            reasoning_output_tokens: None,
            error: error.map(|v| v.to_string()),
            created_at,
        },
        &RequestTokenStat {
            request_log_id: 0,
            key_id: key_id.map(|v| v.to_string()),
            account_id: account_id.map(|v| v.to_string()),
            model: model.map(|v| v.to_string()),
            input_tokens,
            cached_input_tokens,
            output_tokens,
            total_tokens,
            reasoning_output_tokens,
            created_at,
        },
    ) {
        Ok(result) => result,
        Err(err) => {
            let err_text = err.to_string();
            cleanup_uncatalogued_image_results(trace_context.image_results_json);
            super::metrics::record_db_error(err_text.as_str());
            log::error!(
                "event=gateway_request_log_insert_failed path={} status={} account_id={} key_id={} err={}",
                request_path,
                status_code.unwrap_or(0),
                account_id.unwrap_or("-"),
                key_id.unwrap_or("-"),
                err_text
            );
            return;
        }
    };

    if let Some(err) = token_stat_error {
        let err_text = err.to_string();
        super::metrics::record_db_error(err_text.as_str());
        log::error!(
            "event=gateway_request_token_stat_insert_failed path={} status={} account_id={} key_id={} request_log_id={} err={}",
            request_path,
            status_code.unwrap_or(0),
            account_id.unwrap_or("-"),
            key_id.unwrap_or("-"),
            request_log_id,
            err_text
        );
    }

    if should_write_gateway_error_fallback(status_code, error) {
        crate::gateway::write_gateway_error_log(GatewayErrorLogInput {
            trace_id: trace_context.trace_id,
            key_id,
            account_id,
            request_path,
            method,
            stage: "request_log_fallback_non_success",
            upstream_url,
            status_code,
            compression_enabled: false,
            compression_retry_attempted: false,
            message: error.unwrap_or("gateway non-success"),
            ..GatewayErrorLogInput::default()
        });
    }
}

#[cfg(test)]
#[path = "tests/request_log_tests.rs"]
mod tests;
