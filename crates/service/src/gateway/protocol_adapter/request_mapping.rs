use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

use super::prompt_cache;

const DEFAULT_ANTHROPIC_MODEL: &str = "gpt-5.3-codex";
const DEFAULT_ANTHROPIC_REASONING: &str = "high";
const DEFAULT_ANTHROPIC_INSTRUCTIONS: &str =
    "You are Codex, a coding assistant that responds clearly and safely.";
const MAX_ANTHROPIC_TOOLS: usize = 16;
const DEFAULT_COMPLETIONS_PROMPT: &str = "Complete this:";
const DEFAULT_OPENAI_REASONING: &str = "medium";
const MAX_OPENAI_TOOL_NAME_LEN: usize = 64;

fn shorten_openai_tool_name_candidate(name: &str) -> String {
    if name.len() <= MAX_OPENAI_TOOL_NAME_LEN {
        return name.to_string();
    }
    if name.starts_with("mcp__") {
        if let Some(idx) = name.rfind("__") {
            if idx > 0 {
                let mut candidate = format!("mcp__{}", &name[idx + 2..]);
                if candidate.len() > MAX_OPENAI_TOOL_NAME_LEN {
                    candidate.truncate(MAX_OPENAI_TOOL_NAME_LEN);
                }
                return candidate;
            }
        }
    }
    name.chars().take(MAX_OPENAI_TOOL_NAME_LEN).collect()
}

fn collect_openai_tool_names(obj: &serde_json::Map<String, Value>) -> Vec<String> {
    let mut names = Vec::new();

    if let Some(tools) = obj.get("tools").and_then(Value::as_array) {
        for tool in tools {
            let Some(tool_obj) = tool.as_object() else {
                continue;
            };
            let tool_type = tool_obj
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !tool_type.is_empty() && tool_type != "function" {
                continue;
            }
            let name = tool_obj
                .get("function")
                .and_then(|function| function.get("name"))
                .or_else(|| tool_obj.get("name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if let Some(name) = name {
                names.push(name.to_string());
            }
        }
    }

    if let Some(name) = obj
        .get("tool_choice")
        .and_then(Value::as_object)
        .and_then(|tool_choice| {
            let tool_type = tool_choice
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if tool_type != "function" {
                return None;
            }
            tool_choice
                .get("function")
                .and_then(|function| function.get("name"))
                .or_else(|| tool_choice.get("name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
    {
        names.push(name.to_string());
    }

    if let Some(messages) = obj.get("messages").and_then(Value::as_array) {
        for message in messages {
            let Some(message_obj) = message.as_object() else {
                continue;
            };
            if message_obj.get("role").and_then(Value::as_str) != Some("assistant") {
                continue;
            }
            let Some(tool_calls) = message_obj.get("tool_calls").and_then(Value::as_array) else {
                continue;
            };
            for tool_call in tool_calls {
                let Some(name) = tool_call
                    .get("function")
                    .and_then(|function| function.get("name"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    continue;
                };
                names.push(name.to_string());
            }
        }
    }

    names
}

fn build_openai_tool_name_map(obj: &serde_json::Map<String, Value>) -> BTreeMap<String, String> {
    let mut unique_names = BTreeSet::new();
    for name in collect_openai_tool_names(obj) {
        unique_names.insert(name);
    }

    let mut used = BTreeSet::new();
    let mut out = BTreeMap::new();
    for name in unique_names {
        let base = shorten_openai_tool_name_candidate(name.as_str());
        let mut candidate = base.clone();
        let mut suffix = 1usize;
        while used.contains(&candidate) {
            let suffix_text = format!("_{suffix}");
            let mut truncated = base.clone();
            let limit = MAX_OPENAI_TOOL_NAME_LEN.saturating_sub(suffix_text.len());
            if truncated.len() > limit {
                truncated = truncated.chars().take(limit).collect();
            }
            candidate = format!("{truncated}{suffix_text}");
            suffix += 1;
        }
        used.insert(candidate.clone());
        out.insert(name, candidate);
    }
    out
}

fn shorten_openai_tool_name_with_map(
    name: &str,
    tool_name_map: &BTreeMap<String, String>,
) -> String {
    tool_name_map
        .get(name)
        .cloned()
        .unwrap_or_else(|| shorten_openai_tool_name_candidate(name))
}

fn build_openai_tool_name_restore_map(
    tool_name_map: &BTreeMap<String, String>,
) -> super::ToolNameRestoreMap {
    let mut restore_map = super::ToolNameRestoreMap::new();
    for (original, shortened) in tool_name_map {
        if original != shortened {
            restore_map.insert(shortened.clone(), original.clone());
        }
    }
    restore_map
}

fn normalize_openai_role_for_responses(role: &str) -> Option<&'static str> {
    match role {
        "system" | "developer" => Some("system"),
        "user" => Some("user"),
        "assistant" => Some("assistant"),
        "tool" => Some("tool"),
        _ => None,
    }
}

fn extract_openai_message_content_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(items) => {
            let mut out = String::new();
            for item in items {
                if let Some(text) = item.as_str() {
                    out.push_str(text);
                    continue;
                }
                let Some(item_obj) = item.as_object() else {
                    continue;
                };
                let item_type = item_obj
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                match item_type {
                    "text" | "input_text" | "output_text" => {
                        if let Some(text) = item_obj.get("text").and_then(Value::as_str) {
                            out.push_str(text);
                        }
                    }
                    _ => {}
                }
            }
            out
        }
        Value::Null => String::new(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn normalize_openai_chat_messages_for_responses(messages: &[Value]) -> Vec<Value> {
    let mut normalized = Vec::new();
    for message in messages {
        let Some(message_obj) = message.as_object() else {
            continue;
        };
        let Some(role) = message_obj.get("role").and_then(Value::as_str) else {
            continue;
        };
        let Some(normalized_role) = normalize_openai_role_for_responses(role) else {
            continue;
        };
        let mut out = serde_json::Map::new();
        out.insert(
            "role".to_string(),
            Value::String(normalized_role.to_string()),
        );

        if normalized_role == "tool" {
            if let Some(call_id) = message_obj
                .get("tool_call_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                out.insert(
                    "tool_call_id".to_string(),
                    Value::String(call_id.to_string()),
                );
            }
        }

        if let Some(content) = message_obj.get("content") {
            let content_text = extract_openai_message_content_text(content);
            if !content_text.trim().is_empty() {
                out.insert("content".to_string(), Value::String(content_text));
            }
        }

        if normalized_role == "assistant" {
            if let Some(tool_calls) = message_obj.get("tool_calls").and_then(Value::as_array) {
                let mapped_calls = tool_calls
                    .iter()
                    .filter_map(|tool_call| {
                        let tool_obj = tool_call.as_object()?;
                        let id = tool_obj
                            .get("id")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .unwrap_or("call_0");
                        let fn_obj = tool_obj.get("function").and_then(Value::as_object)?;
                        let name = fn_obj
                            .get("name")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|value| !value.is_empty())?;
                        let arguments = fn_obj
                            .get("arguments")
                            .map(|value| {
                                if let Some(text) = value.as_str() {
                                    text.to_string()
                                } else {
                                    serde_json::to_string(value)
                                        .unwrap_or_else(|_| "{}".to_string())
                                }
                            })
                            .unwrap_or_else(|| "{}".to_string());
                        Some(json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": arguments
                            }
                        }))
                    })
                    .collect::<Vec<_>>();
                if !mapped_calls.is_empty() {
                    out.insert("tool_calls".to_string(), Value::Array(mapped_calls));
                }
            }
        }

        normalized.push(Value::Object(out));
    }
    normalized
}

fn map_openai_chat_tools_to_responses(
    obj: &serde_json::Map<String, Value>,
    tool_name_map: &BTreeMap<String, String>,
) -> Option<Vec<Value>> {
    let tools = obj.get("tools")?.as_array()?;
    let mut out = Vec::new();
    for tool in tools {
        let Some(tool_obj) = tool.as_object() else {
            continue;
        };
        let tool_type = tool_obj
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if tool_type != "function" {
            out.push(tool.clone());
            continue;
        }
        let Some(function) = tool_obj.get("function").and_then(Value::as_object) else {
            continue;
        };
        let Some(name) = function
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let mapped_name = shorten_openai_tool_name_with_map(name, tool_name_map);
        let mut mapped = serde_json::Map::new();
        mapped.insert("type".to_string(), Value::String("function".to_string()));
        mapped.insert("name".to_string(), Value::String(mapped_name));
        if let Some(description) = function.get("description") {
            mapped.insert("description".to_string(), description.clone());
        }
        if let Some(parameters) = function.get("parameters") {
            mapped.insert("parameters".to_string(), parameters.clone());
        }
        if let Some(strict) = function.get("strict") {
            mapped.insert("strict".to_string(), strict.clone());
        }
        out.push(Value::Object(mapped));
    }
    Some(out)
}

fn map_openai_chat_tool_choice_to_responses(
    value: &Value,
    tool_name_map: &BTreeMap<String, String>,
) -> Option<Value> {
    if let Some(raw) = value.as_str() {
        return Some(Value::String(raw.to_string()));
    }
    let obj = value.as_object()?;
    let tool_type = obj.get("type").and_then(Value::as_str).unwrap_or_default();
    if tool_type != "function" {
        return Some(value.clone());
    }
    let name = obj
        .get("function")
        .and_then(|function| function.get("name"))
        .and_then(Value::as_str)
        .or_else(|| obj.get("name").and_then(Value::as_str))
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty())?;
    let mapped_name = shorten_openai_tool_name_with_map(name, tool_name_map);
    Some(json!({
        "type": "function",
        "name": mapped_name
    }))
}

pub(super) fn convert_openai_chat_completions_request(
    body: &[u8],
) -> Result<(Vec<u8>, bool, super::ToolNameRestoreMap), String> {
    let payload: Value = serde_json::from_slice(body)
        .map_err(|_| "invalid chat.completions request json".to_string())?;
    let Some(obj) = payload.as_object() else {
        return Err("chat.completions request body must be an object".to_string());
    };

    let tool_name_map = build_openai_tool_name_map(obj);
    let stream = obj.get("stream").and_then(Value::as_bool).unwrap_or(false);
    let source_messages = obj
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| "chat.completions messages field is required".to_string())?;
    let normalized_messages = normalize_openai_chat_messages_for_responses(source_messages);
    let (instructions, input_items) =
        convert_chat_messages_to_responses_input(&normalized_messages, &tool_name_map)?;

    let mut out = serde_json::Map::new();
    if let Some(model) = obj.get("model") {
        out.insert("model".to_string(), model.clone());
    }
    out.insert(
        "instructions".to_string(),
        Value::String(instructions.unwrap_or_default()),
    );
    out.insert("input".to_string(), Value::Array(input_items));
    out.insert("stream".to_string(), Value::Bool(stream));
    out.insert("store".to_string(), Value::Bool(false));
    // 对齐 CPA：
    // - /v1/chat/completions 与 /v1/completions 的 stream 语义默认跟随客户端；
    // - stream_passthrough 默认 false，仅当客户端显式传 true 时才透传其 stream=false。
    let stream_passthrough = obj
        .get("stream_passthrough")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    out.insert(
        "stream_passthrough".to_string(),
        Value::Bool(stream_passthrough),
    );

    let reasoning_effort = obj
        .get("reasoning_effort")
        .and_then(Value::as_str)
        .and_then(crate::reasoning_effort::normalize_reasoning_effort)
        .or_else(|| {
            obj.get("reasoning")
                .and_then(|reasoning| reasoning.get("effort"))
                .and_then(Value::as_str)
                .and_then(crate::reasoning_effort::normalize_reasoning_effort)
        })
        .unwrap_or(DEFAULT_OPENAI_REASONING)
        .to_string();
    out.insert(
        "reasoning".to_string(),
        json!({
            "effort": reasoning_effort
        }),
    );

    let parallel_tool_calls = obj
        .get("parallel_tool_calls")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    out.insert(
        "parallel_tool_calls".to_string(),
        Value::Bool(parallel_tool_calls),
    );
    out.insert(
        "include".to_string(),
        Value::Array(vec![Value::String(
            "reasoning.encrypted_content".to_string(),
        )]),
    );
    if let Some(service_tier) = obj.get("service_tier") {
        // 中文注释：最新 Codex `/responses` 会携带 service_tier；
        // chat.completions 适配到 responses 时透传，避免静默降级到默认层级。
        out.insert("service_tier".to_string(), service_tier.clone());
    }

    if let Some(tools) = map_openai_chat_tools_to_responses(obj, &tool_name_map) {
        if !tools.is_empty() {
            out.insert("tools".to_string(), Value::Array(tools));
        }
    }
    if let Some(tool_choice) = obj
        .get("tool_choice")
        .and_then(|value| map_openai_chat_tool_choice_to_responses(value, &tool_name_map))
    {
        out.insert("tool_choice".to_string(), tool_choice);
    }
    if let Some(text) = obj.get("response_format").cloned() {
        out.insert("text".to_string(), json!({ "format": text }));
    }

    let tool_name_restore_map = build_openai_tool_name_restore_map(&tool_name_map);
    serde_json::to_vec(&Value::Object(out))
        .map(|bytes| (bytes, stream, tool_name_restore_map))
        .map_err(|err| format!("convert chat.completions request failed: {err}"))
}

fn stringify_completion_prompt(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Array(items) => {
            let parts = items
                .iter()
                .filter_map(stringify_completion_prompt)
                .collect::<Vec<_>>();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        Value::Null => None,
        other => serde_json::to_string(other).ok(),
    }
}

pub(super) fn convert_openai_completions_request(body: &[u8]) -> Result<(Vec<u8>, bool), String> {
    let payload: Value =
        serde_json::from_slice(body).map_err(|_| "invalid completions request json".to_string())?;
    let Some(obj) = payload.as_object() else {
        return Err("completions request body must be an object".to_string());
    };

    let stream = obj.get("stream").and_then(Value::as_bool).unwrap_or(false);
    let prompt = obj
        .get("prompt")
        .and_then(stringify_completion_prompt)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_COMPLETIONS_PROMPT.to_string());

    let mut out = serde_json::Map::new();
    if let Some(model) = obj.get("model") {
        out.insert("model".to_string(), model.clone());
    }
    out.insert(
        "messages".to_string(),
        json!([
            {
                "role": "user",
                "content": prompt
            }
        ]),
    );

    const COPIED_KEYS: [&str; 12] = [
        "max_tokens",
        "temperature",
        "top_p",
        "frequency_penalty",
        "presence_penalty",
        "stop",
        "stream",
        "logprobs",
        "top_logprobs",
        "n",
        "user",
        "stream_passthrough",
    ];
    for key in COPIED_KEYS {
        if let Some(value) = obj.get(key) {
            out.insert(key.to_string(), value.clone());
        }
    }

    serde_json::to_vec(&Value::Object(out))
        .map(|bytes| (bytes, stream))
        .map_err(|err| format!("convert completions request failed: {err}"))
}

pub(super) fn convert_anthropic_messages_request(body: &[u8]) -> Result<(Vec<u8>, bool), String> {
    let payload: Value =
        serde_json::from_slice(body).map_err(|_| "invalid claude request json".to_string())?;
    let Some(obj) = payload.as_object() else {
        return Err("claude request body must be an object".to_string());
    };

    let mut messages = Vec::new();

    if let Some(system) = obj.get("system") {
        let system_text = extract_text_content(system)?;
        if !system_text.trim().is_empty() {
            messages.push(json!({
                "role": "system",
                "content": system_text,
            }));
        }
    }

    let source_messages = obj
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| "claude messages field is required".to_string())?;
    for message in source_messages {
        let Some(message_obj) = message.as_object() else {
            return Err("invalid claude message item".to_string());
        };
        let role = message_obj
            .get("role")
            .and_then(Value::as_str)
            .ok_or_else(|| "claude message role is required".to_string())?;
        let content = message_obj
            .get("content")
            .ok_or_else(|| "claude message content is required".to_string())?;
        match role {
            "assistant" => append_assistant_messages(&mut messages, content)?,
            "user" => append_user_messages(&mut messages, content)?,
            "tool" => append_tool_role_message(&mut messages, message_obj, content)?,
            other => return Err(format!("unsupported claude message role: {other}")),
        }
    }

    let empty_tool_name_map = BTreeMap::new();
    let (instructions, input_items) =
        convert_chat_messages_to_responses_input(&messages, &empty_tool_name_map)?;
    let mut out = serde_json::Map::new();
    let resolved_model = resolve_anthropic_upstream_model(obj);
    out.insert("model".to_string(), Value::String(resolved_model));
    let resolved_instructions = instructions
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_ANTHROPIC_INSTRUCTIONS);
    out.insert(
        "instructions".to_string(),
        Value::String(resolved_instructions.to_string()),
    );
    out.insert(
        "text".to_string(),
        json!({
            "format": {
                "type": "text",
            }
        }),
    );
    let resolved_reasoning = resolve_anthropic_reasoning_effort(obj).to_string();
    let mut reasoning = serde_json::Map::new();
    reasoning.insert(
        "effort".to_string(),
        Value::String(resolved_reasoning.clone()),
    );
    if let Some(summary) = resolve_anthropic_reasoning_summary(obj) {
        reasoning.insert("summary".to_string(), Value::String(summary.to_string()));
    }
    out.insert("reasoning".to_string(), Value::Object(reasoning));
    out.insert("input".to_string(), Value::Array(input_items));
    if let Some(encrypted_content) = extract_latest_anthropic_thinking_signature(source_messages) {
        out.insert(
            "encrypted_content".to_string(),
            Value::String(encrypted_content),
        );
    }

    // 中文注释：参考 CLIProxyAPI 的行为：Claude 入口需要一个稳定的 prompt_cache_key，
    // 并在上游请求头把 Session_id/Conversation_id 与之对齐，才能显著降低 challenge 命中率。
    if let Some(prompt_cache_key) = prompt_cache::resolve_prompt_cache_key(obj, out.get("model")) {
        out.insert(
            "prompt_cache_key".to_string(),
            Value::String(prompt_cache_key),
        );
    }
    // 中文注释：上游 codex responses 对低体积请求携带采样参数时更容易触发 challenge，
    // 这里对 anthropic 入口统一不透传 temperature/top_p，优先稳定性。
    if let Some(tools) = obj.get("tools").and_then(Value::as_array) {
        let mapped_tools = tools
            .iter()
            .filter_map(map_anthropic_tool_definition)
            .take(MAX_ANTHROPIC_TOOLS)
            .collect::<Vec<_>>();
        if !mapped_tools.is_empty() {
            out.insert("tools".to_string(), Value::Array(mapped_tools));
            if !obj.contains_key("tool_choice") {
                out.insert("tool_choice".to_string(), Value::String("auto".to_string()));
            }
        }
    }
    if let Some(tool_choice) = obj.get("tool_choice") {
        if !tool_choice.is_null() {
            if let Some(mapped_tool_choice) = map_anthropic_tool_choice(tool_choice) {
                out.insert("tool_choice".to_string(), mapped_tool_choice);
            }
        }
    }
    let request_stream = obj.get("stream").and_then(Value::as_bool).unwrap_or(true);
    // 说明：即使 Claude 请求 stream=false，也统一以 stream=true 请求 upstream，
    // 再在网关侧将 SSE 聚合为 Anthropic JSON，降低 upstream challenge 命中率。
    out.insert("stream".to_string(), Value::Bool(true));
    out.insert(
        "parallel_tool_calls".to_string(),
        Value::Bool(resolve_anthropic_parallel_tool_calls(obj)),
    );
    out.insert("store".to_string(), Value::Bool(false));
    out.insert(
        "include".to_string(),
        Value::Array(vec![Value::String(
            "reasoning.encrypted_content".to_string(),
        )]),
    );

    serde_json::to_vec(&Value::Object(out))
        .map(|bytes| (bytes, request_stream))
        .map_err(|err| format!("convert claude request failed: {err}"))
}

fn resolve_anthropic_upstream_model(source: &serde_json::Map<String, Value>) -> String {
    let requested_model = source
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match requested_model {
        Some(model) if model.to_ascii_lowercase().contains("codex") => model.to_string(),
        _ => DEFAULT_ANTHROPIC_MODEL.to_string(),
    }
}

fn resolve_anthropic_reasoning_effort(source: &serde_json::Map<String, Value>) -> &'static str {
    source
        .get("reasoning")
        .and_then(Value::as_object)
        .and_then(|value| value.get("effort"))
        .and_then(Value::as_str)
        .or_else(|| {
            source
                .get("output_config")
                .and_then(Value::as_object)
                .and_then(|value| value.get("effort"))
                .and_then(Value::as_str)
        })
        .or_else(|| source.get("effort").and_then(Value::as_str))
        .and_then(crate::reasoning_effort::normalize_reasoning_effort)
        .unwrap_or(DEFAULT_ANTHROPIC_REASONING)
}

fn resolve_anthropic_reasoning_summary(
    source: &serde_json::Map<String, Value>,
) -> Option<&'static str> {
    match source.get("thinking") {
        Some(Value::Bool(true)) => Some("detailed"),
        Some(Value::Bool(false)) => Some("none"),
        Some(Value::String(text)) => match text.trim().to_ascii_lowercase().as_str() {
            "enabled" | "on" | "true" => Some("detailed"),
            "disabled" | "off" | "false" => Some("none"),
            _ => None,
        },
        Some(Value::Object(obj)) => {
            let thinking_type = obj
                .get("type")
                .and_then(Value::as_str)
                .map(|value| value.trim().to_ascii_lowercase());
            match thinking_type.as_deref() {
                Some("disabled") => Some("none"),
                Some("enabled") => Some("detailed"),
                _ if obj
                    .get("budget_tokens")
                    .and_then(Value::as_i64)
                    .is_some_and(|value| value > 0) =>
                {
                    Some("detailed")
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn extract_latest_anthropic_thinking_signature(messages: &[Value]) -> Option<String> {
    for message in messages.iter().rev() {
        let Some(message_obj) = message.as_object() else {
            continue;
        };
        let Some(content) = message_obj.get("content") else {
            continue;
        };
        let blocks = if let Some(array) = content.as_array() {
            array
        } else if content.is_object() {
            std::slice::from_ref(content)
        } else {
            continue;
        };
        for block in blocks.iter().rev() {
            let Some(block_obj) = block.as_object() else {
                continue;
            };
            let block_type = block_obj
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !matches!(block_type, "thinking" | "redacted_thinking") {
                continue;
            }
            let signature = block_obj
                .get("signature")
                .or_else(|| block_obj.get("encrypted_content"))
                .or_else(|| block_obj.get("data"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if let Some(signature) = signature {
                return Some(signature.to_string());
            }
        }
    }
    None
}

fn append_assistant_messages(messages: &mut Vec<Value>, content: &Value) -> Result<(), String> {
    if let Some(text) = content.as_str() {
        messages.push(json!({
            "role": "assistant",
            "content": text,
        }));
        return Ok(());
    }

    let blocks = if let Some(array) = content.as_array() {
        array.to_vec()
    } else if content.is_object() {
        vec![content.clone()]
    } else {
        return Err("unsupported assistant content".to_string());
    };

    let mut content_parts = Vec::new();

    for block in blocks {
        let Some(block_obj) = block.as_object() else {
            return Err("invalid assistant content block".to_string());
        };
        let block_type = block_obj
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| "assistant content block missing type".to_string())?;
        match block_type {
            "text" => {
                if let Some(text) = block_obj.get("text").and_then(Value::as_str) {
                    if !text.trim().is_empty() {
                        content_parts.push(json!({
                            "type": "text",
                            "text": text,
                        }));
                    }
                }
            }
            "tool_use" => {
                let id = block_obj
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("toolu_{}", content_parts.len()));
                let Some(name) = block_obj
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                else {
                    continue;
                };
                content_parts.push(json!({
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": block_obj.get("input").cloned().unwrap_or_else(|| json!({})),
                }));
            }
            _ => continue,
        }
    }

    if content_parts.is_empty() {
        return Ok(());
    }
    messages.push(json!({
        "role": "assistant",
        "content": content_parts,
    }));
    Ok(())
}

fn append_user_messages(messages: &mut Vec<Value>, content: &Value) -> Result<(), String> {
    if let Some(text) = content.as_str() {
        if !text.trim().is_empty() {
            messages.push(json!({
                "role": "user",
                "content": text,
            }));
        }
        return Ok(());
    }

    let blocks = if let Some(array) = content.as_array() {
        array.to_vec()
    } else if content.is_object() {
        vec![content.clone()]
    } else {
        return Err("unsupported user content".to_string());
    };

    let mut pending_parts = Vec::new();
    for block in blocks {
        let Some(block_obj) = block.as_object() else {
            return Err("invalid user content block".to_string());
        };
        let block_type = block_obj
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| "user content block missing type".to_string())?;
        match block_type {
            "text" => {
                if let Some(text) = block_obj.get("text").and_then(Value::as_str) {
                    if !text.trim().is_empty() {
                        pending_parts.push(json!({
                            "type": "input_text",
                            "text": text,
                        }));
                    }
                }
            }
            "image" => {
                if let Some(image_item) = map_anthropic_image_block_to_responses_item(block_obj) {
                    pending_parts.push(image_item);
                }
            }
            "tool_result" => {
                flush_user_content_parts(messages, &mut pending_parts);
                let tool_use_id = block_obj
                    .get("tool_use_id")
                    .and_then(Value::as_str)
                    .or_else(|| block_obj.get("id").and_then(Value::as_str))
                    .unwrap_or_default();
                if tool_use_id.is_empty() {
                    continue;
                }
                let mut tool_content = extract_tool_result_output(block_obj.get("content"))?;
                if block_obj
                    .get("is_error")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    tool_content = prefix_tool_error_output(tool_content);
                }
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": tool_use_id,
                    "content": tool_content,
                }));
            }
            _ => continue,
        }
    }
    flush_user_content_parts(messages, &mut pending_parts);
    Ok(())
}

fn append_tool_role_message(
    messages: &mut Vec<Value>,
    message_obj: &serde_json::Map<String, Value>,
    content: &Value,
) -> Result<(), String> {
    let tool_call_id = message_obj
        .get("tool_call_id")
        .or_else(|| message_obj.get("tool_use_id"))
        .and_then(Value::as_str)
        .ok_or_else(|| "tool role message missing tool_call_id".to_string())?;
    let tool_content = extract_tool_result_output(Some(content))?;
    messages.push(json!({
        "role": "tool",
        "tool_call_id": tool_call_id,
        "content": tool_content,
    }));
    Ok(())
}

fn flush_user_content_parts(messages: &mut Vec<Value>, pending_parts: &mut Vec<Value>) {
    if pending_parts.is_empty() {
        return;
    }
    messages.push(json!({
        "role": "user",
        "content": pending_parts.clone(),
    }));
    pending_parts.clear();
}

fn convert_chat_messages_to_responses_input(
    messages: &[Value],
    tool_name_map: &BTreeMap<String, String>,
) -> Result<(Option<String>, Vec<Value>), String> {
    let mut instructions_parts = Vec::new();
    let mut input_items = Vec::new();

    for message in messages {
        let Some(message_obj) = message.as_object() else {
            continue;
        };
        let Some(role) = message_obj.get("role").and_then(Value::as_str) else {
            continue;
        };
        match role {
            "system" => {
                if let Some(content) = message_obj.get("content").and_then(Value::as_str) {
                    if !content.trim().is_empty() {
                        instructions_parts.push(content.to_string());
                    }
                }
            }
            "user" => {
                if let Some(content) = message_obj.get("content") {
                    let content_items = convert_user_message_content_to_responses_items(content);
                    if !content_items.is_empty() {
                        input_items.push(json!({
                            "type": "message",
                            "role": "user",
                            "content": content_items
                        }));
                    }
                }
            }
            "assistant" => {
                if let Some(content) = message_obj.get("content") {
                    append_assistant_content_to_responses_input(
                        &mut input_items,
                        content,
                        tool_name_map,
                    )?;
                }
                if let Some(tool_calls) = message_obj.get("tool_calls").and_then(Value::as_array) {
                    for (index, tool_call) in tool_calls.iter().enumerate() {
                        let Some(tool_obj) = tool_call.as_object() else {
                            continue;
                        };
                        let call_id = tool_obj
                            .get("id")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                            .unwrap_or_else(|| format!("call_{index}"));
                        let Some(function_name) = tool_obj
                            .get("function")
                            .and_then(|value| value.get("name"))
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                        else {
                            continue;
                        };
                        let function_name =
                            shorten_openai_tool_name_with_map(function_name, tool_name_map);
                        let arguments = tool_obj
                            .get("function")
                            .and_then(|value| value.get("arguments"))
                            .map(|value| {
                                if let Some(text) = value.as_str() {
                                    text.to_string()
                                } else {
                                    serde_json::to_string(value)
                                        .unwrap_or_else(|_| "{}".to_string())
                                }
                            })
                            .unwrap_or_else(|| "{}".to_string());
                        input_items.push(json!({
                            "type": "function_call",
                            "call_id": call_id,
                            "name": function_name,
                            "arguments": arguments
                        }));
                    }
                }
            }
            "tool" => {
                let call_id = message_obj
                    .get("tool_call_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "tool role message missing tool_call_id".to_string())?;
                let output =
                    convert_tool_message_content_to_responses_output(message_obj.get("content"))?;
                input_items.push(json!({
                    "type": "function_call_output",
                    "call_id": call_id,
                    "output": output
                }));
            }
            _ => {}
        }
    }

    let instructions = if instructions_parts.is_empty() {
        None
    } else {
        Some(instructions_parts.join("\n\n"))
    };
    Ok((instructions, input_items))
}

fn append_assistant_content_to_responses_input(
    input_items: &mut Vec<Value>,
    content: &Value,
    tool_name_map: &BTreeMap<String, String>,
) -> Result<(), String> {
    if let Some(text) = content.as_str() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            input_items.push(json!({
                "type": "message",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": trimmed }]
            }));
        }
        return Ok(());
    }

    let items = if let Some(array) = content.as_array() {
        array.to_vec()
    } else if content.is_object() {
        vec![content.clone()]
    } else if content.is_null() {
        Vec::new()
    } else {
        return Err("unsupported assistant content".to_string());
    };

    let mut pending_parts = Vec::new();
    for item in items {
        let Some(item_obj) = item.as_object() else {
            continue;
        };
        let item_type = item_obj
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match item_type {
            "text" | "output_text" => {
                if let Some(text) = item_obj.get("text").and_then(Value::as_str) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        pending_parts.push(json!({
                            "type": "output_text",
                            "text": trimmed,
                        }));
                    }
                }
            }
            "tool_use" => {
                flush_assistant_output_parts(input_items, &mut pending_parts);
                let Some(function_name) = item_obj
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    continue;
                };
                let function_name = shorten_openai_tool_name_with_map(function_name, tool_name_map);
                let call_id = item_obj
                    .get("id")
                    .and_then(Value::as_str)
                    .or_else(|| item_obj.get("call_id").and_then(Value::as_str))
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("call_0");
                let arguments = serde_json::to_string(
                    &item_obj.get("input").cloned().unwrap_or_else(|| json!({})),
                )
                .map_err(|err| format!("serialize assistant tool_use input failed: {err}"))?;
                input_items.push(json!({
                    "type": "function_call",
                    "call_id": call_id,
                    "name": function_name,
                    "arguments": arguments
                }));
            }
            _ => continue,
        }
    }
    flush_assistant_output_parts(input_items, &mut pending_parts);
    Ok(())
}

fn flush_assistant_output_parts(input_items: &mut Vec<Value>, pending_parts: &mut Vec<Value>) {
    if pending_parts.is_empty() {
        return;
    }
    input_items.push(json!({
        "type": "message",
        "role": "assistant",
        "content": pending_parts.clone(),
    }));
    pending_parts.clear();
}

fn convert_tool_message_content_to_responses_output(
    value: Option<&Value>,
) -> Result<Value, String> {
    let Some(value) = value else {
        return Ok(Value::String(String::new()));
    };
    if value.is_null() {
        return Ok(Value::String(String::new()));
    }
    if let Some(text) = value.as_str() {
        return Ok(Value::String(text.to_string()));
    }
    if let Some(items) = value.as_array() {
        let mapped_items = items
            .iter()
            .filter_map(map_tool_result_content_item_to_responses_output_item)
            .collect::<Vec<_>>();
        if mapped_items.is_empty() {
            return Ok(Value::String(String::new()));
        }
        return Ok(Value::Array(mapped_items));
    }
    if let Some(item) = map_tool_result_content_item_to_responses_output_item(value) {
        return Ok(Value::Array(vec![item]));
    }
    serde_json::to_string(value)
        .map(Value::String)
        .map_err(|err| format!("serialize tool result content failed: {err}"))
}

fn map_tool_result_content_item_to_responses_output_item(item: &Value) -> Option<Value> {
    if let Some(text) = item.as_str() {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        return Some(json!({
            "type": "input_text",
            "text": trimmed,
        }));
    }

    let obj = item.as_object()?;
    let item_type = obj.get("type").and_then(Value::as_str).unwrap_or_default();
    match item_type {
        "text" | "input_text" => obj
            .get("text")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|text| {
                json!({
                    "type": "input_text",
                    "text": text,
                })
            }),
        "input_image" => {
            let mut mapped = serde_json::Map::new();
            mapped.insert("type".to_string(), Value::String("input_image".to_string()));
            if let Some(image_url) = obj.get("image_url").cloned() {
                mapped.insert("image_url".to_string(), image_url);
            } else if let Some(file_id) = obj.get("file_id").cloned() {
                mapped.insert("file_id".to_string(), file_id);
            } else {
                return None;
            }
            Some(Value::Object(mapped))
        }
        "image" => map_anthropic_image_block_to_responses_item(obj),
        _ => serde_json::to_string(item).ok().and_then(|text| {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(json!({
                    "type": "input_text",
                    "text": trimmed,
                }))
            }
        }),
    }
}

fn prefix_tool_error_output(output: Value) -> Value {
    match output {
        Value::String(text) => Value::String(format!("[tool_error] {text}")),
        Value::Array(mut items) => {
            items.insert(
                0,
                json!({
                    "type": "input_text",
                    "text": "[tool_error]",
                }),
            );
            Value::Array(items)
        }
        other => other,
    }
}

fn convert_user_message_content_to_responses_items(content: &Value) -> Vec<Value> {
    match content {
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Vec::new()
            } else {
                vec![json!({
                    "type": "input_text",
                    "text": trimmed,
                })]
            }
        }
        Value::Array(items) => items
            .iter()
            .filter_map(map_user_content_item_to_responses_item)
            .collect(),
        Value::Null => Vec::new(),
        other => {
            let text = serde_json::to_string(other).unwrap_or_default();
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Vec::new()
            } else {
                vec![json!({
                    "type": "input_text",
                    "text": trimmed,
                })]
            }
        }
    }
}

fn map_user_content_item_to_responses_item(item: &Value) -> Option<Value> {
    if let Some(text) = item.as_str() {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        return Some(json!({
            "type": "input_text",
            "text": trimmed,
        }));
    }

    let obj = item.as_object()?;
    let item_type = obj.get("type").and_then(Value::as_str).unwrap_or_default();
    match item_type {
        "text" | "input_text" | "output_text" => obj
            .get("text")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|text| {
                json!({
                    "type": "input_text",
                    "text": text,
                })
            }),
        "input_image" => {
            let mut mapped = serde_json::Map::new();
            mapped.insert("type".to_string(), Value::String("input_image".to_string()));
            if let Some(image_url) = obj.get("image_url").cloned() {
                mapped.insert("image_url".to_string(), image_url);
            } else if let Some(file_id) = obj.get("file_id").cloned() {
                mapped.insert("file_id".to_string(), file_id);
            } else {
                return None;
            }
            Some(Value::Object(mapped))
        }
        "image_url" => extract_openai_image_url(obj).map(|image_url| {
            json!({
                "type": "input_image",
                "image_url": image_url,
            })
        }),
        _ => None,
    }
}

fn extract_openai_image_url(obj: &serde_json::Map<String, Value>) -> Option<String> {
    if let Some(text) = obj.get("image_url").and_then(Value::as_str) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    let image_url_obj = obj.get("image_url").and_then(Value::as_object)?;
    image_url_obj
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn map_anthropic_image_block_to_responses_item(
    block: &serde_json::Map<String, Value>,
) -> Option<Value> {
    let source = block.get("source")?;
    let source_obj = source.as_object()?;

    if let Some(image_url) = source_obj
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(json!({
            "type": "input_image",
            "image_url": image_url,
        }));
    }

    let source_type = source_obj
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if source_type == "base64" || source_obj.contains_key("data") {
        let media_type = source_obj
            .get("media_type")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("image/png");
        let data = source_obj
            .get("data")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        return Some(json!({
            "type": "input_image",
            "image_url": format!("data:{media_type};base64,{data}"),
        }));
    }

    if let Some(file_id) = source_obj
        .get("file_id")
        .or_else(|| source_obj.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(json!({
            "type": "input_image",
            "file_id": file_id,
        }));
    }

    None
}

fn extract_tool_result_output(value: Option<&Value>) -> Result<Value, String> {
    convert_tool_message_content_to_responses_output(value)
}

fn map_anthropic_tool_definition(value: &Value) -> Option<Value> {
    let Some(obj) = value.as_object() else {
        return None;
    };
    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| obj.get("type").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let description = obj
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let parameters = obj
        .get("input_schema")
        .cloned()
        .unwrap_or_else(|| json!({ "type": "object", "properties": {} }));

    let mut tool_obj = serde_json::Map::new();
    tool_obj.insert("type".to_string(), Value::String("function".to_string()));
    tool_obj.insert("name".to_string(), Value::String(name.to_string()));
    if !description.is_empty() {
        tool_obj.insert("description".to_string(), Value::String(description));
    }
    tool_obj.insert("parameters".to_string(), parameters);

    Some(Value::Object(tool_obj))
}

fn map_anthropic_tool_choice(value: &Value) -> Option<Value> {
    if let Some(text) = value.as_str() {
        return Some(Value::String(text.to_string()));
    }
    let Some(obj) = value.as_object() else {
        return None;
    };
    let choice_type = obj.get("type").and_then(Value::as_str).unwrap_or("auto");
    match choice_type {
        "auto" => Some(Value::String("auto".to_string())),
        "any" => Some(Value::String("required".to_string())),
        "none" => Some(Value::String("none".to_string())),
        "tool" => {
            let name = obj
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())?;
            Some(json!({
                "type": "function",
                "name": name
            }))
        }
        _ => None,
    }
}

fn resolve_anthropic_parallel_tool_calls(source: &serde_json::Map<String, Value>) -> bool {
    !source
        .get("tool_choice")
        .and_then(Value::as_object)
        .and_then(|tool_choice| tool_choice.get("disable_parallel_tool_use"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn extract_text_content(value: &Value) -> Result<String, String> {
    if let Some(text) = value.as_str() {
        return Ok(text.to_string());
    }

    if let Some(block) = value.as_object() {
        return extract_text_from_block(block);
    }

    if let Some(array) = value.as_array() {
        let mut parts = Vec::new();
        for item in array {
            let Some(block) = item.as_object() else {
                return Err("invalid claude content block".to_string());
            };
            parts.push(extract_text_from_block(block)?);
        }
        return Ok(parts.join(""));
    }

    Err("unsupported claude content".to_string())
}

fn extract_text_from_block(block: &serde_json::Map<String, Value>) -> Result<String, String> {
    let block_type = block
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| "claude content block missing type".to_string())?;
    if block_type != "text" {
        return Err(format!(
            "unsupported claude content block type: {block_type}"
        ));
    }
    block
        .get("text")
        .and_then(Value::as_str)
        .map(|v| v.to_string())
        .ok_or_else(|| "claude text block missing text".to_string())
}
