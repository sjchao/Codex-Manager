use rusqlite::params;

use super::Storage;

#[derive(Debug, Clone)]
pub struct QoderMember {
    pub member_id: String,
    pub user_id: Option<String>,
    pub name: Option<String>,
    pub email: Option<String>,
    pub role: Option<String>,
    pub status: Option<String>,
    pub joined_at: Option<String>,
    pub deleted_at: Option<String>,
    pub synced_at: i64,
    pub plan_used: Option<f64>,
    pub plan_limit: Option<f64>,
    pub resource_package_used: Option<f64>,
    pub resource_package_limit: Option<f64>,
    pub total_used: Option<f64>,
    pub total_limit: Option<f64>,
    pub shared_used: Option<f64>,
    pub shared_limit: Option<f64>,
    pub quota_unit: Option<String>,
    pub quota_status: Option<String>,
    pub last_reset_at: Option<String>,
    pub next_reset_at: Option<String>,
    pub quota_synced_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct QoderMemberQuota {
    pub member_id: String,
    pub plan_used: Option<f64>,
    pub plan_limit: Option<f64>,
    pub resource_package_used: Option<f64>,
    pub resource_package_limit: Option<f64>,
    pub total_used: Option<f64>,
    pub total_limit: Option<f64>,
    pub shared_used: Option<f64>,
    pub shared_limit: Option<f64>,
    pub quota_unit: Option<String>,
    pub quota_status: Option<String>,
    pub last_reset_at: Option<String>,
    pub next_reset_at: Option<String>,
    pub synced_at: i64,
}

#[derive(Debug, Clone)]
pub struct QoderUsageEvent {
    pub event_id: String,
    pub member_id: Option<String>,
    pub user_id: Option<String>,
    pub user_email: Option<String>,
    pub source: Option<String>,
    pub operation: Option<String>,
    pub model_tier: Option<String>,
    pub credits: f64,
    pub cost: Option<f64>,
    pub occurred_at: i64,
    pub day: String,
    pub synced_at: i64,
}

#[derive(Debug, Clone, Default)]
pub struct QoderSyncState {
    pub last_sync_at: Option<i64>,
    pub last_sync_status: Option<String>,
    pub last_sync_error: Option<String>,
    pub covered_until: Option<i64>,
    pub backfill_from: Option<i64>,
    pub last_event_at: Option<i64>,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct QoderMemberUsageRow {
    pub member_id: String,
    pub user_id: Option<String>,
    pub user_email: Option<String>,
    pub credits: f64,
    pub events: i64,
}

#[derive(Debug, Clone)]
pub struct QoderModelUsageRow {
    pub model_tier: Option<String>,
    pub credits: f64,
    pub events: i64,
}

#[derive(Debug, Clone)]
pub struct QoderBucketUsageRow {
    pub bucket: String,
    pub model_tier: Option<String>,
    pub credits: f64,
    pub events: i64,
}

fn row_to_usage_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<QoderUsageEvent> {
    Ok(QoderUsageEvent {
        event_id: row.get(0)?,
        member_id: row.get(1)?,
        user_id: row.get(2)?,
        user_email: row.get(3)?,
        source: row.get(4)?,
        operation: row.get(5)?,
        model_tier: row.get(6)?,
        credits: row.get(7)?,
        cost: row.get(8)?,
        occurred_at: row.get(9)?,
        day: row.get(10)?,
        synced_at: row.get(11)?,
    })
}

impl Storage {
    pub(super) fn ensure_qoder_member_quota_columns(&self) -> rusqlite::Result<()> {
        self.ensure_column("qoder_members", "plan_used", "REAL")?;
        self.ensure_column("qoder_members", "plan_limit", "REAL")?;
        self.ensure_column("qoder_members", "resource_package_used", "REAL")?;
        self.ensure_column("qoder_members", "resource_package_limit", "REAL")?;
        self.ensure_column("qoder_members", "total_used", "REAL")?;
        self.ensure_column("qoder_members", "total_limit", "REAL")?;
        self.ensure_column("qoder_members", "shared_used", "REAL")?;
        self.ensure_column("qoder_members", "shared_limit", "REAL")?;
        self.ensure_column("qoder_members", "quota_unit", "TEXT")?;
        self.ensure_column("qoder_members", "quota_status", "TEXT")?;
        self.ensure_column("qoder_members", "last_reset_at", "TEXT")?;
        self.ensure_column("qoder_members", "next_reset_at", "TEXT")?;
        self.ensure_column("qoder_members", "quota_synced_at", "INTEGER")?;
        Ok(())
    }

    pub fn upsert_qoder_members(&self, members: &[QoderMember]) -> rusqlite::Result<()> {
        if members.is_empty() {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO qoder_members (member_id, user_id, name, email, role, status, joined_at, deleted_at, synced_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(member_id) DO UPDATE SET
                   user_id = excluded.user_id,
                   name = excluded.name,
                   email = excluded.email,
                   role = excluded.role,
                   status = excluded.status,
                   joined_at = excluded.joined_at,
                   deleted_at = excluded.deleted_at,
                   synced_at = excluded.synced_at",
            )?;
            for member in members {
                stmt.execute(params![
                    member.member_id,
                    member.user_id,
                    member.name,
                    member.email,
                    member.role,
                    member.status,
                    member.joined_at,
                    member.deleted_at,
                    member.synced_at,
                ])?;
            }
        }
        tx.commit()
    }

    pub fn update_qoder_member_quotas(&self, quotas: &[QoderMemberQuota]) -> rusqlite::Result<()> {
        if quotas.is_empty() {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "UPDATE qoder_members SET
                   plan_used = ?2,
                   plan_limit = ?3,
                   resource_package_used = ?4,
                   resource_package_limit = ?5,
                   total_used = ?6,
                   total_limit = ?7,
                   shared_used = ?8,
                   shared_limit = ?9,
                   quota_unit = ?10,
                   quota_status = ?11,
                   last_reset_at = ?12,
                   next_reset_at = ?13,
                   quota_synced_at = ?14
                 WHERE member_id = ?1",
            )?;
            for quota in quotas {
                stmt.execute(params![
                    quota.member_id,
                    quota.plan_used,
                    quota.plan_limit,
                    quota.resource_package_used,
                    quota.resource_package_limit,
                    quota.total_used,
                    quota.total_limit,
                    quota.shared_used,
                    quota.shared_limit,
                    quota.quota_unit,
                    quota.quota_status,
                    quota.last_reset_at,
                    quota.next_reset_at,
                    quota.synced_at,
                ])?;
            }
        }
        tx.commit()
    }

    /// 写入用量事件；同一 `event_id` 重复拉取时以最新值覆盖。
    /// 官方会在事件产生后修正（Auto 重归类为具体模型、积分重算），UPSERT 保证修正能回写。
    pub fn upsert_qoder_usage_events(&self, events: &[QoderUsageEvent]) -> rusqlite::Result<usize> {
        if events.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.unchecked_transaction()?;
        let mut written = 0usize;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO qoder_usage_events
                   (event_id, member_id, user_id, user_email, source, operation, model_tier, credits, cost, occurred_at, day, synced_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(event_id) DO UPDATE SET
                   member_id = COALESCE(excluded.member_id, qoder_usage_events.member_id),
                   user_id = COALESCE(excluded.user_id, qoder_usage_events.user_id),
                   user_email = COALESCE(excluded.user_email, qoder_usage_events.user_email),
                   source = COALESCE(excluded.source, qoder_usage_events.source),
                   operation = COALESCE(excluded.operation, qoder_usage_events.operation),
                   model_tier = COALESCE(excluded.model_tier, qoder_usage_events.model_tier),
                   credits = excluded.credits,
                   cost = COALESCE(excluded.cost, qoder_usage_events.cost),
                   occurred_at = excluded.occurred_at,
                   day = excluded.day,
                   synced_at = excluded.synced_at",
            )?;
            for event in events {
                written += stmt.execute(params![
                    event.event_id,
                    event.member_id,
                    event.user_id,
                    event.user_email,
                    event.source,
                    event.operation,
                    event.model_tier,
                    event.credits,
                    event.cost,
                    event.occurred_at,
                    event.day,
                    event.synced_at,
                ])?;
            }
        }
        tx.commit()?;
        Ok(written)
    }

    pub fn get_qoder_sync_state(&self) -> rusqlite::Result<Option<QoderSyncState>> {
        let mut stmt = self.conn.prepare(
            "SELECT last_sync_at, last_sync_status, last_sync_error, covered_until, backfill_from, last_event_at, updated_at
             FROM qoder_sync_state
             WHERE id = 1
             LIMIT 1",
        )?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            return Ok(Some(QoderSyncState {
                last_sync_at: row.get(0)?,
                last_sync_status: row.get(1)?,
                last_sync_error: row.get(2)?,
                covered_until: row.get(3)?,
                backfill_from: row.get(4)?,
                last_event_at: row.get(5)?,
                updated_at: row.get(6)?,
            }));
        }
        Ok(None)
    }

    pub fn save_qoder_sync_state(&self, state: &QoderSyncState) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO qoder_sync_state (id, last_sync_at, last_sync_status, last_sync_error, covered_until, backfill_from, last_event_at, updated_at)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
               last_sync_at = excluded.last_sync_at,
               last_sync_status = excluded.last_sync_status,
               last_sync_error = excluded.last_sync_error,
               covered_until = excluded.covered_until,
               backfill_from = excluded.backfill_from,
               last_event_at = excluded.last_event_at,
               updated_at = excluded.updated_at",
            params![
                state.last_sync_at,
                state.last_sync_status,
                state.last_sync_error,
                state.covered_until,
                state.backfill_from,
                state.last_event_at,
                state.updated_at,
            ],
        )?;
        Ok(())
    }

    /// 组织切换时清空既有成员与事件，避免不同组织数据混在一起。
    pub fn clear_qoder_data(&self) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM qoder_usage_events", [])?;
        tx.execute("DELETE FROM qoder_members", [])?;
        tx.execute("DELETE FROM qoder_sync_state", [])?;
        tx.commit()
    }

    pub fn count_qoder_usage_events(&self) -> rusqlite::Result<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM qoder_usage_events", [], |row| row.get(0))
    }

    pub fn count_qoder_members(&self) -> rusqlite::Result<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM qoder_members", [], |row| row.get(0))
    }

    pub fn qoder_usage_day_bounds(&self) -> rusqlite::Result<(Option<String>, Option<String>)> {
        self.conn.query_row(
            "SELECT MIN(day), MAX(day) FROM qoder_usage_events",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
    }

    pub fn list_qoder_members(&self) -> rusqlite::Result<Vec<QoderMember>> {
        let mut stmt = self.conn.prepare(
            "SELECT member_id, user_id, name, email, role, status, joined_at, deleted_at, synced_at,
                    plan_used, plan_limit, resource_package_used, resource_package_limit,
                    total_used, total_limit, shared_used, shared_limit,
                    quota_unit, quota_status, last_reset_at, next_reset_at, quota_synced_at
             FROM qoder_members
             ORDER BY name COLLATE NOCASE ASC, member_id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(QoderMember {
                member_id: row.get(0)?,
                user_id: row.get(1)?,
                name: row.get(2)?,
                email: row.get(3)?,
                role: row.get(4)?,
                status: row.get(5)?,
                joined_at: row.get(6)?,
                deleted_at: row.get(7)?,
                synced_at: row.get(8)?,
                plan_used: row.get(9)?,
                plan_limit: row.get(10)?,
                resource_package_used: row.get(11)?,
                resource_package_limit: row.get(12)?,
                total_used: row.get(13)?,
                total_limit: row.get(14)?,
                shared_used: row.get(15)?,
                shared_limit: row.get(16)?,
                quota_unit: row.get(17)?,
                quota_status: row.get(18)?,
                last_reset_at: row.get(19)?,
                next_reset_at: row.get(20)?,
                quota_synced_at: row.get(21)?,
            })
        })?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn qoder_usage_totals(
        &self,
        start_day: &str,
        end_day: &str,
        member_id: Option<&str>,
    ) -> rusqlite::Result<(f64, i64)> {
        self.conn.query_row(
            "SELECT COALESCE(SUM(credits), 0), COUNT(*)
             FROM qoder_usage_events
             WHERE day >= ?1 AND day <= ?2 AND (?3 IS NULL OR member_id = ?3)",
            params![start_day, end_day, member_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
    }

    pub fn qoder_active_member_count(
        &self,
        start_day: &str,
        end_day: &str,
    ) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(DISTINCT member_id)
             FROM qoder_usage_events
             WHERE day >= ?1 AND day <= ?2 AND member_id IS NOT NULL AND member_id != ''",
            params![start_day, end_day],
            |row| row.get(0),
        )
    }

    pub fn qoder_model_usage(
        &self,
        start_day: &str,
        end_day: &str,
        member_id: Option<&str>,
    ) -> rusqlite::Result<Vec<QoderModelUsageRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT model_tier, COALESCE(SUM(credits), 0) AS credits_sum, COUNT(*) AS events
             FROM qoder_usage_events
             WHERE day >= ?1 AND day <= ?2 AND (?3 IS NULL OR member_id = ?3)
             GROUP BY model_tier
             ORDER BY credits_sum DESC, model_tier ASC",
        )?;
        let rows = stmt.query_map(params![start_day, end_day, member_id], |row| {
            Ok(QoderModelUsageRow {
                model_tier: row.get(0)?,
                credits: row.get(1)?,
                events: row.get(2)?,
            })
        })?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    /// 按时间桶 × 模型档位聚合积分；`half_hour` 为 true 时桶粒度为半小时（本地时区）。
    pub fn qoder_usage_buckets(
        &self,
        start_day: &str,
        end_day: &str,
        half_hour: bool,
        member_id: Option<&str>,
    ) -> rusqlite::Result<Vec<QoderBucketUsageRow>> {
        let sql = if half_hour {
            "SELECT printf('%sT%02d:%02d', day,
                           CAST(strftime('%H', occurred_at / 1000, 'unixepoch', 'localtime') AS INTEGER),
                           CASE WHEN CAST(strftime('%M', occurred_at / 1000, 'unixepoch', 'localtime') AS INTEGER) >= 30
                                THEN 30 ELSE 0 END) AS bucket,
                    model_tier,
                    COALESCE(SUM(credits), 0) AS credits_sum,
                    COUNT(*) AS events
             FROM qoder_usage_events
             WHERE day >= ?1 AND day <= ?2 AND (?3 IS NULL OR member_id = ?3)
             GROUP BY bucket, model_tier
             ORDER BY bucket ASC, credits_sum DESC, model_tier ASC"
        } else {
            "SELECT day AS bucket,
                    model_tier,
                    COALESCE(SUM(credits), 0) AS credits_sum,
                    COUNT(*) AS events
             FROM qoder_usage_events
             WHERE day >= ?1 AND day <= ?2 AND (?3 IS NULL OR member_id = ?3)
             GROUP BY day, model_tier
             ORDER BY bucket ASC, credits_sum DESC, model_tier ASC"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![start_day, end_day, member_id], |row| {
            Ok(QoderBucketUsageRow {
                bucket: row.get(0)?,
                model_tier: row.get(1)?,
                credits: row.get(2)?,
                events: row.get(3)?,
            })
        })?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn qoder_member_usage(
        &self,
        start_day: &str,
        end_day: &str,
    ) -> rusqlite::Result<Vec<QoderMemberUsageRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(member_id, '') AS member_id,
                    MAX(user_id) AS user_id,
                    MAX(user_email) AS user_email,
                    COALESCE(SUM(credits), 0) AS credits_sum,
                    COUNT(*) AS events
             FROM qoder_usage_events
             WHERE day >= ?1 AND day <= ?2
             GROUP BY COALESCE(member_id, '')
             ORDER BY credits_sum DESC, member_id ASC",
        )?;
        let rows = stmt.query_map(params![start_day, end_day], |row| {
            Ok(QoderMemberUsageRow {
                member_id: row.get(0)?,
                user_id: row.get(1)?,
                user_email: row.get(2)?,
                credits: row.get(3)?,
                events: row.get(4)?,
            })
        })?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    /// 按事件时间倒序分页返回用量明细，可按成员过滤。
    pub fn qoder_usage_events_page(
        &self,
        start_day: &str,
        end_day: &str,
        member_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> rusqlite::Result<(Vec<QoderUsageEvent>, i64)> {
        const SELECT_COLUMNS: &str = "SELECT event_id, member_id, user_id, user_email, source, operation, model_tier, credits, cost, occurred_at, day, synced_at
             FROM qoder_usage_events
             WHERE day >= ?1 AND day <= ?2";
        let (total, rows) = match member_id {
            Some(member_id) => {
                let total = self.conn.query_row(
                    "SELECT COUNT(*) FROM qoder_usage_events
                     WHERE day >= ?1 AND day <= ?2 AND member_id = ?3",
                    params![start_day, end_day, member_id],
                    |row| row.get(0),
                )?;
                let sql = format!(
                    "{SELECT_COLUMNS} AND member_id = ?3 ORDER BY occurred_at DESC, event_id DESC LIMIT ?4 OFFSET ?5"
                );
                let mut stmt = self.conn.prepare(&sql)?;
                let items = stmt
                    .query_map(
                        params![start_day, end_day, member_id, limit, offset],
                        row_to_usage_event,
                    )?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                (total, items)
            }
            None => {
                let total = self.conn.query_row(
                    "SELECT COUNT(*) FROM qoder_usage_events WHERE day >= ?1 AND day <= ?2",
                    params![start_day, end_day],
                    |row| row.get(0),
                )?;
                let sql = format!(
                    "{SELECT_COLUMNS} ORDER BY occurred_at DESC, event_id DESC LIMIT ?3 OFFSET ?4"
                );
                let mut stmt = self.conn.prepare(&sql)?;
                let items = stmt
                    .query_map(params![start_day, end_day, limit, offset], row_to_usage_event)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                (total, items)
            }
        };
        Ok((rows, total))
    }
}

#[cfg(test)]
#[path = "tests/qoder_usage_tests.rs"]
mod tests;
