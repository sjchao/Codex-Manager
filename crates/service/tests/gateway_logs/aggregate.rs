use super::*;
use codexmanager_core::storage::AggregateApi;

/// 函数 `insert_aggregate_platform_key`
///
/// 作者: gaohongshun
///
/// 时间: 2026-10-13
///
/// # 参数
/// - storage: 参数 storage
/// - key_id: 参数 key_id
/// - platform_key: 参数 platform_key
///
/// # 返回
/// 无
fn insert_aggregate_platform_key(storage: &Storage, key_id: &str, platform_key: &str) {
    storage
        .insert_api_key(&ApiKey {
            id: key_id.to_string(),
            name: Some(key_id.to_string()),
            group_name: None,
            model_slug: None,
            reasoning_effort: None,
            service_tier: None,
            rotation_strategy: "aggregate_api_rotation".to_string(),
            aggregate_api_id: None,
            aggregate_api_url: None,
            client_type: "codex".to_string(),
            protocol_type: "openai_compat".to_string(),
            auth_scheme: "authorization_bearer".to_string(),
            upstream_base_url: None,
            static_headers_json: None,
            allowed_models: vec!["deepseek-flash".to_string()],
            key_hash: hash_platform_key_for_test(platform_key),
            status: "active".to_string(),
            created_at: now_ts(),
            last_used_at: None,
        })
        .expect("insert api key");
}

/// 函数 `aggregate_upstream_request_forces_identity_accept_encoding`
///
/// 作者: gaohongshun
///
/// 时间: 2026-10-13
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn aggregate_upstream_request_forces_identity_accept_encoding() {
    // 中文注释：客户端声明的 accept-encoding 若透传到聚合上游，上游（如 DeepSeek
    // 官方 API）会压缩 SSE；压缩字节流会让按 UTF-8 逐行解析的转发泵报
    // "stream did not contain valid UTF-8"，这里回归验证上游只收到 identity。
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-aggregate-accept-encoding");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let sse_body = concat!(
        "data: {\"id\":\"chatcmpl-test\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"deepseek-flash\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"hi\"},\"finish_reason\":null}]}\n\n",
        "data: [DONE]\n\n"
    );
    let (upstream_addr, captured_rx, upstream_join) =
        start_mock_upstream_once_with_content_type(sse_body, "text/event-stream");

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init schema");
    insert_aggregate_platform_key(&storage, "gk_aggregate_encoding", "pk_aggregate_encoding");
    storage
        .insert_aggregate_api(&AggregateApi {
            id: "agg-encoding".to_string(),
            provider_type: "codex".to_string(),
            supported_models: vec!["deepseek-flash".to_string()],
            supplier_name: Some("encoding-test".to_string()),
            sort: 0,
            url: format!("http://{upstream_addr}"),
            auth_type: "apikey".to_string(),
            auth_params_json: None,
            action: None,
            status: "active".to_string(),
            created_at: now_ts(),
            updated_at: now_ts(),
            last_test_at: None,
            last_test_status: None,
            last_test_error: None,
            sub2api_account_id: None,
            active_time_start: None,
            active_time_end: None,
        })
        .expect("insert aggregate api");
    storage
        .upsert_aggregate_api_secret("agg-encoding", "sk-test")
        .expect("insert aggregate api secret");

    let server = TestServer::start();
    let req_body =
        r#"{"model":"deepseek-flash","stream":true,"messages":[{"role":"user","content":"hi"}]}"#;
    let (status, body) = post_http_raw(
        &server.addr,
        "/v1/chat/completions",
        req_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", "Bearer pk_aggregate_encoding"),
            ("Accept-Encoding", "gzip, deflate, br, zstd"),
        ],
    );

    let captured = captured_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("captured upstream request");
    let _ = upstream_join.join();

    assert_eq!(status, 200, "response body: {body}");
    assert!(body.contains("[DONE]"), "relayed body: {body}");
    assert_eq!(
        captured.headers.get("accept-encoding").map(String::as_str),
        Some("identity"),
        "captured headers: {:?}",
        captured.headers
    );
}
