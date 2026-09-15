CREATE TABLE IF NOT EXISTS sub2api_accounts (
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
  updated_at INTEGER NOT NULL,
  last_sync_at INTEGER,
  last_sync_status TEXT,
  last_sync_error TEXT
);

CREATE INDEX IF NOT EXISTS idx_sub2api_accounts_last_sync_at
  ON sub2api_accounts(last_sync_at DESC);

ALTER TABLE aggregate_apis ADD COLUMN sub2api_account_id TEXT REFERENCES sub2api_accounts(id) ON DELETE SET NULL;

