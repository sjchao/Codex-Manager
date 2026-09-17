use crate::storage::{RequestLog, RequestTokenStat, Storage};

fn insert_log_with_tokens(
    storage: &Storage,
    aggregate_api_id: Option<&str>,
    created_at: i64,
    input_tokens: i64,
    cached_input_tokens: i64,
) {
    let log = RequestLog {
        request_path: "/v1/responses".to_string(),
        method: "POST".to_string(),
        aggregate_api_id: aggregate_api_id.map(str::to_string),
        created_at,
        ..Default::default()
    };
    let stat = RequestTokenStat {
        request_log_id: 0,
        key_id: Some("gk_1".to_string()),
        account_id: None,
        model: Some("gpt-5".to_string()),
        input_tokens: Some(input_tokens),
        cached_input_tokens: Some(cached_input_tokens),
        output_tokens: Some(1),
        total_tokens: None,
        reasoning_output_tokens: None,
        created_at,
    };
    let (_request_log_id, token_err) = storage
        .insert_request_log_with_token_stat(&log, &stat)
        .expect("insert request log with token stat");
    assert!(token_err.is_none(), "token stat should insert");
}

#[test]
fn summarize_request_token_stats_by_aggregate_api_groups_window_usage() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    insert_log_with_tokens(&storage, Some("agg_a"), 1500, 100, 80);
    insert_log_with_tokens(&storage, Some("agg_a"), 1600, 50, 0);
    insert_log_with_tokens(&storage, Some("agg_b"), 1700, 10, 5);
    insert_log_with_tokens(&storage, None, 1750, 999, 999);
    insert_log_with_tokens(&storage, Some("agg_a"), 500, 777, 777);

    let mut items = storage
        .summarize_request_token_stats_by_aggregate_api(1000, 2000)
        .expect("summarize token stats by aggregate api");
    items.sort_by(|left, right| left.aggregate_api_id.cmp(&right.aggregate_api_id));

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].aggregate_api_id, "agg_a");
    assert_eq!(items[0].input_tokens, 150);
    assert_eq!(items[0].cached_input_tokens, 80);
    assert_eq!(items[1].aggregate_api_id, "agg_b");
    assert_eq!(items[1].input_tokens, 10);
    assert_eq!(items[1].cached_input_tokens, 5);
}

fn insert_log_with_model_tokens(
    storage: &Storage,
    model: &str,
    created_at: i64,
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
) {
    let log = RequestLog {
        request_path: "/v1/responses".to_string(),
        method: "POST".to_string(),
        created_at,
        ..Default::default()
    };
    let stat = RequestTokenStat {
        request_log_id: 0,
        key_id: Some("gk_1".to_string()),
        account_id: None,
        model: Some(model.to_string()),
        input_tokens: Some(input_tokens),
        cached_input_tokens: Some(cached_input_tokens),
        output_tokens: Some(output_tokens),
        total_tokens: None,
        reasoning_output_tokens: None,
        created_at,
    };
    let (_request_log_id, token_err) = storage
        .insert_request_log_with_token_stat(&log, &stat)
        .expect("insert request log with token stat");
    assert!(token_err.is_none(), "token stat should insert");
}

#[test]
fn summarize_request_token_stats_by_model_groups_window_usage() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    insert_log_with_model_tokens(&storage, "gpt-5", 1500, 100, 80, 20);
    insert_log_with_model_tokens(&storage, "gpt-5", 1600, 50, 0, 10);
    insert_log_with_model_tokens(&storage, "deepseek-chat", 1700, 10, 5, 1);
    insert_log_with_model_tokens(&storage, "deepseek-chat", 500, 777, 777, 777);
    insert_log_with_model_tokens(&storage, "gpt-5", 2500, 999, 0, 999);

    let mut items = storage
        .summarize_request_token_stats_by_model(1000, 2000)
        .expect("summarize token stats by model");
    items.sort_by(|left, right| left.model.cmp(&right.model));

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].model, "deepseek-chat");
    assert_eq!(items[0].request_count, 1);
    assert_eq!(items[0].input_tokens, 10);
    assert_eq!(items[0].cached_input_tokens, 5);
    assert_eq!(items[0].output_tokens, 1);
    assert_eq!(items[0].total_tokens, 6);
    assert_eq!(items[1].model, "gpt-5");
    assert_eq!(items[1].request_count, 2);
    assert_eq!(items[1].input_tokens, 150);
    assert_eq!(items[1].cached_input_tokens, 80);
    assert_eq!(items[1].output_tokens, 30);
    assert_eq!(items[1].total_tokens, 100);
}
