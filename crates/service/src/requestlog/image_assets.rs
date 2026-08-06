use std::fs::{self, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use base64::Engine;
use codexmanager_core::rpc::types::{RequestLogImageData, RequestLogImageResult};
use rand::RngCore;
use serde_json::Value;
use url::Url;

const IMAGE_DIRECTORY_NAME: &str = "request-log-images";
const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;
const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;
const IMAGE_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Default)]
struct OpenAiImageResponseItem {
    b64_json: Option<String>,
    url: Option<String>,
}

fn parse_image_response_items(response_body: &[u8]) -> Result<Vec<OpenAiImageResponseItem>, String> {
    if let Ok(value) = serde_json::from_slice::<Value>(response_body) {
        let mut items = Vec::new();
        collect_image_response_items(&value, &mut items);
        return Ok(items);
    }

    let text = std::str::from_utf8(response_body)
        .map_err(|err| format!("response is neither JSON nor UTF-8 SSE: {err}"))?;
    let mut items = Vec::new();
    let mut data_lines = Vec::new();
    let mut parse_event = |data_lines: &mut Vec<&str>| {
        if data_lines.is_empty() {
            return;
        }
        let payload = data_lines.join("\n");
        if payload.trim() != "[DONE]" {
            if let Ok(value) = serde_json::from_str::<Value>(&payload) {
                collect_image_response_items(&value, &mut items);
            }
        }
        data_lines.clear();
    };
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            parse_event(&mut data_lines);
        } else if let Some(data) = line.strip_prefix("data:") {
            data_lines.push(data.strip_prefix(' ').unwrap_or(data));
        }
    }
    parse_event(&mut data_lines);
    if items.is_empty() {
        Err("response is not a recognized image JSON or SSE response".to_string())
    } else {
        Ok(items)
    }
}

fn collect_image_response_items(value: &Value, items: &mut Vec<OpenAiImageResponseItem>) {
    match value {
        Value::Object(object) => {
            let b64_json = object
                .get("b64_json")
                .or_else(|| object.get("b64"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let url = object
                .get("url")
                .or_else(|| object.get("image_url"))
                .and_then(Value::as_str)
                .map(str::to_string);
            if b64_json.is_some() || url.is_some() {
                let duplicate = items.iter().any(|item| {
                    item.b64_json == b64_json && item.url == url
                });
                if !duplicate {
                    items.push(OpenAiImageResponseItem { b64_json, url });
                }
            }
            for child in object.values() {
                collect_image_response_items(child, items);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_image_response_items(child, items);
            }
        }
        _ => {}
    }
}

pub(crate) fn cache_openai_image_results(
    db_path: &Path,
    trace_id: &str,
    response_body: &[u8],
) -> Vec<RequestLogImageResult> {
    cache_openai_image_results_with_encoding(db_path, trace_id, response_body, None)
}

pub(crate) fn cache_openai_image_results_with_encoding(
    db_path: &Path,
    trace_id: &str,
    response_body: &[u8],
    content_encoding: Option<&str>,
) -> Vec<RequestLogImageResult> {
    let response_body = match decode_image_cache_response_body(response_body, content_encoding) {
        Ok(response_body) => response_body,
        Err(err) => {
            log::warn!(
                "event=request_log_image_cache_skipped trace_id={} reason=response_decode_failed err={}",
                trace_id,
                err
            );
            return Vec::new();
        }
    };
    let response_items = match parse_image_response_items(&response_body) {
        Ok(response_items) if !response_items.is_empty() => {
            response_items
        }
        Ok(_) => {
            log::warn!(
                "event=request_log_image_cache_skipped trace_id={} reason=image_data_missing",
                trace_id
            );
            return Vec::new();
        }
        Err(err) => {
            if is_safe_trace_id(trace_id) {
                if let Some(asset) = cache_raw_image_response(db_path, trace_id, &response_body) {
                    return vec![asset];
                }
            }
            log::warn!(
                "event=request_log_image_cache_skipped trace_id={} reason=invalid_response err={}",
                trace_id,
                err
            );
            return Vec::new();
        }
    };
    if !is_safe_trace_id(trace_id) {
        log::warn!(
            "event=request_log_image_cache_skipped trace_id={} reason=unsafe_trace_id",
            trace_id
        );
        return Vec::new();
    }

    let client = build_image_download_client();
    let root = image_root(db_path);
    let mut assets = Vec::new();
    let mut total_bytes = 0usize;

    for item in response_items {
        let image_bytes = match decode_image_response_item(&item, client.as_ref()) {
            Ok(bytes) => bytes,
            Err(err) => {
                log::warn!(
                    "event=request_log_image_cache_skipped trace_id={} reason=image_unavailable err={}",
                    trace_id,
                    err
                );
                continue;
            }
        };
        if image_bytes.len() > MAX_IMAGE_BYTES {
            log::warn!(
                "event=request_log_image_cache_skipped trace_id={} reason=image_too_large bytes={}",
                trace_id,
                image_bytes.len()
            );
            continue;
        }
        if image_bytes.len() > MAX_REQUEST_BYTES.saturating_sub(total_bytes) {
            log::warn!(
                "event=request_log_image_cache_stopped trace_id={} reason=request_too_large bytes={}",
                trace_id,
                total_bytes.saturating_add(image_bytes.len())
            );
            break;
        }
        let Some((mime_type, extension)) = detect_image_type(&image_bytes) else {
            log::warn!(
                "event=request_log_image_cache_skipped trace_id={} reason=unsupported_image_type",
                trace_id
            );
            continue;
        };
        let request_dir = match ensure_request_image_dir(&root, trace_id) {
            Ok(dir) => dir,
            Err(err) => {
                log::warn!(
                    "event=request_log_image_cache_stopped trace_id={} reason=storage_unavailable err={}",
                    trace_id,
                    err
                );
                break;
            }
        };
        let file_name = match write_image_atomically(&request_dir, extension, &image_bytes) {
            Ok(file_name) => file_name,
            Err(err) => {
                log::warn!(
                    "event=request_log_image_cache_skipped trace_id={} reason=write_failed err={}",
                    trace_id,
                    err
                );
                continue;
            }
        };
        total_bytes = total_bytes.saturating_add(image_bytes.len());
        assets.push(RequestLogImageResult {
            storage_key: format!("{trace_id}/{file_name}"),
            mime_type: mime_type.to_string(),
            byte_length: image_bytes.len() as u64,
        });
    }

    assets
}

fn decode_image_cache_response_body(
    response_body: &[u8],
    content_encoding: Option<&str>,
) -> Result<Vec<u8>, String> {
    let encodings = content_encoding
        .into_iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("identity"))
        .collect::<Vec<_>>();
    if !encodings.is_empty() {
        let mut decoded = response_body.to_vec();
        for encoding in encodings.into_iter().rev() {
            decoded = decode_image_cache_layer(&decoded, encoding)?;
        }
        return Ok(decoded);
    }

    if response_body.starts_with(b"\x28\xb5\x2f\xfd") {
        return decode_image_cache_layer(response_body, "zstd");
    }
    if response_body.starts_with(b"\x1f\x8b") {
        return decode_image_cache_layer(response_body, "gzip");
    }
    if response_body.starts_with(b"\x78\x01")
        || response_body.starts_with(b"\x78\x5e")
        || response_body.starts_with(b"\x78\x9c")
        || response_body.starts_with(b"\x78\xda")
    {
        return decode_image_cache_layer(response_body, "deflate");
    }
    Ok(response_body.to_vec())
}

fn decode_image_cache_layer(response_body: &[u8], encoding: &str) -> Result<Vec<u8>, String> {
    let encoding = encoding.trim().to_ascii_lowercase();
    let mut decoded = Vec::new();
    match encoding.as_str() {
        "gzip" | "x-gzip" => {
            let decoder = flate2::read::GzDecoder::new(Cursor::new(response_body));
            decoder
                .take((MAX_REQUEST_BYTES + 1) as u64)
                .read_to_end(&mut decoded)
                .map_err(|err| format!("decode gzip response failed: {err}"))?;
        }
        "deflate" => {
            let mut decoder = flate2::read::ZlibDecoder::new(Cursor::new(response_body));
            let result = decoder
                .by_ref()
                .take((MAX_REQUEST_BYTES + 1) as u64)
                .read_to_end(&mut decoded);
            if result.is_err() {
                decoded.clear();
                let decoder = flate2::read::DeflateDecoder::new(Cursor::new(response_body));
                decoder
                    .take((MAX_REQUEST_BYTES + 1) as u64)
                    .read_to_end(&mut decoded)
                    .map_err(|err| format!("decode deflate response failed: {err}"))?;
            }
        }
        "br" => {
            let decoder = brotli::Decompressor::new(Cursor::new(response_body), 64 * 1024);
            decoder
                .take((MAX_REQUEST_BYTES + 1) as u64)
                .read_to_end(&mut decoded)
                .map_err(|err| format!("decode brotli response failed: {err}"))?;
        }
        "zstd" => {
            let decoder = zstd::stream::read::Decoder::new(Cursor::new(response_body))
                .map_err(|err| format!("create zstd decoder failed: {err}"))?;
            decoder
                .take((MAX_REQUEST_BYTES + 1) as u64)
                .read_to_end(&mut decoded)
                .map_err(|err| format!("decode zstd response failed: {err}"))?;
        }
        other => return Err(format!("unsupported response content encoding: {other}")),
    }
    if decoded.len() > MAX_REQUEST_BYTES {
        return Err("decompressed response exceeds size limit".to_string());
    }
    Ok(decoded)
}

fn build_image_download_client() -> Option<reqwest::blocking::Client> {
    let mut builder =
        reqwest::blocking::Client::builder().timeout(IMAGE_DOWNLOAD_TIMEOUT);
    if let Some(proxy_url) = crate::gateway::current_upstream_proxy_url() {
        match reqwest::Proxy::all(proxy_url.as_str()) {
            Ok(proxy) => builder = builder.proxy(proxy),
            Err(err) => {
                log::warn!(
                    "event=request_log_image_cache_proxy_ignored proxy={} err={}",
                    proxy_url,
                    err
                );
            }
        }
    }
    builder.build().ok()
}

fn cache_raw_image_response(
    db_path: &Path,
    trace_id: &str,
    response_body: &[u8],
) -> Option<RequestLogImageResult> {
    if response_body.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let (mime_type, extension) = detect_image_type(response_body)?;
    let request_dir = ensure_request_image_dir(&image_root(db_path), trace_id).ok()?;
    let file_name = write_image_atomically(&request_dir, extension, response_body).ok()?;
    Some(RequestLogImageResult {
        storage_key: format!("{trace_id}/{file_name}"),
        mime_type: mime_type.to_string(),
        byte_length: response_body.len() as u64,
    })
}

pub(crate) fn read_image_data_urls(
    db_path: &Path,
    trace_id: &str,
    image_results_json: Option<&str>,
) -> Result<Vec<RequestLogImageData>, String> {
    if !is_safe_trace_id(trace_id) {
        return Err("invalid image trace id".to_string());
    }
    let assets = parse_image_results(image_results_json)?;
    if assets.is_empty() {
        return Ok(Vec::new());
    }
    for asset in &assets {
        validate_storage_key(asset.storage_key.as_str(), Some(trace_id))?;
    }
    let root = image_root(db_path);
    let root = match fs::canonicalize(&root) {
        Ok(root) => root,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(format!("resolve request-log image directory failed: {err}")),
    };
    let mut images = Vec::new();
    for asset in assets {
        let path = resolve_asset_path(&root, trace_id, asset.storage_key.as_str())?;
        let path = match fs::canonicalize(&path) {
            Ok(path) => path,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(format!("resolve cached request-log image failed: {err}")),
        };
        if !path.starts_with(&root) {
            return Err("request-log image path escapes storage root".to_string());
        }
        let bytes = fs::read(&path).map_err(|err| format!("read cached request-log image failed: {err}"))?;
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err("cached request-log image exceeds size limit".to_string());
        }
        let Some((actual_mime_type, _)) = detect_image_type(&bytes) else {
            return Err("cached request-log image has unsupported type".to_string());
        };
        if actual_mime_type != asset.mime_type {
            return Err("cached request-log image MIME type mismatch".to_string());
        }
        images.push(RequestLogImageData {
            storage_key: asset.storage_key,
            data_url: format!(
                "data:{actual_mime_type};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            ),
        });
    }
    Ok(images)
}

pub(crate) fn clear_image_results(
    db_path: &Path,
    image_results_jsons: impl IntoIterator<Item = Option<String>>,
) -> Result<(), String> {
    let mut asset_keys = Vec::new();
    for image_results_json in image_results_jsons {
        for asset in parse_image_results(image_results_json.as_deref())? {
            validate_storage_key(asset.storage_key.as_str(), None)?;
            asset_keys.push(asset.storage_key);
        }
    }
    if asset_keys.is_empty() {
        return Ok(());
    }

    let root = image_root(db_path);
    let root = match fs::canonicalize(&root) {
        Ok(root) => root,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(format!("resolve request-log image directory failed: {err}")),
    };
    for storage_key in asset_keys {
        let path = resolve_asset_path(&root, "", storage_key.as_str())?;
        let path = match fs::canonicalize(&path) {
            Ok(path) => path,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(format!("resolve cached request-log image failed: {err}")),
        };
        if !path.starts_with(&root) {
            return Err("request-log image path escapes storage root".to_string());
        }
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(format!("delete cached request-log image failed: {err}")),
        }
    }
    Ok(())
}

fn image_root(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(IMAGE_DIRECTORY_NAME)
}

fn decode_image_response_item(
    item: &OpenAiImageResponseItem,
    client: Option<&reqwest::blocking::Client>,
) -> Result<Vec<u8>, String> {
    if let Some(base64_json) = item.b64_json.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        return decode_base64_image_value(base64_json, "b64_json");
    }
    let url = item
        .url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "image response item has no b64_json or url".to_string())?;
    if let Some(image_bytes) = decode_data_uri(url)? {
        return Ok(image_bytes);
    }
    let client = client.ok_or_else(|| "create image download client failed".to_string())?;
    download_image_url(client, url)
}

fn decode_base64_image_value(value: &str, field_name: &str) -> Result<Vec<u8>, String> {
    if let Some(image_bytes) = decode_data_uri(value)? {
        return Ok(image_bytes);
    }
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|err| format!("decode {field_name} failed: {err}"))
}

fn decode_data_uri(value: &str) -> Result<Option<Vec<u8>>, String> {
    let Some((metadata, payload)) = value.strip_prefix("data:").and_then(|value| value.split_once(',')) else {
        return Ok(None);
    };
    if !metadata
        .split(';')
        .any(|part| part.trim().eq_ignore_ascii_case("base64"))
    {
        return Err("image data uri must use base64 encoding".to_string());
    }
    base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .map(Some)
        .map_err(|err| format!("decode image data uri failed: {err}"))
}

fn download_image_url(client: &reqwest::blocking::Client, raw_url: &str) -> Result<Vec<u8>, String> {
    let url = Url::parse(raw_url).map_err(|err| format!("invalid image url: {err}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("image url must use http or https".to_string());
    }
    let response = client
        .get(url)
        .send()
        .map_err(|err| format!("download image failed: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("download image returned HTTP {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_IMAGE_BYTES as u64)
    {
        return Err("download image exceeds size limit".to_string());
    }
    let mut reader = response.take((MAX_IMAGE_BYTES + 1) as u64);
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|err| format!("read downloaded image failed: {err}"))?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err("download image exceeds size limit".to_string());
    }
    Ok(bytes)
}

fn ensure_request_image_dir(root: &Path, trace_id: &str) -> Result<PathBuf, String> {
    fs::create_dir_all(root).map_err(|err| format!("create request-log image directory failed: {err}"))?;
    let root = fs::canonicalize(root)
        .map_err(|err| format!("resolve request-log image directory failed: {err}"))?;
    let request_dir = root.join(trace_id);
    fs::create_dir_all(&request_dir)
        .map_err(|err| format!("create request-log image trace directory failed: {err}"))?;
    let request_dir = fs::canonicalize(&request_dir)
        .map_err(|err| format!("resolve request-log image trace directory failed: {err}"))?;
    if !request_dir.starts_with(&root) {
        return Err("request-log image trace directory escapes storage root".to_string());
    }
    Ok(request_dir)
}

fn write_image_atomically(dir: &Path, extension: &str, bytes: &[u8]) -> Result<String, String> {
    for _ in 0..8 {
        let file_name = format!("{}.{}", random_hex(16), extension);
        let final_path = dir.join(&file_name);
        if final_path.exists() {
            continue;
        }
        let temporary_path = dir.join(format!(".{file_name}.tmp"));
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(format!("create cached image file failed: {err}")),
        };
        if let Err(err) = file.write_all(bytes).and_then(|()| file.flush()) {
            let _ = fs::remove_file(&temporary_path);
            return Err(format!("write cached image file failed: {err}"));
        }
        if let Err(err) = fs::rename(&temporary_path, &final_path) {
            let _ = fs::remove_file(&temporary_path);
            return Err(format!("commit cached image file failed: {err}"));
        }
        return Ok(file_name);
    }
    Err("allocate cached image file name failed".to_string())
}

fn parse_image_results(
    image_results_json: Option<&str>,
) -> Result<Vec<RequestLogImageResult>, String> {
    let Some(image_results_json) = image_results_json.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(Vec::new());
    };
    serde_json::from_str(image_results_json)
        .map_err(|err| format!("invalid request-log image metadata: {err}"))
}

fn resolve_asset_path(root: &Path, trace_id: &str, storage_key: &str) -> Result<PathBuf, String> {
    validate_storage_key(storage_key, (!trace_id.is_empty()).then_some(trace_id))?;
    let path = root.join(storage_key);
    if !path.starts_with(root) {
        return Err("request-log image storage key escapes storage root".to_string());
    }
    Ok(path)
}

fn validate_storage_key(storage_key: &str, expected_trace_id: Option<&str>) -> Result<(), String> {
    let path = Path::new(storage_key);
    if path.is_absolute() {
        return Err("invalid request-log image storage key".to_string());
    }
    let mut components = path.components();
    let Some(Component::Normal(trace_component)) = components.next() else {
        return Err("invalid request-log image storage key".to_string());
    };
    let Some(Component::Normal(file_component)) = components.next() else {
        return Err("invalid request-log image storage key".to_string());
    };
    if components.next().is_some()
        || trace_component.is_empty()
        || file_component.is_empty()
        || !is_safe_trace_id(trace_component.to_string_lossy().as_ref())
        || expected_trace_id.is_some_and(|trace_id| trace_component != trace_id)
    {
        return Err("invalid request-log image storage key".to_string());
    }
    Ok(())
}

fn is_safe_trace_id(trace_id: &str) -> bool {
    let mut components = Path::new(trace_id).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn detect_image_type(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(("image/png", "png"))
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(("image/jpeg", "jpg"))
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(("image/gif", "gif"))
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(("image/webp", "webp"))
    } else {
        None
    }
}

fn random_hex(byte_count: usize) -> String {
    let mut bytes = vec![0; byte_count];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let mut output = String::with_capacity(byte_count * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}
