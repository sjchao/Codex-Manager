use super::{
    adapt_request_for_protocol, adapt_upstream_response,
    adapt_upstream_response_with_tool_name_restore_map, convert_openai_chat_stream_chunk,
    convert_openai_chat_stream_chunk_with_tool_name_restore_map,
    convert_openai_completions_stream_chunk, ResponseAdapter,
};
use crate::apikey_profile::{PROTOCOL_ANTHROPIC_NATIVE, PROTOCOL_OPENAI_COMPAT};

#[test]
fn openai_chat_completions_are_adapted_to_responses() {
    let body = br#"{"model":"gpt-5.3-codex","messages":[{"role":"user","content":"hi"}]}"#.to_vec();
    let adapted = adapt_request_for_protocol(PROTOCOL_OPENAI_COMPAT, "/v1/chat/completions", body)
        .expect("adapt request");
    let value: serde_json::Value =
        serde_json::from_slice(&adapted.body).expect("parse adapted body");
    assert_eq!(adapted.path, "/v1/responses");
    assert_eq!(
        adapted.response_adapter,
        ResponseAdapter::OpenAIChatCompletionsJson
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(0))
            .and_then(|item| item.get("role"))
            .and_then(serde_json::Value::as_str),
        Some("user")
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(0))
            .and_then(|item| item.get("content"))
            .and_then(|content| content.get(0))
            .and_then(|part| part.get("text"))
            .and_then(serde_json::Value::as_str),
        Some("hi")
    );
    assert_eq!(
        value
            .get("stream_passthrough")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
}

#[test]
fn openai_chat_completions_stream_uses_sse_adapter() {
    let body =
        br#"{"model":"gpt-5.3-codex","messages":[{"role":"user","content":"hi"}],"stream":true}"#
            .to_vec();
    let adapted = adapt_request_for_protocol(PROTOCOL_OPENAI_COMPAT, "/v1/chat/completions", body)
        .expect("adapt request");
    assert_eq!(adapted.path, "/v1/responses");
    assert_eq!(
        adapted.response_adapter,
        ResponseAdapter::OpenAIChatCompletionsSse
    );
}

#[test]
fn openai_chat_completions_forward_service_tier_to_responses() {
    let body = br#"{"model":"gpt-5.3-codex","messages":[{"role":"user","content":"hi"}],"service_tier":"flex"}"#.to_vec();
    let adapted = adapt_request_for_protocol(PROTOCOL_OPENAI_COMPAT, "/v1/chat/completions", body)
        .expect("adapt request");
    let value: serde_json::Value =
        serde_json::from_slice(&adapted.body).expect("parse adapted body");
    assert_eq!(
        value
            .get("service_tier")
            .and_then(serde_json::Value::as_str),
        Some("flex")
    );
}

#[test]
fn openai_chat_completions_shortens_tool_names_and_builds_restore_map() {
    let original_tool_name =
        "mcp__tool_server_namespace_for_codex_manager_gateway_adapter_alignment__very_long_tool_operation_name";
    let body = serde_json::json!({
        "model": "gpt-5.3-codex",
        "messages": [
            {
                "role": "assistant",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": original_tool_name,
                        "arguments": "{}"
                    }
                }]
            },
            {
                "role": "user",
                "content": "hi"
            }
        ],
        "tools": [{
            "type": "function",
            "function": {
                "name": original_tool_name,
                "description": "test tool",
                "parameters": { "type": "object" }
            }
        }],
        "tool_choice": {
            "type": "function",
            "function": {
                "name": original_tool_name
            }
        }
    });
    let adapted = adapt_request_for_protocol(
        PROTOCOL_OPENAI_COMPAT,
        "/v1/chat/completions",
        serde_json::to_vec(&body).expect("serialize body"),
    )
    .expect("adapt request");
    let value: serde_json::Value =
        serde_json::from_slice(&adapted.body).expect("parse adapted body");
    let shortened_name = value
        .get("tools")
        .and_then(|tools| tools.get(0))
        .and_then(|tool| tool.get("name"))
        .and_then(serde_json::Value::as_str)
        .expect("tools[0].name")
        .to_string();
    assert_ne!(shortened_name, original_tool_name);
    assert!(shortened_name.len() <= 64);
    assert_eq!(
        adapted.tool_name_restore_map.get(&shortened_name),
        Some(&original_tool_name.to_string())
    );
    assert_eq!(
        value
            .get("tool_choice")
            .and_then(|tool_choice| tool_choice.get("name"))
            .and_then(serde_json::Value::as_str),
        Some(shortened_name.as_str())
    );
    assert_eq!(
        value
            .get("input")
            .and_then(serde_json::Value::as_array)
            .and_then(|items| {
                items.iter().find(|item| {
                    item.get("type").and_then(serde_json::Value::as_str) == Some("function_call")
                })
            })
            .and_then(|item| item.get("name"))
            .and_then(serde_json::Value::as_str),
        Some(shortened_name.as_str())
    );
}

#[test]
fn openai_chat_completions_stream_passthrough_is_forwarded() {
    let body = br#"{"model":"gpt-5.3-codex","messages":[{"role":"user","content":"hi"}],"stream":false,"stream_passthrough":true}"#.to_vec();
    let adapted = adapt_request_for_protocol(PROTOCOL_OPENAI_COMPAT, "/v1/chat/completions", body)
        .expect("adapt request");
    let value: serde_json::Value =
        serde_json::from_slice(&adapted.body).expect("parse adapted body");
    assert_eq!(adapted.path, "/v1/responses");
    assert_eq!(
        value
            .get("stream_passthrough")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
}

#[test]
fn openai_chat_json_response_restores_shortened_tool_name() {
    let original_tool_name =
        "mcp__tool_server_namespace_for_codex_manager_gateway_adapter_alignment__very_long_tool_operation_name";
    let request = serde_json::json!({
        "model": "gpt-5.3-codex",
        "messages": [{ "role": "user", "content": "hi" }],
        "tools": [{
            "type": "function",
            "function": {
                "name": original_tool_name,
                "description": "test tool",
                "parameters": { "type": "object" }
            }
        }]
    });
    let adapted = adapt_request_for_protocol(
        PROTOCOL_OPENAI_COMPAT,
        "/v1/chat/completions",
        serde_json::to_vec(&request).expect("serialize request"),
    )
    .expect("adapt request");
    let adapted_value: serde_json::Value =
        serde_json::from_slice(&adapted.body).expect("parse adapted body");
    let shortened_name = adapted_value
        .get("tools")
        .and_then(|tools| tools.get(0))
        .and_then(|tool| tool.get("name"))
        .and_then(serde_json::Value::as_str)
        .expect("tools[0].name");
    let upstream = serde_json::json!({
        "id": "resp_restore_1",
        "object": "response",
        "created": 1700001000,
        "model": "gpt-5.3-codex",
        "output": [{
            "type": "function_call",
            "call_id": "call_restore_1",
            "name": shortened_name,
            "arguments": "{}"
        }]
    });
    let upstream_body = serde_json::to_vec(&upstream).expect("serialize upstream");
    let (body, content_type) = adapt_upstream_response_with_tool_name_restore_map(
        ResponseAdapter::OpenAIChatCompletionsJson,
        Some("application/json"),
        &upstream_body,
        Some(&adapted.tool_name_restore_map),
    )
    .expect("convert response");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("parse converted body");
    assert_eq!(content_type, "application/json");
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("tool_calls"))
            .and_then(|tool_calls| tool_calls.get(0))
            .and_then(|tool_call| tool_call.get("function"))
            .and_then(|function| function.get("name"))
            .and_then(serde_json::Value::as_str),
        Some(original_tool_name)
    );
}

#[test]
fn openai_chat_stream_chunk_restores_shortened_tool_name() {
    let mut restore_map = super::ToolNameRestoreMap::new();
    restore_map.insert(
        "mcp__run_query_short".to_string(),
        "mcp__tool_server_namespace_for_codex_manager_gateway_adapter_alignment__run_query"
            .to_string(),
    );
    let value = serde_json::json!({
        "type": "response.output_item.added",
        "response_id": "resp_stream_restore_1",
        "created": 1700001100,
        "model": "gpt-5.3-codex",
        "output_index": 0,
        "item": {
            "type": "function_call",
            "call_id": "call_restore_stream_1",
            "name": "mcp__run_query_short"
        }
    });
    let mapped =
        convert_openai_chat_stream_chunk_with_tool_name_restore_map(&value, Some(&restore_map))
            .expect("map tool chunk");
    assert_eq!(
        mapped
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("delta"))
            .and_then(|delta| delta.get("tool_calls"))
            .and_then(|tool_calls| tool_calls.get(0))
            .and_then(|tool_call| tool_call.get("function"))
            .and_then(|function| function.get("name"))
            .and_then(serde_json::Value::as_str),
        Some("mcp__tool_server_namespace_for_codex_manager_gateway_adapter_alignment__run_query")
    );
}

#[test]
fn openai_responses_passthrough_keeps_responses_path() {
    let body = br#"{"model":"gpt-5.3-codex","input":"hi"}"#.to_vec();
    let adapted = adapt_request_for_protocol(PROTOCOL_OPENAI_COMPAT, "/v1/responses", body.clone())
        .expect("adapt request");
    assert_eq!(adapted.path, "/v1/responses");
    assert_eq!(adapted.body, body);
    assert_eq!(adapted.response_adapter, ResponseAdapter::Passthrough);
}

#[test]
fn openai_completions_are_adapted_to_responses() {
    let body = br#"{"model":"gpt-5.3-codex","prompt":"hello","max_tokens":16}"#.to_vec();
    let adapted = adapt_request_for_protocol(PROTOCOL_OPENAI_COMPAT, "/v1/completions", body)
        .expect("adapt request");
    let value: serde_json::Value =
        serde_json::from_slice(&adapted.body).expect("parse adapted body");
    assert_eq!(adapted.path, "/v1/responses");
    assert_eq!(
        adapted.response_adapter,
        ResponseAdapter::OpenAICompletionsJson
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(0))
            .and_then(|item| item.get("role"))
            .and_then(serde_json::Value::as_str),
        Some("user")
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(0))
            .and_then(|item| item.get("content"))
            .and_then(|content| content.get(0))
            .and_then(|part| part.get("text"))
            .and_then(serde_json::Value::as_str),
        Some("hello")
    );
}

#[test]
fn openai_completions_stream_uses_sse_adapter() {
    let body = br#"{"model":"gpt-5.3-codex","prompt":"hello","stream":true}"#.to_vec();
    let adapted = adapt_request_for_protocol(PROTOCOL_OPENAI_COMPAT, "/v1/completions", body)
        .expect("adapt request");
    assert_eq!(adapted.path, "/v1/responses");
    assert_eq!(
        adapted.response_adapter,
        ResponseAdapter::OpenAICompletionsSse
    );
}

#[test]
fn openai_chat_response_is_converted_from_responses_json() {
    let upstream = br#"{
        "id":"resp_1",
        "object":"response",
        "created":1700000000,
        "model":"gpt-5.3-codex",
        "output":[{"type":"message","content":[{"type":"output_text","text":"hello world"}]}],
        "usage":{"input_tokens":10,"output_tokens":2,"total_tokens":12}
    }"#;
    let (body, content_type) = adapt_upstream_response(
        ResponseAdapter::OpenAIChatCompletionsJson,
        Some("application/json"),
        upstream,
    )
    .expect("convert response");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("parse converted body");
    assert_eq!(content_type, "application/json");
    assert_eq!(
        value.get("object").and_then(serde_json::Value::as_str),
        Some("chat.completion")
    );
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(serde_json::Value::as_str),
        Some("hello world")
    );
}

#[test]
fn openai_chat_response_is_converted_from_output_text_item() {
    let upstream = br#"{
        "id":"resp_1",
        "object":"response",
        "created":1700000000,
        "model":"gpt-5.3-codex",
        "output":[{"type":"output_text","text":"plain output item text"}],
        "usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14}
    }"#;
    let (body, content_type) = adapt_upstream_response(
        ResponseAdapter::OpenAIChatCompletionsJson,
        Some("application/json"),
        upstream,
    )
    .expect("convert response");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("parse converted body");
    assert_eq!(content_type, "application/json");
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(serde_json::Value::as_str),
        Some("plain output item text")
    );
}

#[test]
fn openai_chat_response_is_converted_from_openclaw_tool_call_json() {
    let upstream = br#"{
        "id":"resp_openclaw_tool_1",
        "object":"response",
        "created_at":1700000011,
        "status":"incomplete",
        "model":"openclaw",
        "output":[{
            "type":"function_call",
            "id":"call_item_1",
            "call_id":"call_weather_1",
            "name":"get_weather",
            "arguments":"{\"city\":\"Shanghai\"}"
        }],
        "usage":{"input_tokens":12,"output_tokens":3,"total_tokens":15}
    }"#;
    let (body, content_type) = adapt_upstream_response(
        ResponseAdapter::OpenAIChatCompletionsJson,
        Some("application/json"),
        upstream,
    )
    .expect("convert response");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("parse converted body");
    assert_eq!(content_type, "application/json");
    assert_eq!(
        value.get("created").and_then(serde_json::Value::as_i64),
        Some(1700000011)
    );
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("finish_reason"))
            .and_then(serde_json::Value::as_str),
        Some("tool_calls")
    );
    assert!(value
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .is_some_and(serde_json::Value::is_null));
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("tool_calls"))
            .and_then(|tool_calls| tool_calls.get(0))
            .and_then(|tool_call| tool_call.get("id"))
            .and_then(serde_json::Value::as_str),
        Some("call_weather_1")
    );
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("tool_calls"))
            .and_then(|tool_calls| tool_calls.get(0))
            .and_then(|tool_call| tool_call.get("function"))
            .and_then(|function| function.get("name"))
            .and_then(serde_json::Value::as_str),
        Some("get_weather")
    );
}

#[test]
fn openai_chat_stream_response_is_collapsed_to_chat_completion_json() {
    let upstream = br#"data: {"type":"response.output_text.delta","response_id":"resp_1","created":1700000001,"model":"gpt-5.3-codex","delta":"hello "}

data: {"type":"response.output_text.delta","response_id":"resp_1","created":1700000001,"model":"gpt-5.3-codex","delta":"world"}

data: {"type":"response.completed","response":{"id":"resp_1","created":1700000001,"model":"gpt-5.3-codex","usage":{"input_tokens":10,"output_tokens":2,"total_tokens":12}}}

data: [DONE]

"#;
    let (body, content_type) = adapt_upstream_response(
        ResponseAdapter::OpenAIChatCompletionsSse,
        Some("text/event-stream"),
        upstream,
    )
    .expect("convert response");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("parse converted body");
    assert_eq!(content_type, "application/json");
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(serde_json::Value::as_str),
        Some("hello world")
    );
}

#[test]
fn openai_chat_stream_collapse_avoids_done_and_item_text_duplication() {
    let upstream = br#"data: {"type":"response.output_text.delta","response_id":"resp_dup_1","created":1700000010,"model":"gpt-5.3-codex","delta":"hello "}

data: {"type":"response.output_text.delta","response_id":"resp_dup_1","created":1700000010,"model":"gpt-5.3-codex","delta":"world"}

data: {"type":"response.output_text.done","response_id":"resp_dup_1","created":1700000010,"model":"gpt-5.3-codex","text":"hello world"}

data: {"type":"response.content_part.done","response_id":"resp_dup_1","created":1700000010,"model":"gpt-5.3-codex","part":{"type":"output_text","text":"hello world"}}

data: {"type":"response.output_item.done","response_id":"resp_dup_1","created":1700000010,"model":"gpt-5.3-codex","item":{"type":"message","content":[{"type":"output_text","text":"hello world"}]}}

data: {"type":"response.completed","response":{"id":"resp_dup_1","created":1700000010,"model":"gpt-5.3-codex","usage":{"input_tokens":10,"output_tokens":2,"total_tokens":12}}}

data: [DONE]

"#;
    let (body, content_type) = adapt_upstream_response(
        ResponseAdapter::OpenAIChatCompletionsJson,
        Some("text/event-stream"),
        upstream,
    )
    .expect("convert response");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("parse converted body");
    assert_eq!(content_type, "application/json");
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(serde_json::Value::as_str),
        Some("hello world")
    );
}

#[test]
fn openai_chat_stream_response_accepts_output_item_done_text() {
    let upstream = br#"data: {"type":"response.output_item.done","response_id":"resp_2","created":1700000002,"model":"gpt-5.3-codex","item":{"type":"message","content":[{"type":"output_text","text":"from output item"}]}}

data: {"type":"response.completed","response":{"id":"resp_2","created":1700000002,"model":"gpt-5.3-codex","usage":{"input_tokens":8,"output_tokens":3,"total_tokens":11}}}

data: [DONE]

"#;
    let (body, content_type) = adapt_upstream_response(
        ResponseAdapter::OpenAIChatCompletionsJson,
        Some("text/event-stream"),
        upstream,
    )
    .expect("convert response");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("parse converted body");
    assert_eq!(content_type, "application/json");
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(serde_json::Value::as_str),
        Some("from output item")
    );
}

#[test]
fn openai_chat_stream_response_accepts_output_item_added_text() {
    let upstream = br#"data: {"type":"response.output_item.added","response_id":"resp_2b","created":1700000002,"model":"gpt-5.3-codex","item":{"type":"message","content":[{"type":"output_text","text":"from output item added"}]}}

data: {"type":"response.completed","response":{"id":"resp_2b","created":1700000002,"model":"gpt-5.3-codex","usage":{"input_tokens":8,"output_tokens":3,"total_tokens":11}}}

data: [DONE]

"#;
    let (body, content_type) = adapt_upstream_response(
        ResponseAdapter::OpenAIChatCompletionsJson,
        Some("text/event-stream"),
        upstream,
    )
    .expect("convert response");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("parse converted body");
    assert_eq!(content_type, "application/json");
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(serde_json::Value::as_str),
        Some("from output item added")
    );
}

#[test]
fn openai_chat_stream_response_completed_only_preserves_tool_calls() {
    let upstream = br#"data: {"type":"response.completed","response":{"id":"resp_tool_only","created":1700000005,"model":"gpt-5.3-codex","output":[{"type":"function_call","call_id":"call_tool_only","name":"read_file","arguments":"{\"path\":\"README.md\"}"}],"usage":{"input_tokens":7,"output_tokens":3,"total_tokens":10}}}

data: [DONE]

"#;
    let (body, content_type) = adapt_upstream_response(
        ResponseAdapter::OpenAIChatCompletionsJson,
        Some("text/event-stream"),
        upstream,
    )
    .expect("convert response");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("parse converted body");
    assert_eq!(content_type, "application/json");
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content")),
        Some(&serde_json::Value::Null)
    );
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("tool_calls"))
            .and_then(|tool_calls| tool_calls.get(0))
            .and_then(|tool_call| tool_call.get("id"))
            .and_then(serde_json::Value::as_str),
        Some("call_tool_only")
    );
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("tool_calls"))
            .and_then(|tool_calls| tool_calls.get(0))
            .and_then(|tool_call| tool_call.get("function"))
            .and_then(|function| function.get("name"))
            .and_then(serde_json::Value::as_str),
        Some("read_file")
    );
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("tool_calls"))
            .and_then(|tool_calls| tool_calls.get(0))
            .and_then(|tool_call| tool_call.get("function"))
            .and_then(|function| function.get("arguments"))
            .and_then(serde_json::Value::as_str),
        Some("{\"path\":\"README.md\"}")
    );
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("finish_reason"))
            .and_then(serde_json::Value::as_str),
        Some("tool_calls")
    );
}

#[test]
fn openai_chat_stream_response_delta_only_preserves_tool_calls() {
    let upstream = br#"data: {"type":"response.output_item.added","response_id":"resp_tool_delta","created":1700000006,"model":"gpt-5.3-codex","output_index":0,"item":{"type":"function_call","call_id":"call_tool_delta","name":"read_file"}}

data: {"type":"response.function_call_arguments.delta","response_id":"resp_tool_delta","created":1700000006,"model":"gpt-5.3-codex","output_index":0,"delta":"{\"path\":\"REA"}

data: {"type":"response.function_call_arguments.delta","response_id":"resp_tool_delta","created":1700000006,"model":"gpt-5.3-codex","output_index":0,"delta":"DME.md\"}"}

data: {"type":"response.completed","response":{"id":"resp_tool_delta","created":1700000006,"model":"gpt-5.3-codex","usage":{"input_tokens":8,"output_tokens":2,"total_tokens":10}}}

data: [DONE]

"#;
    let (body, content_type) = adapt_upstream_response(
        ResponseAdapter::OpenAIChatCompletionsJson,
        Some("text/event-stream"),
        upstream,
    )
    .expect("convert response");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("parse converted body");
    assert_eq!(content_type, "application/json");
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("tool_calls"))
            .and_then(|tool_calls| tool_calls.get(0))
            .and_then(|tool_call| tool_call.get("id"))
            .and_then(serde_json::Value::as_str),
        Some("call_tool_delta")
    );
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("tool_calls"))
            .and_then(|tool_calls| tool_calls.get(0))
            .and_then(|tool_call| tool_call.get("function"))
            .and_then(|function| function.get("arguments"))
            .and_then(serde_json::Value::as_str),
        Some("{\"path\":\"README.md\"}")
    );
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("finish_reason"))
            .and_then(serde_json::Value::as_str),
        Some("tool_calls")
    );
}

#[test]
fn openai_chat_stream_chunk_maps_function_call_argument_delta() {
    let value = serde_json::json!({
        "type": "response.function_call_arguments.delta",
        "response_id": "resp_call_1",
        "created": 1700000100,
        "model": "gpt-5.3-codex",
        "output_index": 0,
        "delta": "{\"x\":1}"
    });
    let mapped =
        convert_openai_chat_stream_chunk(&value).expect("map function_call_arguments.delta");
    assert_eq!(
        mapped.get("object").and_then(serde_json::Value::as_str),
        Some("chat.completion.chunk")
    );
    assert_eq!(
        mapped
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("delta"))
            .and_then(|delta| delta.get("tool_calls"))
            .and_then(|tool_calls| tool_calls.get(0))
            .and_then(|tool_call| tool_call.get("function"))
            .and_then(|function| function.get("arguments"))
            .and_then(serde_json::Value::as_str),
        Some("{\"x\":1}")
    );
}

#[test]
fn openai_chat_stream_chunk_fallback_maps_unknown_text_event() {
    let value = serde_json::json!({
        "type": "response.output_markdown.delta",
        "response_id": "resp_txt_1",
        "created": 1700000101,
        "model": "gpt-5.3-codex",
        "delta": "fallback text"
    });
    let mapped = convert_openai_chat_stream_chunk(&value).expect("map unknown text event");
    assert_eq!(
        mapped
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("delta"))
            .and_then(|delta| delta.get("content"))
            .and_then(serde_json::Value::as_str),
        Some("fallback text")
    );
}

#[test]
fn openai_completions_stream_chunk_fallback_maps_unknown_text_event() {
    let value = serde_json::json!({
        "type": "response.output_markdown.delta",
        "response_id": "resp_txt_2",
        "created": 1700000102,
        "model": "gpt-5.3-codex",
        "delta": "completion fallback"
    });
    let mapped = convert_openai_completions_stream_chunk(&value).expect("map unknown text event");
    assert_eq!(
        mapped
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("text"))
            .and_then(serde_json::Value::as_str),
        Some("completion fallback")
    );
}

#[test]
fn openai_chat_stream_response_completed_only_still_outputs_text() {
    let upstream = br#"data: {"type":"response.completed","response":{"id":"resp_3","created":1700000003,"model":"gpt-5.3-codex","output":[{"type":"message","content":[{"type":"output_text","text":"completed only text"}]}],"usage":{"input_tokens":8,"output_tokens":3,"total_tokens":11}}}

data: [DONE]

"#;
    let (body, content_type) = adapt_upstream_response(
        ResponseAdapter::OpenAIChatCompletionsJson,
        Some("text/event-stream"),
        upstream,
    )
    .expect("convert response");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("parse converted body");
    assert_eq!(content_type, "application/json");
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(serde_json::Value::as_str),
        Some("completed only text")
    );
}

#[test]
fn openai_chat_stream_event_only_completed_still_outputs_text() {
    let upstream = br#"event: response.completed
data: {"response":{"id":"resp_3_evt","created":1700000003,"model":"gpt-5.3-codex","output":[{"type":"message","content":[{"type":"output_text","text":"event completed only text"}]}],"usage":{"input_tokens":8,"output_tokens":3,"total_tokens":11}}}

data: [DONE]

"#;
    let (body, content_type) = adapt_upstream_response(
        ResponseAdapter::OpenAIChatCompletionsJson,
        Some("text/event-stream"),
        upstream,
    )
    .expect("convert response");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("parse converted body");
    assert_eq!(content_type, "application/json");
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(serde_json::Value::as_str),
        Some("event completed only text")
    );
}

#[test]
fn openai_chat_stream_response_done_only_still_outputs_text() {
    let upstream = br#"data: {"type":"response.done","response":{"id":"resp_3_done","created":1700000003,"model":"gpt-5.3-codex","output":[{"type":"message","content":[{"type":"output_text","text":"done only text"}]}],"usage":{"input_tokens":8,"output_tokens":3,"total_tokens":11}}}

data: [DONE]

"#;
    let (body, content_type) = adapt_upstream_response(
        ResponseAdapter::OpenAIChatCompletionsJson,
        Some("text/event-stream"),
        upstream,
    )
    .expect("convert response");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("parse converted body");
    assert_eq!(content_type, "application/json");
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(serde_json::Value::as_str),
        Some("done only text")
    );
}

#[test]
fn openai_completions_response_is_converted_from_responses_json() {
    let upstream = br#"{
        "id":"resp_1",
        "object":"response",
        "created":1700000000,
        "model":"gpt-5.3-codex",
        "output":[{"type":"message","content":[{"type":"output_text","text":"hello world"}]}],
        "usage":{"input_tokens":10,"output_tokens":2,"total_tokens":12}
    }"#;
    let (body, content_type) = adapt_upstream_response(
        ResponseAdapter::OpenAICompletionsJson,
        Some("application/json"),
        upstream,
    )
    .expect("convert response");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("parse converted body");
    assert_eq!(content_type, "application/json");
    assert_eq!(
        value.get("object").and_then(serde_json::Value::as_str),
        Some("text_completion")
    );
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("text"))
            .and_then(serde_json::Value::as_str),
        Some("hello world")
    );
}

#[test]
fn openai_completions_stream_completed_only_still_outputs_text() {
    let upstream = br#"data: {"type":"response.completed","response":{"id":"resp_4","created":1700000004,"model":"gpt-5.3-codex","output":[{"type":"message","content":[{"type":"output_text","text":"completed only completion text"}]}],"usage":{"input_tokens":9,"output_tokens":4,"total_tokens":13}}}

data: [DONE]

"#;
    let (body, content_type) = adapt_upstream_response(
        ResponseAdapter::OpenAICompletionsJson,
        Some("text/event-stream"),
        upstream,
    )
    .expect("convert response");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("parse converted body");
    assert_eq!(content_type, "application/json");
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("text"))
            .and_then(serde_json::Value::as_str),
        Some("completed only completion text")
    );
}

#[test]
fn openai_completions_stream_event_only_done_still_outputs_text() {
    let upstream = br#"event: response.done
data: {"response":{"id":"resp_4_done_evt","created":1700000004,"model":"gpt-5.3-codex","output":[{"type":"message","content":[{"type":"output_text","text":"event done only completion text"}]}],"usage":{"input_tokens":9,"output_tokens":4,"total_tokens":13}}}

data: [DONE]

"#;
    let (body, content_type) = adapt_upstream_response(
        ResponseAdapter::OpenAICompletionsJson,
        Some("text/event-stream"),
        upstream,
    )
    .expect("convert response");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("parse converted body");
    assert_eq!(content_type, "application/json");
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("text"))
            .and_then(serde_json::Value::as_str),
        Some("event done only completion text")
    );
}

#[test]
fn openai_completions_stream_done_only_still_outputs_text() {
    let upstream = br#"data: {"type":"response.done","response":{"id":"resp_4_done","created":1700000004,"model":"gpt-5.3-codex","output":[{"type":"message","content":[{"type":"output_text","text":"done only completion text"}]}],"usage":{"input_tokens":9,"output_tokens":4,"total_tokens":13}}}

data: [DONE]

"#;
    let (body, content_type) = adapt_upstream_response(
        ResponseAdapter::OpenAICompletionsJson,
        Some("text/event-stream"),
        upstream,
    )
    .expect("convert response");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("parse converted body");
    assert_eq!(content_type, "application/json");
    assert_eq!(
        value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("text"))
            .and_then(serde_json::Value::as_str),
        Some("done only completion text")
    );
}

#[test]
fn anthropic_messages_are_the_only_path_adapted_to_responses() {
    let body =
        br#"{"model":"claude-3-5-sonnet","messages":[{"role":"user","content":"hello"}]}"#.to_vec();
    let adapted = adapt_request_for_protocol(PROTOCOL_ANTHROPIC_NATIVE, "/v1/messages", body)
        .expect("adapt request");
    assert_eq!(adapted.path, "/v1/responses");
    assert_ne!(adapted.response_adapter, ResponseAdapter::Passthrough);
}

#[test]
fn anthropic_messages_map_text_and_base64_image_to_responses_input() {
    let body = serde_json::json!({
        "model": "claude-3-5-sonnet",
        "messages": [{
            "role": "user",
            "content": [
                {
                    "type": "text",
                    "text": "阅读一下这个图片里面是什么"
                },
                {
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": "image/png",
                        "data": "ZmFrZV9pbWFnZQ=="
                    }
                }
            ]
        }]
    });
    let adapted = adapt_request_for_protocol(
        PROTOCOL_ANTHROPIC_NATIVE,
        "/v1/messages",
        serde_json::to_vec(&body).expect("serialize body"),
    )
    .expect("adapt request");
    let value: serde_json::Value =
        serde_json::from_slice(&adapted.body).expect("parse adapted body");
    assert_eq!(adapted.path, "/v1/responses");
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(0))
            .and_then(|item| item.get("role"))
            .and_then(serde_json::Value::as_str),
        Some("user")
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(0))
            .and_then(|item| item.get("content"))
            .and_then(|content| content.get(0))
            .and_then(|part| part.get("type"))
            .and_then(serde_json::Value::as_str),
        Some("input_text")
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(0))
            .and_then(|item| item.get("content"))
            .and_then(|content| content.get(0))
            .and_then(|part| part.get("text"))
            .and_then(serde_json::Value::as_str),
        Some("阅读一下这个图片里面是什么")
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(0))
            .and_then(|item| item.get("content"))
            .and_then(|content| content.get(1))
            .and_then(|part| part.get("type"))
            .and_then(serde_json::Value::as_str),
        Some("input_image")
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(0))
            .and_then(|item| item.get("content"))
            .and_then(|content| content.get(1))
            .and_then(|part| part.get("image_url"))
            .and_then(serde_json::Value::as_str),
        Some("data:image/png;base64,ZmFrZV9pbWFnZQ==")
    );
}

#[test]
fn anthropic_messages_map_url_image_to_responses_input() {
    let body = serde_json::json!({
        "model": "claude-3-5-sonnet",
        "messages": [{
            "role": "user",
            "content": [{
                "type": "image",
                "source": {
                    "type": "url",
                    "url": "https://example.com/screenshot.png"
                }
            }]
        }]
    });
    let adapted = adapt_request_for_protocol(
        PROTOCOL_ANTHROPIC_NATIVE,
        "/v1/messages",
        serde_json::to_vec(&body).expect("serialize body"),
    )
    .expect("adapt request");
    let value: serde_json::Value =
        serde_json::from_slice(&adapted.body).expect("parse adapted body");
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(0))
            .and_then(|item| item.get("content"))
            .and_then(|content| content.get(0))
            .and_then(|part| part.get("image_url"))
            .and_then(serde_json::Value::as_str),
        Some("https://example.com/screenshot.png")
    );
}

#[test]
fn anthropic_messages_map_thinking_and_output_config_effort_to_responses() {
    let body = serde_json::json!({
        "model": "claude-sonnet-4-5",
        "thinking": {
            "type": "enabled",
            "budget_tokens": 10000
        },
        "output_config": {
            "effort": "medium"
        },
        "messages": [
            {
                "role": "assistant",
                "content": [
                    {
                        "type": "thinking",
                        "thinking": "先检查上下文。",
                        "signature": "sig_claude_prev_turn"
                    }
                ]
            },
            {
                "role": "user",
                "content": "继续"
            }
        ]
    });
    let adapted = adapt_request_for_protocol(
        PROTOCOL_ANTHROPIC_NATIVE,
        "/v1/messages",
        serde_json::to_vec(&body).expect("serialize body"),
    )
    .expect("adapt request");
    let value: serde_json::Value =
        serde_json::from_slice(&adapted.body).expect("parse adapted body");

    assert_eq!(
        value
            .get("reasoning")
            .and_then(|reasoning| reasoning.get("effort"))
            .and_then(serde_json::Value::as_str),
        Some("medium")
    );
    assert_eq!(
        value
            .get("reasoning")
            .and_then(|reasoning| reasoning.get("summary"))
            .and_then(serde_json::Value::as_str),
        Some("detailed")
    );
    assert_eq!(
        value
            .get("encrypted_content")
            .and_then(serde_json::Value::as_str),
        Some("sig_claude_prev_turn")
    );
}

#[test]
fn anthropic_messages_map_disabled_thinking_to_summary_none() {
    let body = serde_json::json!({
        "model": "claude-sonnet-4-5",
        "thinking": {
            "type": "disabled"
        },
        "messages": [{
            "role": "user",
            "content": "hello"
        }]
    });
    let adapted = adapt_request_for_protocol(
        PROTOCOL_ANTHROPIC_NATIVE,
        "/v1/messages",
        serde_json::to_vec(&body).expect("serialize body"),
    )
    .expect("adapt request");
    let value: serde_json::Value =
        serde_json::from_slice(&adapted.body).expect("parse adapted body");

    assert_eq!(
        value
            .get("reasoning")
            .and_then(|reasoning| reasoning.get("summary"))
            .and_then(serde_json::Value::as_str),
        Some("none")
    );
}

#[test]
fn anthropic_assistant_tool_use_preserves_text_order_in_responses_input() {
    let body = serde_json::json!({
        "model": "claude-3-5-sonnet",
        "messages": [
            {
                "role": "user",
                "content": "继续上一轮"
            },
            {
                "role": "assistant",
                "content": [
                    {
                        "type": "text",
                        "text": "先读取 README。"
                    },
                    {
                        "type": "tool_use",
                        "id": "toolu_readme_1",
                        "name": "read_file",
                        "input": {
                            "path": "README.md"
                        }
                    },
                    {
                        "type": "text",
                        "text": "读取后继续总结。"
                    }
                ]
            }
        ]
    });
    let adapted = adapt_request_for_protocol(
        PROTOCOL_ANTHROPIC_NATIVE,
        "/v1/messages",
        serde_json::to_vec(&body).expect("serialize body"),
    )
    .expect("adapt request");
    let value: serde_json::Value =
        serde_json::from_slice(&adapted.body).expect("parse adapted body");

    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(1))
            .and_then(|item| item.get("type"))
            .and_then(serde_json::Value::as_str),
        Some("message")
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(1))
            .and_then(|item| item.get("role"))
            .and_then(serde_json::Value::as_str),
        Some("assistant")
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(1))
            .and_then(|item| item.get("content"))
            .and_then(|content| content.get(0))
            .and_then(|part| part.get("text"))
            .and_then(serde_json::Value::as_str),
        Some("先读取 README。")
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(2))
            .and_then(|item| item.get("type"))
            .and_then(serde_json::Value::as_str),
        Some("function_call")
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(2))
            .and_then(|item| item.get("call_id"))
            .and_then(serde_json::Value::as_str),
        Some("toolu_readme_1")
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(2))
            .and_then(|item| item.get("name"))
            .and_then(serde_json::Value::as_str),
        Some("read_file")
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(2))
            .and_then(|item| item.get("arguments"))
            .and_then(serde_json::Value::as_str),
        Some("{\"path\":\"README.md\"}")
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(3))
            .and_then(|item| item.get("type"))
            .and_then(serde_json::Value::as_str),
        Some("message")
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(3))
            .and_then(|item| item.get("role"))
            .and_then(serde_json::Value::as_str),
        Some("assistant")
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(3))
            .and_then(|item| item.get("content"))
            .and_then(|content| content.get(0))
            .and_then(|part| part.get("text"))
            .and_then(serde_json::Value::as_str),
        Some("读取后继续总结。")
    );
}

#[test]
fn anthropic_tool_result_with_image_maps_to_function_call_output_items() {
    let body = serde_json::json!({
        "model": "claude-3-5-sonnet",
        "messages": [
            {
                "role": "assistant",
                "content": [
                    {
                        "type": "tool_use",
                        "id": "toolu_image_1",
                        "name": "inspect_image",
                        "input": {
                            "path": "screen.png"
                        }
                    }
                ]
            },
            {
                "role": "user",
                "content": [
                    {
                        "type": "tool_result",
                        "tool_use_id": "toolu_image_1",
                        "content": [
                            {
                                "type": "text",
                                "text": "这是截图。"
                            },
                            {
                                "type": "image",
                                "source": {
                                    "type": "base64",
                                    "media_type": "image/png",
                                    "data": "ZmFrZV9pbWFnZQ=="
                                }
                            }
                        ]
                    }
                ]
            }
        ]
    });
    let adapted = adapt_request_for_protocol(
        PROTOCOL_ANTHROPIC_NATIVE,
        "/v1/messages",
        serde_json::to_vec(&body).expect("serialize body"),
    )
    .expect("adapt request");
    let value: serde_json::Value =
        serde_json::from_slice(&adapted.body).expect("parse adapted body");

    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(1))
            .and_then(|item| item.get("type"))
            .and_then(serde_json::Value::as_str),
        Some("function_call_output")
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(1))
            .and_then(|item| item.get("call_id"))
            .and_then(serde_json::Value::as_str),
        Some("toolu_image_1")
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(1))
            .and_then(|item| item.get("output"))
            .and_then(|output| output.get(0))
            .and_then(|part| part.get("type"))
            .and_then(serde_json::Value::as_str),
        Some("input_text")
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(1))
            .and_then(|item| item.get("output"))
            .and_then(|output| output.get(0))
            .and_then(|part| part.get("text"))
            .and_then(serde_json::Value::as_str),
        Some("这是截图。")
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(1))
            .and_then(|item| item.get("output"))
            .and_then(|output| output.get(1))
            .and_then(|part| part.get("type"))
            .and_then(serde_json::Value::as_str),
        Some("input_image")
    );
    assert_eq!(
        value
            .get("input")
            .and_then(|input| input.get(1))
            .and_then(|item| item.get("output"))
            .and_then(|output| output.get(1))
            .and_then(|part| part.get("image_url"))
            .and_then(serde_json::Value::as_str),
        Some("data:image/png;base64,ZmFrZV9pbWFnZQ==")
    );
}

#[test]
fn anthropic_json_response_maps_reasoning_item_to_thinking_block() {
    let upstream = serde_json::json!({
        "id": "resp_reasoning_1",
        "object": "response",
        "created": 1700001200,
        "model": "gpt-5.3-codex",
        "output": [
            {
                "type": "reasoning",
                "id": "rs_1",
                "summary": [{"type": "summary_text", "text": "先检查配置，再执行。"}],
                "encrypted_content": "sig_reasoning_1"
            },
            {
                "type": "message",
                "content": [{"type": "output_text", "text": "已经处理完成。"}]
            }
        ],
        "usage": {"input_tokens": 12, "output_tokens": 6, "total_tokens": 18}
    });
    let (body, content_type) = adapt_upstream_response(
        ResponseAdapter::AnthropicJson,
        Some("application/json"),
        &serde_json::to_vec(&upstream).expect("serialize upstream"),
    )
    .expect("convert response");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("parse converted body");
    assert_eq!(content_type, "application/json");
    assert_eq!(
        value
            .get("content")
            .and_then(|content| content.get(0))
            .and_then(|block| block.get("type"))
            .and_then(serde_json::Value::as_str),
        Some("thinking")
    );
    assert_eq!(
        value
            .get("content")
            .and_then(|content| content.get(0))
            .and_then(|block| block.get("thinking"))
            .and_then(serde_json::Value::as_str),
        Some("先检查配置，再执行。")
    );
    assert_eq!(
        value
            .get("content")
            .and_then(|content| content.get(0))
            .and_then(|block| block.get("signature"))
            .and_then(serde_json::Value::as_str),
        Some("sig_reasoning_1")
    );
}

#[test]
fn anthropic_sse_response_maps_reasoning_deltas_to_thinking_events() {
    let upstream = r#"data: {"type":"response.output_item.added","output_index":0,"item":{"type":"reasoning","id":"rs_stream_1","summary":[]}}

data: {"type":"response.reasoning_summary_text.delta","output_index":0,"summary_index":0,"delta":"先读配置"}

data: {"type":"response.output_item.done","output_index":0,"item":{"type":"reasoning","id":"rs_stream_1","summary":[{"type":"summary_text","text":"先读配置"}],"encrypted_content":"sig_stream_1"}}

data: [DONE]

"#;
    let (body, content_type) = adapt_upstream_response(
        ResponseAdapter::AnthropicSse,
        Some("text/event-stream"),
        upstream.as_bytes(),
    )
    .expect("convert response");
    let text = String::from_utf8(body).expect("parse sse body");
    assert_eq!(content_type, "text/event-stream");
    assert!(text.contains("\"type\":\"thinking_delta\""));
    assert!(text.contains("\"thinking\":\"先读配置\""));
    assert!(text.contains("\"type\":\"signature_delta\""));
    assert!(text.contains("\"signature\":\"sig_stream_1\""));
}

#[test]
fn anthropic_json_response_from_sse_preserves_thinking_block() {
    let upstream = r#"data: {"type":"response.output_item.added","output_index":0,"item":{"type":"reasoning","id":"rs_stream_json_1","summary":[]}}

data: {"type":"response.reasoning_text.delta","output_index":0,"content_index":0,"delta":"逐步分析"}

data: {"type":"response.output_item.done","output_index":0,"item":{"type":"reasoning","id":"rs_stream_json_1","summary":[],"encrypted_content":"sig_stream_json_1"}}

data: [DONE]

"#;
    let (body, content_type) = adapt_upstream_response(
        ResponseAdapter::AnthropicJson,
        Some("text/event-stream"),
        upstream.as_bytes(),
    )
    .expect("convert response");
    let value: serde_json::Value = serde_json::from_slice(&body).expect("parse converted body");
    assert_eq!(content_type, "application/json");
    assert_eq!(
        value
            .get("content")
            .and_then(|content| content.get(0))
            .and_then(|block| block.get("type"))
            .and_then(serde_json::Value::as_str),
        Some("thinking")
    );
    assert_eq!(
        value
            .get("content")
            .and_then(|content| content.get(0))
            .and_then(|block| block.get("thinking"))
            .and_then(serde_json::Value::as_str),
        Some("逐步分析")
    );
    assert_eq!(
        value
            .get("content")
            .and_then(|content| content.get(0))
            .and_then(|block| block.get("signature"))
            .and_then(serde_json::Value::as_str),
        Some("sig_stream_json_1")
    );
}

#[test]
fn anthropic_chat_completions_still_passthrough() {
    let body =
        br#"{"model":"gpt-5.3-codex","messages":[{"role":"user","content":"hello"}]}"#.to_vec();
    let adapted = adapt_request_for_protocol(
        PROTOCOL_ANTHROPIC_NATIVE,
        "/v1/chat/completions",
        body.clone(),
    )
    .expect("adapt request");
    assert_eq!(adapted.path, "/v1/chat/completions");
    assert_eq!(adapted.body, body);
    assert_eq!(adapted.response_adapter, ResponseAdapter::Passthrough);
}
