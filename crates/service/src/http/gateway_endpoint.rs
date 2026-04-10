use tiny_http::{Header, Request, Response};

/// 函数 `handle_gateway`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - request: 参数 request
///
/// # 返回
/// 无
pub fn handle_gateway(request: crate::http::backend_router::BackendRequest) {
    if let Err(err) = crate::gateway::handle_gateway_request(
        request.request,
        request.prefetched_body,
        request.prefetched_body_error,
        request.queue_wait_ms,
    ) {
        log::error!("gateway request error: {err}");
    }
}

/// 函数 `handle_metrics`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - request: 参数 request
///
/// # 返回
/// 无
pub fn handle_metrics(request: Request) {
    let body = crate::gateway::gateway_metrics_prometheus();
    let mut response = Response::from_string(body);
    if let Ok(content_type) = Header::from_bytes(b"Content-Type", b"text/plain; version=0.0.4") {
        response = response.with_header(content_type);
    }
    let _ = request.respond(response);
}
