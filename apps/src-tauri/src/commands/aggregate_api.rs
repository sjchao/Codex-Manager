use crate::commands::shared::rpc_call_in_background;

/// 函数 `service_aggregate_api_list`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
///
/// # 返回
/// 返回函数执行结果
#[tauri::command]
pub async fn service_aggregate_api_list(
    addr: Option<String>,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background("aggregateApi/list", addr, None).await
}

/// 函数 `service_aggregate_api_create`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
/// - provider_type: 参数 provider_type
/// - supplier_name: 参数 supplier_name
/// - sort: 参数 sort
/// - url: 参数 url
/// - key: 参数 key
///
/// # 返回
/// 返回函数执行结果
#[tauri::command]
pub async fn service_aggregate_api_create(
    addr: Option<String>,
    provider_type: Option<String>,
    supported_models: Option<Vec<String>>,
    supplier_name: Option<String>,
    sort: Option<i64>,
    weight: Option<i64>,
    url: Option<String>,
    key: Option<String>,
    auth_type: Option<String>,
    auth_custom_enabled: Option<bool>,
    auth_params: Option<serde_json::Value>,
    action_custom_enabled: Option<bool>,
    action: Option<String>,
    username: Option<String>,
    password: Option<String>,
    sub2api_account_id: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
        "providerType": provider_type,
        "supportedModels": supported_models,
        "supplierName": supplier_name,
        "sort": sort,
        "weight": weight,
        "url": url,
        "key": key,
        "authType": auth_type,
        "authCustomEnabled": auth_custom_enabled,
        "authParams": auth_params,
        "actionCustomEnabled": action_custom_enabled,
        "action": action,
        "username": username,
        "password": password,
        "sub2apiAccountId": sub2api_account_id,
    });
    rpc_call_in_background("aggregateApi/create", addr, Some(params)).await
}

/// 函数 `service_aggregate_api_update`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
/// - id: 参数 id
/// - provider_type: 参数 provider_type
/// - supplier_name: 参数 supplier_name
/// - sort: 参数 sort
/// - url: 参数 url
/// - key: 参数 key
///
/// # 返回
/// 返回函数执行结果
#[tauri::command]
pub async fn service_aggregate_api_update(
    addr: Option<String>,
    id: String,
    provider_type: Option<String>,
    supported_models: Option<Vec<String>>,
    supplier_name: Option<String>,
    sort: Option<i64>,
    weight: Option<i64>,
    url: Option<String>,
    key: Option<String>,
    auth_type: Option<String>,
    auth_custom_enabled: Option<bool>,
    auth_params: Option<serde_json::Value>,
    action_custom_enabled: Option<bool>,
    action: Option<String>,
    username: Option<String>,
    password: Option<String>,
    sub2api_account_id: Option<String>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
        "id": id,
        "providerType": provider_type,
        "supportedModels": supported_models,
        "supplierName": supplier_name,
        "sort": sort,
        "weight": weight,
        "url": url,
        "key": key,
        "authType": auth_type,
        "authCustomEnabled": auth_custom_enabled,
        "authParams": auth_params,
        "actionCustomEnabled": action_custom_enabled,
        "action": action,
        "username": username,
        "password": password,
        "sub2apiAccountId": sub2api_account_id,
    });
    rpc_call_in_background("aggregateApi/update", addr, Some(params)).await
}

/// 函数 `service_aggregate_api_read_secret`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
/// - id: 参数 id
///
/// # 返回
/// 返回函数执行结果
#[tauri::command]
pub async fn service_aggregate_api_read_secret(
    addr: Option<String>,
    id: String,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "id": id });
    rpc_call_in_background("aggregateApi/readSecret", addr, Some(params)).await
}

/// 函数 `service_aggregate_api_delete`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
/// - id: 参数 id
///
/// # 返回
/// 返回函数执行结果
#[tauri::command]
pub async fn service_aggregate_api_delete(
    addr: Option<String>,
    id: String,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "id": id });
    rpc_call_in_background("aggregateApi/delete", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_aggregate_api_disable(
    addr: Option<String>,
    id: String,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "id": id });
    rpc_call_in_background("aggregateApi/disable", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_aggregate_api_enable(
    addr: Option<String>,
    id: String,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "id": id });
    rpc_call_in_background("aggregateApi/enable", addr, Some(params)).await
}

/// 函数 `service_aggregate_api_test_connection`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - addr: 参数 addr
/// - id: 参数 id
///
/// # 返回
/// 返回函数执行结果
#[tauri::command]
pub async fn service_aggregate_api_test_connection(
    addr: Option<String>,
    id: String,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "id": id });
    rpc_call_in_background("aggregateApi/testConnection", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_aggregate_api_refresh_models(
    addr: Option<String>,
    id: String,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "id": id });
    rpc_call_in_background("aggregateApi/refreshModels", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_aggregate_api_usage_summary(
    addr: Option<String>,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background("aggregateApi/usageSummary", addr, None).await
}

#[tauri::command]
pub async fn service_aggregate_api_usage_sync(
    addr: Option<String>,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background("aggregateApi/usageSync", addr, None).await
}

#[tauri::command]
pub async fn service_aggregate_api_usage_credentials_update(
    addr: Option<String>,
    id: String,
    auth_token: String,
    refresh_token: Option<String>,
    token_expires_at: Option<i64>,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({
        "id": id,
        "authToken": auth_token,
        "refreshToken": refresh_token,
        "tokenExpiresAt": token_expires_at,
    });
    rpc_call_in_background("aggregateApi/usageCredentials/update", addr, Some(params)).await
}

#[tauri::command]
pub async fn service_sub2api_list(addr: Option<String>) -> Result<serde_json::Value, String> {
    rpc_call_in_background("sub2api/list", addr, None).await
}

#[tauri::command]
pub async fn service_sub2api_create(
    addr: Option<String>, base_url: String, auth_token: String,
    refresh_token: Option<String>, token_expires_at: Option<i64>,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background("sub2api/create", addr, Some(serde_json::json!({
        "baseUrl": base_url, "authToken": auth_token, "refreshToken": refresh_token,
        "tokenExpiresAt": token_expires_at,
    }))).await
}

#[tauri::command]
pub async fn service_sub2api_update(
    addr: Option<String>, id: String, base_url: String, auth_token: Option<String>,
    refresh_token: Option<String>, token_expires_at: Option<i64>,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background("sub2api/update", addr, Some(serde_json::json!({
        "id": id, "baseUrl": base_url, "authToken": auth_token, "refreshToken": refresh_token,
        "tokenExpiresAt": token_expires_at,
    }))).await
}

#[tauri::command]
pub async fn service_sub2api_delete(addr: Option<String>, id: String) -> Result<serde_json::Value, String> {
    rpc_call_in_background("sub2api/delete", addr, Some(serde_json::json!({"id": id}))).await
}

#[tauri::command]
pub async fn service_sub2api_sync(addr: Option<String>, id: String) -> Result<serde_json::Value, String> {
    rpc_call_in_background("sub2api/sync", addr, Some(serde_json::json!({"id": id}))).await
}

#[tauri::command]
pub async fn service_sub2api_sync_all(addr: Option<String>) -> Result<serde_json::Value, String> {
    rpc_call_in_background("sub2api/syncAll", addr, None).await
}
