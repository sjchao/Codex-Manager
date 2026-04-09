use axum::http::{HeaderMap, HeaderName, HeaderValue, Uri};

use crate::http::header_filter::should_skip_request_header;

/// 函数 `build_target_url`
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
pub(crate) fn build_target_url(backend_base_url: &str, uri: &Uri) -> String {
    // 中文注释：部分 tiny_http 请求在重写后可能丢失 query；统一在这里拼接可避免多处实现不一致。
    let path_and_query = uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or("/");
    format!("{backend_base_url}{path_and_query}")
}

/// 函数 `filter_request_headers`
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
pub(crate) fn filter_request_headers(headers: &HeaderMap) -> HeaderMap {
    let mut outbound_headers = HeaderMap::new();
    for (name, value) in headers.iter() {
        if should_skip_request_header(name, value) {
            continue;
        }
        let _ = outbound_headers.insert(name.clone(), value.clone());
    }
    outbound_headers
}

pub(crate) fn apply_forwarded_client_ip(headers: &mut HeaderMap, client_ip: &str) {
    let normalized_ip = client_ip.trim();
    if normalized_ip.is_empty() {
        return;
    }

    let forwarded_for = HeaderName::from_static("x-forwarded-for");
    let real_ip = HeaderName::from_static("x-real-ip");
    let merged_forwarded = headers
        .get(&forwarded_for)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|existing| {
            if existing
                .split(',')
                .any(|item| item.trim().eq_ignore_ascii_case(normalized_ip))
            {
                existing.to_string()
            } else {
                format!("{existing}, {normalized_ip}")
            }
        })
        .unwrap_or_else(|| normalized_ip.to_string());

    if let Ok(value) = HeaderValue::from_str(&merged_forwarded) {
        headers.insert(forwarded_for, value);
    }

    let should_set_real_ip = headers
        .get(&real_ip)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .is_none_or(|value| value.is_empty());
    if should_set_real_ip {
        if let Ok(value) = HeaderValue::from_str(normalized_ip) {
            headers.insert(real_ip, value);
        }
    }
}

pub(crate) fn apply_front_trace_header(headers: &mut HeaderMap, front_trace: &str) {
    let normalized_trace = front_trace.trim();
    if normalized_trace.is_empty() {
        return;
    }

    let front_trace_name = HeaderName::from_static(crate::gateway::FRONT_TRACE_HEADER_NAME);
    if let Ok(value) = HeaderValue::from_str(normalized_trace) {
        headers.insert(front_trace_name, value);
    }
}

#[cfg(test)]
#[path = "tests/proxy_request_tests.rs"]
mod tests;
