pub(crate) const CLIENT_CODEX: &str = "codex";
pub(crate) const PROTOCOL_OPENAI_COMPAT: &str = "openai_compat";
pub(crate) const PROTOCOL_ANTHROPIC_NATIVE: &str = "anthropic_native";
pub(crate) const PROTOCOL_AZURE_OPENAI: &str = "azure_openai";
pub(crate) const AUTH_BEARER: &str = "authorization_bearer";
pub(crate) const AUTH_X_API_KEY: &str = "x_api_key";
pub(crate) const AUTH_API_KEY: &str = "api_key";
pub(crate) const ROTATION_ACCOUNT: &str = "account_rotation";
pub(crate) const ROTATION_AGGREGATE_API: &str = "aggregate_api_rotation";

/// 函数 `normalize_key`
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
fn normalize_key(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('-', "_")
}

/// 函数 `is_anthropic_request_path`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-05
///
/// # 参数
/// - path: 参数 path
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn is_anthropic_request_path(path: &str) -> bool {
    path == "/v1/messages" || path.starts_with("/v1/messages/") || path.starts_with("/v1/messages?")
}

/// 函数 `resolve_gateway_protocol_type`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-05
///
/// # 参数
/// - protocol_type: 参数 protocol_type
/// - path: 参数 path
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn resolve_gateway_protocol_type(protocol_type: &str, path: &str) -> &'static str {
    match normalize_key(protocol_type).as_str() {
        "azure" | "azure_openai" => PROTOCOL_AZURE_OPENAI,
        // 中文注释：平台 Key 对 Codex / Claude Code 默认按路径通配；
        // `/v1/messages*` 走 Claude 语义，其余标准路径走 OpenAI/Codex 语义。
        _ if is_anthropic_request_path(path) => PROTOCOL_ANTHROPIC_NATIVE,
        _ => PROTOCOL_OPENAI_COMPAT,
    }
}

/// 函数 `normalize_protocol_type`
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
pub(crate) fn normalize_protocol_type(value: Option<String>) -> Result<String, String> {
    match value {
        Some(raw) => match normalize_key(&raw).as_str() {
            "openai" | "openai_compat" => Ok(PROTOCOL_OPENAI_COMPAT.to_string()),
            "anthropic" | "anthropic_native" => Ok(PROTOCOL_ANTHROPIC_NATIVE.to_string()),
            "azure" | "azure_openai" => Ok(PROTOCOL_AZURE_OPENAI.to_string()),
            other => Err(format!("unsupported protocol type: {other}")),
        },
        None => Ok(PROTOCOL_OPENAI_COMPAT.to_string()),
    }
}

/// 函数 `profile_from_protocol`
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
pub(crate) fn profile_from_protocol(
    protocol_type: &str,
) -> Result<(String, String, String), String> {
    let protocol = normalize_protocol_type(Some(protocol_type.to_string()))?;
    let auth_scheme = if protocol == PROTOCOL_ANTHROPIC_NATIVE {
        AUTH_X_API_KEY.to_string()
    } else if protocol == PROTOCOL_AZURE_OPENAI {
        AUTH_API_KEY.to_string()
    } else {
        AUTH_BEARER.to_string()
    };
    Ok((CLIENT_CODEX.to_string(), protocol, auth_scheme))
}

/// 函数 `normalize_rotation_strategy`
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
pub(crate) fn normalize_rotation_strategy(value: Option<String>) -> Result<String, String> {
    match value {
        Some(raw) => match normalize_key(&raw).as_str() {
            "account" | "account_rotation" | "account_rotate" | "账号轮转" => {
                Ok(ROTATION_ACCOUNT.to_string())
            }
            "aggregateapi"
            | "aggregate_api"
            | "aggregate_api_rotation"
            | "aggregateapirotation"
            | "聚合api"
            | "聚合api轮转" => Ok(ROTATION_AGGREGATE_API.to_string()),
            other => Err(format!("unsupported rotation strategy: {other}")),
        },
        None => Ok(ROTATION_ACCOUNT.to_string()),
    }
}

/// 函数 `normalize_upstream_base_url`
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
pub(crate) fn normalize_upstream_base_url(value: Option<String>) -> Result<Option<String>, String> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let trimmed = raw.trim().trim_end_matches('/').to_string();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let parsed =
        reqwest::Url::parse(trimmed.as_str()).map_err(|_| "invalid upstreamBaseUrl".to_string())?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("invalid upstreamBaseUrl scheme".to_string());
    }
    Ok(Some(trimmed))
}

/// 函数 `normalize_static_headers_json`
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
pub(crate) fn normalize_static_headers_json(
    value: Option<String>,
) -> Result<Option<String>, String> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let parsed: serde_json::Value = serde_json::from_str(trimmed)
        .map_err(|_| "invalid staticHeadersJson: must be a JSON object".to_string())?;
    let obj = parsed
        .as_object()
        .ok_or_else(|| "invalid staticHeadersJson: must be a JSON object".to_string())?;
    for (name, value) in obj {
        if name.trim().is_empty() {
            return Err("invalid staticHeadersJson: header name is empty".to_string());
        }
        if !value.is_string() {
            return Err(format!(
                "invalid staticHeadersJson: header {name} value must be string"
            ));
        }
    }
    Ok(Some(trimmed.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{
        is_anthropic_request_path, resolve_gateway_protocol_type, PROTOCOL_ANTHROPIC_NATIVE,
        PROTOCOL_AZURE_OPENAI, PROTOCOL_OPENAI_COMPAT,
    };

    #[test]
    fn wildcard_protocol_routes_messages_path_to_anthropic() {
        assert!(is_anthropic_request_path("/v1/messages"));
        assert_eq!(
            resolve_gateway_protocol_type(PROTOCOL_OPENAI_COMPAT, "/v1/messages"),
            PROTOCOL_ANTHROPIC_NATIVE
        );
    }

    #[test]
    fn wildcard_protocol_routes_responses_path_to_openai() {
        assert_eq!(
            resolve_gateway_protocol_type(PROTOCOL_ANTHROPIC_NATIVE, "/v1/responses"),
            PROTOCOL_OPENAI_COMPAT
        );
    }

    #[test]
    fn azure_protocol_keeps_azure_mapping() {
        assert_eq!(
            resolve_gateway_protocol_type(PROTOCOL_AZURE_OPENAI, "/v1/messages"),
            PROTOCOL_AZURE_OPENAI
        );
    }
}
