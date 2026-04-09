use std::io::Read;

use tiny_http::Request;

/// 函数 `read_request_body`
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
pub(super) fn read_request_body(
    request: &mut Request,
) -> Result<Vec<u8>, super::LocalValidationError> {
    // 中文注释：先把请求体读完再进入鉴权判断，避免客户端写流还在进行时被提前断开。
    let max_body_bytes = crate::gateway::front_proxy_max_body_bytes();
    let expected_content_length = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Content-Length"))
        .and_then(|header| header.value.as_str().trim().parse::<usize>().ok());
    let path = request.url().to_string();
    let reader = request.as_reader();

    read_request_body_from_reader(reader, max_body_bytes, expected_content_length, &path)
}

fn read_request_body_from_reader<R: Read + ?Sized>(
    reader: &mut R,
    max_body_bytes: usize,
    expected_content_length: Option<usize>,
    path: &str,
) -> Result<Vec<u8>, super::LocalValidationError> {
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
                return Err(super::LocalValidationError::new(
                    400,
                    format!("request body read failed after {} bytes: {err}", body.len()),
                ));
            }
        };
        body.extend_from_slice(&chunk[..read]);
        if body.len() > max_body_bytes {
            return Err(super::LocalValidationError::new(
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
            return Err(super::LocalValidationError::new(
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

pub(super) fn header_text_for_log(request: &Request, header_name: &str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|header| {
            header
                .field
                .as_str()
                .as_str()
                .eq_ignore_ascii_case(header_name)
        })
        .map(|header| header.value.as_str().trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(super) fn host_port_for_log(host: Option<&str>) -> Option<String> {
    let raw = host?.trim();
    if raw.is_empty() {
        return None;
    }

    raw.rsplit(':')
        .next()
        .filter(|value| !value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit()))
        .map(|value| value.to_string())
}

/// 函数 `extract_platform_key_or_error`
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
pub(super) fn extract_platform_key_or_error(
    request: &Request,
    incoming_headers: &super::super::IncomingHeaderSnapshot,
    debug: bool,
) -> Result<String, super::LocalValidationError> {
    if let Some(platform_key) = incoming_headers.platform_key() {
        return Ok(platform_key.to_string());
    }

    if debug {
        let remote = request
            .remote_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|| "<none>".to_string());
        let auth_scheme = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("Authorization"))
            .and_then(|h| h.value.as_str().split_whitespace().next())
            .unwrap_or("<none>");
        let header_names = request
            .headers()
            .iter()
            .map(|h| h.field.as_str().as_str())
            .collect::<Vec<_>>()
            .join(",");
        log::warn!(
            "event=gateway_auth_missing path={} status=401 remote={} has_auth={} auth_scheme={} has_x_api_key={} headers=[{}]",
            request.url(),
            remote,
            incoming_headers.has_authorization(),
            auth_scheme,
            incoming_headers.has_x_api_key(),
            header_names,
        );
    }

    Err(super::LocalValidationError::new(401, "missing api key"))
}

#[cfg(test)]
mod tests {
    use std::io::{self, Cursor, Read};

    use super::read_request_body_from_reader;

    struct FailingReader {
        bytes: Vec<u8>,
        position: usize,
        fail_after: usize,
    }

    impl Read for FailingReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.position >= self.fail_after {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "simulated disconnect",
                ));
            }

            let remaining_bytes = self.bytes.len().saturating_sub(self.position);
            let remaining_before_failure = self.fail_after.saturating_sub(self.position);
            let to_copy = remaining_bytes.min(remaining_before_failure).min(buf.len());

            if to_copy == 0 {
                return Ok(0);
            }

            buf[..to_copy].copy_from_slice(&self.bytes[self.position..self.position + to_copy]);
            self.position += to_copy;
            Ok(to_copy)
        }
    }

    #[test]
    fn complete_body_passes_validation() {
        let body = br#"{"model":"gpt-5.4-mini"}"#.to_vec();
        let mut reader = Cursor::new(body.clone());

        let out =
            read_request_body_from_reader(&mut reader, 1024, Some(body.len()), "/v1/responses");

        assert!(out.is_ok(), "body should be read");
        assert_eq!(out.unwrap_or_default(), body);
    }

    #[test]
    fn read_error_returns_client_error_instead_of_partial_body() {
        let body = br#"{"model":"gpt-5.4-mini"}"#.to_vec();
        let mut reader = FailingReader {
            bytes: body.clone(),
            position: 0,
            fail_after: 10,
        };

        let err =
            read_request_body_from_reader(&mut reader, 1024, Some(body.len()), "/v1/responses")
                .expect_err("partial body should fail");

        assert_eq!(err.status_code, 400);
        assert!(err
            .message
            .contains("request body read failed after 10 bytes"));
    }

    #[test]
    fn content_length_mismatch_is_reported_as_truncated_body() {
        let body = br#"{"model":"gpt-5.4-mini"}"#.to_vec();
        let mut reader = Cursor::new(body[..12].to_vec());

        let err =
            read_request_body_from_reader(&mut reader, 1024, Some(body.len()), "/v1/responses")
                .expect_err("short body should fail");

        assert_eq!(err.status_code, 400);
        assert!(err.message.contains("request body truncated"));
        assert!(err.message.contains(&body.len().to_string()));
        assert!(err.message.contains("12"));
    }

    #[test]
    fn host_port_for_log_extracts_port_from_standard_hosts() {
        assert_eq!(
            super::host_port_for_log(Some("127.0.0.1:48760")),
            Some("48760".to_string())
        );
        assert_eq!(
            super::host_port_for_log(Some("[::1]:43123")),
            Some("43123".to_string())
        );
        assert_eq!(super::host_port_for_log(Some("localhost")), None);
    }
}
