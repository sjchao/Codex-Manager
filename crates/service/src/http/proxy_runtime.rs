use axum::body::{to_bytes, Body};
use axum::extract::{ConnectInfo, State};
use axum::http::{header, HeaderMap, Request as HttpRequest, Response, StatusCode};
use axum::routing::{any, post};
use axum::Router;
use reqwest::Client;
use std::io;
use std::net::SocketAddr;

use crate::http::proxy_bridge::run_proxy_server;
use crate::http::proxy_request::{
    apply_forwarded_client_ip, apply_front_trace_header, build_target_url, filter_request_headers,
};
use crate::http::proxy_response::{merge_upstream_headers, text_error_response};

#[derive(Clone)]
struct ProxyState {
    backend_base_url: String,
    listen_port: String,
    client: Client,
}

fn should_log_front_proxy_ingress(path: &str) -> bool {
    path.starts_with("/v1/responses")
}

fn header_text_for_log(headers: &HeaderMap, header_name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(name, _)| name.as_str().eq_ignore_ascii_case(header_name))
        .and_then(|(_, value)| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

fn listen_port_for_log(addr: &str) -> String {
    addr.rsplit(':')
        .next()
        .filter(|value| !value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit()))
        .unwrap_or("-")
        .to_string()
}

/// 函数 `log_proxy_error`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - status: 参数 status
/// - target_url: 参数 target_url
/// - message: 参数 message
///
/// # 返回
/// 无
fn log_proxy_error(status: StatusCode, target_url: &str, message: &str) {
    log::warn!(
        "event=front_proxy_error code={} status={} target_url={} message={}",
        crate::error_codes::classify_message(message).as_str(),
        status.as_u16(),
        target_url,
        message
    );
}

/// 函数 `build_backend_base_url`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - backend_addr: 参数 backend_addr
///
/// # 返回
/// 返回函数执行结果
fn build_backend_base_url(backend_addr: &str) -> String {
    format!("http://{backend_addr}")
}

/// 函数 `build_local_backend_client`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
fn build_local_backend_client() -> Result<Client, reqwest::Error> {
    Client::builder()
        .no_proxy()
        // Loopback requests should avoid idle pool reuse so the tiny_http backend always sees a
        // fresh request stream.
        .pool_max_idle_per_host(0)
        .build()
}

/// 函数 `proxy_handler`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - State(state): 参数 State(state)
/// - request: 参数 request
///
/// # 返回
/// 返回函数执行结果
async fn proxy_handler(
    State(state): State<ProxyState>,
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    request: HttpRequest<Body>,
) -> Response<Body> {
    proxy_handler_inner(state, Some(remote_addr), request).await
}

async fn proxy_handler_inner(
    state: ProxyState,
    remote_addr: Option<SocketAddr>,
    request: HttpRequest<Body>,
) -> Response<Body> {
    let (parts, body) = request.into_parts();
    let target_url = build_target_url(&state.backend_base_url, &parts.uri);
    let max_body_bytes = crate::gateway::front_proxy_max_body_bytes();
    let request_path_for_log = parts.uri.path().to_string();
    let request_method_for_log = parts.method.as_str().to_string();
    let remote_addr_for_log = remote_addr.map(|addr| addr.to_string());
    let forwarded_client_ip = remote_addr.map(|addr| addr.ip().to_string());
    let should_log_ingress = should_log_front_proxy_ingress(request_path_for_log.as_str());
    let front_trace = should_log_ingress.then(crate::gateway::next_trace_id);
    let host_for_log = should_log_ingress
        .then(|| header_text_for_log(&parts.headers, "host"))
        .flatten();
    let content_length_for_log = should_log_ingress
        .then(|| header_text_for_log(&parts.headers, "content-length"))
        .flatten();
    let transfer_encoding_for_log = should_log_ingress
        .then(|| header_text_for_log(&parts.headers, "transfer-encoding"))
        .flatten();
    let content_type_for_log = should_log_ingress
        .then(|| header_text_for_log(&parts.headers, "content-type"))
        .flatten();
    let user_agent_for_log = should_log_ingress
        .then(|| header_text_for_log(&parts.headers, "user-agent"))
        .flatten();
    let client_request_id_for_log = should_log_ingress
        .then(|| header_text_for_log(&parts.headers, "x-client-request-id"))
        .flatten();

    if let Some(content_length) = parts
        .headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
    {
        if content_length > max_body_bytes as u64 {
            let message = format!("request body too large: content-length={content_length}");
            log_proxy_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                target_url.as_str(),
                message.as_str(),
            );
            return text_error_response(StatusCode::PAYLOAD_TOO_LARGE, message);
        }
    }

    let mut outbound_headers = filter_request_headers(&parts.headers);
    if let Some(front_trace) = front_trace.as_deref() {
        apply_front_trace_header(&mut outbound_headers, front_trace);
    }
    if let Some(client_ip) = forwarded_client_ip.as_deref() {
        apply_forwarded_client_ip(&mut outbound_headers, client_ip);
    }
    let body_bytes = match to_bytes(body, max_body_bytes).await {
        Ok(bytes) => bytes,
        Err(_) => {
            let message = format!("request body too large: content-length>{max_body_bytes}");
            log_proxy_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                target_url.as_str(),
                message.as_str(),
            );
            return text_error_response(StatusCode::PAYLOAD_TOO_LARGE, message);
        }
    };

    if let Some(front_trace) = front_trace.as_deref() {
        crate::gateway::log_front_proxy_ingress(
            front_trace,
            "front_proxy",
            Some(state.listen_port.as_str()),
            remote_addr_for_log.as_deref(),
            request_method_for_log.as_str(),
            request_path_for_log.as_str(),
            host_for_log.as_deref(),
            content_length_for_log.as_deref(),
            transfer_encoding_for_log.as_deref(),
            content_type_for_log.as_deref(),
            user_agent_for_log.as_deref(),
            client_request_id_for_log.as_deref(),
            body_bytes.as_ref(),
        );
    }

    let mut builder = state.client.request(parts.method, target_url.as_str());
    builder = builder.headers(outbound_headers);
    // Keep the front proxy -> backend hop conservative to avoid stale loopback keep-alive state.
    builder = builder.header(header::CONNECTION, "close");
    builder = builder.body(body_bytes);

    let upstream = match builder.send().await {
        Ok(response) => response,
        Err(err) => {
            let message = format!("backend proxy error: {err}");
            log_proxy_error(
                StatusCode::BAD_GATEWAY,
                target_url.as_str(),
                message.as_str(),
            );
            return text_error_response(StatusCode::BAD_GATEWAY, message);
        }
    };

    let response_builder = merge_upstream_headers(
        Response::builder().status(upstream.status()),
        upstream.headers(),
    );

    match response_builder.body(Body::from_stream(upstream.bytes_stream())) {
        Ok(response) => response,
        Err(err) => {
            let message = format!("build response failed: {err}");
            log_proxy_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                target_url.as_str(),
                message.as_str(),
            );
            text_error_response(StatusCode::INTERNAL_SERVER_ERROR, message)
        }
    }
}

async fn responses_handler(
    State(state): State<ProxyState>,
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    request: HttpRequest<Body>,
) -> Response<Body> {
    if request.method() == axum::http::Method::GET
        && crate::http::responses_websocket::is_websocket_upgrade_request(request.headers())
    {
        return crate::http::responses_websocket::upgrade_responses_websocket(request).await;
    }
    proxy_handler_inner(state, Some(remote_addr), request).await
}

/// 函数 `build_front_proxy_app`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - state: 参数 state
///
/// # 返回
/// 返回函数执行结果
fn build_front_proxy_app(state: ProxyState) -> Router {
    Router::new()
        .route("/rpc", post(crate::http::rpc_endpoint::handle_rpc_http))
        .route("/v1/responses", any(responses_handler))
        .fallback(any(proxy_handler))
        .with_state(state)
}

/// 函数 `run_front_proxy`
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
pub(crate) fn run_front_proxy(addr: &str, backend_addr: &str) -> io::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;

    runtime.block_on(async move {
        let client = build_local_backend_client()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
        let state = ProxyState {
            backend_base_url: build_backend_base_url(backend_addr),
            listen_port: listen_port_for_log(addr),
            client,
        };
        let app = build_front_proxy_app(state);
        run_proxy_server(addr, app).await
    })
}

#[cfg(test)]
#[path = "tests/proxy_runtime_tests.rs"]
mod tests;
