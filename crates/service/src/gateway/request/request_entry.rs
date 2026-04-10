use tiny_http::{Request, Response};

fn should_log_gateway_ingress(path: &str) -> bool {
    path.starts_with("/v1/responses")
}

fn respond_local_validation_error(
    request: Request,
    trace_id: &str,
    request_method_for_log: &str,
    request_path_for_log: &str,
    status_code: u16,
    message: String,
) -> Result<(), String> {
    super::trace_log::log_request_start(
        trace_id,
        "-",
        request_method_for_log,
        request_path_for_log,
        None,
        None,
        None,
        false,
        "http",
        "-",
    );
    super::trace_log::log_request_final(trace_id, status_code, None, None, Some(message.as_str()), 0);
    super::record_gateway_request_outcome(request_path_for_log, status_code, None);
    if let Some(storage) = super::open_storage() {
        super::write_request_log(
            &storage,
            super::request_log::RequestLogTraceContext {
                trace_id: Some(trace_id),
                original_path: Some(request_path_for_log),
                adapted_path: Some(request_path_for_log),
                response_adapter: None,
                ..Default::default()
            },
            None,
            None,
            request_path_for_log,
            request_method_for_log,
            None,
            None,
            None,
            Some(status_code),
            super::request_log::RequestLogUsage::default(),
            Some(message.as_str()),
            None,
        );
    }
    let response =
        super::error_response::terminal_text_response(status_code, message, Some(trace_id));
    let _ = request.respond(response);
    Ok(())
}

/// 函数 `handle_gateway_request`
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
pub(crate) fn handle_gateway_request(
    mut request: Request,
    prefetched_body: Option<Vec<u8>>,
    prefetched_body_error: Option<(u16, String)>,
) -> Result<(), String> {
    // 处理代理请求（鉴权后转发到上游）
    let debug = super::DEFAULT_GATEWAY_DEBUG;
    if request.method().as_str() == "OPTIONS" {
        let response = Response::empty(204);
        let _ = request.respond(response);
        return Ok(());
    }

    if request.url() == "/health" {
        let response = Response::from_string("ok");
        let _ = request.respond(response);
        return Ok(());
    }

    let _request_guard = super::begin_gateway_request();
    let trace_id = super::trace_log::next_trace_id();
    let request_path_for_log = super::normalize_models_path(request.url());
    let request_method_for_log = request.method().as_str().to_string();
    if should_log_gateway_ingress(request_path_for_log.as_str()) {
        let front_trace_for_log =
            super::local_validation::header_text_for_log(&request, super::FRONT_TRACE_HEADER_NAME);
        let host_for_log = super::local_validation::header_text_for_log(&request, "host");
        let content_length_for_log =
            super::local_validation::header_text_for_log(&request, "content-length");
        let transfer_encoding_for_log =
            super::local_validation::header_text_for_log(&request, "transfer-encoding");
        let content_type_for_log =
            super::local_validation::header_text_for_log(&request, "content-type");
        let user_agent_for_log =
            super::local_validation::header_text_for_log(&request, "user-agent");
        let client_request_id_for_log =
            super::local_validation::header_text_for_log(&request, "x-client-request-id");
        let x_forwarded_for_for_log =
            super::local_validation::header_text_for_log(&request, "x-forwarded-for");
        let x_real_ip_for_log = super::local_validation::header_text_for_log(&request, "x-real-ip");
        let listen_port_for_log =
            super::local_validation::host_port_for_log(host_for_log.as_deref());
        let remote_addr_for_log = request.remote_addr().map(|addr| addr.to_string());
        super::log_gateway_ingress(
            trace_id.as_str(),
            front_trace_for_log.as_deref(),
            "gateway_backend",
            listen_port_for_log.as_deref(),
            remote_addr_for_log.as_deref(),
            request_method_for_log.as_str(),
            request_path_for_log.as_str(),
            host_for_log.as_deref(),
            content_length_for_log.as_deref(),
            transfer_encoding_for_log.as_deref(),
            content_type_for_log.as_deref(),
            user_agent_for_log.as_deref(),
            client_request_id_for_log.as_deref(),
            x_forwarded_for_for_log.as_deref(),
            x_real_ip_for_log.as_deref(),
        );
    }
    if let Some((status_code, message)) = prefetched_body_error {
        return respond_local_validation_error(
            request,
            trace_id.as_str(),
            request_method_for_log.as_str(),
            request_path_for_log.as_str(),
            status_code,
            message,
        );
    }
    let validated =
        match super::local_validation::prepare_local_request(
            &mut request,
            prefetched_body,
            trace_id.clone(),
            debug,
        ) {
            Ok(v) => v,
            Err(err) => {
                return respond_local_validation_error(
                    request,
                    trace_id.as_str(),
                    request_method_for_log.as_str(),
                    request_path_for_log.as_str(),
                    err.status_code,
                    err.message,
                );
            }
        };

    let request = if validated.rotation_strategy == crate::apikey_profile::ROTATION_AGGREGATE_API {
        request
    } else {
        match super::maybe_respond_local_models(
            request,
            validated.trace_id.as_str(),
            validated.key_id.as_str(),
            validated.protocol_type.as_str(),
            validated.original_path.as_str(),
            validated.path.as_str(),
            validated.response_adapter,
            validated.request_method.as_str(),
            validated.model_for_log.as_deref(),
            validated.reasoning_for_log.as_deref(),
            &validated.storage,
        )? {
            Some(request) => request,
            None => return Ok(()),
        }
    };

    let trace_id_for_count_tokens = validated.trace_id.clone();
    let key_id_for_count_tokens = validated.key_id.clone();
    let protocol_type_for_count_tokens = validated.protocol_type.clone();
    let path_for_count_tokens = validated.path.clone();
    let request_method_for_count_tokens = validated.request_method.clone();
    let model_for_count_tokens = validated.model_for_log.clone();
    let reasoning_for_count_tokens = validated.reasoning_for_log.clone();
    let request = if validated.rotation_strategy == crate::apikey_profile::ROTATION_AGGREGATE_API {
        request
    } else {
        match super::maybe_respond_local_count_tokens(
            request,
            trace_id_for_count_tokens.as_str(),
            key_id_for_count_tokens.as_str(),
            protocol_type_for_count_tokens.as_str(),
            validated.original_path.as_str(),
            path_for_count_tokens.as_str(),
            validated.response_adapter,
            request_method_for_count_tokens.as_str(),
            validated.body.as_ref(),
            model_for_count_tokens.as_deref(),
            reasoning_for_count_tokens.as_deref(),
            &validated.storage,
        )? {
            Some(request) => request,
            None => return Ok(()),
        }
    };

    super::proxy_validated_request(request, validated, debug)
}
