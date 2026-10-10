use crate::storage::{QoderUsageEvent, Storage};

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
