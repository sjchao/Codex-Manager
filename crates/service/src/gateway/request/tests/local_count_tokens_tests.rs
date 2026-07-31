use super::*;
use crate::gateway::{ModelType, ResponseAdapter};

#[test]
fn local_count_tokens_log_context_preserves_image_request_metadata() {
    let context = request_log_trace_context(
        "trace-image",
        "/v1/messages/count_tokens",
        "/v1/messages/count_tokens",
        ResponseAdapter::Passthrough,
        ModelType::Image,
        Some(2),
        Some("4K"),
        None,
    );

    assert_eq!(context.model_type, Some(ModelType::Image));
    assert_eq!(context.image_count, Some(2));
    assert_eq!(context.image_size, Some("4K"));
}

/// 函数 `estimate_input_tokens_uses_messages_and_system_text`
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
fn estimate_input_tokens_uses_messages_and_system_text() {
    let body = br#"{
        "model":"gpt-5.3-codex",
        "system":"abcdabcd",
        "messages":[
            {"role":"user","content":"abcd"},
            {"role":"assistant","content":[{"type":"text","text":"abcdabcd"}]}
        ]
    }"#;
    let count = estimate_input_tokens_from_anthropic_messages(body).expect("estimate failed");
    assert_eq!(count, 5);
}

/// 函数 `estimate_input_tokens_rejects_invalid_json`
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
fn estimate_input_tokens_rejects_invalid_json() {
    let err = estimate_input_tokens_from_anthropic_messages(br#"{"messages":["#)
        .expect_err("should reject invalid json");
    assert_eq!(err, "invalid claude request json");
}

/// 函数 `estimate_input_tokens_rejects_non_object_payload`
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
fn estimate_input_tokens_rejects_non_object_payload() {
    let err = estimate_input_tokens_from_anthropic_messages(br#"["bad"]"#)
        .expect_err("should reject non-object payload");
    assert_eq!(err, "claude request body must be an object");
}

#[test]
fn estimate_gemini_input_tokens_uses_contents_and_system_instruction() {
    let body = br#"{
        "systemInstruction":{"parts":[{"text":"abcdabcd"}]},
        "contents":[
            {"role":"user","parts":[{"text":"abcd"}]},
            {"role":"model","parts":[{"text":"abcdabcd"}]}
        ]
    }"#;
    let count = estimate_input_tokens_from_gemini_request(body).expect("estimate failed");
    assert_eq!(count, 5);
}

#[test]
fn estimate_gemini_input_tokens_rejects_invalid_json() {
    let err = estimate_input_tokens_from_gemini_request(br#"{"contents":["#)
        .expect_err("should reject invalid json");
    assert_eq!(err, "invalid gemini request json");
}

#[test]
fn estimate_gemini_cli_wrapped_input_tokens_uses_nested_request() {
    let body = br#"{
        "model":"gemini-2.5-pro",
        "request":{
            "system_instruction":{"parts":[{"text":"abcdabcd"}]},
            "contents":[
                {"parts":[{"text":"abcd"}]},
                {"parts":[{"text":"abcdabcd"}]}
            ]
        }
    }"#;
    let count = estimate_input_tokens_from_gemini_request(body).expect("estimate failed");
    assert_eq!(count, 5);
}
