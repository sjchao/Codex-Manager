use std::io;
use std::io::Write;
use std::net::TcpStream;
use std::panic::AssertUnwindSafe;
use std::thread;
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, SendTimeoutError, Sender};
use tiny_http::Server;

use crate::http::backend_router::BackendRequest;

const HTTP_WORKER_FACTOR: usize = 4;
const HTTP_WORKER_MIN: usize = 8;
const HTTP_STREAM_WORKER_FACTOR: usize = 1;
const HTTP_STREAM_WORKER_MIN: usize = 2;
const HTTP_QUEUE_FACTOR: usize = 4;
const HTTP_QUEUE_MIN: usize = 32;
const HTTP_STREAM_QUEUE_FACTOR: usize = 2;
const HTTP_STREAM_QUEUE_MIN: usize = 16;
const HTTP_QUEUE_SEND_TIMEOUT_MS: u64 = 100;
const HTTP_STREAM_QUEUE_SEND_TIMEOUT_MS: u64 = 100;
const ENV_HTTP_WORKER_FACTOR: &str = "CODEXMANAGER_HTTP_WORKER_FACTOR";
const ENV_HTTP_WORKER_MIN: &str = "CODEXMANAGER_HTTP_WORKER_MIN";
const ENV_HTTP_STREAM_WORKER_FACTOR: &str = "CODEXMANAGER_HTTP_STREAM_WORKER_FACTOR";
const ENV_HTTP_STREAM_WORKER_MIN: &str = "CODEXMANAGER_HTTP_STREAM_WORKER_MIN";
const ENV_HTTP_QUEUE_FACTOR: &str = "CODEXMANAGER_HTTP_QUEUE_FACTOR";
const ENV_HTTP_QUEUE_MIN: &str = "CODEXMANAGER_HTTP_QUEUE_MIN";
const ENV_HTTP_STREAM_QUEUE_FACTOR: &str = "CODEXMANAGER_HTTP_STREAM_QUEUE_FACTOR";
const ENV_HTTP_STREAM_QUEUE_MIN: &str = "CODEXMANAGER_HTTP_STREAM_QUEUE_MIN";

pub(crate) struct BackendServer {
    pub(crate) addr: String,
    pub(crate) join: thread::JoinHandle<()>,
}

/// 函数 `http_worker_count`
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
fn http_worker_count() -> usize {
    // 中文注释：长流请求会占用处理线程；这里固定 worker 上限，避免并发时无限 spawn 拖垮进程。
    let cpus = thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(4);
    let factor = env_usize_or(ENV_HTTP_WORKER_FACTOR, HTTP_WORKER_FACTOR).max(1);
    let min = env_usize_or(ENV_HTTP_WORKER_MIN, HTTP_WORKER_MIN).max(1);
    (cpus.saturating_mul(factor)).max(min)
}

/// 函数 `http_stream_worker_count`
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
fn http_stream_worker_count() -> usize {
    let cpus = thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(4);
    let factor = env_usize_or(ENV_HTTP_STREAM_WORKER_FACTOR, HTTP_STREAM_WORKER_FACTOR).max(1);
    let min = env_usize_or(ENV_HTTP_STREAM_WORKER_MIN, HTTP_STREAM_WORKER_MIN).max(1);
    (cpus.saturating_mul(factor)).max(min)
}

/// 函数 `http_queue_size`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - worker_count: 参数 worker_count
///
/// # 返回
/// 返回函数执行结果
fn http_queue_size(worker_count: usize) -> usize {
    // 中文注释：使用有界队列给入口施加背压；不这样做会在峰值流量下无限堆积请求并放大内存抖动。
    let factor = env_usize_or(ENV_HTTP_QUEUE_FACTOR, HTTP_QUEUE_FACTOR).max(1);
    let min = env_usize_or(ENV_HTTP_QUEUE_MIN, HTTP_QUEUE_MIN).max(1);
    worker_count.saturating_mul(factor).max(min)
}

/// 函数 `http_stream_queue_size`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - worker_count: 参数 worker_count
///
/// # 返回
/// 返回函数执行结果
fn http_stream_queue_size(worker_count: usize) -> usize {
    let factor = env_usize_or(ENV_HTTP_STREAM_QUEUE_FACTOR, HTTP_STREAM_QUEUE_FACTOR).max(1);
    let min = env_usize_or(ENV_HTTP_STREAM_QUEUE_MIN, HTTP_STREAM_QUEUE_MIN).max(1);
    worker_count.saturating_mul(factor).max(min)
}

/// 函数 `env_usize_or`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - name: 参数 name
/// - default: 参数 default
///
/// # 返回
/// 返回函数执行结果
fn env_usize_or(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

/// 函数 `spawn_request_workers`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - worker_count: 参数 worker_count
/// - rx: 参数 rx
/// - is_stream_queue: 参数 is_stream_queue
///
/// # 返回
/// 无
fn spawn_request_workers(
    worker_count: usize,
    rx: Receiver<BackendRequest>,
    is_stream_queue: bool,
) {
    for _ in 0..worker_count {
        let worker_rx = rx.clone();
        let _ = thread::spawn(move || {
            while let Ok(mut request) = worker_rx.recv() {
                crate::gateway::record_http_queue_dequeue(is_stream_queue);
                request.mark_queue_wait_finished();
                handle_backend_request_safely(request);
            }
        });
    }
}

/// 函数 `handle_backend_request_safely`
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
fn handle_backend_request_safely(request: BackendRequest) {
    let method = request.request.method().as_str().to_string();
    let path = request.request.url().to_string();
    if let Err(payload) = std::panic::catch_unwind(AssertUnwindSafe(|| {
        crate::http::backend_router::handle_backend_request(request);
    })) {
        log::error!(
            "backend request handler panicked: method={} path={} panic={}",
            method,
            path,
            panic_payload_message(payload.as_ref())
        );
    }
}

/// 函数 `panic_payload_message`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - payload: 参数 payload
///
/// # 返回
/// 返回函数执行结果
fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_string();
    }
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    "unknown panic payload".to_string()
}

/// 函数 `request_accept_header`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - request: 参数 request
///
/// # 返回
/// 返回函数执行结果
fn request_accept_header(request: &BackendRequest) -> Option<String> {
    request
        .request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Accept"))
        .map(|header| header.value.as_str().to_ascii_lowercase())
}

/// 函数 `request_is_stream_like`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - request: 参数 request
///
/// # 返回
/// 返回函数执行结果
fn request_is_stream_like(request: &BackendRequest) -> bool {
    request_accept_header(request).is_some_and(|value| value.contains("text/event-stream"))
}

fn matches_exact_or_query(path: &str, endpoint: &str) -> bool {
    path == endpoint
        || path
            .strip_prefix(endpoint)
            .map(|suffix| suffix.starts_with('?'))
            .unwrap_or(false)
}

fn should_prefetch_request_body(request: &BackendRequest) -> bool {
    request.request.method().as_str() == "POST"
        && matches_exact_or_query(request.request.url(), "/v1/responses")
}

fn prefetch_request_body(request: &mut tiny_http::Request) -> Result<Vec<u8>, (u16, String)> {
    let max_body_bytes = crate::gateway::front_proxy_max_body_bytes();
    let expected_content_length = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Content-Length"))
        .and_then(|header| header.value.as_str().trim().parse::<usize>().ok());
    let path = request.url().to_string();
    let reader = request.as_reader();
    let mut body = Vec::new();
    let mut chunk = [0_u8; 8192];

    loop {
        let read = match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => read,
            Err(err) => {
                log::warn!(
                    "event=gateway_request_body_read_failed path={} bytes_read={} expected_content_length={} err={}",
                    path,
                    body.len(),
                    expected_content_length
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    err,
                );
                return Err((
                    400,
                    format!("request body read failed after {} bytes: {err}", body.len()),
                ));
            }
        };
        body.extend_from_slice(&chunk[..read]);
        if body.len() > max_body_bytes {
            return Err((
                413,
                format!("request body too large: content-length>{max_body_bytes}"),
            ));
        }
    }

    if let Some(expected_content_length) = expected_content_length {
        if body.len() != expected_content_length {
            log::warn!(
                "event=gateway_request_body_truncated path={} bytes_read={} expected_content_length={}",
                path,
                body.len(),
                expected_content_length,
            );
            return Err((
                400,
                format!(
                    "request body truncated: expected {expected_content_length} bytes, got {}",
                    body.len()
                ),
            ));
        }
    }

    Ok(body)
}

/// 函数 `enqueue_request`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - request: 参数 request
/// - normal_tx: 参数 normal_tx
/// - stream_tx: 参数 stream_tx
///
/// # 返回
/// 返回函数执行结果
fn enqueue_request(
    mut request: BackendRequest,
    normal_tx: &Sender<BackendRequest>,
    stream_tx: &Sender<BackendRequest>,
) -> Result<(), BackendRequest> {
    request.mark_queue_wait_started();
    let prefer_stream = request_is_stream_like(&request);
    if prefer_stream {
        match send_with_timeout(
            stream_tx,
            request,
            Duration::from_millis(HTTP_STREAM_QUEUE_SEND_TIMEOUT_MS),
        ) {
            Ok(()) => {
                crate::gateway::record_http_queue_enqueue(true);
                Ok(())
            }
            Err(request) => match send_with_timeout(
                normal_tx,
                request,
                Duration::from_millis(HTTP_QUEUE_SEND_TIMEOUT_MS),
            ) {
                Ok(()) => {
                    crate::gateway::record_http_queue_enqueue(false);
                    Ok(())
                }
                Err(request) => Err(request),
            },
        }
    } else {
        match send_with_timeout(
            normal_tx,
            request,
            Duration::from_millis(HTTP_QUEUE_SEND_TIMEOUT_MS),
        ) {
            Ok(()) => {
                crate::gateway::record_http_queue_enqueue(false);
                Ok(())
            }
            Err(request) => match send_with_timeout(
                stream_tx,
                request,
                Duration::from_millis(HTTP_STREAM_QUEUE_SEND_TIMEOUT_MS),
            ) {
                Ok(()) => {
                    crate::gateway::record_http_queue_enqueue(true);
                    Ok(())
                }
                Err(request) => Err(request),
            },
        }
    }
}

/// 函数 `send_with_timeout`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - tx: 参数 tx
/// - request: 参数 request
/// - timeout: 参数 timeout
///
/// # 返回
/// 返回函数执行结果
fn send_with_timeout<T>(tx: &Sender<T>, request: T, timeout: Duration) -> Result<(), T> {
    match tx.send_timeout(request, timeout) {
        Ok(()) => Ok(()),
        Err(SendTimeoutError::Timeout(request)) | Err(SendTimeoutError::Disconnected(request)) => {
            Err(request)
        }
    }
}

/// 函数 `run_backend_server`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - server: 参数 server
///
/// # 返回
/// 无
fn run_backend_server(server: Server) {
    let worker_count = http_worker_count();
    let stream_worker_count = http_stream_worker_count();
    let queue_size = http_queue_size(worker_count);
    let stream_queue_size = http_stream_queue_size(stream_worker_count);
    let (normal_tx, normal_rx) = bounded::<BackendRequest>(queue_size);
    let (stream_tx, stream_rx) = bounded::<BackendRequest>(stream_queue_size);
    crate::gateway::record_http_queue_capacity(queue_size, stream_queue_size);
    spawn_request_workers(worker_count, normal_rx, false);
    spawn_request_workers(stream_worker_count, stream_rx, true);

    for request in server.incoming_requests() {
        if crate::shutdown_requested() || request.url() == "/__shutdown" {
            let _ = request.respond(tiny_http::Response::from_string("shutdown"));
            break;
        }
        let mut request = BackendRequest::new(request);
        if should_prefetch_request_body(&request) {
            match prefetch_request_body(&mut request.request) {
                Ok(body) => request.prefetched_body = Some(body),
                Err(err) => request.prefetched_body_error = Some(err),
            }
        }
        if should_bypass_queue(request.request.url()) {
            handle_backend_request_safely(request);
            continue;
        }
        match enqueue_request(request, &normal_tx, &stream_tx) {
            Ok(()) => {}
            Err(request) => {
                crate::gateway::record_http_queue_enqueue_failure();
                let _ = request
                    .request
                    .respond(tiny_http::Response::from_string("server busy").with_status_code(503));
            }
        }
    }
}

/// 函数 `should_bypass_queue`
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
fn should_bypass_queue(path: &str) -> bool {
    path == "/health" || path == "/metrics"
}

/// 函数 `start_backend_server`
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
pub(crate) fn start_backend_server() -> io::Result<BackendServer> {
    let server =
        Server::http("127.0.0.1:0").map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
    let addr = server
        .server_addr()
        .to_ip()
        .map(|address| address.to_string())
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "backend addr missing"))?;
    let join = thread::spawn(move || run_backend_server(server));
    Ok(BackendServer { addr, join })
}

/// 函数 `wake_backend_shutdown`
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
pub(crate) fn wake_backend_shutdown(addr: &str) {
    let Ok(mut stream) = TcpStream::connect(addr) else {
        return;
    };

    let _ = stream.set_write_timeout(Some(Duration::from_millis(200)));
    let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));

    let request = format!("GET /__shutdown HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    let _ = stream.write_all(request.as_bytes());
}

#[cfg(test)]
#[path = "tests/backend_runtime_tests.rs"]
mod tests;
