use crate::gateway;
use crate::usage_refresh;
use serde::Deserialize;
use std::sync::{Mutex, OnceLock};

use super::{
    get_persisted_app_setting, normalize_optional_text, save_persisted_app_setting,
    save_persisted_bool_setting, APP_SETTING_GATEWAY_ACCOUNT_MAX_INFLIGHT_KEY,
    APP_SETTING_GATEWAY_AGGREGATE_API_TEST_MODEL_KEY, APP_SETTING_GATEWAY_BACKGROUND_TASKS_KEY,
    APP_SETTING_GATEWAY_FREE_ACCOUNT_MAX_MODEL_KEY, APP_SETTING_GATEWAY_IMAGE_MODELS_KEY,
    APP_SETTING_GATEWAY_MODEL_FORWARD_RULES_KEY,
    APP_SETTING_GATEWAY_ORIGINATOR_KEY, APP_SETTING_GATEWAY_REQUEST_COMPRESSION_ENABLED_KEY,
    APP_SETTING_GATEWAY_RESIDENCY_REQUIREMENT_KEY, APP_SETTING_GATEWAY_ROUTE_STRATEGY_KEY,
    APP_SETTING_GATEWAY_SSE_KEEPALIVE_INTERVAL_MS_KEY, APP_SETTING_GATEWAY_UPSTREAM_PROXY_URL_KEY,
    APP_SETTING_GATEWAY_UPSTREAM_STREAM_TIMEOUT_MS_KEY, APP_SETTING_GATEWAY_USER_AGENT_VERSION_KEY,
    APP_SETTING_GATEWAY_VIDEO_MODELS_KEY,
};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundTasksInput {
    pub usage_polling_enabled: Option<bool>,
    pub usage_poll_interval_secs: Option<u64>,
    pub gateway_keepalive_enabled: Option<bool>,
    pub gateway_keepalive_interval_secs: Option<u64>,
    pub token_refresh_polling_enabled: Option<bool>,
    pub token_refresh_poll_interval_secs: Option<u64>,
    pub usage_refresh_workers: Option<usize>,
    pub http_worker_factor: Option<usize>,
    pub http_worker_min: Option<usize>,
    pub http_stream_worker_factor: Option<usize>,
    pub http_stream_worker_min: Option<usize>,
}

const DEFAULT_GATEWAY_AGGREGATE_API_TEST_MODEL: &str = "gpt-5.6-terra";

/// 函数 `normalize_gateway_aggregate_api_test_model`
///
/// 作者: gaohongshun
///
/// 时间: 2026-07-10
///
/// # 参数
/// - raw: 参数 raw
///
/// # 返回
/// 返回函数执行结果
fn normalize_gateway_aggregate_api_test_model(raw: Option<&str>) -> String {
    normalize_optional_text(raw)
        .unwrap_or_else(|| DEFAULT_GATEWAY_AGGREGATE_API_TEST_MODEL.to_string())
}

fn normalize_gateway_model_list(raw: &str) -> String {
    let mut models = Vec::new();
    for model in raw.lines().map(str::trim).filter(|value| !value.is_empty()) {
        if !models
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(model))
        {
            models.push(model.to_string());
        }
    }
    models.join("\n")
}

fn model_list_values(raw: &str) -> Vec<String> {
    normalize_gateway_model_list(raw)
        .lines()
        .map(ToString::to_string)
        .collect()
}

fn has_overlapping_model(image_models: &[String], video_models: &[String]) -> bool {
    image_models.iter().any(|image_model| {
        video_models
            .iter()
            .any(|video_model| image_model.eq_ignore_ascii_case(video_model))
    })
}

impl BackgroundTasksInput {
    /// 函数 `into_patch`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - crate: 参数 crate
    ///
    /// # 返回
    /// 返回函数执行结果
    pub(crate) fn into_patch(self) -> usage_refresh::BackgroundTasksSettingsPatch {
        usage_refresh::BackgroundTasksSettingsPatch {
            usage_polling_enabled: self.usage_polling_enabled,
            usage_poll_interval_secs: self.usage_poll_interval_secs,
            gateway_keepalive_enabled: self.gateway_keepalive_enabled,
            gateway_keepalive_interval_secs: self.gateway_keepalive_interval_secs,
            token_refresh_polling_enabled: self.token_refresh_polling_enabled,
            token_refresh_poll_interval_secs: self.token_refresh_poll_interval_secs,
            usage_refresh_workers: self.usage_refresh_workers,
            http_worker_factor: self.http_worker_factor,
            http_worker_min: self.http_worker_min,
            http_stream_worker_factor: self.http_stream_worker_factor,
            http_stream_worker_min: self.http_stream_worker_min,
        }
    }
}

/// 函数 `set_gateway_route_strategy`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - strategy: 参数 strategy
///
/// # 返回
/// 返回函数执行结果
pub fn set_gateway_route_strategy(strategy: &str) -> Result<String, String> {
    let applied = gateway::set_route_strategy(strategy)?.to_string();
    save_persisted_app_setting(APP_SETTING_GATEWAY_ROUTE_STRATEGY_KEY, Some(&applied))?;
    Ok(applied)
}

/// 函数 `set_gateway_free_account_max_model`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - model: 参数 model
///
/// # 返回
/// 返回函数执行结果
pub fn set_gateway_free_account_max_model(model: &str) -> Result<String, String> {
    let applied = gateway::set_free_account_max_model(model)?;
    save_persisted_app_setting(
        APP_SETTING_GATEWAY_FREE_ACCOUNT_MAX_MODEL_KEY,
        Some(&applied),
    )?;
    Ok(applied)
}

/// 函数 `current_gateway_free_account_max_model`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
pub fn current_gateway_free_account_max_model() -> String {
    gateway::current_free_account_max_model()
}

/// 函数 `set_gateway_aggregate_api_test_model`
///
/// 作者: gaohongshun
///
/// 时间: 2026-07-10
///
/// # 参数
/// - model: 参数 model
///
/// # 返回
/// 返回函数执行结果
pub fn set_gateway_aggregate_api_test_model(model: &str) -> Result<String, String> {
    let applied = normalize_gateway_aggregate_api_test_model(Some(model));
    save_persisted_app_setting(
        APP_SETTING_GATEWAY_AGGREGATE_API_TEST_MODEL_KEY,
        Some(&applied),
    )?;
    Ok(applied)
}

/// 函数 `current_gateway_aggregate_api_test_model`
///
/// 作者: gaohongshun
///
/// 时间: 2026-07-10
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
pub fn current_gateway_aggregate_api_test_model() -> String {
    normalize_gateway_aggregate_api_test_model(
        get_persisted_app_setting(APP_SETTING_GATEWAY_AGGREGATE_API_TEST_MODEL_KEY).as_deref(),
    )
}

pub fn current_gateway_image_models() -> String {
    normalize_gateway_model_list(
        get_persisted_app_setting(APP_SETTING_GATEWAY_IMAGE_MODELS_KEY)
            .as_deref()
            .unwrap_or_default(),
    )
}

pub fn current_gateway_video_models() -> String {
    normalize_gateway_model_list(
        get_persisted_app_setting(APP_SETTING_GATEWAY_VIDEO_MODELS_KEY)
            .as_deref()
            .unwrap_or_default(),
    )
}

pub fn current_gateway_image_model_list() -> Vec<String> {
    model_list_values(&current_gateway_image_models())
}

pub fn current_gateway_video_model_list() -> Vec<String> {
    model_list_values(&current_gateway_video_models())
}

fn gateway_model_lists_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub fn set_gateway_model_lists(
    image_models: &str,
    video_models: &str,
) -> Result<(String, String), String> {
    let _guard = gateway_model_lists_lock()
        .lock()
        .map_err(|_| "gateway model list lock poisoned".to_string())?;
    set_gateway_model_lists_unlocked(image_models, video_models)
}

pub(super) fn apply_gateway_model_lists_patch(
    image_models: Option<&str>,
    video_models: Option<&str>,
) -> Result<(String, String), String> {
    let _guard = gateway_model_lists_lock()
        .lock()
        .map_err(|_| "gateway model list lock poisoned".to_string())?;
    let image_models = image_models
        .map(str::to_string)
        .unwrap_or_else(current_gateway_image_models);
    let video_models = video_models
        .map(str::to_string)
        .unwrap_or_else(current_gateway_video_models);
    set_gateway_model_lists_unlocked(&image_models, &video_models)
}

fn set_gateway_model_lists_unlocked(
    image_models: &str,
    video_models: &str,
) -> Result<(String, String), String> {
    let image_models = normalize_gateway_model_list(image_models);
    let video_models = normalize_gateway_model_list(video_models);
    if has_overlapping_model(
        &model_list_values(&image_models),
        &model_list_values(&video_models),
    ) {
        return Err("生图模型和视频模型不能包含同一个模型".to_string());
    }
    save_persisted_app_setting(
        APP_SETTING_GATEWAY_IMAGE_MODELS_KEY,
        (!image_models.is_empty()).then_some(image_models.as_str()),
    )?;
    save_persisted_app_setting(
        APP_SETTING_GATEWAY_VIDEO_MODELS_KEY,
        (!video_models.is_empty()).then_some(video_models.as_str()),
    )?;
    Ok((image_models, video_models))
}

#[cfg(test)]
mod tests {
    use super::{apply_gateway_model_lists_patch, current_gateway_image_models, current_gateway_video_models, set_gateway_model_lists};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_db_path() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("codexmanager-model-list-patch-test-{unique}.db"))
    }

    #[test]
    fn partial_model_list_update_preserves_the_other_list() {
        let _guard = crate::test_env_guard();
        let db_path = unique_temp_db_path();
        let previous_db_path = std::env::var("CODEXMANAGER_DB_PATH").ok();
        std::env::set_var("CODEXMANAGER_DB_PATH", &db_path);

        set_gateway_model_lists("old-image", "sora-2").expect("seed model lists");
        apply_gateway_model_lists_patch(Some("gpt-image2"), None)
            .expect("update image model list");

        assert_eq!(current_gateway_image_models(), "gpt-image2");
        assert_eq!(current_gateway_video_models(), "sora-2");

        if let Some(value) = previous_db_path {
            std::env::set_var("CODEXMANAGER_DB_PATH", value);
        } else {
            std::env::remove_var("CODEXMANAGER_DB_PATH");
        }
        let _ = std::fs::remove_file(&db_path);
    }
}

/// 函数 `set_gateway_model_forward_rules`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-05
///
/// # 参数
/// - raw: 参数 raw
///
/// # 返回
/// 返回函数执行结果
pub fn set_gateway_model_forward_rules(raw: &str) -> Result<String, String> {
    let applied = gateway::set_model_forward_rules(raw)?;
    save_persisted_app_setting(
        APP_SETTING_GATEWAY_MODEL_FORWARD_RULES_KEY,
        if applied.trim().is_empty() {
            None
        } else {
            Some(applied.as_str())
        },
    )?;
    Ok(applied)
}

/// 函数 `current_gateway_model_forward_rules`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-05
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
pub fn current_gateway_model_forward_rules() -> String {
    gateway::current_model_forward_rules()
}

/// 函数 `set_gateway_account_max_inflight`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - limit: 参数 limit
///
/// # 返回
/// 返回函数执行结果
pub fn set_gateway_account_max_inflight(limit: usize) -> Result<usize, String> {
    let applied = gateway::set_account_max_inflight_limit(limit);
    save_persisted_app_setting(
        APP_SETTING_GATEWAY_ACCOUNT_MAX_INFLIGHT_KEY,
        Some(&applied.to_string()),
    )?;
    Ok(applied)
}

/// 函数 `current_gateway_account_max_inflight`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
pub fn current_gateway_account_max_inflight() -> usize {
    gateway::account_max_inflight_limit()
}

/// 函数 `set_gateway_request_compression_enabled`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - enabled: 参数 enabled
///
/// # 返回
/// 返回函数执行结果
pub fn set_gateway_request_compression_enabled(enabled: bool) -> Result<bool, String> {
    let applied = gateway::set_request_compression_enabled(enabled);
    save_persisted_bool_setting(APP_SETTING_GATEWAY_REQUEST_COMPRESSION_ENABLED_KEY, applied)?;
    Ok(applied)
}

/// 函数 `current_gateway_request_compression_enabled`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
pub fn current_gateway_request_compression_enabled() -> bool {
    gateway::request_compression_enabled()
}

/// 函数 `set_gateway_originator`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - originator: 参数 originator
///
/// # 返回
/// 返回函数执行结果
pub fn set_gateway_originator(originator: &str) -> Result<String, String> {
    let applied = gateway::set_originator(originator)?;
    save_persisted_app_setting(APP_SETTING_GATEWAY_ORIGINATOR_KEY, Some(&applied))?;
    Ok(applied)
}

/// 函数 `current_gateway_originator`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
pub fn current_gateway_originator() -> String {
    gateway::current_originator()
}

/// 函数 `set_gateway_user_agent_version`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - version: 参数 version
///
/// # 返回
/// 返回函数执行结果
pub fn set_gateway_user_agent_version(version: &str) -> Result<String, String> {
    let applied = gateway::set_codex_user_agent_version(version)?;
    save_persisted_app_setting(APP_SETTING_GATEWAY_USER_AGENT_VERSION_KEY, Some(&applied))?;
    Ok(applied)
}

/// 函数 `current_gateway_user_agent_version`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
pub fn current_gateway_user_agent_version() -> String {
    gateway::current_codex_user_agent_version()
}

/// 函数 `set_gateway_residency_requirement`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - value: 参数 value
///
/// # 返回
/// 返回函数执行结果
pub fn set_gateway_residency_requirement(value: Option<&str>) -> Result<Option<String>, String> {
    let normalized = normalize_optional_text(value);
    let applied = gateway::set_residency_requirement(normalized.as_deref())?;
    save_persisted_app_setting(
        APP_SETTING_GATEWAY_RESIDENCY_REQUIREMENT_KEY,
        applied.as_deref(),
    )?;
    Ok(applied)
}

/// 函数 `current_gateway_residency_requirement`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
pub fn current_gateway_residency_requirement() -> Option<String> {
    gateway::current_residency_requirement()
}

/// 函数 `residency_requirement_options`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
pub fn residency_requirement_options() -> &'static [&'static str] {
    &["", "us"]
}

/// 函数 `set_gateway_upstream_proxy_url`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - proxy_url: 参数 proxy_url
///
/// # 返回
/// 返回函数执行结果
pub fn set_gateway_upstream_proxy_url(proxy_url: Option<&str>) -> Result<Option<String>, String> {
    let normalized = normalize_optional_text(proxy_url);
    let applied = gateway::set_upstream_proxy_url(normalized.as_deref())?;
    save_persisted_app_setting(
        APP_SETTING_GATEWAY_UPSTREAM_PROXY_URL_KEY,
        applied.as_deref(),
    )?;
    Ok(applied)
}

/// 函数 `set_gateway_upstream_stream_timeout_ms`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - timeout_ms: 参数 timeout_ms
///
/// # 返回
/// 返回函数执行结果
pub fn set_gateway_upstream_stream_timeout_ms(timeout_ms: u64) -> Result<u64, String> {
    let applied = gateway::set_upstream_stream_timeout_ms(timeout_ms);
    save_persisted_app_setting(
        APP_SETTING_GATEWAY_UPSTREAM_STREAM_TIMEOUT_MS_KEY,
        Some(&applied.to_string()),
    )?;
    Ok(applied)
}

/// 函数 `current_gateway_upstream_stream_timeout_ms`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
pub fn current_gateway_upstream_stream_timeout_ms() -> u64 {
    gateway::current_upstream_stream_timeout_ms()
}

/// 函数 `set_gateway_sse_keepalive_interval_ms`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - interval_ms: 参数 interval_ms
///
/// # 返回
/// 返回函数执行结果
pub fn set_gateway_sse_keepalive_interval_ms(interval_ms: u64) -> Result<u64, String> {
    let applied = gateway::set_sse_keepalive_interval_ms(interval_ms)?;
    save_persisted_app_setting(
        APP_SETTING_GATEWAY_SSE_KEEPALIVE_INTERVAL_MS_KEY,
        Some(&applied.to_string()),
    )?;
    Ok(applied)
}

/// 函数 `current_gateway_sse_keepalive_interval_ms`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
pub fn current_gateway_sse_keepalive_interval_ms() -> u64 {
    gateway::current_sse_keepalive_interval_ms()
}

/// 函数 `set_gateway_background_tasks`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - input: 参数 input
///
/// # 返回
/// 返回函数执行结果
pub fn set_gateway_background_tasks(
    input: BackgroundTasksInput,
) -> Result<serde_json::Value, String> {
    let applied = usage_refresh::set_background_tasks_settings(input.into_patch());
    let raw = serde_json::to_string(&applied)
        .map_err(|err| format!("serialize background tasks failed: {err}"))?;
    save_persisted_app_setting(APP_SETTING_GATEWAY_BACKGROUND_TASKS_KEY, Some(&raw))?;
    serde_json::to_value(applied).map_err(|err| err.to_string())
}

/// 函数 `current_background_tasks_snapshot_value`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn current_background_tasks_snapshot_value() -> Result<serde_json::Value, String> {
    serde_json::to_value(usage_refresh::background_tasks_settings()).map_err(|err| err.to_string())
}
