use crate::gateway::ModelType;
use codexmanager_core::storage::{Account, Storage, Token};
use tiny_http::Request;

pub(in super::super) enum CandidatePrecheckResult {
    Ready {
        request: Request,
        candidates: Vec<(Account, Token)>,
    },
    Responded,
}

fn request_log_trace_context<'a>(
    trace_id: &'a str,
    original_path: &'a str,
    path: &'a str,
    response_adapter: super::super::super::ResponseAdapter,
    model_type: ModelType,
    image_count: Option<i64>,
    image_size: Option<&'a str>,
) -> super::super::super::request_log::RequestLogTraceContext<'a> {
    super::super::super::request_log::RequestLogTraceContext {
        trace_id: Some(trace_id),
        original_path: Some(original_path),
        adapted_path: Some(path),
        response_adapter: Some(response_adapter),
        model_type: Some(model_type),
        image_count,
        image_size,
        ..Default::default()
    }
}

/// 函数 `prepare_candidates_for_proxy`
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
pub(in super::super) fn prepare_candidates_for_proxy(
    request: Request,
    storage: &Storage,
    trace_id: &str,
    key_id: &str,
    original_path: &str,
    path: &str,
    response_adapter: super::super::super::ResponseAdapter,
    request_method: &str,
    model_for_log: Option<&str>,
    model_type: ModelType,
    image_count: Option<i64>,
    image_size: Option<&str>,
    reasoning_for_log: Option<&str>,
) -> CandidatePrecheckResult {
    let candidates: Vec<(Account, Token)> =
        match super::candidates::prepare_gateway_candidates(storage, model_for_log) {
            Ok(v) => v,
            Err(err) => {
                let err_text = format!("candidate resolve failed: {err}");
                super::super::super::write_request_log(
                    storage,
                    request_log_trace_context(
                        trace_id,
                        original_path,
                        path,
                        response_adapter,
                        model_type,
                        image_count,
                        image_size,
                    ),
                    Some(key_id),
                    None,
                    path,
                    request_method,
                    model_for_log,
                    reasoning_for_log,
                    None,
                    Some(500),
                    super::super::super::request_log::RequestLogUsage::default(),
                    Some(err_text.as_str()),
                    None,
                );
                let response = super::super::super::error_response::terminal_text_response(
                    500,
                    err_text.clone(),
                    Some(trace_id),
                );
                let _ = request.respond(response);
                super::super::super::trace_log::log_request_final(
                    trace_id,
                    500,
                    None,
                    None,
                    Some(err_text.as_str()),
                    0,
                );
                return CandidatePrecheckResult::Responded;
            }
        };

    if candidates.is_empty() {
        super::super::super::write_request_log(
            storage,
            request_log_trace_context(
                trace_id,
                original_path,
                path,
                response_adapter,
                model_type,
                image_count,
                image_size,
            ),
            Some(key_id),
            None,
            path,
            request_method,
            model_for_log,
            reasoning_for_log,
            None,
            Some(503),
            super::super::super::request_log::RequestLogUsage::default(),
            Some("no available account"),
            None,
        );
        let response = super::super::super::error_response::terminal_text_response(
            503,
            "no available account",
            Some(trace_id),
        );
        let _ = request.respond(response);
        super::super::super::trace_log::log_request_final(
            trace_id,
            503,
            None,
            None,
            Some("no available account"),
            0,
        );
        return CandidatePrecheckResult::Responded;
    }

    CandidatePrecheckResult::Ready {
        request,
        candidates,
    }
}

#[cfg(test)]
mod tests {
    use super::request_log_trace_context;
    use crate::gateway::{ModelType, ResponseAdapter};

    #[test]
    fn request_log_context_preserves_image_request_metadata() {
        let context = request_log_trace_context(
            "trace-image",
            "/v1/images/generations",
            "/v1/images/generations",
            ResponseAdapter::Passthrough,
            ModelType::Image,
            Some(2),
            Some("4K"),
        );

        assert_eq!(context.model_type, Some(ModelType::Image));
        assert_eq!(context.image_count, Some(2));
        assert_eq!(context.image_size, Some("4K"));
    }
}
