use super::*;

/// 函数 `gateway_logs_invalid_api_key_error`
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
fn gateway_logs_invalid_api_key_error() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-logs");
    let db_path: PathBuf = dir.join("codexmanager.db");

    let _guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let server = TestServer::start();
    let req_body = r#"{"model":"gpt-5.3-codex","input":"hello"}"#;
    let (status, _) = post_http_raw(
        &server.addr,
        "/v1/responses",
        req_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", "Bearer invalid-platform-key"),
        ],
    );
    assert_eq!(status, 403);

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init schema");
    let mut logs = Vec::new();
    for _ in 0..40 {
        logs = storage
            .list_request_logs(None, 100)
            .expect("list request logs");
        if !logs.is_empty() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let found = logs.iter().any(|item| {
        item.request_path == "/v1/responses"
            && item.status_code == Some(403)
            && item.input_tokens.is_none()
            && item.cached_input_tokens.is_none()
            && item.output_tokens.is_none()
            && item.total_tokens.is_none()
            && item.reasoning_output_tokens.is_none()
            && item.error.as_deref() == Some("invalid api key")
    });
    assert!(
        found,
        "expected invalid api key request to be logged, got {:?}",
        logs.iter()
            .map(|v| (&v.request_path, v.status_code, v.error.as_deref()))
            .collect::<Vec<_>>()
    );
}

/// 函数 `gateway_tolerates_non_ascii_turn_metadata_header`
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
fn gateway_tolerates_non_ascii_turn_metadata_header() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-logs-nonascii");
    let db_path: PathBuf = dir.join("codexmanager.db");

    let _guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let server = TestServer::start();
    let req_body = r#"{"model":"gpt-5.3-codex","input":"hello"}"#;
    let metadata = r#"{"workspaces":{"D:\\MyComputer\\own\\GPTTeam相关\\CodexManager\\CodexManager":{"latest_git_commit_hash":"abc123"}}}"#;
    let (status, body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        req_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", "Bearer invalid-platform-key"),
            ("x-codex-turn-metadata", metadata),
        ],
    );
    assert_eq!(status, 403, "response body: {body}");
}

/// 函数 `insert_api_key_with_allowed_models`
///
/// 作者: gaohongshun
///
/// 时间: 2026-09-17
///
/// # 参数
/// - storage: 参数 storage
/// - key_id: 参数 key_id
/// - platform_key: 参数 platform_key
/// - allowed_models: 参数 allowed_models
///
/// # 返回
/// 无
fn insert_api_key_with_allowed_models(
    storage: &Storage,
    key_id: &str,
    platform_key: &str,
    allowed_models: Vec<&str>,
) {
    storage
        .insert_api_key(&ApiKey {
            id: key_id.to_string(),
            name: Some(key_id.to_string()),
            group_name: None,
            model_slug: None,
            reasoning_effort: None,
            service_tier: None,
            rotation_strategy: "account_rotation".to_string(),
            aggregate_api_id: None,
            aggregate_api_url: None,
            client_type: "codex".to_string(),
            protocol_type: "openai_compat".to_string(),
            auth_scheme: "authorization_bearer".to_string(),
            upstream_base_url: None,
            static_headers_json: None,
            allowed_models: allowed_models.into_iter().map(str::to_string).collect(),
            key_hash: hash_platform_key_for_test(platform_key),
            status: "active".to_string(),
            created_at: now_ts(),
            last_used_at: None,
        })
        .expect("insert api key");
}

/// 函数 `gateway_restricted_platform_key_rejects_disallowed_model`
///
/// 作者: gaohongshun
///
/// 时间: 2026-09-17
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn gateway_restricted_platform_key_rejects_disallowed_model() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-allowed-models");
    let db_path: PathBuf = dir.join("codexmanager.db");

    let _guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init schema");
    let platform_key = "pk_restricted_models";
    insert_api_key_with_allowed_models(
        &storage,
        "gk_restricted_models",
        platform_key,
        vec!["gpt-5.4-mini"],
    );

    let server = TestServer::start();
    let req_body = r#"{"model":"gpt-5.3-codex","input":"hello"}"#;
    let (status, body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        req_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    assert_eq!(status, 403, "response body: {body}");
    assert!(
        body.contains("model not allowed for this platform key: gpt-5.3-codex"),
        "response body: {body}"
    );
}

/// 函数 `gateway_restricted_platform_key_filters_models_list`
///
/// 作者: gaohongshun
///
/// 时间: 2026-09-17
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn gateway_restricted_platform_key_filters_models_list() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-allowed-models-list");
    let db_path: PathBuf = dir.join("codexmanager.db");

    let _db_guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());
    let _upstream_guard = EnvGuard::set(
        "CODEXMANAGER_UPSTREAM_BASE_URL",
        "http://127.0.0.1:1/backend-api/codex",
    );

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init db");
    let platform_key = "pk_restricted_models_list";
    insert_api_key_with_allowed_models(
        &storage,
        "gk_restricted_models_list",
        platform_key,
        vec!["gpt-5.4-mini"],
    );
    seed_model_options_cache(
        &storage,
        &["gpt-5.3-codex", "gpt-5.4-mini", "gpt-image2"],
    );

    let server = TestServer::start();
    let (status, response_body) = get_http_raw(
        &server.addr,
        "/v1/models",
        &[("Authorization", &format!("Bearer {platform_key}"))],
    );
    assert_eq!(status, 200, "gateway response: {response_body}");

    let value: serde_json::Value =
        serde_json::from_str(&response_body).expect("parse models list response");
    let data = value
        .get("data")
        .and_then(|v| v.as_array())
        .expect("models list data array");
    let ids = data
        .iter()
        .filter_map(|item| item.get("id").and_then(|v| v.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["gpt-5.4-mini"], "models response: {response_body}");
}
