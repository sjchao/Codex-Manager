use codexmanager_core::storage::Storage;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    request_body_text, should_write_gateway_error_fallback, write_request_log, RequestLogTraceContext,
    RequestLogUsage,
};
use crate::gateway::ModelType;

struct EnvGuard {
    original: Option<std::ffi::OsString>,
}

impl EnvGuard {
    fn set_database_path(path: &std::path::Path) -> Self {
        let original = std::env::var_os("CODEXMANAGER_DB_PATH");
        std::env::set_var("CODEXMANAGER_DB_PATH", path);
        Self { original }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(value) = &self.original {
            std::env::set_var("CODEXMANAGER_DB_PATH", value);
        } else {
            std::env::remove_var("CODEXMANAGER_DB_PATH");
        }
    }
}

fn temporary_database_path(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir()
        .join(format!("codexmanager-request-log-{name}-{}-{unique}", std::process::id()))
        .join("data")
        .join("codexmanager.db")
}

#[test]
fn request_log_persists_image_type_and_defaults_missing_image_count_to_one() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    write_request_log(
        &storage,
        RequestLogTraceContext {
            model_type: Some(ModelType::Image),
            image_size: Some("4K"),
            ..Default::default()
        },
        Some("key-image"),
        None,
        "/v1/images/generations",
        "POST",
        Some("gpt-image2"),
        None,
        None,
        Some(200),
        RequestLogUsage::default(),
        None,
        Some(12),
    );

    let logs = storage
        .list_request_logs_paginated(None, None, 0, 20)
        .expect("list logs");
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].model_type.as_deref(), Some("image"));
    assert_eq!(logs[0].image_count, Some(1));
    assert_eq!(logs[0].image_size.as_deref(), Some("4K"));
}

#[test]
fn request_log_write_failure_removes_uncatalogued_image_results() {
    let _env_lock = crate::test_env_guard();
    let db_path = temporary_database_path("image-cleanup");
    fs::create_dir_all(db_path.parent().expect("database parent"))
        .expect("create database directory");
    let _db_path_guard = EnvGuard::set_database_path(&db_path);
    let body = br#"{"data":[{"b64_json":"iVBORw0KGgo="}]}"#;
    let image_results = crate::requestlog::image_assets::cache_openai_image_results(
        &db_path,
        "trc-image-cleanup",
        body,
    );
    let metadata = serde_json::to_string(&image_results).expect("serialize image metadata");
    let image_path = db_path
        .parent()
        .expect("database parent")
        .join("request-log-images")
        .join(&image_results[0].storage_key);
    assert!(image_path.is_file());

    let storage = Storage::open_in_memory().expect("open uninitialized storage");
    write_request_log(
        &storage,
        RequestLogTraceContext {
            trace_id: Some("trc-image-cleanup"),
            model_type: Some(ModelType::Image),
            image_results_json: Some(metadata.as_str()),
            ..Default::default()
        },
        Some("key-image-cleanup"),
        None,
        "/v1/images/generations",
        "POST",
        Some("gpt-image2"),
        None,
        None,
        Some(200),
        RequestLogUsage::default(),
        None,
        Some(1),
    );

    assert!(!image_path.exists());
    let _ = fs::remove_dir_all(db_path.parent().expect("database parent").parent().expect("test root"));
}

#[test]
fn gateway_error_fallback_matches_cloudflare_and_rate_limit_errors() {
    assert!(should_write_gateway_error_fallback(
        Some(403),
        Some("Cloudflare 安全验证页（title=Just a moment...）"),
    ));
    assert!(should_write_gateway_error_fallback(
        Some(429),
        Some("type=usage_limit_reached The usage limit"),
    ));
    assert!(!should_write_gateway_error_fallback(
        Some(500),
        Some("internal server error"),
    ));
}

#[test]
fn request_body_text_skips_blank_and_invalid_utf8_bodies() {
    assert_eq!(
        request_body_text(Some(br#" {"model":"gpt-5"} "#)),
        Some(r#"{"model":"gpt-5"}"#)
    );
    assert_eq!(request_body_text(Some(b"   ")), None);
    assert_eq!(request_body_text(Some(&[0xff, 0xfe])), None);
    assert_eq!(request_body_text(None), None);
}

#[test]
fn request_log_persists_request_and_response_bodies() {
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");

    write_request_log(
        &storage,
        RequestLogTraceContext {
            trace_id: Some("trc-bodies"),
            request_body: Some(r#"{"model":"gpt-5","input":"hi"}"#),
            response_body: Some("你好，有什么可以帮你？"),
            ..Default::default()
        },
        Some("key-bodies"),
        None,
        "/v1/responses",
        "POST",
        Some("gpt-5"),
        None,
        None,
        Some(200),
        RequestLogUsage::default(),
        None,
        Some(12),
    );

    let logs = storage.list_request_logs(None, 20).expect("list logs");
    assert_eq!(logs.len(), 1);
    assert!(logs[0].has_request_body);
    assert!(logs[0].has_response_body);
    assert!(logs[0].request_body.is_none());
    assert!(logs[0].response_body.is_none());

    let bodies = storage
        .read_request_log_bodies_by_trace_id("trc-bodies")
        .expect("read bodies")
        .expect("bodies present");
    assert_eq!(
        bodies.request_body.as_deref(),
        Some(r#"{"model":"gpt-5","input":"hi"}"#)
    );
    assert_eq!(bodies.response_body.as_deref(), Some("你好，有什么可以帮你？"));
}

#[test]
fn request_log_truncates_bodies_at_configured_limit() {
    let _env_lock = crate::test_env_guard();
    let storage = Storage::open_in_memory().expect("open storage");
    storage.init().expect("init storage");
    let original_limit = std::env::var_os("CODEXMANAGER_REQUEST_LOG_BODY_LIMIT_BYTES");
    std::env::set_var("CODEXMANAGER_REQUEST_LOG_BODY_LIMIT_BYTES", "16");

    let long_request_body = "x".repeat(64);
    write_request_log(
        &storage,
        RequestLogTraceContext {
            trace_id: Some("trc-body-limit"),
            request_body: Some(long_request_body.as_str()),
            response_body: Some("short"),
            ..Default::default()
        },
        Some("key-body-limit"),
        None,
        "/v1/responses",
        "POST",
        Some("gpt-5"),
        None,
        None,
        Some(200),
        RequestLogUsage::default(),
        None,
        Some(12),
    );

    match original_limit {
        Some(value) => std::env::set_var("CODEXMANAGER_REQUEST_LOG_BODY_LIMIT_BYTES", value),
        None => std::env::remove_var("CODEXMANAGER_REQUEST_LOG_BODY_LIMIT_BYTES"),
    }

    let bodies = storage
        .read_request_log_bodies_by_trace_id("trc-body-limit")
        .expect("read bodies")
        .expect("bodies present");
    let stored = bodies.request_body.expect("request body stored");
    assert!(stored.starts_with("xxxxxxxxxxxxxxxx"));
    assert!(stored.ends_with("[truncated]"));
    assert_eq!(bodies.response_body.as_deref(), Some("short"));
}
