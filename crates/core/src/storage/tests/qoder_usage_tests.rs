use crate::storage::{QoderSyncState, QoderUsageEvent, Storage};

fn usage_event(event_id: &str, model_tier: &str, credits: f64) -> QoderUsageEvent {
    QoderUsageEvent {
        event_id: event_id.to_string(),
        member_id: Some("mem_1".to_string()),
        user_id: Some("u_1".to_string()),
        user_email: Some("member@example.com".to_string()),
        source: Some("Qoder".to_string()),
        operation: Some("Agent".to_string()),
        model_tier: Some(model_tier.to_string()),
        credits,
        cost: Some(0.1),
        occurred_at: 1_790_000_000_000,
        day: "2026-10-10".to_string(),
        synced_at: 1_790_000_000,
    }
}

#[test]
fn upsert_usage_events_overwrites_revised_values() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    let written = storage
        .upsert_qoder_usage_events(&[usage_event("evt_1", "Auto", 1.0)])
        .expect("first upsert");
    assert_eq!(written, 1);

    // 模拟官方事后修正：同一事件被重归类为具体模型并重算积分
    let written = storage
        .upsert_qoder_usage_events(&[usage_event("evt_1", "DeepSeek-Flash", 9.5)])
        .expect("second upsert");
    assert_eq!(written, 1);
    assert_eq!(storage.count_qoder_usage_events().expect("count"), 1);
    assert_eq!(
        storage
            .qoder_usage_totals("2026-10-10", "2026-10-10", None)
            .expect("totals"),
        (9.5, 1)
    );

    let models = storage
        .qoder_model_usage("2026-10-10", "2026-10-10", None)
        .expect("model usage");
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].model_tier.as_deref(), Some("DeepSeek-Flash"));
    assert_eq!(models[0].credits, 9.5);
}

#[test]
fn sync_state_roundtrips_shared_package_totals() {
    let storage = Storage::open_in_memory().expect("open");
    storage.init().expect("init");

    storage
        .save_qoder_sync_state(&QoderSyncState {
            last_sync_at: Some(1_790_000_000),
            last_sync_status: Some("success".to_string()),
            last_sync_error: None,
            covered_until: Some(1_790_000_000_000),
            backfill_from: Some(1_780_000_000_000),
            last_event_at: Some(1_790_000_000_000),
            updated_at: 1_790_000_000,
            shared_package_limit: Some(3000.0),
            shared_package_remaining: Some(2200.0),
            shared_package_synced_at: Some(1_790_000_001),
        })
        .expect("save sync state");

    let state = storage
        .get_qoder_sync_state()
        .expect("get sync state")
        .expect("state exists");
    assert_eq!(state.shared_package_limit, Some(3000.0));
    assert_eq!(state.shared_package_remaining, Some(2200.0));
    assert_eq!(state.shared_package_synced_at, Some(1_790_000_001));
}
