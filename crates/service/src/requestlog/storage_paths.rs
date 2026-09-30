use chrono::{DateTime, Local};

/// 请求日志落盘资源的日期目录名（本地时区，形如 20260930）。
pub(crate) fn date_dir_name(ts: i64) -> Option<String> {
    let utc = DateTime::from_timestamp(ts, 0)?;
    Some(utc.with_timezone(&Local).format("%Y%m%d").to_string())
}

pub(crate) fn is_date_dir_name(name: &str) -> bool {
    name.len() == 8 && name.bytes().all(|byte| byte.is_ascii_digit())
}
