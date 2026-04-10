use codexmanager_core::rpc::types::ModelOption;
use codexmanager_core::storage::now_ts;
use serde_json::json;
use tiny_http::Response;

const MODEL_CACHE_SCOPE_DEFAULT: &str = "default";
const MODELS_OWNED_BY: &str = "openai";

/// 函数 `build_openai_models_list`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - items: 参数 items
///
/// # 返回
/// 返回函数执行结果
fn build_openai_models_list(items: &[ModelOption]) -> String {
    let mut ordered = items.iter().collect::<Vec<_>>();
    ordered.sort_by(|a, b| a.slug.cmp(&b.slug));
    let data = ordered
        .iter()
        .map(|item| {
            let created = created_timestamp_for_model(item.slug.as_str());
            json!({
                "id": item.slug.as_str(),
                "object": "model",
                "created": created,
                "owned_by": MODELS_OWNED_BY,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "object": "list",
        "data": data,
    })
    .to_string()
}

/// 函数 `created_timestamp_for_model`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - slug: 参数 slug
///
/// # 返回
/// 返回函数执行结果
fn created_timestamp_for_model(slug: &str) -> i64 {
    let normalized = slug.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "gpt-5.4-mini" => 1773705600,
        "gpt-5.4-pro" | "gpt-5.4" => 1772668800,
        "gpt-5.3-codex" => 1770249600,
        "gpt-5.2-pro" | "gpt-5.2-codex" | "gpt-5.2" => 1765411200,
        "gpt-5.1-codex-mini" | "gpt-5-codex-mini" | "gpt-5.1-codex-max" | "gpt-5.1-codex"
        | "gpt-5.1" => 1762992000,
        "gpt-5-mini" | "gpt-5-nano" | "gpt-5-codex" | "gpt-5" => 1754524800,
        "gpt-4.1-nano" | "gpt-4.1-mini" | "gpt-4.1" => 1744588800,
        "o3-deep-research" => 1738454400,
        "o3-mini" => 1738281600,
        "o3" => 1744761600,
        "o3-pro" => 1749513600,
        "gpt-4o-mini" => 1721260800,
        "gpt-4o-2024-05-13" | "gpt-4o" => 1715558400,
        "gpt-4" => 1678752000,
        _ => now_ts(),
    }
}

/// 函数 `fallback_model_options`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - model_for_log: 参数 model_for_log
///
/// # 返回
/// 返回函数执行结果
fn fallback_model_options(model_for_log: Option<&str>) -> Vec<ModelOption> {
    let Some(slug) = model_for_log
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Vec::new();
    };
    vec![ModelOption {
        slug: slug.to_string(),
        display_name: slug.to_string(),
    }]
}

/// 函数 `maybe_respond_local_models`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 返回函数执行结果
pub(super) fn maybe_respond_local_models(
    request: tiny_http::Request,
    trace_id: &str,
    key_id: &str,
    protocol_type: &str,
    original_path: &str,
    path: &str,
    response_adapter: super::ResponseAdapter,
    request_method: &str,
    model_for_log: Option<&str>,
    reasoning_for_log: Option<&str>,
    queue_wait_ms: Option<u128>,
    storage: &codexmanager_core::storage::Storage,
) -> Result<Option<tiny_http::Request>, String> {
    let is_models_list = request_method.eq_ignore_ascii_case("GET")
        && (path == "/v1/models" || path.starts_with("/v1/models?"));
    if !is_models_list {
        return Ok(Some(request));
    }

    let mut fallback_reason: Option<String> = None;
    let cached_items = match storage.get_model_options_cache(MODEL_CACHE_SCOPE_DEFAULT) {
        Ok(Some(record)) => {
            serde_json::from_str::<Vec<ModelOption>>(&record.items_json).unwrap_or_default()
        }
        Ok(None) => Vec::new(),
        Err(err) => {
            let message = format!("model options cache read failed: {err}");
            super::trace_log::log_attempt_result(trace_id, "-", None, 503, Some(message.as_str()));
            super::trace_log::log_request_final(
                trace_id,
                503,
                None,
                None,
                Some(message.as_str()),
                0,
            );
            super::record_gateway_request_outcome(path, 503, Some(protocol_type));
            super::write_request_log(
                storage,
                super::request_log::RequestLogTraceContext {
                    trace_id: Some(trace_id),
                    original_path: Some(original_path),
                    adapted_path: Some(path),
                    queue_wait_ms,
                    response_adapter: Some(response_adapter),
                    ..Default::default()
                },
                Some(key_id),
                None,
                path,
                request_method,
                model_for_log,
                reasoning_for_log,
                None,
                Some(503),
                super::request_log::RequestLogUsage::default(),
                Some(message.as_str()),
                None,
            );
            let response =
                super::error_response::terminal_text_response(503, message, Some(trace_id));
            let _ = request.respond(response);
            return Ok(None);
        }
    };

    let items = if !cached_items.is_empty() {
        cached_items
    } else {
        match super::fetch_models_for_picker() {
            Ok(fetched) if !fetched.is_empty() => {
                if let Ok(items_json) = serde_json::to_string(&fetched) {
                    if let Err(err) = storage.upsert_model_options_cache(
                        MODEL_CACHE_SCOPE_DEFAULT,
                        items_json.as_str(),
                        now_ts(),
                    ) {
                        log::warn!(
                            "event=gateway_model_options_cache_upsert_failed scope={} err={}",
                            MODEL_CACHE_SCOPE_DEFAULT,
                            err
                        );
                    }
                }
                fetched
            }
            Ok(_) => {
                let message = "models refresh returned empty list".to_string();
                fallback_reason = Some(message);
                fallback_model_options(model_for_log)
            }
            Err(err) => {
                let message = format!("models refresh failed: {err}");
                fallback_reason = Some(message);
                fallback_model_options(model_for_log)
            }
        }
    };

    let output = build_openai_models_list(&items);
    super::trace_log::log_attempt_result(trace_id, "-", None, 200, None);
    super::trace_log::log_request_final(trace_id, 200, None, None, None, 0);
    super::record_gateway_request_outcome(path, 200, Some(protocol_type));
    super::write_request_log(
        storage,
        super::request_log::RequestLogTraceContext {
            trace_id: Some(trace_id),
            original_path: Some(original_path),
            adapted_path: Some(path),
            queue_wait_ms,
            response_adapter: Some(response_adapter),
            ..Default::default()
        },
        Some(key_id),
        None,
        path,
        request_method,
        model_for_log,
        reasoning_for_log,
        None,
        Some(200),
        super::request_log::RequestLogUsage::default(),
        fallback_reason.as_deref(),
        None,
    );
    let response = super::error_response::with_trace_id_header(
        Response::from_string(output)
            .with_status_code(200)
            .with_header(
                tiny_http::Header::from_bytes(
                    b"content-type".as_slice(),
                    b"application/json".as_slice(),
                )
                .map_err(|_| "build content-type header failed".to_string())?,
            ),
        Some(trace_id),
    );
    let _ = request.respond(response);
    Ok(None)
}

#[cfg(test)]
#[path = "tests/local_models_tests.rs"]
mod tests;
