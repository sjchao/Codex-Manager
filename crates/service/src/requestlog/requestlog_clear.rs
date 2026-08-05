use crate::storage_helpers::open_storage;
use std::path::Path;

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
