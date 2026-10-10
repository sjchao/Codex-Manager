use crate::commands::shared::rpc_call_in_background;

#[tauri::command]
pub async fn service_qoder_settings_get(addr: Option<String>) -> Result<serde_json::Value, String> {
    rpc_call_in_background("qoder/settingsGet", addr, None).await
}

#[tauri::command]
pub async fn service_qoder_settings_set(
    addr: Option<String>,
    base_url: Option<String>,
    organization_id: Option<String>,
    api_key: Option<String>,
    enabled: Option<bool>,
    backfill_days: Option<i64>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
        "baseUrl": base_url,
        "organizationId": organization_id,
        "apiKey": api_key,
        "enabled": enabled,
        "backfillDays": backfill_days,
    });
    rpc_call_in_background("qoder/settingsSet", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_qoder_status(addr: Option<String>) -> Result<serde_json::Value, String> {
    rpc_call_in_background("qoder/status", addr, None).await
}

#[tauri::command]
pub async fn service_qoder_sync(addr: Option<String>) -> Result<serde_json::Value, String> {
    rpc_call_in_background("qoder/sync", addr, None).await
}

#[tauri::command]
pub async fn service_qoder_overview(
    addr: Option<String>,
    start_day: Option<String>,
    end_day: Option<String>,
    member_id: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
        "startDay": start_day,
        "endDay": end_day,
        "memberId": member_id,
    });
    rpc_call_in_background("qoder/overview", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_qoder_events(
    addr: Option<String>,
    start_day: Option<String>,
    end_day: Option<String>,
    member_id: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
        "startDay": start_day,
        "endDay": end_day,
        "memberId": member_id,
        "page": page,
        "pageSize": page_size,
    });
    rpc_call_in_background("qoder/events", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_qoder_today_top(
    addr: Option<String>,
    limit: Option<i64>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "limit": limit });
    rpc_call_in_background("qoder/todayTop", addr, Some(params)).await
}
