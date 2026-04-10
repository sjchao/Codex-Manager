use tiny_http::Request;

pub(crate) struct BackendRequest {
    pub(crate) request: Request,
    pub(crate) prefetched_body: Option<Vec<u8>>,
    pub(crate) prefetched_body_error: Option<(u16, String)>,
}

impl BackendRequest {
    pub(crate) fn new(request: Request) -> Self {
        Self {
            request,
            prefetched_body: None,
            prefetched_body_error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackendRoute {
    Rpc,
    AuthCallback,
    Metrics,
    Gateway,
}

/// 函数 `resolve_backend_route`
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
pub(crate) fn resolve_backend_route(method: &str, path: &str) -> BackendRoute {
    if method == "POST" && path == "/rpc" {
        return BackendRoute::Rpc;
    }
    if method == "GET" && path.starts_with("/auth/callback") {
        return BackendRoute::AuthCallback;
    }
    if method == "GET" && path == "/metrics" {
        return BackendRoute::Metrics;
    }
    BackendRoute::Gateway
}

/// 函数 `handle_backend_request`
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
pub(crate) fn handle_backend_request(request: BackendRequest) {
    let route = resolve_backend_route(request.request.method().as_str(), request.request.url());
    match route {
        BackendRoute::Rpc => crate::http::rpc_endpoint::handle_rpc(request.request),
        BackendRoute::AuthCallback => crate::http::callback_endpoint::handle_callback(request.request),
        BackendRoute::Metrics => crate::http::gateway_endpoint::handle_metrics(request.request),
        BackendRoute::Gateway => crate::http::gateway_endpoint::handle_gateway(request),
    }
}

#[cfg(test)]
#[path = "tests/backend_router_tests.rs"]
mod tests;
