use rusqlite::Result;

use super::Storage;

/// 空闲页超过该数量（默认页大小 4KiB 时约 4MiB）才考虑整体重写数据库文件。
const VACUUM_MIN_FREELIST_PAGES: i64 = 1024;
const VACUUM_FREELIST_RATIO_DIVISOR: i64 = 4;

impl Storage {
    /// 删除历史版本写入 sqlite 的模型输入/输出列（含数据），返回是否执行了删除。
    ///
    /// 中文注释：DROP COLUMN 会重写整表，历史大库可能耗时较久，只应在服务就绪后的维护任务里调用。
    pub fn drop_legacy_request_log_body_columns(&self) -> Result<bool> {
        let mut dropped = false;
        for column in ["request_body", "response_body"] {
            if !self.has_column("request_logs", column)? {
                continue;
            }
            self.conn.execute(
                &format!("ALTER TABLE request_logs DROP COLUMN {column}"),
                [],
            )?;
            dropped = true;
        }
        Ok(dropped)
    }

    /// 空闲页足够多时 VACUUM 收缩数据库文件，返回是否执行了 VACUUM。
    pub fn vacuum_if_fragmented(&self) -> Result<bool> {
        let freelist_pages: i64 = self
            .conn
            .query_row("PRAGMA freelist_count", [], |row| row.get(0))?;
        if freelist_pages < VACUUM_MIN_FREELIST_PAGES {
            return Ok(false);
        }
        let total_pages: i64 = self
            .conn
            .query_row("PRAGMA page_count", [], |row| row.get(0))?;
        if freelist_pages.saturating_mul(VACUUM_FREELIST_RATIO_DIVISOR) < total_pages {
            return Ok(false);
        }
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")?;
        Ok(true)
    }
}
