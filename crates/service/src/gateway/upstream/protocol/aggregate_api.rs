use bytes::Bytes;
use codexmanager_core::storage::{AggregateApi, Storage};
use reqwest::header::{HeaderName, HeaderValue};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use tiny_http::Request;

use crate::aggregate_api::{
    AGGREGATE_API_AUTH_APIKEY, AGGREGATE_API_AUTH_USERPASS, AGGREGATE_API_PROVIDER_CLAUDE,
    AGGREGATE_API_PROVIDER_CODEX, AGGREGATE_API_STATUS_ACTIVE,
};
use crate::gateway::request_log::{AggregateApiAttemptFailure, RequestLogUsage};

const AGGREGATE_API_RETRY_ATTEMPTS_PER_CHANNEL: usize = 3;
static AGGREGATE_API_BUCKET_ROUTE_STATE: OnceLock<Mutex<HashMap<String, HashMap<String, i64>>>> =
    OnceLock::new();

fn should_retry_same_aggregate_candidate(status_code: u16) -> bool {
    !matches!(status_code, 403 | 409)
}

fn should_failover_after_aggregate_bridge(
    error: Option<&str>,
    has_more_candidates: bool,
    request_preserved: bool,
) -> bool {
    request_preserved
        && error.is_some_and(|error| {
            crate::account_status::should_failover_for_gateway_error(error, has_more_candidates)
        })
}

fn push_aggregate_api_attempt_failure(
    failures: &mut Vec<AggregateApiAttemptFailure>,
    candidate: &AggregateApi,
    supplier_name: Option<&str>,
    status_code: u16,
    error: Option<&str>,
) {
    let Some(error) = error.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    failures.push(AggregateApiAttemptFailure {
        aggregate_api_id: Some(candidate.id.clone()),
        supplier_name: supplier_name
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        status_code: Some(i64::from(status_code)),
        error: error.to_string(),
    });
}

fn is_aggregate_api_active(status: &str) -> bool {
    status
        .trim()
        .eq_ignore_ascii_case(AGGREGATE_API_STATUS_ACTIVE)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiKeyAuthParams {
    location: String,
    name: String,
    #[serde(default)]
    header_value_format: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserPassAuthParams {
    mode: String,
    #[serde(default)]
    username_name: Option<String>,
    #[serde(default)]
    password_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserPassSecret {
    username: String,
    password: String,
}

#[derive(Debug, Clone)]
enum AggregateApiAuthConfig {
    ApiKeyDefaultBearer,
    ApiKeyHeader { name: String, format: String },
    ApiKeyQuery { name: String },
    UserPassBasic,
    UserPassHeaderPair {
        username_name: String,
        password_name: String,
    },
    UserPassQueryPair {
        username_name: String,
        password_name: String,
    },
}

fn normalize_header_key(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

fn normalize_action_path(action: &str) -> String {
    let action_trimmed = action.trim();
    if action_trimmed.is_empty() {
        return String::new();
    }
    if action_trimmed.starts_with('/') {
        action_trimmed.to_string()
    } else {
        format!("/{action_trimmed}")
    }
}

fn effective_action_path(candidate: &AggregateApi, path: &str) -> String {
    match candidate.action.as_deref().map(str::trim) {
        Some("") => path.to_string(),
        Some(value) => normalize_action_path(value),
        None => path.to_string(),
    }
}

fn build_upstream_url(base_url: &str, effective_path: &str) -> Result<reqwest::Url, ()> {
    let mut url = reqwest::Url::parse(base_url).map_err(|_| ())?;
    let trimmed_path = effective_path.trim();
    if trimmed_path.is_empty() {
        return Ok(url);
    }
    let (path_part, query_part) = trimmed_path
        .split_once('?')
        .map_or((trimmed_path, None), |(path, query)| (path, Some(query)));
    let suffix = path_part.trim_start_matches('/');
    let base_path = url.path().trim_end_matches('/').to_string();
    let combined_path = if base_path.is_empty() || base_path == "/" {
        format!("/{}", suffix)
    } else if suffix.is_empty() {
        base_path
    } else {
        format!("{}/{}", base_path, suffix)
    };
    url.set_path(combined_path.as_str());
    url.set_query(query_part.filter(|query| !query.trim().is_empty()));
    Ok(url)
}

fn replace_query_param(mut url: reqwest::Url, name: &str, value: &str) -> reqwest::Url {
    let name_trimmed = name.trim();
    if name_trimmed.is_empty() {
        return url;
    }
    let existing = url.query_pairs().into_owned().collect::<Vec<_>>();
    url.set_query(None);
    {
        let mut qp = url.query_pairs_mut();
        for (k, v) in existing {
            if k == name_trimmed {
                continue;
            }
            qp.append_pair(k.as_str(), v.as_str());
        }
        qp.append_pair(name_trimmed, value);
    }
    url
}

fn parse_auth_config(candidate: &AggregateApi) -> Result<(AggregateApiAuthConfig, HashSet<String>), String> {
    let auth_type = candidate.auth_type.trim().to_ascii_lowercase();
    let raw_params = candidate
        .auth_params_json
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let mut injected_headers = HashSet::new();

    if raw_params.is_none() {
        if auth_type == AGGREGATE_API_AUTH_USERPASS {
            return Ok((AggregateApiAuthConfig::UserPassBasic, injected_headers));
        }
        return Ok((AggregateApiAuthConfig::ApiKeyDefaultBearer, injected_headers));
    }

    let value: serde_json::Value = serde_json::from_str(raw_params.unwrap())
        .map_err(|_| "invalid aggregate api authParams".to_string())?;

    if auth_type == AGGREGATE_API_AUTH_APIKEY {
        let parsed: ApiKeyAuthParams = serde_json::from_value(value)
            .map_err(|_| "invalid aggregate api authParams".to_string())?;
        let location = parsed.location.trim().to_ascii_lowercase();
        if location == "query" {
            return Ok((
                AggregateApiAuthConfig::ApiKeyQuery {
                    name: parsed.name.trim().to_string(),
                },
                injected_headers,
            ));
        }
        let header_name = parsed.name.trim().to_string();
        injected_headers.insert(normalize_header_key(header_name.as_str()));
        let format = parsed
            .header_value_format
            .as_deref()
            .unwrap_or("bearer")
            .trim()
            .to_ascii_lowercase();
        return Ok((
            AggregateApiAuthConfig::ApiKeyHeader {
                name: header_name,
                format,
            },
            injected_headers,
        ));
    }

    if auth_type == AGGREGATE_API_AUTH_USERPASS {
        let parsed: UserPassAuthParams = serde_json::from_value(value)
            .map_err(|_| "invalid aggregate api authParams".to_string())?;
        let mode = parsed.mode.trim().to_ascii_lowercase();
        match mode.as_str() {
            "basic" => return Ok((AggregateApiAuthConfig::UserPassBasic, injected_headers)),
            "headerpair" => {
                let username_name = parsed
                    .username_name
                    .as_deref()
                    .unwrap_or("username")
                    .trim()
                    .to_string();
                let password_name = parsed
                    .password_name
                    .as_deref()
                    .unwrap_or("password")
                    .trim()
                    .to_string();
                injected_headers.insert(normalize_header_key(username_name.as_str()));
                injected_headers.insert(normalize_header_key(password_name.as_str()));
                return Ok((
                    AggregateApiAuthConfig::UserPassHeaderPair {
                        username_name,
                        password_name,
                    },
                    injected_headers,
                ));
            }
            "querypair" => {
                let username_name = parsed
                    .username_name
                    .as_deref()
                    .unwrap_or("username")
                    .trim()
                    .to_string();
                let password_name = parsed
                    .password_name
                    .as_deref()
                    .unwrap_or("password")
                    .trim()
                    .to_string();
                return Ok((
                    AggregateApiAuthConfig::UserPassQueryPair {
                        username_name,
                        password_name,
                    },
                    injected_headers,
                ));
            }
            _ => return Err("invalid aggregate api authParams".to_string()),
        }
    }

    Ok((AggregateApiAuthConfig::ApiKeyDefaultBearer, injected_headers))
}

fn resolve_passthrough_sse_protocol(
    candidate: &AggregateApi,
    path: &str,
    response_adapter: super::super::super::ResponseAdapter,
) -> Option<super::super::super::PassthroughSseProtocol> {
    if response_adapter != super::super::super::ResponseAdapter::Passthrough {
        return None;
    }
    let provider_type = normalize_provider_type_value(candidate.provider_type.as_str());
    if provider_type != AGGREGATE_API_PROVIDER_CLAUDE {
        return None;
    }
    if path == "/v1/messages" || path.starts_with("/v1/messages?") {
        return Some(super::super::super::PassthroughSseProtocol::AnthropicNative);
    }
    None
}

/// 函数 `should_skip_forward_header`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - name: 参数 name
///
/// # 返回
/// 返回函数执行结果
fn should_skip_forward_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization"
            | "x-api-key"
            | "api-key"
            | "content-length"
            | "connection"
            | "proxy-authorization"
            | "proxy-authenticate"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "host"
    )
}

fn should_skip_forward_header_with_overrides(name: &str, injected: &HashSet<String>) -> bool {
    if should_skip_forward_header(name) {
        return true;
    }
    injected.contains(normalize_header_key(name).as_str())
}

/// 函数 `respond_error`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - request: 参数 request
/// - status: 参数 status
/// - message: 参数 message
/// - trace_id: 参数 trace_id
///
/// # 返回
/// 无
fn respond_error(request: Request, status: u16, message: &str, trace_id: Option<&str>) {
    let response = super::super::super::error_response::terminal_text_response(
        status,
        message.to_string(),
        trace_id,
    );
    let _ = request.respond(response);
}

/// 函数 `normalize_candidate_order`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - candidates: 参数 candidates
///
/// # 返回
/// 返回函数执行结果
fn normalize_candidate_order(mut candidates: Vec<AggregateApi>) -> Vec<AggregateApi> {
    candidates.sort_by(|left, right| {
        left.sort
            .cmp(&right.sort)
            .then(right.weight.cmp(&left.weight))
            .then(right.created_at.cmp(&left.created_at))
            .then(left.id.cmp(&right.id))
    });
    candidates
}

fn aggregate_api_weight(candidate: &AggregateApi) -> i64 {
    candidate.weight.max(1)
}

fn aggregate_api_bucket_signature(candidates: &[AggregateApi]) -> String {
    let mut signature = String::new();
    for candidate in candidates {
        if !signature.is_empty() {
            signature.push('\u{1f}');
        }
        signature.push_str(candidate.id.as_str());
        signature.push(':');
        signature.push_str(candidate.sort.to_string().as_str());
        signature.push(':');
        signature.push_str(aggregate_api_weight(candidate).to_string().as_str());
    }
    signature
}

fn select_smooth_weighted_candidate_position(
    entries: &[(usize, &AggregateApi)],
    currents: &mut HashMap<String, i64>,
) -> Option<usize> {
    if entries.is_empty() {
        return None;
    }

    let total_weight = entries.iter().fold(0_i64, |sum, (_, candidate)| {
        sum.saturating_add(aggregate_api_weight(candidate))
    });
    let mut selected_position = 0usize;
    let mut best_score = i64::MIN;
    let mut best_original_index = usize::MAX;

    for (position, (original_index, candidate)) in entries.iter().enumerate() {
        let score = {
            let current = currents.entry(candidate.id.clone()).or_insert(0);
            *current = current.saturating_add(aggregate_api_weight(candidate));
            *current
        };
        if score > best_score || (score == best_score && *original_index < best_original_index) {
            best_score = score;
            best_original_index = *original_index;
            selected_position = position;
        }
    }

    let selected_id = entries[selected_position].1.id.clone();
    if let Some(current) = currents.get_mut(selected_id.as_str()) {
        *current = current.saturating_sub(total_weight);
    }
    Some(selected_position)
}

fn build_weighted_bucket_order(candidates: &[AggregateApi]) -> Vec<String> {
    if candidates.len() <= 1 {
        return candidates.iter().map(|candidate| candidate.id.clone()).collect();
    }

    let signature = aggregate_api_bucket_signature(candidates);
    let route_state = AGGREGATE_API_BUCKET_ROUTE_STATE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = crate::lock_utils::lock_recover(
        route_state,
        "aggregate_api_bucket_route_state",
    );
    let bucket_state = guard.entry(signature).or_default();
    bucket_state.retain(|candidate_id, _| {
        candidates.iter().any(|candidate| candidate.id == *candidate_id)
    });
    for candidate in candidates {
        bucket_state.entry(candidate.id.clone()).or_insert(0);
    }

    let entries = candidates.iter().enumerate().collect::<Vec<_>>();
    let mut persisted_currents = bucket_state.clone();
    let Some(first_position) =
        select_smooth_weighted_candidate_position(entries.as_slice(), &mut persisted_currents)
    else {
        return candidates.iter().map(|candidate| candidate.id.clone()).collect();
    };
    *bucket_state = persisted_currents.clone();
    drop(guard);

    let first_index = entries[first_position].0;
    let mut order = Vec::with_capacity(candidates.len());
    order.push(candidates[first_index].id.clone());

    let mut remaining = candidates
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != first_index)
        .collect::<Vec<_>>();
    let mut local_currents = persisted_currents;
    while !remaining.is_empty() {
        let Some(next_position) =
            select_smooth_weighted_candidate_position(remaining.as_slice(), &mut local_currents)
        else {
            break;
        };
        let (_, selected) = remaining.remove(next_position);
        local_currents.remove(selected.id.as_str());
        order.push(selected.id.clone());
    }

    order
}

fn apply_weighted_order_within_same_sort_groups(
    candidates: &mut [AggregateApi],
    preserve_head: bool,
) {
    let mut start = if preserve_head { 1 } else { 0 };
    while start < candidates.len() {
        let current_sort = candidates[start].sort;
        let mut end = start + 1;
        while end < candidates.len() && candidates[end].sort == current_sort {
            end += 1;
        }

        if end - start > 1 {
            let order = build_weighted_bucket_order(&candidates[start..end]);
            let order_index = order
                .into_iter()
                .enumerate()
                .map(|(index, candidate_id)| (candidate_id, index))
                .collect::<HashMap<_, _>>();
            let mut reordered = candidates[start..end].to_vec();
            reordered.sort_by_key(|candidate| {
                order_index
                    .get(candidate.id.as_str())
                    .copied()
                    .unwrap_or(usize::MAX)
            });
            candidates[start..end].clone_from_slice(reordered.as_slice());
        }

        start = end;
    }
}

#[cfg(test)]
fn clear_aggregate_api_bucket_route_state_for_tests() {
    if let Some(lock) = AGGREGATE_API_BUCKET_ROUTE_STATE.get() {
        let mut guard =
            crate::lock_utils::lock_recover(lock, "aggregate_api_bucket_route_state");
        guard.clear();
    }
}

/// 函数 `apply_gateway_route_strategy_to_aggregate_candidates`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 无
pub(crate) fn apply_gateway_route_strategy_to_aggregate_candidates(
    candidates: &mut [AggregateApi],
    _key_id: &str,
    _model: Option<&str>,
    preferred_aggregate_api_id: Option<&str>,
) {
    if candidates.len() <= 1 {
        return;
    }

    let preferred_id = preferred_aggregate_api_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let preserves_head = preferred_id
        .and_then(|preferred_id| candidates.first().map(|first| (preferred_id, first)))
        .is_some_and(|(preferred_id, first)| first.id == preferred_id);

    apply_weighted_order_within_same_sort_groups(candidates, preserves_head);
}

/// 函数 `normalize_provider_type_value`
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
fn normalize_provider_type_value(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase().replace('-', "_");
    match normalized.as_str() {
        "claude" | "anthropic" | "anthropic_native" | "claude_code" => {
            AGGREGATE_API_PROVIDER_CLAUDE.to_string()
        }
        _ => AGGREGATE_API_PROVIDER_CODEX.to_string(),
    }
}

/// 函数 `first_upstream_header`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - headers: 参数 headers
/// - names: 参数 names
///
/// # 返回
/// 返回函数执行结果
fn first_upstream_header(headers: &reqwest::header::HeaderMap, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        headers
            .get(*name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

/// 函数 `aggregate_api_failure_message`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - status_code: 参数 status_code
/// - body: 参数 body
/// - request_id: 参数 request_id
/// - cf_ray: 参数 cf_ray
/// - auth_error: 参数 auth_error
/// - identity_error_code: 参数 identity_error_code
///
/// # 返回
/// 返回函数执行结果
fn aggregate_api_failure_message(
    status_code: u16,
    body: &[u8],
    request_id: Option<&str>,
    cf_ray: Option<&str>,
    auth_error: Option<&str>,
    identity_error_code: Option<&str>,
) -> String {
    let mut parts =
        vec![
            crate::gateway::summarize_upstream_error_hint_from_body(status_code, body)
                .unwrap_or_else(|| format!("aggregate api upstream status={status_code}")),
        ];
    if let Some(request_id) = request_id.map(str::trim).filter(|value| !value.is_empty()) {
        parts.push(format!("request_id={request_id}"));
    }
    if let Some(cf_ray) = cf_ray.map(str::trim).filter(|value| !value.is_empty()) {
        parts.push(format!("cf_ray={cf_ray}"));
    }
    if let Some(auth_error) = auth_error.map(str::trim).filter(|value| !value.is_empty()) {
        parts.push(format!("auth_error={auth_error}"));
    }
    if let Some(identity_error_code) = identity_error_code
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        parts.push(format!("identity_error_code={identity_error_code}"));
    }
    if parts.len() == 1 {
        parts.remove(0)
    } else {
        format!("{} [{}]", parts.remove(0), parts.join(", "))
    }
}

/// 函数 `build_aggregate_api_request`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - client: 参数 client
/// - request: 参数 request
/// - method: 参数 method
/// - url: 参数 url
/// - body: 参数 body
/// - secret: 参数 secret
/// - request_deadline: 参数 request_deadline
/// - is_stream: 参数 is_stream
///
/// # 返回
/// 返回函数执行结果
fn build_aggregate_api_request(
    client: &reqwest::blocking::Client,
    request: &Request,
    method: &reqwest::Method,
    url: reqwest::Url,
    body: &Bytes,
    secret: &str,
    auth_config: &AggregateApiAuthConfig,
    injected_headers: &HashSet<String>,
    request_deadline: Option<Instant>,
    is_stream: bool,
) -> Result<reqwest::blocking::RequestBuilder, String> {
    let mut builder = client.request(method.clone(), url);
    if let Some(timeout) =
        super::super::support::deadline::send_timeout(request_deadline, is_stream)
    {
        builder = builder.timeout(timeout);
    }
    let request_headers = request.headers().to_vec();
    for header in &request_headers {
        if should_skip_forward_header_with_overrides(
            header.field.as_str().into(),
            injected_headers,
        ) {
            continue;
        }
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(header.field.as_str().as_bytes()),
            HeaderValue::from_str(header.value.as_str()),
        ) {
            builder = builder.header(name, value);
        }
    }

    let secret_trimmed = secret.trim();
    match auth_config {
        AggregateApiAuthConfig::ApiKeyDefaultBearer => {
            builder = builder.header(
                HeaderName::from_static("authorization"),
                HeaderValue::from_str(format!("Bearer {}", secret_trimmed).as_str())
                    .map_err(|_| "invalid aggregate api secret".to_string())?,
            );
        }
        AggregateApiAuthConfig::ApiKeyHeader { name, format } => {
            let header_name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| "invalid aggregate api auth header".to_string())?;
            let value = if format == "raw" {
                secret_trimmed.to_string()
            } else {
                format!("Bearer {}", secret_trimmed)
            };
            builder = builder.header(
                header_name,
                HeaderValue::from_str(value.as_str())
                    .map_err(|_| "invalid aggregate api secret".to_string())?,
            );
        }
        AggregateApiAuthConfig::ApiKeyQuery { .. } => {}
        AggregateApiAuthConfig::UserPassBasic
        | AggregateApiAuthConfig::UserPassHeaderPair { .. }
        | AggregateApiAuthConfig::UserPassQueryPair { .. } => {
            let parsed: UserPassSecret = serde_json::from_str(secret_trimmed)
                .map_err(|_| "invalid aggregate api secret".to_string())?;
            match auth_config {
                AggregateApiAuthConfig::UserPassBasic => {
                    builder = builder.basic_auth(parsed.username, Some(parsed.password));
                }
                AggregateApiAuthConfig::UserPassHeaderPair {
                    username_name,
                    password_name,
                } => {
                    let user_header = HeaderName::from_bytes(username_name.as_bytes())
                        .map_err(|_| "invalid aggregate api auth header".to_string())?;
                    let pass_header = HeaderName::from_bytes(password_name.as_bytes())
                        .map_err(|_| "invalid aggregate api auth header".to_string())?;
                    builder = builder.header(
                        user_header,
                        HeaderValue::from_str(parsed.username.as_str())
                            .map_err(|_| "invalid aggregate api secret".to_string())?,
                    );
                    builder = builder.header(
                        pass_header,
                        HeaderValue::from_str(parsed.password.as_str())
                            .map_err(|_| "invalid aggregate api secret".to_string())?,
                    );
                }
                AggregateApiAuthConfig::UserPassQueryPair { .. } => {}
                _ => {}
            }
        }
    }
    if !body.is_empty() {
        builder = builder.body(body.clone());
    }
    Ok(builder)
}

/// 函数 `resolve_aggregate_api_rotation_candidates`
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
pub(crate) fn resolve_aggregate_api_rotation_candidates(
    storage: &Storage,
    protocol_type: &str,
    aggregate_api_id: Option<&str>,
) -> Result<Vec<AggregateApi>, String> {
    let provider_type = if protocol_type == "anthropic_native" {
        AGGREGATE_API_PROVIDER_CLAUDE
    } else {
        AGGREGATE_API_PROVIDER_CODEX
    };

    let mut candidates = storage
        .list_aggregate_apis()
        .map_err(|err| err.to_string())?
        .into_iter()
        .filter(|api| {
            is_aggregate_api_active(api.status.as_str())
                && normalize_provider_type_value(api.provider_type.as_str()) == provider_type
        })
        .collect::<Vec<_>>();
    candidates = normalize_candidate_order(candidates);

    if let Some(api_id) = aggregate_api_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Some(preferred) = storage
            .find_aggregate_api_by_id(api_id)
            .map_err(|err| err.to_string())?
        {
            let preferred_provider = normalize_provider_type_value(preferred.provider_type.as_str());
            if is_aggregate_api_active(preferred.status.as_str()) && preferred_provider == provider_type
            {
                candidates.retain(|api| api.id != preferred.id);
                candidates.insert(0, preferred);
            }
        }
    }

    if candidates.is_empty() {
        Err(format!(
            "aggregate api not found for provider {provider_type}"
        ))
    } else {
        Ok(candidates)
    }
}

/// 函数 `proxy_aggregate_request`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - in super: 参数 in super
///
/// # 返回
/// 返回函数执行结果
#[allow(clippy::too_many_arguments)]
pub(in super::super) fn proxy_aggregate_request(
    request: Request,
    storage: &Storage,
    trace_id: &str,
    key_id: &str,
    original_path: &str,
    path: &str,
    request_method: &str,
    method: &reqwest::Method,
    body: &Bytes,
    is_stream: bool,
    response_adapter: super::super::super::ResponseAdapter,
    model_for_log: Option<&str>,
    reasoning_for_log: Option<&str>,
    effective_service_tier_for_log: Option<&str>,
    aggregate_api_candidates: Vec<AggregateApi>,
    request_deadline: Option<Instant>,
    started_at: Instant,
    queue_wait_ms: Option<u128>,
) -> Result<(), String> {
    if aggregate_api_candidates.is_empty() {
        let message = "aggregate api not found".to_string();
        super::super::super::record_gateway_request_outcome(path, 404, Some("aggregate_api"));
        super::super::super::trace_log::log_request_final(
            trace_id,
            404,
            Some(key_id),
            None,
            Some(message.as_str()),
            started_at.elapsed().as_millis(),
        );
        let request = request;
        respond_error(request, 404, message.as_str(), Some(trace_id));
        return Ok(());
    }

    let client = super::super::super::fresh_upstream_client();
    let mut request = Some(request);
    let mut attempted_aggregate_api_ids = Vec::new();
    let mut aggregate_api_attempt_failures = Vec::new();
    let mut last_attempt_url: Option<String> = None;
    let mut last_attempt_supplier_name: Option<String> = None;
    let mut last_attempt_error: Option<String> = None;
    let mut last_failure_status = 502u16;

    let total_candidates = aggregate_api_candidates.len();
    for (candidate_idx, candidate) in aggregate_api_candidates.into_iter().enumerate() {
        attempted_aggregate_api_ids.push(candidate.id.clone());
        let candidate_supplier_name = candidate.supplier_name.clone();
        let candidate_url = candidate.url.clone();
        let Some(secret) = storage
            .find_aggregate_api_secret_by_id(candidate.id.as_str())
            .map_err(|err| err.to_string())?
        else {
            last_attempt_url = Some(candidate_url.clone());
            last_attempt_supplier_name = candidate_supplier_name.clone();
            last_attempt_error = Some("aggregate api secret not found".to_string());
            last_failure_status = 403;
            push_aggregate_api_attempt_failure(
                &mut aggregate_api_attempt_failures,
                &candidate,
                candidate_supplier_name.as_deref(),
                403,
                last_attempt_error.as_deref(),
            );
            continue;
        };

        let effective_path = effective_action_path(&candidate, path);
        let (auth_config, injected_headers) = match parse_auth_config(&candidate) {
            Ok(value) => value,
            Err(err) => {
                last_attempt_url = Some(candidate_url.clone());
                last_attempt_supplier_name = candidate_supplier_name.clone();
                last_attempt_error = Some(err);
                last_failure_status = 502;
                push_aggregate_api_attempt_failure(
                    &mut aggregate_api_attempt_failures,
                    &candidate,
                    candidate_supplier_name.as_deref(),
                    502,
                    last_attempt_error.as_deref(),
                );
                continue;
            }
        };

        let mut succeeded = false;
        let mut candidate_failure_status: Option<u16> = None;
        let mut candidate_failure_error: Option<String> = None;
        for attempt_idx in 0..=AGGREGATE_API_RETRY_ATTEMPTS_PER_CHANNEL {
            if super::super::support::deadline::is_expired(request_deadline) {
                let message = "aggregate api request timeout".to_string();
                push_aggregate_api_attempt_failure(
                    &mut aggregate_api_attempt_failures,
                    &candidate,
                    candidate_supplier_name.as_deref(),
                    504,
                    Some(message.as_str()),
                );
                let request = request
                    .take()
                    .expect("request should still be available for timeout response");
                super::super::super::record_gateway_request_outcome(
                    path,
                    504,
                    Some("aggregate_api"),
                );
                super::super::super::trace_log::log_request_final(
                    trace_id,
                    504,
                    Some(key_id),
                    Some(candidate_url.as_str()),
                    Some(message.as_str()),
                    started_at.elapsed().as_millis(),
                );
                super::super::super::write_request_log(
                    storage,
                    super::super::super::request_log::RequestLogTraceContext {
                        trace_id: Some(trace_id),
                        original_path: Some(original_path),
                        adapted_path: Some(path),
                        queue_wait_ms,
                        response_adapter: Some(response_adapter),
                        effective_service_tier: effective_service_tier_for_log,
                        aggregate_api_supplier_name: candidate_supplier_name.as_deref(),
                        aggregate_api_url: Some(candidate_url.as_str()),
                        attempted_aggregate_api_ids: Some(attempted_aggregate_api_ids.as_slice()),
                        aggregate_api_attempt_failures: (!aggregate_api_attempt_failures
                            .is_empty())
                        .then_some(aggregate_api_attempt_failures.as_slice()),
                        ..Default::default()
                    },
                    Some(key_id),
                    None,
                    path,
                    request_method,
                    model_for_log,
                    reasoning_for_log,
                    Some(candidate_url.as_str()),
                    Some(504),
                    RequestLogUsage::default(),
                    Some(message.as_str()),
                    Some(started_at.elapsed().as_millis()),
                );
                respond_error(request, 504, message.as_str(), Some(trace_id));
                return Ok(());
            }

            let mut url = match build_upstream_url(candidate_url.as_str(), effective_path.as_str()) {
                Ok(url) => url,
                Err(_) => {
                    last_attempt_url = Some(candidate_url.clone());
                    last_attempt_supplier_name = candidate_supplier_name.clone();
                    last_attempt_error = Some("invalid aggregate api url".to_string());
                    last_failure_status = 502;
                    candidate_failure_status = Some(502);
                    candidate_failure_error = last_attempt_error.clone();
                    break;
                }
            };

            match &auth_config {
                AggregateApiAuthConfig::ApiKeyQuery { name } => {
                    url = replace_query_param(url, name.as_str(), secret.trim());
                }
                AggregateApiAuthConfig::UserPassQueryPair {
                    username_name,
                    password_name,
                } => {
                    let parsed: UserPassSecret = serde_json::from_str(secret.trim())
                        .map_err(|_| "invalid aggregate api secret".to_string())?;
                    url = replace_query_param(url, username_name.as_str(), parsed.username.as_str());
                    url = replace_query_param(url, password_name.as_str(), parsed.password.as_str());
                }
                _ => {}
            }

            let builder = build_aggregate_api_request(
                &client,
                request.as_ref().expect("request should still be available"),
                method,
                url.clone(),
                body,
                secret.as_str(),
                &auth_config,
                &injected_headers,
                request_deadline,
                is_stream,
            )?;
            let dispatch = format!("candidate_attempt_{attempt_idx}");
            super::super::super::trace_log::log_upstream_request_body(
                trace_id,
                "aggregate_api",
                dispatch.as_str(),
                url.as_str(),
                None,
                body.as_ref(),
                body.as_ref(),
            );

            let attempt_started_at = Instant::now();
            let upstream = match builder.send() {
                Ok(resp) => {
                    let duration_ms =
                        super::super::super::duration_to_millis(attempt_started_at.elapsed());
                    super::super::super::metrics::record_gateway_upstream_attempt(
                        duration_ms,
                        false,
                    );
                    resp
                }
                Err(err) => {
                    let duration_ms =
                        super::super::super::duration_to_millis(attempt_started_at.elapsed());
                    super::super::super::metrics::record_gateway_upstream_attempt(
                        duration_ms,
                        true,
                    );
                    let message = format!("aggregate api upstream error: {err}");
                    last_attempt_url = Some(url.as_str().to_string());
                    last_attempt_supplier_name = candidate_supplier_name.clone();
                    last_attempt_error = Some(message.clone());
                    last_failure_status = 502;
                    candidate_failure_status = Some(502);
                    candidate_failure_error = Some(message);
                    if attempt_idx < AGGREGATE_API_RETRY_ATTEMPTS_PER_CHANNEL {
                        continue;
                    }
                    break;
                }
            };

            if !upstream.status().is_success() {
                let status_code = upstream.status().as_u16();
                let upstream_request_id = first_upstream_header(
                    upstream.headers(),
                    &["x-request-id", "x-oai-request-id"],
                );
                let upstream_cf_ray = first_upstream_header(upstream.headers(), &["cf-ray"]);
                let upstream_auth_error =
                    first_upstream_header(upstream.headers(), &["x-openai-authorization-error"]);
                let upstream_identity_error_code =
                    crate::gateway::extract_identity_error_code_from_headers(upstream.headers());
                let upstream_body = upstream
                    .bytes()
                    .map_err(|err| format!("read upstream body failed: {err}"))?;
                let message = aggregate_api_failure_message(
                    status_code,
                    upstream_body.as_ref(),
                    upstream_request_id.as_deref(),
                    upstream_cf_ray.as_deref(),
                    upstream_auth_error.as_deref(),
                    upstream_identity_error_code.as_deref(),
                );
                last_attempt_url = Some(url.as_str().to_string());
                last_attempt_supplier_name = candidate_supplier_name.clone();
                last_attempt_error = Some(message.clone());
                last_failure_status = 502;
                candidate_failure_status = Some(status_code);
                candidate_failure_error = Some(message);
                if should_retry_same_aggregate_candidate(status_code)
                    && attempt_idx < AGGREGATE_API_RETRY_ATTEMPTS_PER_CHANNEL
                {
                    continue;
                }
                break;
            }

            let inflight_guard = super::super::super::acquire_account_inflight(key_id);
            let passthrough_sse_protocol =
                resolve_passthrough_sse_protocol(&candidate, path, response_adapter);
            let (bridge, failover_request) = super::super::super::respond_with_upstream(
                request
                    .take()
                    .expect("request should be available before bridge"),
                upstream,
                inflight_guard,
                response_adapter,
                passthrough_sse_protocol,
                None,
                path,
                None,
                is_stream,
                candidate_idx + 1 < total_candidates,
                Some(trace_id),
                started_at,
            )?
            .into_parts();
            let bridge_output_text_len = bridge
                .usage
                .output_text
                .as_deref()
                .map(str::trim)
                .map(str::len)
                .unwrap_or(0);
            super::super::super::trace_log::log_bridge_result(
                trace_id,
                format!("{response_adapter:?}").as_str(),
                path,
                is_stream,
                bridge.stream_terminal_seen,
                bridge.stream_terminal_error.as_deref(),
                bridge.delivery_error.as_deref(),
                bridge_output_text_len,
                bridge.usage.output_tokens,
                bridge.delivered_status_code,
                bridge.upstream_error_hint.as_deref(),
                bridge.upstream_request_id.as_deref(),
                bridge.upstream_cf_ray.as_deref(),
                bridge.upstream_auth_error.as_deref(),
                bridge.upstream_identity_error_code.as_deref(),
                bridge.upstream_content_type.as_deref(),
                bridge.last_sse_event_type.as_deref(),
            );
            let bridge_ok = bridge.is_ok(is_stream);
            let mut final_error = bridge.upstream_error_hint.clone();
            if final_error.is_none() && !bridge_ok {
                final_error =
                    Some(bridge.error_message(is_stream).unwrap_or_else(|| {
                        "aggregate api upstream response incomplete".to_string()
                    }));
            }
            let status_code =
                bridge
                    .delivered_status_code
                    .unwrap_or_else(|| if bridge_ok { 200 } else { 502 });
            let status_code = if final_error.is_some() && status_code < 400 {
                502
            } else {
                status_code
            };
            let gateway_failover = should_failover_after_aggregate_bridge(
                final_error.as_deref(),
                candidate_idx + 1 < total_candidates,
                failover_request.is_some(),
            );
            let usage = bridge.usage;

            last_attempt_url = Some(url.as_str().to_string());
            last_attempt_supplier_name = candidate_supplier_name.clone();
            last_attempt_error = final_error.clone();
            last_failure_status = status_code;
            if gateway_failover {
                candidate_failure_status = Some(status_code);
                candidate_failure_error = final_error.clone();
                request = failover_request;
                break;
            }

            super::super::super::record_gateway_request_outcome(
                path,
                status_code,
                Some("aggregate_api"),
            );
            super::super::super::trace_log::log_request_final(
                trace_id,
                status_code,
                Some(key_id),
                Some(url.as_str()),
                final_error.as_deref(),
                started_at.elapsed().as_millis(),
            );
            if let Some(final_error_text) = final_error.as_deref() {
                push_aggregate_api_attempt_failure(
                    &mut aggregate_api_attempt_failures,
                    &candidate,
                    candidate_supplier_name.as_deref(),
                    status_code,
                    Some(final_error_text),
                );
            }
            super::super::super::write_request_log(
                storage,
                super::super::super::request_log::RequestLogTraceContext {
                    trace_id: Some(trace_id),
                    original_path: Some(original_path),
                    adapted_path: Some(path),
                    queue_wait_ms,
                    response_adapter: Some(response_adapter),
                    effective_service_tier: effective_service_tier_for_log,
                    aggregate_api_supplier_name: candidate_supplier_name.as_deref(),
                    aggregate_api_url: Some(candidate_url.as_str()),
                    attempted_aggregate_api_ids: Some(attempted_aggregate_api_ids.as_slice()),
                    aggregate_api_attempt_failures: (!aggregate_api_attempt_failures
                        .is_empty())
                        .then_some(aggregate_api_attempt_failures.as_slice()),
                    ..Default::default()
                },
                Some(key_id),
                None,
                path,
                request_method,
                model_for_log,
                reasoning_for_log,
                Some(url.as_str()),
                Some(status_code),
                RequestLogUsage {
                    input_tokens: usage.input_tokens,
                    cached_input_tokens: usage.cached_input_tokens,
                    output_tokens: usage.output_tokens,
                    total_tokens: usage.total_tokens,
                    reasoning_output_tokens: usage.reasoning_output_tokens,
                    first_response_ms: usage.first_response_ms,
                },
                final_error.as_deref(),
                Some(started_at.elapsed().as_millis()),
            );
            succeeded = true;
            break;
        }

        if succeeded {
            return Ok(());
        }

        push_aggregate_api_attempt_failure(
            &mut aggregate_api_attempt_failures,
            &candidate,
            candidate_supplier_name.as_deref(),
            candidate_failure_status.unwrap_or(last_failure_status),
            candidate_failure_error.as_deref(),
        );

        if candidate_idx + 1 < total_candidates {
            super::super::super::record_gateway_failover_attempt();
        }
    }

    let message =
        last_attempt_error.unwrap_or_else(|| "aggregate api upstream response failed".to_string());
    let status_code = last_failure_status;
    let request = request
        .take()
        .expect("request should still be available for failure response");
    super::super::super::record_gateway_request_outcome(path, status_code, Some("aggregate_api"));
    super::super::super::trace_log::log_request_final(
        trace_id,
        status_code,
        Some(key_id),
        last_attempt_url.as_deref(),
        Some(message.as_str()),
        started_at.elapsed().as_millis(),
    );
    super::super::super::write_request_log(
        storage,
        super::super::super::request_log::RequestLogTraceContext {
            trace_id: Some(trace_id),
            original_path: Some(original_path),
            adapted_path: Some(path),
            queue_wait_ms,
            response_adapter: Some(response_adapter),
            effective_service_tier: effective_service_tier_for_log,
            aggregate_api_supplier_name: last_attempt_supplier_name.as_deref(),
            aggregate_api_url: last_attempt_url.as_deref(),
            attempted_aggregate_api_ids: Some(attempted_aggregate_api_ids.as_slice()),
            aggregate_api_attempt_failures: (!aggregate_api_attempt_failures.is_empty())
                .then_some(aggregate_api_attempt_failures.as_slice()),
            ..Default::default()
        },
        Some(key_id),
        None,
        path,
        request_method,
        model_for_log,
        reasoning_for_log,
        last_attempt_url.as_deref(),
        Some(status_code),
        RequestLogUsage::default(),
        Some(message.as_str()),
        Some(started_at.elapsed().as_millis()),
    );
    respond_error(request, status_code, message.as_str(), Some(trace_id));
    Ok(())
}

#[cfg(test)]
mod bridge_tests {
    use super::*;

    /// 函数 `candidate`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - id: 参数 id
    /// - sort: 参数 sort
    ///
    /// # 返回
    /// 返回函数执行结果
    fn candidate(id: &str, sort: i64) -> AggregateApi {
        candidate_with_weight(id, sort, 100)
    }

    fn candidate_with_weight(id: &str, sort: i64, weight: i64) -> AggregateApi {
        AggregateApi {
            id: id.to_string(),
            provider_type: AGGREGATE_API_PROVIDER_CODEX.to_string(),
            supplier_name: None,
            sort,
            weight,
            url: format!("https://{id}.example.com"),
            auth_type: AGGREGATE_API_AUTH_APIKEY.to_string(),
            auth_params_json: None,
            action: None,
            status: "active".to_string(),
            created_at: sort,
            updated_at: sort,
            last_test_at: None,
            last_test_status: None,
            last_test_error: None,
        }
    }

    /// 函数 `ids`
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
    fn ids(items: &[AggregateApi]) -> Vec<String> {
        items.iter().map(|item| item.id.clone()).collect()
    }

    /// 函数 `balanced_route_strategy_rotates_aggregate_candidates`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// 无
    ///
    /// # 返回
    /// 无
    #[test]
    fn same_sort_candidates_with_equal_weight_rotate_first_pick() {
        let _guard = crate::test_env_guard();
        clear_aggregate_api_bucket_route_state_for_tests();

        let mut first = vec![
            candidate("agg-a", 0),
            candidate("agg-b", 0),
            candidate("agg-c", 5),
        ];
        apply_gateway_route_strategy_to_aggregate_candidates(
            &mut first,
            "gk-aggregate-weighted",
            Some("gpt-5.4-mini"),
            None,
        );
        assert_eq!(ids(&first), vec!["agg-a", "agg-b", "agg-c"]);

        let mut second = vec![
            candidate("agg-a", 0),
            candidate("agg-b", 0),
            candidate("agg-c", 5),
        ];
        apply_gateway_route_strategy_to_aggregate_candidates(
            &mut second,
            "gk-aggregate-weighted",
            Some("gpt-5.4-mini"),
            None,
        );
        assert_eq!(ids(&second), vec!["agg-b", "agg-a", "agg-c"]);

        let mut third = vec![
            candidate("agg-a", 0),
            candidate("agg-b", 0),
            candidate("agg-c", 5),
        ];
        apply_gateway_route_strategy_to_aggregate_candidates(
            &mut third,
            "gk-aggregate-weighted",
            Some("gpt-5.4-mini"),
            None,
        );
        assert_eq!(ids(&third), vec!["agg-a", "agg-b", "agg-c"]);

        clear_aggregate_api_bucket_route_state_for_tests();
    }

    /// 函数 `balanced_route_strategy_preserves_explicit_preferred_aggregate_api`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// 无
    ///
    /// # 返回
    /// 无
    #[test]
    fn same_sort_candidates_respect_weight_bias() {
        let _guard = crate::test_env_guard();
        clear_aggregate_api_bucket_route_state_for_tests();

        let mut first_pick_counts = std::collections::HashMap::<String, usize>::new();
        for _ in 0..6 {
            let mut candidates = vec![
                candidate_with_weight("agg-heavy", 0, 300),
                candidate_with_weight("agg-light", 0, 100),
                candidate("agg-tail", 5),
            ];
            apply_gateway_route_strategy_to_aggregate_candidates(
                &mut candidates,
                "gk-aggregate-weight-bias",
                Some("gpt-5.4-mini"),
                None,
            );
            *first_pick_counts.entry(candidates[0].id.clone()).or_default() += 1;
            assert_eq!(candidates[2].id, "agg-tail");
        }
        let heavy = first_pick_counts.get("agg-heavy").copied().unwrap_or_default();
        let light = first_pick_counts.get("agg-light").copied().unwrap_or_default();
        assert!(heavy > light);
        assert!(light > 0);

        clear_aggregate_api_bucket_route_state_for_tests();
    }

    #[test]
    fn higher_sort_group_never_jumps_ahead_of_lower_sort_group() {
        let _guard = crate::test_env_guard();
        clear_aggregate_api_bucket_route_state_for_tests();

        for _ in 0..6 {
            let mut candidates = vec![
                candidate("agg-a", 0),
                candidate("agg-b", 0),
                candidate("agg-c", 10),
            ];
            apply_gateway_route_strategy_to_aggregate_candidates(
                &mut candidates,
                "gk-aggregate-sort-tier",
                Some("gpt-5.4-mini"),
                None,
            );
            assert!(matches!(candidates[0].id.as_str(), "agg-a" | "agg-b"));
            assert_eq!(candidates[2].id, "agg-c");
        }

        clear_aggregate_api_bucket_route_state_for_tests();
    }

    #[test]
    fn preferred_aggregate_api_stays_first_when_same_sort_group_reorders() {
        let _guard = crate::test_env_guard();
        clear_aggregate_api_bucket_route_state_for_tests();

        let mut first = vec![
            candidate("agg-preferred", 0),
            candidate("agg-b", 0),
            candidate("agg-c", 0),
        ];
        apply_gateway_route_strategy_to_aggregate_candidates(
            &mut first,
            "gk-aggregate-route-strategy-preferred",
            Some("gpt-5.4-mini"),
            Some("agg-preferred"),
        );
        assert_eq!(ids(&first), vec!["agg-preferred", "agg-b", "agg-c"]);

        let mut second = vec![
            candidate("agg-preferred", 0),
            candidate("agg-b", 0),
            candidate("agg-c", 0),
        ];
        apply_gateway_route_strategy_to_aggregate_candidates(
            &mut second,
            "gk-aggregate-route-strategy-preferred",
            Some("gpt-5.4-mini"),
            Some("agg-preferred"),
        );
        assert_eq!(ids(&second), vec!["agg-preferred", "agg-c", "agg-b"]);

        clear_aggregate_api_bucket_route_state_for_tests();
    }
}

#[cfg(test)]
mod tests {
    use codexmanager_core::storage::AggregateApi;

    use super::{
        build_upstream_url, effective_action_path, resolve_passthrough_sse_protocol,
        should_failover_after_aggregate_bridge, should_retry_same_aggregate_candidate,
    };
    use crate::gateway::{PassthroughSseProtocol, ResponseAdapter};

    fn aggregate_api_with_action(action: Option<&str>) -> AggregateApi {
        AggregateApi {
            id: "agg-path-test".to_string(),
            provider_type: "claude".to_string(),
            supplier_name: Some("test".to_string()),
            sort: 0,
            weight: 100,
            url: "https://open.bigmodel.cn/api/anthropic".to_string(),
            auth_type: "apikey".to_string(),
            auth_params_json: None,
            action: action.map(str::to_string),
            status: "active".to_string(),
            created_at: 0,
            updated_at: 0,
            last_test_at: None,
            last_test_status: None,
            last_test_error: None,
        }
    }

    #[test]
    fn empty_custom_action_falls_back_to_original_path() {
        let api = aggregate_api_with_action(Some(""));
        let path = effective_action_path(&api, "/v1/messages?beta=true");
        assert_eq!(path, "/v1/messages?beta=true");
    }

    #[test]
    fn claude_messages_passthrough_uses_anthropic_native_terminal_rules() {
        let api = aggregate_api_with_action(None);
        let protocol =
            resolve_passthrough_sse_protocol(&api, "/v1/messages?beta=true", ResponseAdapter::Passthrough);
        assert_eq!(protocol, Some(PassthroughSseProtocol::AnthropicNative));
    }

    #[test]
    fn non_passthrough_adapter_does_not_override_sse_protocol() {
        let api = aggregate_api_with_action(None);
        let protocol =
            resolve_passthrough_sse_protocol(&api, "/v1/messages?beta=true", ResponseAdapter::AnthropicSse);
        assert_eq!(protocol, None);
    }

    #[test]
    fn build_upstream_url_preserves_base_path_prefix() {
        let url = build_upstream_url(
            "https://open.bigmodel.cn/api/anthropic",
            "/v1/messages?beta=true",
        )
        .expect("build upstream url");
        assert_eq!(
            url.as_str(),
            "https://open.bigmodel.cn/api/anthropic/v1/messages?beta=true"
        );
    }

    #[test]
    fn build_upstream_url_keeps_root_base_behavior() {
        let url = build_upstream_url("https://api.example.com", "/v1/messages?beta=true")
            .expect("build upstream url");
        assert_eq!(url.as_str(), "https://api.example.com/v1/messages?beta=true");
    }

    #[test]
    fn aggregate_api_409_does_not_retry_same_candidate() {
        assert!(!should_retry_same_aggregate_candidate(409));
    }

    #[test]
    fn aggregate_api_403_does_not_retry_same_candidate() {
        assert!(!should_retry_same_aggregate_candidate(403));
    }

    #[test]
    fn aggregate_api_5xx_still_retries_same_candidate() {
        assert!(should_retry_same_aggregate_candidate(503));
    }

    #[test]
    fn aggregate_bridge_failover_requires_preserved_request() {
        assert!(should_failover_after_aggregate_bridge(
            Some("workspace_deactivated"),
            true,
            true,
        ));
        assert!(!should_failover_after_aggregate_bridge(
            Some("workspace_deactivated"),
            true,
            false,
        ));
        assert!(!should_failover_after_aggregate_bridge(
            Some("workspace_deactivated"),
            false,
            true,
        ));
    }

    /// 函数 `final_error_promotes_success_status_to_bad_gateway`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// 无
    ///
    /// # 返回
    /// 无
    #[test]
    fn final_error_promotes_success_status_to_bad_gateway() {
        let status_code = bridge_status_code(Some(200), true, Some("unsupported model"));
        assert_eq!(status_code, 502);
    }

    /// 函数 `successful_bridge_keeps_success_status`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// 无
    ///
    /// # 返回
    /// 无
    #[test]
    fn successful_bridge_keeps_success_status() {
        let status_code = bridge_status_code(Some(200), true, None);
        assert_eq!(status_code, 200);
    }

    /// 函数 `incomplete_bridge_without_status_defaults_to_bad_gateway`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// 无
    ///
    /// # 返回
    /// 无
    #[test]
    fn incomplete_bridge_without_status_defaults_to_bad_gateway() {
        let status_code = bridge_status_code(None, false, None);
        assert_eq!(status_code, 502);
    }

    /// 函数 `bridge_status_code`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - delivered_status_code: 参数 delivered_status_code
    /// - bridge_ok: 参数 bridge_ok
    /// - final_error: 参数 final_error
    ///
    /// # 返回
    /// 返回函数执行结果
    fn bridge_status_code(
        delivered_status_code: Option<u16>,
        bridge_ok: bool,
        final_error: Option<&str>,
    ) -> u16 {
        let status_code =
            delivered_status_code.unwrap_or_else(|| if bridge_ok { 200 } else { 502 });
        if final_error.is_some() && status_code < 400 {
            502
        } else {
            status_code
        }
    }
}
