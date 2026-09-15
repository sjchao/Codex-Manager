use codexmanager_core::rpc::types::{Sub2ApiAccountListResult, Sub2ApiAccountSummary};
use codexmanager_core::storage::{now_ts, Storage, Sub2ApiAccount};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde_json::Value;
use std::time::Duration;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::thread;

use crate::storage_helpers::open_storage;

const TOKEN_REFRESH_AHEAD_SECS: i64 = 300;
const TOKEN_REFRESH_POLL_INTERVAL: Duration = Duration::from_secs(300);

static TOKEN_REFRESH_POLLING_STARTED: OnceLock<()> = OnceLock::new();

fn text(value: Option<&Value>) -> Option<String> {
    value.and_then(|v| v.as_str()).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
}
fn number(value: Option<&Value>) -> Option<f64> {
    value.and_then(|v| v.as_f64().or_else(|| v.as_str()?.trim().parse().ok()))
}
fn integer(value: Option<&Value>) -> Option<i64> {
    value.and_then(|v| v.as_i64().or_else(|| v.as_str()?.trim().parse().ok()))
}
fn data(payload: &Value) -> &Value { payload.get("data").unwrap_or(payload) }
fn field<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Value> {
    value.as_object().and_then(|object| names.iter().find_map(|name| object.get(*name)))
}
fn deep_field<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Value> {
    if let Some(found) = field(value, names) { return Some(found); }
    match value {
        Value::Object(object) => object.values().find_map(|child| deep_field(child, names)),
        Value::Array(items) => items.iter().find_map(|child| deep_field(child, names)),
        _ => None,
    }
}
fn origin(raw: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(raw.trim()).map_err(|_| "Sub2API URL 无效".to_string())?;
    let origin = url.origin().ascii_serialization();
    if origin == "null" { Err("Sub2API URL 必须使用 HTTP 或 HTTPS".to_string()) } else { Ok(origin) }
}
/// 面板登录返回的是 JWT，未手动填写过期时间时从 exp 声明补齐。
pub(crate) fn jwt_expires_at(token: &str) -> Option<i64> {
    use base64::Engine;
    let payload = token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload.trim()).ok()?;
    let value: Value = serde_json::from_slice(&decoded).ok()?;
    integer(field(&value, &["exp"])).map(normalize_expires_at)
}
/// 兼容秒与毫秒两种 Unix 时间戳写法。
fn normalize_expires_at(value: i64) -> i64 {
    if value > 1_000_000_000_000 { value / 1_000 } else { value }
}
fn successful(payload: &Value) -> bool {
    let object = payload.as_object();
    if matches!(object.and_then(|o| o.get("success")).and_then(Value::as_bool), Some(false)) {
        return false;
    }
    match object.and_then(|o| o.get("code")) {
        Some(Value::Number(code)) => code.as_i64() == Some(0),
        Some(Value::String(code)) => code.trim() == "0",
        Some(_) => false,
        None => true,
    }
}
fn request_json(_client: &Client, builder: reqwest::blocking::RequestBuilder) -> Result<Value, String> {
    let response = builder.send().map_err(|e| format!("Sub2API 请求失败: {e}"))?;
    let status = response.status();
    let payload = response.json::<Value>().map_err(|e| format!("Sub2API 返回 JSON 无效: {e}"))?;
    if status == StatusCode::UNAUTHORIZED { return Err("HTTP 401".to_string()); }
    if !status.is_success() { return Err(format!("Sub2API 返回 HTTP {}", status.as_u16())); }
    if !successful(&payload) { return Err(text(field(&payload, &["message", "msg", "error"])).unwrap_or_else(|| "Sub2API 返回业务错误".to_string())); }
    Ok(payload)
}
fn refresh(storage: &Storage, account: &mut Sub2ApiAccount, client: &Client, base: &str) -> Result<(), String> {
    let refresh_token = account.refresh_token.as_deref().filter(|v| !v.trim().is_empty()).ok_or_else(|| "缺少 refresh_token".to_string())?;
    let payload = request_json(client, client.post(format!("{base}/api/v1/auth/refresh")).json(&serde_json::json!({"refresh_token": refresh_token})))?;
    let body = data(&payload);
    let access = text(field(body, &["access_token", "accessToken"])).ok_or_else(|| "刷新响应缺少 access_token".to_string())?;
    let next_refresh = text(field(body, &["refresh_token", "refreshToken"]));
    let expires = integer(field(body, &["expires_in", "expiresIn"]))
        .map(|v| now_ts().saturating_add(v.max(0)))
        .or_else(|| jwt_expires_at(&access));
    account.auth_token = access;
    if next_refresh.is_some() { account.refresh_token = next_refresh; }
    account.token_expires_at = expires.or(account.token_expires_at);
    storage.update_sub2api_account_tokens(&account.id, &account.auth_token, account.refresh_token.as_deref(), expires).map_err(|e| format!("保存刷新 token 失败: {e}"))?;
    log::info!("event=sub2api_token_refreshed account_id={} expires_at={:?}", account.id, account.token_expires_at);
    Ok(())
}
fn token_expiring_soon(expires_at: Option<i64>, ahead_secs: i64) -> bool {
    expires_at.is_some_and(|expires| expires <= now_ts().saturating_add(ahead_secs))
}
fn account_label(account: &Sub2ApiAccount) -> String {
    account.account_name.clone().or_else(|| account.account_email.clone()).unwrap_or_else(|| account.base_url.clone())
}
fn has_refresh_token(account: &Sub2ApiAccount) -> bool {
    account.refresh_token.as_deref().is_some_and(|value| !value.trim().is_empty())
}
fn authorized(client: &Client, account: &mut Sub2ApiAccount, base: &str, path: &str) -> Result<Value, String> {
    request_json(client, client.get(format!("{base}{path}")).bearer_auth(&account.auth_token))
}
/// 在过期前主动刷新；未记录过期时间但有 refresh_token 时先换一次新 token 以补全 expires_in。
fn refresh_when_needed(storage: &Storage, account: &mut Sub2ApiAccount, client: &Client, base: &str) -> Result<(), String> {
    let should_refresh = token_expiring_soon(account.token_expires_at, TOKEN_REFRESH_AHEAD_SECS) || account.token_expires_at.is_none();
    if !should_refresh || !has_refresh_token(account) { return Ok(()); }
    refresh(storage, account, client, base)
}
fn sync_one(storage: &Storage, account: &mut Sub2ApiAccount) -> Result<(), String> {
    let base = origin(&account.base_url)?;
    let client = crate::gateway::fresh_upstream_client_with_timeout(Duration::from_secs(30));
    if let Err(error) = refresh_when_needed(storage, account, &client, &base) {
        log::warn!("event=sub2api_token_refresh_failed account={} err={}", account_label(account), error);
        if token_expiring_soon(account.token_expires_at, 0) { return Err(format!("Sub2API token 刷新失败: {error}")); }
    }
    let fetch = |account: &mut Sub2ApiAccount, path: &str| -> Result<Value, String> {
        match authorized(&client, account, &base, path) {
            Err(error) if error == "HTTP 401" => {
                refresh(storage, account, &client, &base)?;
                authorized(&client, account, &base, path)
            }
            result => result,
        }
    };
    let me = fetch(account, "/api/v1/auth/me")?;
    let profile = fetch(account, "/api/v1/user/profile").ok();
    // 今日消费直接取面板统计接口：逐页拉取用量明细会持续占用 Heavy 限流配额，极易触发 429。
    let stats = fetch(account, "/api/v1/usage/dashboard/stats")?;
    let me_data = data(&me);
    let profile_data = profile.as_ref().map(data).unwrap_or(&Value::Null);
    let stats_data = data(&stats);
    let account_name = text(deep_field(me_data, &["username", "user_name", "name", "account_name", "accountName"]))
        .or_else(|| text(deep_field(profile_data, &["username", "user_name", "name", "account_name", "accountName"])))
        .or_else(|| text(deep_field(me_data, &["email", "account_email", "accountEmail"])))
        .or_else(|| text(deep_field(profile_data, &["email", "account_email", "accountEmail"])));
    let account_email = text(deep_field(me_data, &["email", "account_email", "accountEmail"]))
        .or_else(|| text(deep_field(profile_data, &["email", "account_email", "accountEmail"])));
    let balance = number(deep_field(me_data, &["balance", "credits", "remaining_balance", "remainingBalance"]))
        .or_else(|| number(deep_field(profile_data, &["balance", "credits", "remaining_balance", "remainingBalance"])))
        .or_else(|| number(deep_field(stats_data, &["balance", "credits", "remaining_balance", "remainingBalance"])));
    let actual_cost = number(deep_field(stats_data, &["today_actual_cost", "todayActualCost"]));
    let total_cost = number(deep_field(stats_data, &["today_cost", "todayCost"]));
    let request_count = integer(deep_field(stats_data, &["today_requests", "todayRequests"]));
    storage.update_sub2api_account_sync(
        &account.id,
        account_name.as_deref(),
        account_email.as_deref(),
        balance,
        actual_cost,
        total_cost,
        request_count,
        "success",
        None,
    ).map_err(|e| format!("保存 Sub2API 同步结果失败: {e}"))
}
fn generate_id() -> String {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    format!("sub2api_{}_{}", now_ts(), NEXT_ID.fetch_add(1, Ordering::Relaxed))
}

pub(crate) fn list_sub2api_accounts() -> Result<Sub2ApiAccountListResult, String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let items = storage.list_sub2api_account_summaries().map_err(|e| e.to_string())?.into_iter().map(|item| Sub2ApiAccountSummary {
        id: item.id, base_url: item.base_url, token_expires_at: item.token_expires_at,
        account_name: item.account_name, account_email: item.account_email,
        balance: item.balance, today_actual_cost: item.today_actual_cost, today_total_cost: item.today_total_cost,
        today_request_count: item.today_request_count, updated_at: item.updated_at, last_sync_at: item.last_sync_at,
        last_sync_status: item.last_sync_status, last_sync_error: item.last_sync_error,
    }).collect();
    Ok(Sub2ApiAccountListResult { items })
}
pub(crate) fn save_sub2api_account(id: Option<String>, base_url: String, auth_token: Option<String>, refresh_token: Option<String>, token_expires_at: Option<i64>) -> Result<String, String> {
    let base_url = base_url.trim().to_string();
    if base_url.is_empty() { return Err("Sub2API URL 不能为空".to_string()); }
    origin(&base_url)?;
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let account_id = id.filter(|v| !v.trim().is_empty()).unwrap_or_else(generate_id);
    let current = storage.find_sub2api_account_by_id(&account_id).map_err(|e| e.to_string())?;
    let token = auth_token.unwrap_or_default().trim().to_string();
    if current.is_none() && token.is_empty() { return Err("新增 Sub2API 时必须填写 auth_token".to_string()); }
    let effective_token = if token.is_empty() { current.as_ref().map(|item| item.auth_token.clone()).unwrap_or_default() } else { token.clone() };
    let expires_at = match token_expires_at {
        Some(value) => Some(normalize_expires_at(value)),
        None => jwt_expires_at(&effective_token).or_else(|| current.as_ref().and_then(|item| item.token_expires_at)),
    };
    storage.upsert_sub2api_account(&account_id, &base_url, &token, refresh_token.as_deref().map(str::trim).filter(|v| !v.is_empty()), expires_at).map_err(|e| e.to_string())?;
    let mut account = storage.find_sub2api_account_by_id(&account_id).map_err(|e| e.to_string())?.ok_or_else(|| "Sub2API 账号保存失败".to_string())?;
    match sync_one(&storage, &mut account) {
        Ok(()) => Ok(account_id),
        Err(error) => { let _ = storage.update_sub2api_account_sync(&account_id, None, None, None, None, None, None, "failed", Some(error.as_str())); Err(error) }
    }
}
pub(crate) fn delete_sub2api_account(account_id: &str) -> Result<(), String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    storage.delete_sub2api_account(account_id.trim()).map_err(|e| e.to_string())
}
pub(crate) fn sync_sub2api_account(account_id: &str) -> Result<(), String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let mut account = storage.find_sub2api_account_by_id(account_id.trim()).map_err(|e| e.to_string())?.ok_or_else(|| "Sub2API 账号不存在".to_string())?;
    sync_one(&storage, &mut account).map_err(|error| { let _ = storage.update_sub2api_account_sync(account_id, None, None, None, None, None, None, "failed", Some(error.as_str())); error })
}
pub(crate) fn sync_all_sub2api_accounts() -> Result<Sub2ApiAccountListResult, String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let accounts = storage.list_sub2api_accounts().map_err(|e| e.to_string())?;
    for mut account in accounts { if let Err(error) = sync_one(&storage, &mut account) { let _ = storage.update_sub2api_account_sync(&account.id, None, None, None, None, None, None, "failed", Some(error.as_str())); } }
    list_sub2api_accounts()
}

/// 只做 token 续期，不拉取用量；供后台定时任务使用。
fn refresh_due_tokens() -> Result<(), String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let accounts = storage.list_sub2api_accounts().map_err(|e| e.to_string())?;
    for mut account in accounts {
        let due = token_expiring_soon(account.token_expires_at, TOKEN_REFRESH_AHEAD_SECS)
            || account.token_expires_at.is_none();
        if !due || !has_refresh_token(&account) { continue; }
        let base = match origin(&account.base_url) {
            Ok(base) => base,
            Err(error) => { log::warn!("event=sub2api_token_refresh_skipped account={} err={}", account_label(&account), error); continue; }
        };
        let client = crate::gateway::fresh_upstream_client_with_timeout(Duration::from_secs(30));
        let label = account_label(&account);
        if let Err(error) = refresh_when_needed(&storage, &mut account, &client, &base) {
            log::warn!("event=sub2api_token_refresh_failed account={} err={}", label, error);
        }
    }
    Ok(())
}

pub(crate) fn ensure_sub2api_token_refresh_polling() {
    TOKEN_REFRESH_POLLING_STARTED.get_or_init(|| {
        let _ = thread::Builder::new()
            .name("sub2api-token-refresh".to_string())
            .spawn(|| loop {
                if let Err(error) = refresh_due_tokens() {
                    log::warn!("event=sub2api_token_refresh_polling_failed err={}", error);
                }
                thread::sleep(TOKEN_REFRESH_POLL_INTERVAL);
            });
    });
}

#[cfg(test)]
mod tests {
    use super::{jwt_expires_at, normalize_expires_at, token_expiring_soon, TOKEN_REFRESH_AHEAD_SECS};
    use base64::Engine;
    use codexmanager_core::storage::now_ts;

    fn jwt_with_payload(payload: serde_json::Value) -> String {
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).expect("payload json"));
        format!("header.{encoded}.signature")
    }

    #[test]
    fn jwt_expiry_reads_exp_claim() {
        let token = jwt_with_payload(serde_json::json!({ "sub": "1", "exp": 1_800_000_000 }));
        assert_eq!(jwt_expires_at(&token), Some(1_800_000_000));
    }

    #[test]
    fn jwt_expiry_accepts_milliseconds_and_ignores_invalid_tokens() {
        let millis = jwt_with_payload(serde_json::json!({ "exp": 1_800_000_000_000i64 }));
        assert_eq!(jwt_expires_at(&millis), Some(1_800_000_000));
        assert_eq!(jwt_expires_at("not-a-jwt"), None);
        assert_eq!(jwt_expires_at("header.@@@.sig"), None);
    }

    #[test]
    fn expiry_within_refresh_window_is_refreshed_early() {
        let now = now_ts();
        assert!(token_expiring_soon(Some(now + TOKEN_REFRESH_AHEAD_SECS - 1), TOKEN_REFRESH_AHEAD_SECS));
        assert!(token_expiring_soon(Some(now - 1), TOKEN_REFRESH_AHEAD_SECS));
        assert!(!token_expiring_soon(Some(now + TOKEN_REFRESH_AHEAD_SECS + 60), TOKEN_REFRESH_AHEAD_SECS));
        assert!(!token_expiring_soon(None, TOKEN_REFRESH_AHEAD_SECS));
    }

    #[test]
    fn expires_at_normalization_detects_milliseconds() {
        assert_eq!(normalize_expires_at(1_800_000_000), 1_800_000_000);
        assert_eq!(normalize_expires_at(1_800_000_000_000), 1_800_000_000);
    }
}
