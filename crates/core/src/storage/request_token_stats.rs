use rusqlite::{params, Connection, Result};

use super::{now_ts, ApiKeyTokenUsageSummary, RequestLogTodaySummary, RequestTokenStat, Storage};

const DEFAULT_REQUEST_TOKEN_STATS_RETAIN_DAYS: i64 = 90;
const REQUEST_TOKEN_STATS_RETAIN_DAYS_ENV: &str = "CODEXMANAGER_REQUEST_TOKEN_STATS_RETAIN_DAYS";
const DEFAULT_REQUEST_TOKEN_STATS_MAINTENANCE_INTERVAL_SECONDS: i64 = 60 * 60;
const REQUEST_TOKEN_STATS_MAINTENANCE_INTERVAL_SECONDS_ENV: &str =
    "CODEXMANAGER_REQUEST_TOKEN_STATS_MAINTENANCE_INTERVAL_SECONDS";
const DEFAULT_REQUEST_TOKEN_STATS_VACUUM_INTERVAL_SECONDS: i64 = 7 * 24 * 60 * 60;
const REQUEST_TOKEN_STATS_VACUUM_INTERVAL_SECONDS_ENV: &str =
    "CODEXMANAGER_REQUEST_TOKEN_STATS_VACUUM_INTERVAL_SECONDS";
const DEFAULT_REQUEST_TOKEN_STATS_VACUUM_MIN_FREELIST_PAGES: i64 = 256;
const REQUEST_TOKEN_STATS_VACUUM_MIN_FREELIST_PAGES_ENV: &str =
    "CODEXMANAGER_REQUEST_TOKEN_STATS_VACUUM_MIN_FREELIST_PAGES";
const APP_SETTING_REQUEST_TOKEN_STATS_LAST_MAINTAINED_AT: &str =
    "storage.request_token_stats.last_maintained_at";
const APP_SETTING_REQUEST_TOKEN_STATS_LAST_VACUUM_AT: &str =
    "storage.request_token_stats.last_vacuum_at";

fn env_i64(name: &str, default: i64) -> i64 {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<i64>().ok())
        .unwrap_or(default)
}

fn request_token_stats_retain_days() -> i64 {
    env_i64(
        REQUEST_TOKEN_STATS_RETAIN_DAYS_ENV,
        DEFAULT_REQUEST_TOKEN_STATS_RETAIN_DAYS,
    )
    .max(0)
}

fn request_token_stats_maintenance_interval_seconds() -> i64 {
    env_i64(
        REQUEST_TOKEN_STATS_MAINTENANCE_INTERVAL_SECONDS_ENV,
        DEFAULT_REQUEST_TOKEN_STATS_MAINTENANCE_INTERVAL_SECONDS,
    )
    .max(0)
}

fn request_token_stats_vacuum_interval_seconds() -> i64 {
    env_i64(
        REQUEST_TOKEN_STATS_VACUUM_INTERVAL_SECONDS_ENV,
        DEFAULT_REQUEST_TOKEN_STATS_VACUUM_INTERVAL_SECONDS,
    )
    .max(0)
}

fn request_token_stats_vacuum_min_freelist_pages() -> i64 {
    env_i64(
        REQUEST_TOKEN_STATS_VACUUM_MIN_FREELIST_PAGES_ENV,
        DEFAULT_REQUEST_TOKEN_STATS_VACUUM_MIN_FREELIST_PAGES,
    )
    .max(0)
}

fn normalized_token_value(value: Option<i64>) -> i64 {
    value.unwrap_or(0).max(0)
}

fn normalized_cost_value(value: Option<f64>) -> f64 {
    value.unwrap_or(0.0).max(0.0)
}

fn effective_total_tokens(stat: &RequestTokenStat) -> i64 {
    let total_tokens = normalized_token_value(stat.total_tokens);
    if total_tokens > 0 {
        return total_tokens;
    }
    let net_tokens = normalized_token_value(stat.input_tokens)
        - normalized_token_value(stat.cached_input_tokens)
        + normalized_token_value(stat.output_tokens);
    net_tokens.max(0)
}

fn normalized_key_id(stat: &RequestTokenStat) -> Option<&str> {
    stat.key_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(super) fn upsert_request_token_daily_stats(
    conn: &Connection,
    stat: &RequestTokenStat,
) -> Result<()> {
    let Some(key_id) = normalized_key_id(stat) else {
        return Ok(());
    };
    conn.execute(
        "INSERT INTO request_token_daily_stats (
            day_key,
            key_id,
            request_count,
            input_tokens,
            cached_input_tokens,
            output_tokens,
            total_tokens,
            reasoning_output_tokens,
            estimated_cost_usd
         ) VALUES (
            date(?1, 'unixepoch', 'localtime'),
            ?2,
            1,
            ?3,
            ?4,
            ?5,
            ?6,
            ?7,
            ?8
         )
         ON CONFLICT(day_key, key_id) DO UPDATE SET
            request_count = request_token_daily_stats.request_count + excluded.request_count,
            input_tokens = request_token_daily_stats.input_tokens + excluded.input_tokens,
            cached_input_tokens =
                request_token_daily_stats.cached_input_tokens + excluded.cached_input_tokens,
            output_tokens = request_token_daily_stats.output_tokens + excluded.output_tokens,
            total_tokens = request_token_daily_stats.total_tokens + excluded.total_tokens,
            reasoning_output_tokens =
                request_token_daily_stats.reasoning_output_tokens
                + excluded.reasoning_output_tokens,
            estimated_cost_usd =
                request_token_daily_stats.estimated_cost_usd + excluded.estimated_cost_usd",
        params![
            stat.created_at,
            key_id,
            normalized_token_value(stat.input_tokens),
            normalized_token_value(stat.cached_input_tokens),
            normalized_token_value(stat.output_tokens),
            effective_total_tokens(stat),
            normalized_token_value(stat.reasoning_output_tokens),
            normalized_cost_value(stat.estimated_cost_usd),
        ],
    )?;
    Ok(())
}

fn parse_setting_i64(value: Option<String>) -> Option<i64> {
    value.and_then(|raw| raw.trim().parse::<i64>().ok())
}

impl Storage {
    /// 函数 `insert_request_token_stat`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - stat: 参数 stat
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn insert_request_token_stat(&self, stat: &RequestTokenStat) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO request_token_stats (
                request_log_id, key_id, account_id, model,
                input_tokens, cached_input_tokens, output_tokens, total_tokens, reasoning_output_tokens,
                estimated_cost_usd, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            (
                stat.request_log_id,
                &stat.key_id,
                &stat.account_id,
                &stat.model,
                stat.input_tokens,
                stat.cached_input_tokens,
                stat.output_tokens,
                stat.total_tokens,
                stat.reasoning_output_tokens,
                stat.estimated_cost_usd,
                stat.created_at,
            ),
        )?;
        upsert_request_token_daily_stats(&tx, stat)?;
        tx.commit()?;
        let _ = self.maintain_request_token_stats_if_due();
        Ok(())
    }

    /// 函数 `summarize_request_token_stats_between`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - start_ts: 参数 start_ts
    /// - end_ts: 参数 end_ts
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn summarize_request_token_stats_between(
        &self,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<RequestLogTodaySummary> {
        let mut stmt = self.conn.prepare(
            "SELECT
                IFNULL(SUM(input_tokens), 0),
                IFNULL(SUM(cached_input_tokens), 0),
                IFNULL(SUM(output_tokens), 0),
                IFNULL(SUM(reasoning_output_tokens), 0),
                IFNULL(SUM(estimated_cost_usd), 0.0)
             FROM request_token_stats
             WHERE created_at >= ?1 AND created_at < ?2",
        )?;
        let mut rows = stmt.query((start_ts, end_ts))?;
        if let Some(row) = rows.next()? {
            return Ok(RequestLogTodaySummary {
                input_tokens: row.get(0)?,
                cached_input_tokens: row.get(1)?,
                output_tokens: row.get(2)?,
                reasoning_output_tokens: row.get(3)?,
                estimated_cost_usd: row.get(4)?,
            });
        }
        Ok(RequestLogTodaySummary {
            input_tokens: 0,
            cached_input_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            estimated_cost_usd: 0.0,
        })
    }

    /// 函数 `summarize_request_token_stats_by_key`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn summarize_request_token_stats_by_key(
        &self,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<Vec<ApiKeyTokenUsageSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                key_id,
                IFNULL(
                    SUM(
                        CASE
                            WHEN day_key >= date(?1, 'unixepoch', 'localtime')
                                 AND day_key <= date(?2 - 1, 'unixepoch', 'localtime')
                                THEN total_tokens
                            ELSE 0
                        END
                    ),
                    0
                ) AS today_tokens,
                IFNULL(SUM(total_tokens), 0) AS total_tokens,
                IFNULL(
                    SUM(
                        CASE
                            WHEN day_key >= date(?1, 'unixepoch', 'localtime')
                                 AND day_key <= date(?2 - 1, 'unixepoch', 'localtime')
                                THEN estimated_cost_usd
                            ELSE 0.0
                        END
                    ),
                    0.0
                ) AS today_estimated_cost_usd,
                IFNULL(SUM(estimated_cost_usd), 0.0) AS estimated_cost_usd
             FROM request_token_daily_stats
             GROUP BY key_id
             ORDER BY total_tokens DESC, key_id ASC",
        )?;
        let mut rows = stmt.query((start_ts, end_ts))?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            items.push(ApiKeyTokenUsageSummary {
                key_id: row.get(0)?,
                today_tokens: row.get(1)?,
                total_tokens: row.get(2)?,
                today_estimated_cost_usd: row.get(3)?,
                estimated_cost_usd: row.get(4)?,
            });
        }
        Ok(items)
    }

    pub fn maintain_request_token_stats_if_due(&self) -> Result<bool> {
        let now = now_ts();
        let last_maintained_at = parse_setting_i64(
            self.get_app_setting(APP_SETTING_REQUEST_TOKEN_STATS_LAST_MAINTAINED_AT)?,
        )
        .unwrap_or(0);
        let interval = request_token_stats_maintenance_interval_seconds();
        if interval > 0 && now.saturating_sub(last_maintained_at) < interval {
            return Ok(false);
        }
        let deleted_rows = self.maintain_request_token_stats(now)?;
        self.set_app_setting(
            APP_SETTING_REQUEST_TOKEN_STATS_LAST_MAINTAINED_AT,
            &now.to_string(),
            now,
        )?;
        if deleted_rows > 0 {
            let _ = self.maybe_vacuum_request_token_stats(now);
        }
        Ok(true)
    }

    pub fn maintain_request_token_stats(&self, now: i64) -> Result<usize> {
        let retain_days = request_token_stats_retain_days();
        if retain_days <= 0 {
            return Ok(0);
        }
        let retain_seconds = retain_days.saturating_mul(24 * 60 * 60);
        let cutoff_ts = now.saturating_sub(retain_seconds);
        self.prune_request_token_stats_before(cutoff_ts)
    }

    pub fn prune_request_token_stats_before(&self, cutoff_ts: i64) -> Result<usize> {
        self.conn.execute(
            "DELETE FROM request_token_stats WHERE created_at < ?1",
            [cutoff_ts],
        )
    }

    fn maybe_vacuum_request_token_stats(&self, now: i64) -> Result<bool> {
        let last_vacuum_at = parse_setting_i64(
            self.get_app_setting(APP_SETTING_REQUEST_TOKEN_STATS_LAST_VACUUM_AT)?,
        )
        .unwrap_or(0);
        let vacuum_interval = request_token_stats_vacuum_interval_seconds();
        if vacuum_interval > 0 && now.saturating_sub(last_vacuum_at) < vacuum_interval {
            return Ok(false);
        }

        let freelist_count: i64 = self
            .conn
            .query_row("PRAGMA freelist_count", [], |row| row.get(0))?;
        if freelist_count < request_token_stats_vacuum_min_freelist_pages() {
            return Ok(false);
        }

        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")?;
        self.set_app_setting(
            APP_SETTING_REQUEST_TOKEN_STATS_LAST_VACUUM_AT,
            &now.to_string(),
            now,
        )?;
        Ok(true)
    }

    pub(super) fn ensure_request_token_daily_stats_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS request_token_daily_stats (
                day_key TEXT NOT NULL,
                key_id TEXT NOT NULL,
                request_count INTEGER NOT NULL DEFAULT 0,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                cached_input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                reasoning_output_tokens INTEGER NOT NULL DEFAULT 0,
                estimated_cost_usd REAL NOT NULL DEFAULT 0.0,
                PRIMARY KEY(day_key, key_id)
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_daily_stats_key_id_day_key
             ON request_token_daily_stats(key_id, day_key DESC)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_daily_stats_day_key
             ON request_token_daily_stats(day_key DESC)",
            [],
        )?;

        let summary_rows: i64 = self.conn.query_row(
            "SELECT COUNT(1) FROM request_token_daily_stats",
            [],
            |row| row.get(0),
        )?;
        if summary_rows > 0 {
            return Ok(());
        }

        self.conn.execute(
            "INSERT INTO request_token_daily_stats (
                day_key,
                key_id,
                request_count,
                input_tokens,
                cached_input_tokens,
                output_tokens,
                total_tokens,
                reasoning_output_tokens,
                estimated_cost_usd
             )
             SELECT
                date(created_at, 'unixepoch', 'localtime') AS day_key,
                key_id,
                COUNT(1) AS request_count,
                IFNULL(SUM(CASE WHEN input_tokens > 0 THEN input_tokens ELSE 0 END), 0),
                IFNULL(SUM(CASE WHEN cached_input_tokens > 0 THEN cached_input_tokens ELSE 0 END), 0),
                IFNULL(SUM(CASE WHEN output_tokens > 0 THEN output_tokens ELSE 0 END), 0),
                IFNULL(
                    SUM(
                        CASE
                            WHEN total_tokens IS NOT NULL THEN
                                CASE WHEN total_tokens > 0 THEN total_tokens ELSE 0 END
                            ELSE
                                CASE
                                    WHEN IFNULL(input_tokens, 0) - IFNULL(cached_input_tokens, 0) + IFNULL(output_tokens, 0) > 0
                                        THEN IFNULL(input_tokens, 0) - IFNULL(cached_input_tokens, 0) + IFNULL(output_tokens, 0)
                                    ELSE 0
                                END
                        END
                    ),
                    0
                ) AS total_tokens,
                IFNULL(
                    SUM(CASE WHEN reasoning_output_tokens > 0 THEN reasoning_output_tokens ELSE 0 END),
                    0
                ),
                IFNULL(SUM(CASE WHEN estimated_cost_usd > 0 THEN estimated_cost_usd ELSE 0.0 END), 0.0)
             FROM request_token_stats
             WHERE key_id IS NOT NULL AND TRIM(key_id) <> ''
             GROUP BY day_key, key_id",
            [],
        )?;
        Ok(())
    }

    /// 函数 `ensure_request_token_stats_table`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - super: 参数 super
    ///
    /// # 返回
    /// 返回函数执行结果
    pub(super) fn ensure_request_token_stats_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS request_token_stats (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                request_log_id INTEGER NOT NULL,
                key_id TEXT,
                account_id TEXT,
                model TEXT,
                input_tokens INTEGER,
                cached_input_tokens INTEGER,
                output_tokens INTEGER,
                total_tokens INTEGER,
                reasoning_output_tokens INTEGER,
                estimated_cost_usd REAL,
                created_at INTEGER NOT NULL
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_request_token_stats_request_log_id
             ON request_token_stats(request_log_id)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_stats_created_at
             ON request_token_stats(created_at DESC)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_stats_account_id_created_at
             ON request_token_stats(account_id, created_at DESC)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_request_token_stats_key_id_created_at
             ON request_token_stats(key_id, created_at DESC)",
            [],
        )?;
        self.ensure_column("request_token_stats", "total_tokens", "INTEGER")?;

        if self.has_column("request_logs", "input_tokens")? {
            // 中文注释：迁移历史 request_logs 里的 token 字段，避免升级后今日统计突然归零。
            self.conn.execute(
                "INSERT OR IGNORE INTO request_token_stats (
                    request_log_id, key_id, account_id, model,
                    input_tokens, cached_input_tokens, output_tokens, total_tokens, reasoning_output_tokens,
                    estimated_cost_usd, created_at
                 )
                 SELECT
                    id, key_id, account_id, model,
                    input_tokens, cached_input_tokens, output_tokens, NULL, reasoning_output_tokens,
                    estimated_cost_usd, created_at
                 FROM request_logs
                 WHERE input_tokens IS NOT NULL
                    OR cached_input_tokens IS NOT NULL
                    OR output_tokens IS NOT NULL
                    OR reasoning_output_tokens IS NOT NULL
                    OR estimated_cost_usd IS NOT NULL",
                [],
            )?;
        }
        self.ensure_request_token_daily_stats_table()?;
        Ok(())
    }
}
