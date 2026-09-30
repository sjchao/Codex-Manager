use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;

use super::image_assets::is_safe_trace_id;
use super::storage_paths::{date_dir_name, is_date_dir_name};

const BODY_DIRECTORY_NAME: &str = "request-log-bodies";
const REQUEST_BODY_FILE_SUFFIX: &str = "-request.gz";
const RESPONSE_BODY_FILE_SUFFIX: &str = "-response.gz";

/// 把模型输入/输出文本 gzip 压缩后落盘到 `<db_dir>/request-log-bodies/<日期>/`，
/// 文件名为 `<trace_id>-request.gz` / `<trace_id>-response.gz`，返回是否写入了内容；
/// 同一 trace 重复写入时以最后一次为准。
pub(crate) fn write_request_log_bodies(
    db_path: &Path,
    trace_id: &str,
    created_at: i64,
    request_body: Option<&str>,
    response_body: Option<&str>,
) -> Result<bool, String> {
    if request_body.is_none() && response_body.is_none() {
        return Ok(false);
    }
    if !is_safe_trace_id(trace_id) {
        return Err("invalid request-log body trace id".to_string());
    }
    let date = date_dir_name(created_at)
        .ok_or_else(|| "invalid request-log body timestamp".to_string())?;
    let dir = ensure_body_dir(&body_root(db_path), &date)?;
    let request_file = request_body_file_name(trace_id);
    let response_file = response_body_file_name(trace_id);
    match request_body {
        Some(text) => write_body_file(&dir, &request_file, text)?,
        None => remove_body_file(&dir, &request_file),
    }
    match response_body {
        Some(text) => write_body_file(&dir, &response_file, text)?,
        None => remove_body_file(&dir, &response_file),
    }
    Ok(true)
}

/// 按 trace 读取输入/输出文本；日期目录由日志时间决定，读取时按日期倒序查找。
pub(crate) fn read_request_log_bodies(
    db_path: &Path,
    trace_id: &str,
) -> Result<(Option<String>, Option<String>), String> {
    if !is_safe_trace_id(trace_id) {
        return Err("invalid request-log body trace id".to_string());
    }
    let Some(dir) = find_latest_body_dir(&body_root(db_path), trace_id) else {
        return Ok((None, None));
    };
    let request_body = read_body_file(&dir.join(request_body_file_name(trace_id)))?;
    let response_body = read_body_file(&dir.join(response_body_file_name(trace_id)))?;
    Ok((request_body, response_body))
}

pub(crate) fn request_log_body_presence(
    db_path: &Path,
    trace_id: &str,
    created_at: i64,
) -> (bool, bool) {
    if !is_safe_trace_id(trace_id) {
        return (false, false);
    }
    let Some(date) = date_dir_name(created_at) else {
        return (false, false);
    };
    let dir = body_root(db_path).join(date);
    (
        dir.join(request_body_file_name(trace_id)).is_file(),
        dir.join(response_body_file_name(trace_id)).is_file(),
    )
}

/// 删除指定 trace 的输入/输出文件（用于日志被清理的场景）。
pub(crate) fn clear_request_log_bodies(
    db_path: &Path,
    traces: impl IntoIterator<Item = (String, i64)>,
) -> Result<(), String> {
    let root = body_root(db_path);
    for (trace_id, created_at) in traces {
        if !is_safe_trace_id(&trace_id) {
            continue;
        }
        let Some(date) = date_dir_name(created_at) else {
            continue;
        };
        let dir = root.join(date);
        for file_name in [
            request_body_file_name(&trace_id),
            response_body_file_name(&trace_id),
        ] {
            match fs::remove_file(dir.join(file_name)) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(format!("delete request-log body file failed: {err}")),
            }
        }
        // 当天文件清理完后顺带删除空目录，避免残留大量空日期目录。
        let _ = fs::remove_dir(&dir);
    }
    Ok(())
}

pub(crate) fn clear_all_request_log_bodies(db_path: &Path) -> Result<(), String> {
    match fs::remove_dir_all(body_root(db_path)) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("delete request-log body directory failed: {err}")),
    }
}

fn body_root(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(BODY_DIRECTORY_NAME)
}

fn request_body_file_name(trace_id: &str) -> String {
    format!("{trace_id}{REQUEST_BODY_FILE_SUFFIX}")
}

fn response_body_file_name(trace_id: &str) -> String {
    format!("{trace_id}{RESPONSE_BODY_FILE_SUFFIX}")
}

fn find_latest_body_dir(root: &Path, trace_id: &str) -> Option<PathBuf> {
    let request_file = request_body_file_name(trace_id);
    let response_file = response_body_file_name(trace_id);
    let mut candidates = fs::read_dir(root)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !is_date_dir_name(&name) {
                return None;
            }
            let dir = entry.path();
            let has_body = dir.join(&request_file).is_file() || dir.join(&response_file).is_file();
            has_body.then_some((name, dir))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| right.0.cmp(&left.0));
    candidates.into_iter().next().map(|(_, dir)| dir)
}

fn ensure_body_dir(root: &Path, date: &str) -> Result<PathBuf, String> {
    fs::create_dir_all(root)
        .map_err(|err| format!("create request-log body directory failed: {err}"))?;
    let root = fs::canonicalize(root)
        .map_err(|err| format!("resolve request-log body directory failed: {err}"))?;
    let dir = root.join(date);
    fs::create_dir_all(&dir)
        .map_err(|err| format!("create request-log body date directory failed: {err}"))?;
    let dir = fs::canonicalize(&dir)
        .map_err(|err| format!("resolve request-log body date directory failed: {err}"))?;
    if !dir.starts_with(&root) {
        return Err("request-log body date directory escapes storage root".to_string());
    }
    Ok(dir)
}

fn write_body_file(dir: &Path, file_name: &str, text: &str) -> Result<(), String> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(text.as_bytes())
        .map_err(|err| format!("compress request-log body failed: {err}"))?;
    let compressed = encoder
        .finish()
        .map_err(|err| format!("compress request-log body failed: {err}"))?;

    let final_path = dir.join(file_name);
    let temporary_path = dir.join(format!(".{file_name}.tmp"));
    let result = fs::File::create(&temporary_path)
        .and_then(|mut file| file.write_all(&compressed).and_then(|()| file.flush()))
        .and_then(|()| fs::rename(&temporary_path, &final_path));
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result.map_err(|err| format!("write request-log body file failed: {err}"))
}

fn remove_body_file(dir: &Path, file_name: &str) {
    match fs::remove_file(dir.join(file_name)) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => log::warn!(
            "event=request_log_body_file_remove_failed file={} err={}",
            file_name,
            err
        ),
    }
}

fn read_body_file(path: &Path) -> Result<Option<String>, String> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(format!("read request-log body file failed: {err}")),
    };
    let mut text = String::new();
    GzDecoder::new(file)
        .read_to_string(&mut text)
        .map_err(|err| format!("read request-log body file failed: {err}"))?;
    Ok(Some(text))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        clear_all_request_log_bodies, clear_request_log_bodies, read_request_log_bodies,
        request_log_body_presence, write_request_log_bodies,
    };
    use crate::requestlog::storage_paths::date_dir_name;

    fn test_db_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir()
            .join(format!(
                "codexmanager-request-log-body-{name}-{}-{unique}",
                std::process::id()
            ))
            .join("data")
            .join("codexmanager.db")
    }

    #[test]
    fn writes_and_reads_bodies_below_the_database_data_directory() {
        let db_path = test_db_path("roundtrip");
        fs::create_dir_all(db_path.parent().expect("database parent"))
            .expect("create data directory");
        let created_at = codexmanager_core::storage::now_ts();
        let date = date_dir_name(created_at).expect("date directory");

        let written = write_request_log_bodies(
            &db_path,
            "trc-body",
            created_at,
            Some(r#"{"input":"hi"}"#),
            Some("你好"),
        )
        .expect("write bodies");
        assert!(written);
        let root = db_path
            .parent()
            .expect("database parent")
            .join("request-log-bodies");
        let dir = root.join(&date);
        assert!(dir.join("trc-body-request.gz").is_file());
        assert!(dir.join("trc-body-response.gz").is_file());
        let compressed = fs::read(dir.join("trc-body-request.gz")).expect("read compressed body");
        assert!(compressed.starts_with(&[0x1f, 0x8b]));
        assert_eq!(
            request_log_body_presence(&db_path, "trc-body", created_at),
            (true, true)
        );

        let (request_body, response_body) =
            read_request_log_bodies(&db_path, "trc-body").expect("read bodies");
        assert_eq!(request_body.as_deref(), Some(r#"{"input":"hi"}"#));
        assert_eq!(response_body.as_deref(), Some("你好"));

        let _ = fs::remove_dir_all(
            db_path
                .parent()
                .expect("database parent")
                .parent()
                .expect("test root"),
        );
    }

    #[test]
    fn reads_bodies_from_older_date_directories() {
        let db_path = test_db_path("older-date");
        let created_at = codexmanager_core::storage::now_ts() - 2 * 24 * 60 * 60;

        write_request_log_bodies(&db_path, "trc-old", created_at, Some("old"), None)
            .expect("write bodies");

        let (request_body, response_body) =
            read_request_log_bodies(&db_path, "trc-old").expect("read bodies");
        assert_eq!(request_body.as_deref(), Some("old"));
        assert!(response_body.is_none());

        let _ = fs::remove_dir_all(
            db_path
                .parent()
                .expect("database parent")
                .parent()
                .expect("test root"),
        );
    }

    #[test]
    fn repeated_writes_replace_previous_body_files() {
        let db_path = test_db_path("overwrite");
        let created_at = codexmanager_core::storage::now_ts();
        write_request_log_bodies(
            &db_path,
            "trc-overwrite",
            created_at,
            Some("first"),
            Some("first"),
        )
        .expect("write first bodies");
        write_request_log_bodies(&db_path, "trc-overwrite", created_at, Some("second"), None)
            .expect("write second bodies");

        let (request_body, response_body) =
            read_request_log_bodies(&db_path, "trc-overwrite").expect("read bodies");
        assert_eq!(request_body.as_deref(), Some("second"));
        assert!(response_body.is_none());
        assert_eq!(
            request_log_body_presence(&db_path, "trc-overwrite", created_at),
            (true, false)
        );

        let _ = fs::remove_dir_all(
            db_path
                .parent()
                .expect("database parent")
                .parent()
                .expect("test root"),
        );
    }

    #[test]
    fn skips_blank_bodies_and_unsafe_trace_ids() {
        let db_path = test_db_path("skip");
        let created_at = codexmanager_core::storage::now_ts();
        assert!(
            !write_request_log_bodies(&db_path, "trc-skip", created_at, None, None)
                .expect("skip empty bodies")
        );
        assert!(
            write_request_log_bodies(&db_path, "../escape", created_at, Some("x"), None).is_err()
        );
        assert!(write_request_log_bodies(&db_path, "", created_at, Some("x"), None).is_err());
        assert!(read_request_log_bodies(&db_path, "../escape").is_err());
        assert_eq!(
            request_log_body_presence(&db_path, "../escape", created_at),
            (false, false)
        );

        let (request_body, response_body) =
            read_request_log_bodies(&db_path, "trc-unknown").expect("read unknown bodies");
        assert!(request_body.is_none());
        assert!(response_body.is_none());
    }

    #[test]
    fn clears_bodies_by_trace_and_whole_directory() {
        let db_path = test_db_path("clear");
        let created_at = codexmanager_core::storage::now_ts();
        let date = date_dir_name(created_at).expect("date directory");
        write_request_log_bodies(&db_path, "trc-clear", created_at, Some("x"), Some("y"))
            .expect("write bodies");
        write_request_log_bodies(&db_path, "trc-keep", created_at, Some("x"), Some("y"))
            .expect("write bodies");
        let root = db_path
            .parent()
            .expect("database parent")
            .join("request-log-bodies");

        clear_request_log_bodies(&db_path, [("trc-clear".to_string(), created_at)])
            .expect("clear trace bodies");
        assert!(!root.join(&date).join("trc-clear-request.gz").exists());
        assert!(!root.join(&date).join("trc-clear-response.gz").exists());
        assert!(root.join(&date).join("trc-keep-request.gz").is_file());

        clear_all_request_log_bodies(&db_path).expect("clear all bodies");
        assert!(!root.exists());

        let _ = fs::remove_dir_all(
            db_path
                .parent()
                .expect("database parent")
                .parent()
                .expect("test root"),
        );
    }
}
