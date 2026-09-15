CREATE TABLE IF NOT EXISTS aggregate_api_usage_credentials (
  aggregate_api_id TEXT PRIMARY KEY REFERENCES aggregate_apis(id) ON DELETE CASCADE,
  auth_token TEXT NOT NULL,
  refresh_token TEXT,
  token_expires_at INTEGER,
  updated_at INTEGER NOT NULL,
  last_sync_at INTEGER,
  last_sync_status TEXT,
  last_sync_error TEXT
);

CREATE INDEX IF NOT EXISTS idx_aggregate_api_usage_credentials_last_sync_at
  ON aggregate_api_usage_credentials(last_sync_at DESC);

CREATE TABLE IF NOT EXISTS aggregate_api_daily_usage (
  aggregate_api_id TEXT NOT NULL REFERENCES aggregate_apis(id) ON DELETE CASCADE,
  usage_date TEXT NOT NULL,
  actual_cost REAL NOT NULL DEFAULT 0,
  total_cost REAL,
  request_count INTEGER,
  synced_at INTEGER NOT NULL,
  PRIMARY KEY (aggregate_api_id, usage_date)
);

CREATE INDEX IF NOT EXISTS idx_aggregate_api_daily_usage_date
  ON aggregate_api_daily_usage(usage_date DESC, aggregate_api_id);

ALTER TABLE request_logs ADD COLUMN aggregate_api_id TEXT;
ALTER TABLE request_logs ADD COLUMN upstream_actual_cost REAL;
ALTER TABLE request_logs ADD COLUMN upstream_total_cost REAL;
ALTER TABLE request_logs ADD COLUMN upstream_duration_ms INTEGER;
ALTER TABLE request_logs ADD COLUMN upstream_first_response_ms INTEGER;
ALTER TABLE request_logs ADD COLUMN upstream_usage_synced_at INTEGER;

CREATE INDEX IF NOT EXISTS idx_request_logs_aggregate_api_trace_id
  ON request_logs(aggregate_api_id, trace_id);
