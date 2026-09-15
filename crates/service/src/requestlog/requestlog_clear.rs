use crate::storage_helpers::open_storage;
use codexmanager_core::storage::now_ts;
use std::path::Path;

const REQUEST_LOG_PRUNE_RETAIN_DAYS: i64 = 7;

/// 函数 `clear_request_logs`
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
pub(crate) fn clear_request_logs() -> Result<(), String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let image_results_jsons = storage
        .list_request_log_image_results_jsons()
        .map_err(|err| format!("list request-log image results failed: {err}"))?;
    let db_path = std::env::var("CODEXMANAGER_DB_PATH")
        .map_err(|_| "CODEXMANAGER_DB_PATH not set".to_string())?;
    crate::requestlog::image_assets::clear_image_results(Path::new(&db_path), image_results_jsons)?;
    storage.clear_request_logs().map_err(|e| e.to_string())
}

/// 函数 `clear_gateway_error_logs`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-04
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn clear_gateway_error_logs() -> Result<(), String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    storage
        .clear_gateway_error_logs()
        .map_err(|e| e.to_string())
}

/// 删除一周前的请求日志，同时清理这些日志落盘的图片资源；token 统计保留用于历史用量汇总。
pub(crate) fn prune_request_logs_older_than_week() -> Result<usize, String> {
    let cutoff_ts =
        now_ts().saturating_sub(REQUEST_LOG_PRUNE_RETAIN_DAYS.saturating_mul(24 * 60 * 60));
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let image_results_jsons = storage
        .list_request_log_image_results_jsons_before(cutoff_ts)
        .map_err(|err| format!("list request-log image results failed: {err}"))?;
    let db_path = std::env::var("CODEXMANAGER_DB_PATH")
        .map_err(|_| "CODEXMANAGER_DB_PATH not set".to_string())?;
    crate::requestlog::image_assets::clear_image_results(Path::new(&db_path), image_results_jsons)?;
    storage
        .delete_request_logs_before(cutoff_ts)
        .map_err(|e| e.to_string())
}
