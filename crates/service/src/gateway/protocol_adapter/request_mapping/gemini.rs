use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// 函数 `convert_gemini_generate_content_request`
///
/// 作者: Codex
///
/// 时间: 2026-04-05
///
/// # 参数
/// - path: Gemini 原生请求路径
/// - body: Gemini 原生请求体
///
/// # 返回
/// 返回转换后的 Responses 请求体、是否流式和工具名还原映射
pub(crate) fn convert_gemini_generate_content_request(
    path: &str,
    body: &[u8],
) -> Result<(Vec<u8>, bool, super::ToolNameRestoreMap), String> {
    let payload: Value =
        serde_json::from_slice(body).map_err(|_| "invalid gemini request json".to_string())?;
    let Some(obj) = payload.as_object() else {
        return Err("gemini request body must be an object".to_string());
    };

    let model =
        extract_model_from_path(path).ok_or_else(|| "gemini model is required".to_string())?;
    let request_stream = normalized_request_path(path).contains(":streamGenerateContent");
    let tool_names = collect_gemini_tool_names(obj);
    let allowed_function_names = collect_gemini_allowed_function_names(obj);
    let (tool_name_map, tool_name_restore_map) = super::build_shortened_tool_name_maps(tool_names);
    let (instructions, input_items) =
        convert_gemini_contents_to_responses_input(obj, &tool_name_map)?;

    let mut out = serde_json::Map::new();
    out.insert("model".to_string(), Value::String(model));
    out.insert(
        "instructions".to_string(),
        Value::String(instructions.unwrap_or_default()),
    );
    out.insert("input".to_string(), Value::Array(input_items));
    out.insert("stream".to_string(), Value::Bool(request_stream));
    out.insert("store".to_string(), Value::Bool(false));
    out.insert(
        "parallel_tool_calls".to_string(),
        Value::Bool(resolve_parallel_tool_calls(obj)),
    );
    out.insert(
        "tool_choice".to_string(),
        resolve_tool_choice(obj, &tool_name_map, allowed_function_names.as_ref())
            .unwrap_or_else(|| Value::String("auto".to_string())),
    );

    if let Some(tools) =
        map_gemini_tools_to_responses(obj, &tool_name_map, allowed_function_names.as_ref())
    {
        out.insert("tools".to_string(), Value::Array(tools));
    }

    if let Some(generation_config) = get_object_field(obj, &["generationConfig", "generation_config"])
    {
        copy_optional_field_alias(
            generation_config,
            &mut out,
            &["temperature"],
            "temperature",
        );
        copy_optional_field_alias(generation_config, &mut out, &["topP", "top_p"], "top_p");
        copy_optional_field_alias(generation_config, &mut out, &["topK", "top_k"], "top_k");
        copy_optional_field_alias(
            generation_config,
            &mut out,
            &["candidateCount", "candidate_count"],
            "n",
        );
        copy_optional_field_alias(
            generation_config,
            &mut out,
            &["maxOutputTokens", "max_output_tokens"],
            "max_output_tokens",
        );
        if let Some(stop_sequences) =
            get_value_field(generation_config, &["stopSequences", "stop_sequences"])
        {
            out.insert("stop".to_string(), stop_sequences.clone());
        }
    }

    if let Some(reasoning) = resolve_gemini_reasoning_payload(obj) {
        out.insert("reasoning".to_string(), reasoning);
        out.insert(
            "include".to_string(),
            Value::Array(vec![Value::String(
                "reasoning.encrypted_content".to_string(),
            )]),
        );
    }

    if let Some(prompt_cache_key) = super::resolve_prompt_cache_key(obj, out.get("model")) {
        out.insert(
            "prompt_cache_key".to_string(),
            Value::String(prompt_cache_key),
        );
    }

    serde_json::to_vec(&Value::Object(out))
        .map(|bytes| (bytes, request_stream, tool_name_restore_map))
        .map_err(|err| format!("convert gemini request failed: {err}"))
}

fn normalized_request_path(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}

fn extract_model_from_path(path: &str) -> Option<String> {
    let normalized = normalized_request_path(path);
    ["/v1/models/", "/v1beta/models/", "/v1alpha/models/"]
        .iter()
        .find_map(|prefix| {
            normalized.strip_prefix(prefix).and_then(|rest| {
                let (model, _) = rest.split_once(':')?;
                let trimmed = model.trim();
                if trimmed.is_empty() {
                    None
                } else if let Some(stripped) = trimmed.strip_prefix("models/") {
                    Some(stripped.to_string())
                } else {
                    Some(trimmed.to_string())
                }
            })
        })
}

fn collect_gemini_tool_names(source: &serde_json::Map<String, Value>) -> Vec<String> {
    let mut names = Vec::new();

    if let Some(tools) = get_array_field(source, &["tools"]) {
        for tool in tools {
            let Some(tool_obj) = tool.as_object() else {
                continue;
            };
            let Some(function_declarations) =
                get_array_field(tool_obj, &["functionDeclarations", "function_declarations"])
            else {
                continue;
            };
            for declaration in function_declarations {
                let Some(name) = declaration
                    .get("name")
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

    if let Some(allowed_function_names) = get_tool_config_field_array(source, &[
        "allowedFunctionNames",
        "allowed_function_names",
    ]) {
        for name in allowed_function_names {
            let Some(name) = name
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            names.push(name.to_string());
        }
    }

    if let Some(contents) = source.get("contents").and_then(Value::as_array) {
        for content in contents {
            let Some(parts) = content.get("parts").and_then(Value::as_array) else {
                continue;
            };
            for part in parts {
                if let Some(name) = part
                    .get("functionCall")
                    .and_then(|value| value.get("name"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    names.push(name.to_string());
                }
                if let Some(name) = part
                    .get("functionResponse")
                    .and_then(|value| value.get("name"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    names.push(name.to_string());
                }
            }
        }
    }

    names
}

fn collect_gemini_allowed_function_names(
    source: &serde_json::Map<String, Value>,
) -> Option<BTreeSet<String>> {
    let allowed_items = source
        .get("toolConfig")
        .or_else(|| source.get("tool_config"))
        .and_then(Value::as_object)
        .and_then(|value| get_object_field(value, &["functionCallingConfig", "function_calling_config"]))
        .and_then(|value| get_value_field(value, &["allowedFunctionNames", "allowed_function_names"]))?;
    let mut names = BTreeSet::new();
    if let Some(items) = allowed_items.as_array() {
        for item in items {
            let Some(name) = item
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            names.insert(name.to_string());
        }
    }
    Some(names)
}

fn convert_gemini_contents_to_responses_input(
    source: &serde_json::Map<String, Value>,
    tool_name_map: &BTreeMap<String, String>,
) -> Result<(Option<String>, Vec<Value>), String> {
    let mut messages = Vec::new();
    let mut pending_tool_calls: BTreeMap<String, VecDeque<String>> = BTreeMap::new();
    let mut synthetic_call_index = 0usize;

    if let Some(system_instruction) = get_value_field(source, &["systemInstruction", "system_instruction"])
    {
        let system_text = extract_text_from_content_like(system_instruction);
        if !system_text.trim().is_empty() {
            messages.push(json!({
                "role": "system",
                "content": system_text,
            }));
        }
    }

    let contents = source
        .get("contents")
        .and_then(Value::as_array)
        .ok_or_else(|| "gemini contents field is required".to_string())?;
    for content in contents {
        let Some(content_obj) = content.as_object() else {
            return Err("invalid gemini content item".to_string());
        };
        let role = content_obj
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("user");
        let parts = content_obj
            .get("parts")
            .and_then(Value::as_array)
            .ok_or_else(|| "gemini content parts field is required".to_string())?;

        let mut pending_user_parts = Vec::new();
        let mut pending_model_parts = Vec::new();

        let mut part_index = 0usize;
        while part_index < parts.len() {
            let Some(part_obj) = parts[part_index].as_object() else {
                part_index += 1;
                continue;
            };
            if let Some(text) = part_obj
                .get("text")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                match role {
                    "model" => pending_model_parts.push(json!({
                        "type": "output_text",
                        "text": text,
                    })),
                    _ => pending_user_parts.push(json!({
                        "type": "input_text",
                        "text": text,
                    })),
                }
                part_index += 1;
                continue;
            }

            if let Some(image_item) = map_gemini_image_part_to_responses_item(part_obj) {
                pending_user_parts.push(image_item);
                part_index += 1;
                continue;
            }

            if let Some(function_call) =
                get_object_field(part_obj, &["functionCall", "function_call"])
            {
                if !pending_model_parts.is_empty() {
                    messages.push(json!({
                        "role": "assistant",
                        "content": pending_model_parts.clone(),
                    }));
                    pending_model_parts.clear();
                }
                let Some(name) = function_call
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    continue;
                };
                let mapped_name =
                    super::openai::shorten_openai_tool_name_with_map(name, tool_name_map);
                let call_id = function_call
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        synthetic_call_index += 1;
                        format!("call_gemini_{synthetic_call_index}")
                    });
                pending_tool_calls
                    .entry(mapped_name.clone())
                    .or_default()
                    .push_back(call_id.clone());
                let arguments = function_call
                    .get("args")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                messages.push(json!({
                    "role": "assistant",
                    "content": [{
                        "type": "tool_use",
                        "id": call_id,
                        "name": mapped_name,
                        "input": arguments,
                    }],
                }));
                part_index += 1;
                continue;
            }

            if let Some(function_response) =
                get_object_field(part_obj, &["functionResponse", "function_response"])
            {
                if !pending_user_parts.is_empty() {
                    messages.push(json!({
                        "role": "user",
                        "content": pending_user_parts.clone(),
                    }));
                    pending_user_parts.clear();
                }
                let Some(name) = function_response
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    continue;
                };
                let mapped_name =
                    super::openai::shorten_openai_tool_name_with_map(name, tool_name_map);
                let call_id = function_response
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .or_else(|| {
                        pending_tool_calls
                            .get_mut(mapped_name.as_str())
                            .and_then(VecDeque::pop_front)
                    })
                    .unwrap_or_else(|| {
                        synthetic_call_index += 1;
                        format!("call_gemini_{synthetic_call_index}")
                    });
                let output = function_response
                    .get("response")
                    .cloned()
                    .unwrap_or_else(|| Value::String(String::new()));
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": collect_gemini_function_response_content(parts, part_index, output),
                }));
                part_index = advance_gemini_function_response_index(parts, part_index);
                continue;
            }

            part_index += 1;
        }

        if !pending_user_parts.is_empty() {
            messages.push(json!({
                "role": "user",
                "content": pending_user_parts,
            }));
        }
        if !pending_model_parts.is_empty() {
            messages.push(json!({
                "role": "assistant",
                "content": pending_model_parts,
            }));
        }
    }

    super::convert_chat_messages_to_responses_input(&messages, tool_name_map)
}

fn advance_gemini_function_response_index(parts: &[Value], start_index: usize) -> usize {
    let mut index = start_index + 1;
    while index < parts.len() {
        let Some(part_obj) = parts[index].as_object() else {
            break;
        };
        if part_obj.get("functionResponse").is_some() || part_obj.get("functionCall").is_some() {
            break;
        }
        if part_obj
            .get("text")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
            || map_gemini_image_part_to_responses_item(part_obj).is_some()
        {
            index += 1;
            continue;
        }
        break;
    }
    index
}

fn collect_gemini_function_response_content(
    parts: &[Value],
    start_index: usize,
    output: Value,
) -> Value {
    let mut content_items = map_gemini_function_response_output_items(&output);
    let mut index = start_index + 1;
    while index < parts.len() {
        let Some(part_obj) = parts[index].as_object() else {
            break;
        };
        if part_obj.get("functionResponse").is_some() || part_obj.get("functionCall").is_some() {
            break;
        }
        if let Some(text) = part_obj
            .get("text")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            content_items.push(json!({
                "type": "input_text",
                "text": text,
            }));
            index += 1;
            continue;
        }
        if let Some(image_item) = map_gemini_image_part_to_responses_item(part_obj) {
            content_items.push(image_item);
            index += 1;
            continue;
        }
        break;
    }
    if content_items.is_empty() {
        output
    } else {
        Value::Array(content_items)
    }
}

fn map_gemini_function_response_output_items(output: &Value) -> Vec<Value> {
    match output {
        Value::Null => Vec::new(),
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
            .flat_map(map_gemini_function_response_output_items)
            .collect(),
        Value::Object(obj) => {
            if let Some(output_value) = obj.get("output") {
                let nested_items = map_gemini_function_response_output_items(output_value);
                if !nested_items.is_empty() {
                    return nested_items;
                }
            }
            let serialized = serde_json::to_string(output).unwrap_or_default();
            let trimmed = serialized.trim();
            if trimmed.is_empty() {
                Vec::new()
            } else {
                vec![json!({
                    "type": "input_text",
                    "text": trimmed,
                })]
            }
        }
        other => {
            let serialized = serde_json::to_string(other).unwrap_or_default();
            let trimmed = serialized.trim();
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

fn resolve_parallel_tool_calls(source: &serde_json::Map<String, Value>) -> bool {
    get_tool_config_field_value(source, &["disableParallelToolUse", "disable_parallel_tool_use"])
        .and_then(Value::as_bool)
        .map(|disabled| !disabled)
        .unwrap_or(true)
}

fn resolve_tool_choice(
    source: &serde_json::Map<String, Value>,
    tool_name_map: &BTreeMap<String, String>,
    allowed_function_names: Option<&BTreeSet<String>>,
) -> Option<Value> {
    let config = source
        .get("toolConfig")
        .or_else(|| source.get("tool_config"))
        .and_then(Value::as_object)
        .and_then(|value| get_object_field(value, &["functionCallingConfig", "function_calling_config"]))?;
    let mode = config
        .get("mode")
        .and_then(Value::as_str)
        .map(|value| value.trim().to_ascii_uppercase())
        .unwrap_or_else(|| "AUTO".to_string());
    let allowed = get_value_field(config, &["allowedFunctionNames", "allowed_function_names"])
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .filter(|value| {
                    allowed_function_names
                        .map(|allowed_names| allowed_names.contains(*value))
                        .unwrap_or(true)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    match mode.as_str() {
        "NONE" => Some(Value::String("none".to_string())),
        "ANY" => {
            if allowed.is_empty() && allowed_function_names.is_some() {
                return Some(Value::String("none".to_string()));
            }
            if allowed.len() == 1 {
                let mapped_name =
                    super::openai::shorten_openai_tool_name_with_map(allowed[0], tool_name_map);
                Some(json!({
                    "type": "function",
                    "name": mapped_name,
                }))
            } else {
                Some(Value::String("required".to_string()))
            }
        }
        _ => Some(Value::String("auto".to_string())),
    }
}

fn map_gemini_tools_to_responses(
    source: &serde_json::Map<String, Value>,
    tool_name_map: &BTreeMap<String, String>,
    allowed_function_names: Option<&BTreeSet<String>>,
) -> Option<Vec<Value>> {
    let mut out = Vec::new();
    let Some(tools) = get_array_field(source, &["tools"]) else {
        return None;
    };
    for tool in tools {
        let Some(tool_obj) = tool.as_object() else {
            continue;
        };
        let Some(function_declarations) =
            get_array_field(tool_obj, &["functionDeclarations", "function_declarations"])
        else {
            continue;
        };
        for declaration in function_declarations {
            let Some(name) = declaration
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            if allowed_function_names.is_some_and(|items| !items.contains(name)) {
                continue;
            }
            let mapped_name = super::openai::shorten_openai_tool_name_with_map(name, tool_name_map);
            let mut mapped = serde_json::Map::new();
            mapped.insert("type".to_string(), Value::String("function".to_string()));
            mapped.insert("name".to_string(), Value::String(mapped_name));
            if let Some(description) = declaration.get("description") {
                mapped.insert("description".to_string(), description.clone());
            }
            let parameters = get_value_field(
                declaration.as_object().expect("declaration object"),
                &["parametersJsonSchema", "parameters_json_schema", "parameters"],
            )
                .cloned()
                .unwrap_or_else(|| json!({ "type": "object", "properties": {} }));
            mapped.insert(
                "parameters".to_string(),
                super::fix_array_items_in_schema(parameters),
            );
            out.push(Value::Object(mapped));
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn map_gemini_image_part_to_responses_item(
    part_obj: &serde_json::Map<String, Value>,
) -> Option<Value> {
    if let Some(inline_data) = get_object_field(part_obj, &["inlineData", "inline_data"]) {
        let mime_type = inline_data
            .get("mimeType")
            .or_else(|| inline_data.get("mime_type"))
            .and_then(Value::as_str)
            .unwrap_or("image/png");
        let data = inline_data
            .get("data")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        return Some(json!({
            "type": "input_image",
            "image_url": format!("data:{mime_type};base64,{data}"),
        }));
    }

    let file_data = get_object_field(part_obj, &["fileData", "file_data"])?;
    let file_uri = file_data
        .get("fileUri")
        .or_else(|| file_data.get("file_uri"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(json!({
        "type": "input_image",
        "image_url": file_uri,
    }))
}

fn extract_text_from_content_like(value: &Value) -> String {
    let mut parts = Vec::new();
    match value {
        Value::String(text) => parts.push(text.to_string()),
        Value::Object(obj) => {
            if let Some(part_items) = obj.get("parts").and_then(Value::as_array) {
                for item in part_items {
                    if let Some(text) = item.get("text").and_then(Value::as_str) {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            parts.push(trimmed.to_string());
                        }
                    }
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                let text = extract_text_from_content_like(item);
                if !text.is_empty() {
                    parts.push(text);
                }
            }
        }
        _ => {}
    }
    parts.join("\n\n")
}

fn copy_optional_field_alias(
    source: &serde_json::Map<String, Value>,
    target: &mut serde_json::Map<String, Value>,
    from_keys: &[&str],
    to_key: &str,
) {
    if let Some(value) = get_value_field(source, from_keys) {
        target.insert(to_key.to_string(), value.clone());
    }
}

fn get_value_field<'a>(
    source: &'a serde_json::Map<String, Value>,
    keys: &[&str],
) -> Option<&'a Value> {
    keys.iter().find_map(|key| source.get(*key))
}

fn get_array_field<'a>(
    source: &'a serde_json::Map<String, Value>,
    keys: &[&str],
) -> Option<&'a Vec<Value>> {
    get_value_field(source, keys).and_then(Value::as_array)
}

fn get_object_field<'a>(
    source: &'a serde_json::Map<String, Value>,
    keys: &[&str],
) -> Option<&'a serde_json::Map<String, Value>> {
    get_value_field(source, keys).and_then(Value::as_object)
}

fn get_tool_config_field_value<'a>(
    source: &'a serde_json::Map<String, Value>,
    keys: &[&str],
) -> Option<&'a Value> {
    source
        .get("toolConfig")
        .or_else(|| source.get("tool_config"))
        .and_then(Value::as_object)
        .and_then(|value| get_object_field(value, &["functionCallingConfig", "function_calling_config"]))
        .and_then(|value| get_value_field(value, keys))
}

fn get_tool_config_field_array<'a>(
    source: &'a serde_json::Map<String, Value>,
    keys: &[&str],
) -> Option<&'a Vec<Value>> {
    get_tool_config_field_value(source, keys).and_then(Value::as_array)
}

fn resolve_gemini_reasoning_payload(
    source: &serde_json::Map<String, Value>,
) -> Option<Value> {
    let generation_config = get_object_field(source, &["generationConfig", "generation_config"])?;
    let thinking_config =
        get_object_field(generation_config, &["thinkingConfig", "thinking_config"])?;

    if let Some(level) = get_value_field(thinking_config, &["thinkingLevel", "thinking_level"])
        .and_then(Value::as_str)
        .and_then(crate::reasoning_effort::normalize_reasoning_effort)
    {
        return Some(json!({ "effort": level }));
    }

    let budget = get_value_field(thinking_config, &["thinkingBudget", "thinking_budget"])
        .and_then(Value::as_i64)?;
    let effort = map_gemini_thinking_budget_to_effort(budget)?;
    Some(json!({ "effort": effort }))
}

fn map_gemini_thinking_budget_to_effort(budget: i64) -> Option<&'static str> {
    match budget {
        i64::MIN..=-1 => None,
        0..=1024 => Some("low"),
        1025..=8192 => Some("medium"),
        8193..=24576 => Some("high"),
        24577..=i64::MAX => Some("xhigh"),
    }
}
