use std::sync::Once;
use std::thread;
use std::time::Duration;

const MAINTENANCE_START_DELAY: Duration = Duration::from_secs(5);

static STORAGE_MAINTENANCE_ONCE: Once = Once::new();

/// 服务就绪后异步执行一次 sqlite 维护：删除历史版本写入的模型输入/输出大列，
/// 并在空闲页足够多时 VACUUM 收缩文件。放在后台线程执行以免阻塞启动与请求处理。
pub(crate) fn ensure_storage_maintenance() {
    STORAGE_MAINTENANCE_ONCE.call_once(|| {
        let spawned = thread::Builder::new()
            .name("request-log-storage-maintenance".to_string())
            .spawn(|| {
                // 中文注释：稍作延后，避免与启动阶段刚结束的初始化迁移抢数据库写锁。
                thread::sleep(MAINTENANCE_START_DELAY);
                run_storage_maintenance();
            });
        if let Err(err) = spawned {
            log::warn!("event=request_log_storage_maintenance_spawn_failed err={err}");
        }
    });
}

fn run_storage_maintenance() {
    let Some(storage) = crate::storage_helpers::open_storage() else {
        log::warn!("event=request_log_storage_maintenance_skipped reason=storage_unavailable");
        return;
    };
    match storage.drop_legacy_request_log_body_columns() {
        Ok(true) => log::info!("event=request_log_body_columns_dropped table=request_logs"),
        Ok(false) => {}
        Err(err) => log::warn!("event=request_log_body_columns_drop_failed err={err}"),
    }
    match storage.vacuum_if_fragmented() {
        Ok(true) => log::info!("event=storage_vacuum_completed"),
        Ok(false) => {}
        Err(err) => log::warn!("event=storage_vacuum_failed err={err}"),
    }
}
