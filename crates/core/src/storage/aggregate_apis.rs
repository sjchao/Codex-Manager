use rusqlite::{params, Result, Row};
use serde_json::{from_str, to_string};

use super::{
    now_ts, AggregateApi, AggregateApiDailyUsage, AggregateApiUsageCredential,
    AggregateApiUsageSyncStatus, Storage, Sub2ApiAccount, Sub2ApiAccountSummary,
};

const AGGREGATE_API_SELECT_SQL: &str = "SELECT
    id,
    provider_type,
    supported_models_json,
    supplier_name,
    sort,
    weight,
    url,
    auth_type,
    auth_params_json,
    action,
    status,
    created_at,
    updated_at,
    last_test_at,
    last_test_status,
    last_test_error,
    sub2api_account_id
 FROM aggregate_apis";

impl Storage {
    /// 函数 `insert_aggregate_api`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - api: 参数 api
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn insert_aggregate_api(&self, api: &AggregateApi) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO aggregate_apis (
                id,
                provider_type,
                supported_models_json,
                supplier_name,
                sort,
                weight,
                url,
                auth_type,
                auth_params_json,
                action,
                status,
                created_at,
                updated_at,
                last_test_at,
                last_test_status,
                last_test_error,
                sub2api_account_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                &api.id,
                &api.provider_type,
                serialize_supported_models(api.supported_models.as_slice()),
                &api.supplier_name,
                api.sort,
                api.weight,
                &api.url,
                &api.auth_type,
                &api.auth_params_json,
                &api.action,
                &api.status,
                api.created_at,
                api.updated_at,
                &api.last_test_at,
                &api.last_test_status,
                &api.last_test_error,
                &api.sub2api_account_id,
            ],
        )?;
        Ok(())
    }

    /// 函数 `list_aggregate_apis`
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
    pub fn list_aggregate_apis(&self) -> Result<Vec<AggregateApi>> {
        self.ensure_aggregate_apis_table()?;
        let mut stmt = self.conn.prepare(&format!(
            "{AGGREGATE_API_SELECT_SQL} ORDER BY sort ASC, weight DESC, created_at DESC"
        ))?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(map_aggregate_api_row(row)?);
        }
        Ok(out)
    }

    /// 函数 `find_aggregate_api_by_id`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - api_id: 参数 api_id
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn find_aggregate_api_by_id(&self, api_id: &str) -> Result<Option<AggregateApi>> {
        self.ensure_aggregate_apis_table()?;
        let mut stmt = self.conn.prepare(&format!(
            "{AGGREGATE_API_SELECT_SQL}
             WHERE id = ?1
             LIMIT 1"
        ))?;
        let mut rows = stmt.query([api_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(map_aggregate_api_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// 函数 `update_aggregate_api`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - api_id: 参数 api_id
    /// - url: 参数 url
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn update_aggregate_api(&self, api_id: &str, url: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis SET url = ?1, updated_at = ?2 WHERE id = ?3",
            (url, now_ts(), api_id),
        )?;
        Ok(())
    }

    /// 函数 `update_aggregate_api_supplier_name`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - api_id: 参数 api_id
    /// - supplier_name: 参数 supplier_name
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn update_aggregate_api_supplier_name(
        &self,
        api_id: &str,
        supplier_name: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis SET supplier_name = ?1, updated_at = ?2 WHERE id = ?3",
            (supplier_name, now_ts(), api_id),
        )?;
        Ok(())
    }

    /// 函数 `update_aggregate_api_sort`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - api_id: 参数 api_id
    /// - sort: 参数 sort
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn update_aggregate_api_sort(&self, api_id: &str, sort: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis SET sort = ?1, updated_at = ?2 WHERE id = ?3",
            (sort, now_ts(), api_id),
        )?;
        Ok(())
    }

    pub fn update_aggregate_api_weight(&self, api_id: &str, weight: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis SET weight = ?1, updated_at = ?2 WHERE id = ?3",
            (weight, now_ts(), api_id),
        )?;
        Ok(())
    }

    /// 函数 `update_aggregate_api_type`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - api_id: 参数 api_id
    /// - provider_type: 参数 provider_type
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn update_aggregate_api_type(&self, api_id: &str, provider_type: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis SET provider_type = ?1, updated_at = ?2 WHERE id = ?3",
            (provider_type, now_ts(), api_id),
        )?;
        Ok(())
    }

    pub fn update_aggregate_api_supported_models(
        &self,
        api_id: &str,
        supported_models: &[String],
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis
             SET supported_models_json = ?1, updated_at = ?2
             WHERE id = ?3",
            (
                serialize_supported_models(supported_models),
                now_ts(),
                api_id,
            ),
        )?;
        Ok(())
    }

    pub fn update_aggregate_api_auth_type(&self, api_id: &str, auth_type: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis SET auth_type = ?1, updated_at = ?2 WHERE id = ?3",
            (auth_type, now_ts(), api_id),
        )?;
        Ok(())
    }

    pub fn update_aggregate_api_auth_params_json(
        &self,
        api_id: &str,
        auth_params_json: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis SET auth_params_json = ?1, updated_at = ?2 WHERE id = ?3",
            (auth_params_json, now_ts(), api_id),
        )?;
        Ok(())
    }

    pub fn update_aggregate_api_action(&self, api_id: &str, action: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis SET action = ?1, updated_at = ?2 WHERE id = ?3",
            (action, now_ts(), api_id),
        )?;
        Ok(())
    }

    pub fn update_aggregate_api_status(&self, api_id: &str, status: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis SET status = ?1, updated_at = ?2 WHERE id = ?3",
            (status, now_ts(), api_id),
        )?;
        Ok(())
    }

    pub fn update_aggregate_api_sub2api_account_id(
        &self,
        api_id: &str,
        account_id: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_apis SET sub2api_account_id = ?1, updated_at = ?2 WHERE id = ?3",
            (account_id, now_ts(), api_id),
        )?;
        Ok(())
    }

    /// 函数 `delete_aggregate_api`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - api_id: 参数 api_id
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn delete_aggregate_api(&self, api_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM aggregate_api_usage_credentials WHERE aggregate_api_id = ?1",
            [api_id],
        )?;
        self.conn.execute(
            "DELETE FROM aggregate_api_daily_usage WHERE aggregate_api_id = ?1",
            [api_id],
        )?;
        self.conn.execute(
            "DELETE FROM aggregate_api_secrets WHERE aggregate_api_id = ?1",
            [api_id],
        )?;
        self.conn
            .execute("DELETE FROM aggregate_apis WHERE id = ?1", [api_id])?;
        Ok(())
    }

    /// 函数 `upsert_aggregate_api_secret`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - api_id: 参数 api_id
    /// - secret_value: 参数 secret_value
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn upsert_aggregate_api_secret(&self, api_id: &str, secret_value: &str) -> Result<()> {
        let now = now_ts();
        self.conn.execute(
            "INSERT INTO aggregate_api_secrets (aggregate_api_id, secret_value, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(aggregate_api_id) DO UPDATE SET
               secret_value = excluded.secret_value,
               updated_at = excluded.updated_at",
            (api_id, secret_value, now),
        )?;
        Ok(())
    }

    /// 函数 `find_aggregate_api_secret_by_id`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - api_id: 参数 api_id
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn find_aggregate_api_secret_by_id(&self, api_id: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT secret_value FROM aggregate_api_secrets WHERE aggregate_api_id = ?1 LIMIT 1",
        )?;
        let mut rows = stmt.query([api_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    /// 函数 `update_aggregate_api_test_result`
    ///
    /// 作者: gaohongshun
    ///
    /// 时间: 2026-04-02
    ///
    /// # 参数
    /// - self: 参数 self
    /// - api_id: 参数 api_id
    /// - ok: 参数 ok
    /// - status_code: 参数 status_code
    /// - error: 参数 error
    ///
    /// # 返回
    /// 返回函数执行结果
    pub fn update_aggregate_api_test_result(
        &self,
        api_id: &str,
        ok: bool,
        status_code: Option<i64>,
        error: Option<&str>,
    ) -> Result<()> {
        let now = now_ts();
        let last_test_status = if ok { Some("success") } else { Some("failed") };
        self.conn.execute(
            "UPDATE aggregate_apis
             SET last_test_at = ?1,
                 last_test_status = ?2,
                 last_test_error = ?3,
                 updated_at = ?1
             WHERE id = ?4",
            (now, last_test_status, error, api_id),
        )?;
        if let Some(code) = status_code {
            if !ok {
                let message = format!("http_status={code}");
                self.conn.execute(
                    "UPDATE aggregate_apis SET last_test_error = ?1 WHERE id = ?2",
                    (message, api_id),
                )?;
            }
        }
        Ok(())
    }

    /// 函数 `ensure_aggregate_apis_table`
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
    pub(super) fn ensure_aggregate_apis_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS aggregate_apis (
                id TEXT PRIMARY KEY,
                provider_type TEXT NOT NULL DEFAULT 'codex',
                supported_models_json TEXT NOT NULL DEFAULT '[]',
                supplier_name TEXT,
                sort INTEGER NOT NULL DEFAULT 0,
                weight INTEGER NOT NULL DEFAULT 100,
                url TEXT NOT NULL,
                auth_type TEXT NOT NULL DEFAULT 'apikey',
                auth_params_json TEXT,
                action TEXT,
                status TEXT NOT NULL DEFAULT 'active',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                last_test_at INTEGER,
                last_test_status TEXT,
                last_test_error TEXT,
                sub2api_account_id TEXT REFERENCES sub2api_accounts(id) ON DELETE SET NULL
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_aggregate_apis_created_at ON aggregate_apis(created_at DESC)",
            [],
        )?;
        self.ensure_column("aggregate_apis", "provider_type", "TEXT")?;
        self.ensure_column(
            "aggregate_apis",
            "supported_models_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )?;
        self.ensure_column("aggregate_apis", "supplier_name", "TEXT")?;
        self.ensure_column("aggregate_apis", "sort", "INTEGER DEFAULT 0")?;
        self.ensure_column("aggregate_apis", "weight", "INTEGER NOT NULL DEFAULT 100")?;
        self.ensure_column("aggregate_apis", "auth_type", "TEXT NOT NULL DEFAULT 'apikey'")?;
        self.ensure_column("aggregate_apis", "auth_params_json", "TEXT")?;
        self.ensure_column("aggregate_apis", "action", "TEXT")?;
        self.ensure_column(
            "aggregate_apis",
            "sub2api_account_id",
            "TEXT REFERENCES sub2api_accounts(id) ON DELETE SET NULL",
        )?;
        self.conn.execute(
            "UPDATE aggregate_apis
             SET provider_type = COALESCE(NULLIF(TRIM(provider_type), ''), 'codex')
             WHERE provider_type IS NULL OR TRIM(provider_type) = ''",
            [],
        )?;
        self.conn.execute(
            "UPDATE aggregate_apis
             SET supported_models_json = '[]'
             WHERE supported_models_json IS NULL OR TRIM(supported_models_json) = ''",
            [],
        )?;
        self.conn.execute(
            "UPDATE aggregate_apis
             SET auth_type = COALESCE(NULLIF(TRIM(auth_type), ''), 'apikey')
             WHERE auth_type IS NULL OR TRIM(auth_type) = ''",
            [],
        )?;
        self.conn.execute(
            "UPDATE aggregate_apis
             SET sort = COALESCE(sort, 0)
             WHERE sort IS NULL",
            [],
        )?;
        self.conn.execute(
            "UPDATE aggregate_apis
             SET weight = 100
             WHERE weight IS NULL OR weight <= 0",
            [],
        )?;
        Ok(())
    }

    pub(super) fn ensure_sub2api_accounts_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS sub2api_accounts (
                id TEXT PRIMARY KEY,
                base_url TEXT NOT NULL,
                auth_token TEXT NOT NULL,
                refresh_token TEXT,
                token_expires_at INTEGER,
                account_name TEXT,
                account_email TEXT,
                balance REAL,
                today_actual_cost REAL,
                today_total_cost REAL,
                today_request_count INTEGER,
                created_at INTEGER,
                updated_at INTEGER NOT NULL,
                last_sync_at INTEGER,
                last_sync_status TEXT,
                last_sync_error TEXT
            )",
            [],
        )?;
        self.ensure_sub2api_accounts_created_at_column()?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_sub2api_accounts_last_sync_at
             ON sub2api_accounts(last_sync_at DESC)",
            [],
        )?;
        Ok(())
    }

    pub(super) fn ensure_sub2api_accounts_created_at_column(&self) -> Result<()> {
        let has_column = {
            let mut stmt = self.conn.prepare("PRAGMA table_info(sub2api_accounts)")?;
            let mut rows = stmt.query([])?;
            let mut found = false;
            while let Some(row) = rows.next()? {
                if row.get::<_, String>(1)?.eq_ignore_ascii_case("created_at") {
                    found = true;
                    break;
                }
            }
            found
        };
        if !has_column {
            self.conn
                .execute("ALTER TABLE sub2api_accounts ADD COLUMN created_at INTEGER", [])?;
            self.conn.execute(
                "UPDATE sub2api_accounts
                 SET created_at = CASE
                   WHEN substr(id, 1, 8) = 'sub2api_'
                     AND length(substr(id, 9, 10)) = 10
                     AND substr(id, 9, 10) NOT GLOB '*[^0-9]*'
                   THEN CAST(substr(id, 9, 10) AS INTEGER)
                   ELSE updated_at
                 END
                 WHERE created_at IS NULL",
                [],
            )?;
        }
        Ok(())
    }

    pub fn list_sub2api_accounts(&self) -> Result<Vec<Sub2ApiAccount>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, base_url, auth_token, refresh_token, token_expires_at,
                    account_name, account_email, balance, today_actual_cost,
                    today_total_cost, today_request_count, created_at, updated_at,
                    last_sync_at, last_sync_status, last_sync_error
             FROM sub2api_accounts ORDER BY created_at ASC, id ASC",
        )?;
        let mut rows = stmt.query([])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            items.push(Sub2ApiAccount {
                id: row.get(0)?, base_url: row.get(1)?, auth_token: row.get(2)?,
                refresh_token: row.get(3)?, token_expires_at: row.get(4)?,
                account_name: row.get(5)?, account_email: row.get(6)?, balance: row.get(7)?,
                today_actual_cost: row.get(8)?, today_total_cost: row.get(9)?,
                today_request_count: row.get(10)?, created_at: row.get(11)?,
                updated_at: row.get(12)?, last_sync_at: row.get(13)?,
                last_sync_status: row.get(14)?, last_sync_error: row.get(15)?,
            });
        }
        Ok(items)
    }

    pub fn find_sub2api_account_by_id(&self, id: &str) -> Result<Option<Sub2ApiAccount>> {
        Ok(self.list_sub2api_accounts()?.into_iter().find(|item| item.id == id))
    }

    pub fn upsert_sub2api_account(
        &self,
        id: &str,
        base_url: &str,
        auth_token: &str,
        refresh_token: Option<&str>,
        token_expires_at: Option<i64>,
    ) -> Result<()> {
        let now = now_ts();
        self.conn.execute(
            "INSERT INTO sub2api_accounts (id, base_url, auth_token, refresh_token, token_expires_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(id) DO UPDATE SET
               base_url = excluded.base_url,
               auth_token = CASE WHEN length(excluded.auth_token) > 0 THEN excluded.auth_token ELSE sub2api_accounts.auth_token END,
               refresh_token = CASE WHEN excluded.refresh_token IS NOT NULL THEN excluded.refresh_token ELSE sub2api_accounts.refresh_token END,
               token_expires_at = COALESCE(excluded.token_expires_at, sub2api_accounts.token_expires_at),
               created_at = COALESCE(sub2api_accounts.created_at, excluded.created_at),
               updated_at = excluded.updated_at",
            params![id, base_url, auth_token, refresh_token, token_expires_at, now],
        )?;
        Ok(())
    }

    pub fn update_sub2api_account_sync(
        &self,
        id: &str,
        account_name: Option<&str>,
        account_email: Option<&str>,
        balance: Option<f64>,
        today_actual_cost: Option<f64>,
        today_total_cost: Option<f64>,
        today_request_count: Option<i64>,
        status: &str,
        error: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE sub2api_accounts SET account_name = COALESCE(?1, account_name),
                account_email = COALESCE(?2, account_email), balance = COALESCE(?3, balance),
                today_actual_cost = ?4, today_total_cost = ?5, today_request_count = ?6,
                last_sync_at = ?7, last_sync_status = ?8, last_sync_error = ?9, updated_at = ?7
             WHERE id = ?10",
            params![account_name, account_email, balance, today_actual_cost, today_total_cost,
                today_request_count, now_ts(), status, error, id],
        )?;
        Ok(())
    }

    pub fn update_sub2api_account_tokens(
        &self,
        id: &str,
        auth_token: &str,
        refresh_token: Option<&str>,
        token_expires_at: Option<i64>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE sub2api_accounts SET auth_token = ?1, refresh_token = COALESCE(?2, refresh_token),
                token_expires_at = COALESCE(?3, token_expires_at), updated_at = ?4 WHERE id = ?5",
            params![auth_token, refresh_token, token_expires_at, now_ts(), id],
        )?;
        Ok(())
    }

    pub fn delete_sub2api_account(&self, id: &str) -> Result<()> {
        self.conn.execute("UPDATE aggregate_apis SET sub2api_account_id = NULL WHERE sub2api_account_id = ?1", [id])?;
        self.conn.execute("DELETE FROM sub2api_accounts WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn list_sub2api_account_summaries(&self) -> Result<Vec<Sub2ApiAccountSummary>> {
        Ok(self.list_sub2api_accounts()?.into_iter().map(|item| Sub2ApiAccountSummary {
            id: item.id, base_url: item.base_url, token_expires_at: item.token_expires_at,
            account_name: item.account_name,
            account_email: item.account_email, balance: item.balance,
            today_actual_cost: item.today_actual_cost, today_total_cost: item.today_total_cost,
            today_request_count: item.today_request_count, updated_at: item.updated_at,
            last_sync_at: item.last_sync_at, last_sync_status: item.last_sync_status,
            last_sync_error: item.last_sync_error,
        }).collect())
    }

    /// 函数 `ensure_aggregate_api_secrets_table`
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
    pub(super) fn ensure_aggregate_api_secrets_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS aggregate_api_secrets (
                aggregate_api_id TEXT PRIMARY KEY REFERENCES aggregate_apis(id) ON DELETE CASCADE,
                secret_value TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_aggregate_api_secrets_updated_at ON aggregate_api_secrets(updated_at)",
            [],
        )?;
        Ok(())
    }

    pub(super) fn ensure_aggregate_api_usage_tables(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS aggregate_api_usage_credentials (
                aggregate_api_id TEXT PRIMARY KEY REFERENCES aggregate_apis(id) ON DELETE CASCADE,
                auth_token TEXT NOT NULL,
                refresh_token TEXT,
                token_expires_at INTEGER,
                updated_at INTEGER NOT NULL,
                last_sync_at INTEGER,
                last_sync_status TEXT,
                last_sync_error TEXT
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_aggregate_api_usage_credentials_last_sync_at
             ON aggregate_api_usage_credentials(last_sync_at DESC)",
            [],
        )?;
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS aggregate_api_daily_usage (
                aggregate_api_id TEXT NOT NULL REFERENCES aggregate_apis(id) ON DELETE CASCADE,
                usage_date TEXT NOT NULL,
                actual_cost REAL NOT NULL DEFAULT 0,
                total_cost REAL,
                request_count INTEGER,
                synced_at INTEGER NOT NULL,
                PRIMARY KEY (aggregate_api_id, usage_date)
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_aggregate_api_daily_usage_date
             ON aggregate_api_daily_usage(usage_date DESC, aggregate_api_id)",
            [],
        )?;
        Ok(())
    }

    pub fn find_aggregate_api_usage_credential_by_id(
        &self,
        api_id: &str,
    ) -> Result<Option<AggregateApiUsageCredential>> {
        let mut stmt = self.conn.prepare(
            "SELECT aggregate_api_id, auth_token, refresh_token, token_expires_at,
                    updated_at, last_sync_at, last_sync_status, last_sync_error
             FROM aggregate_api_usage_credentials
             WHERE aggregate_api_id = ?1
             LIMIT 1",
        )?;
        let mut rows = stmt.query([api_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(AggregateApiUsageCredential {
                aggregate_api_id: row.get(0)?,
                auth_token: row.get(1)?,
                refresh_token: row.get(2)?,
                token_expires_at: row.get(3)?,
                updated_at: row.get(4)?,
                last_sync_at: row.get(5)?,
                last_sync_status: row.get(6)?,
                last_sync_error: row.get(7)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_aggregate_api_usage_sync_statuses(
        &self,
    ) -> Result<Vec<AggregateApiUsageSyncStatus>> {
        let mut stmt = self.conn.prepare(
            "SELECT aggregate_api_id, last_sync_at, last_sync_status, last_sync_error
             FROM aggregate_api_usage_credentials",
        )?;
        let mut rows = stmt.query([])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            items.push(AggregateApiUsageSyncStatus {
                aggregate_api_id: row.get(0)?,
                configured: true,
                last_sync_at: row.get(1)?,
                last_sync_status: row.get(2)?,
                last_sync_error: row.get(3)?,
            });
        }
        Ok(items)
    }

    pub fn upsert_aggregate_api_usage_credential(
        &self,
        api_id: &str,
        auth_token: &str,
        refresh_token: Option<&str>,
        token_expires_at: Option<i64>,
    ) -> Result<()> {
        let now = now_ts();
        self.conn.execute(
            "INSERT INTO aggregate_api_usage_credentials (
                aggregate_api_id, auth_token, refresh_token, token_expires_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(aggregate_api_id) DO UPDATE SET
                auth_token = excluded.auth_token,
                refresh_token = excluded.refresh_token,
                token_expires_at = excluded.token_expires_at,
                updated_at = excluded.updated_at",
            params![api_id, auth_token, refresh_token, token_expires_at, now],
        )?;
        Ok(())
    }

    pub fn update_aggregate_api_usage_tokens(
        &self,
        api_id: &str,
        auth_token: &str,
        refresh_token: Option<&str>,
        token_expires_at: Option<i64>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_api_usage_credentials
             SET auth_token = ?1, refresh_token = ?2, token_expires_at = ?3, updated_at = ?4
             WHERE aggregate_api_id = ?5",
            params![api_id, auth_token, refresh_token, token_expires_at, now_ts()],
        )?;
        Ok(())
    }

    pub fn update_aggregate_api_usage_sync_status(
        &self,
        api_id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE aggregate_api_usage_credentials
             SET last_sync_at = ?1, last_sync_status = ?2, last_sync_error = ?3
             WHERE aggregate_api_id = ?4",
            params![now_ts(), status, error, api_id],
        )?;
        Ok(())
    }

    pub fn upsert_aggregate_api_daily_usage(&self, item: &AggregateApiDailyUsage) -> Result<()> {
        self.conn.execute(
            "INSERT INTO aggregate_api_daily_usage (
                aggregate_api_id, usage_date, actual_cost, total_cost, request_count, synced_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(aggregate_api_id, usage_date) DO UPDATE SET
                actual_cost = excluded.actual_cost,
                total_cost = excluded.total_cost,
                request_count = excluded.request_count,
                synced_at = excluded.synced_at",
            params![
                &item.aggregate_api_id,
                &item.usage_date,
                item.actual_cost,
                item.total_cost,
                item.request_count,
                item.synced_at,
            ],
        )?;
        Ok(())
    }

    pub fn find_aggregate_api_daily_usage(
        &self,
        api_id: &str,
        usage_date: &str,
    ) -> Result<Option<AggregateApiDailyUsage>> {
        let mut stmt = self.conn.prepare(
            "SELECT aggregate_api_id, usage_date, actual_cost, total_cost, request_count, synced_at
             FROM aggregate_api_daily_usage
             WHERE aggregate_api_id = ?1 AND usage_date = ?2
             LIMIT 1",
        )?;
        let mut rows = stmt.query(params![api_id, usage_date])?;
        if let Some(row) = rows.next()? {
            Ok(Some(AggregateApiDailyUsage {
                aggregate_api_id: row.get(0)?,
                usage_date: row.get(1)?,
                actual_cost: row.get(2)?,
                total_cost: row.get(3)?,
                request_count: row.get(4)?,
                synced_at: row.get(5)?,
            }))
        } else {
            Ok(None)
        }
    }
}

/// 函数 `map_aggregate_api_row`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - row: 参数 row
///
/// # 返回
/// 返回函数执行结果
fn map_aggregate_api_row(row: &Row<'_>) -> Result<AggregateApi> {
    Ok(AggregateApi {
        id: row.get(0)?,
        provider_type: row.get(1)?,
        supported_models: parse_supported_models(row.get::<_, String>(2)?.as_str()),
        supplier_name: row.get(3)?,
        sort: row.get(4)?,
        weight: row.get(5)?,
        url: row.get(6)?,
        auth_type: row.get(7)?,
        auth_params_json: row.get(8)?,
        action: row.get(9)?,
        status: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
        last_test_at: row.get(13)?,
        last_test_status: row.get(14)?,
        last_test_error: row.get(15)?,
        sub2api_account_id: row.get(16)?,
    })
}

pub fn normalize_supported_models(models: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for model in models {
        let model = model.trim();
        if !model.is_empty()
            && !normalized
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(model))
        {
            normalized.push(model.to_string());
        }
    }
    normalized
}

fn parse_supported_models(value: &str) -> Vec<String> {
    from_str::<Vec<String>>(value)
        .map(|models| normalize_supported_models(models.as_slice()))
        .unwrap_or_default()
}

fn serialize_supported_models(models: &[String]) -> String {
    to_string(&normalize_supported_models(models)).expect("serialize supported models")
}
